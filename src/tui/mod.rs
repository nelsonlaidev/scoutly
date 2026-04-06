pub mod app;
pub mod render;

use std::future::Future;
use std::time::Duration;

#[cfg(test)]
use std::pin::Pin;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::{Terminal, backend::Backend};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::LocalSet;

use crate::config::RuntimeOptions;
use crate::execute_scan;
use crate::runtime::RunEvent;
use crate::update;

use self::app::{App, AppAction};

pub async fn run(runtime: RuntimeOptions) -> Result<()> {
    #[cfg(test)]
    if let Some(launch_ui) = take_test_ui_launch_override() {
        return run_with_ui_launcher(runtime, move |runtime, receiver, command_sender| {
            launch_ui(runtime, receiver, command_sender)
        })
        .await;
    }

    run_with_ui_launcher(runtime, launch_default_ui).await
}

async fn launch_default_ui(
    runtime: RuntimeOptions,
    receiver: UnboundedReceiver<RunEvent>,
    command_sender: UnboundedSender<String>,
) -> Result<UiOutcome> {
    launch_default_ui_with(
        runtime,
        receiver,
        command_sender,
        ratatui::init,
        ratatui::restore,
        run_app_with_default_events,
    )
    .await
}

fn run_app_with_default_events<B>(
    terminal: &mut Terminal<B>,
    app: App,
    receiver: UnboundedReceiver<RunEvent>,
    command_sender: UnboundedSender<String>,
) -> Result<UiOutcome>
where
    B: Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    run_app_with_events(
        terminal,
        app,
        receiver,
        command_sender,
        event::poll,
        event::read,
    )
}

async fn launch_default_ui_with<B, Init, Restore, Run>(
    runtime: RuntimeOptions,
    receiver: UnboundedReceiver<RunEvent>,
    command_sender: UnboundedSender<String>,
    init_terminal: Init,
    restore_terminal: Restore,
    run_app: Run,
) -> Result<UiOutcome>
where
    B: Backend + Send + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
    Init: FnOnce() -> Terminal<B> + Send + 'static,
    Restore: FnOnce() + Send + 'static,
    Run: FnOnce(
            &mut Terminal<B>,
            App,
            UnboundedReceiver<RunEvent>,
            UnboundedSender<String>,
        ) -> Result<UiOutcome>
        + Send
        + 'static,
{
    tokio::task::spawn_blocking(move || {
        let mut terminal = init_terminal();
        let result = run_app(&mut terminal, App::new(runtime), receiver, command_sender);
        restore_terminal();
        result
    })
    .await?
}

async fn run_with_ui_launcher<F, Fut>(runtime: RuntimeOptions, launch_ui: F) -> Result<()>
where
    F: FnOnce(RuntimeOptions, UnboundedReceiver<RunEvent>, UnboundedSender<String>) -> Fut,
    Fut: Future<Output = Result<UiOutcome>>,
{
    LocalSet::new()
        .run_until(async move {
            let (event_sender, event_receiver) = unbounded_channel();
            let (command_sender, command_receiver) = unbounded_channel::<String>();
            let scan_template = runtime.clone();
            let initial_url = scan_template.url.clone();
            let event_sender_for_scans = event_sender.clone();
            spawn_update_check(event_sender.clone());

            let scan_handle = tokio::task::spawn_local(scan_loop(
                initial_url,
                scan_template,
                event_sender_for_scans,
                command_receiver,
            ));

            let ui_outcome = launch_ui(runtime, event_receiver, command_sender).await?;

            if ui_outcome.abort_scan {
                scan_handle.abort();
                return Ok(());
            }

            scan_handle.await.expect("scan task should not panic")
        })
        .await
}

async fn scan_loop(
    initial_url: Option<String>,
    scan_template: RuntimeOptions,
    event_sender_for_scans: UnboundedSender<RunEvent>,
    mut command_receiver: UnboundedReceiver<String>,
) -> Result<()> {
    if let Some(url) = initial_url {
        run_scan(url, &scan_template, &event_sender_for_scans).await?;
    }

    while let Some(url) = command_receiver.recv().await {
        run_scan(url, &scan_template, &event_sender_for_scans).await?;
    }

    Ok(())
}

async fn run_scan(
    url: String,
    template: &RuntimeOptions,
    event_sender: &UnboundedSender<RunEvent>,
) -> Result<()> {
    let mut runtime = template.clone();
    runtime.url = Some(url);

    let result = execute_scan(&runtime, Some(event_sender.clone()), false).await;
    if let Err(error) = &result {
        let _ = event_sender.send(RunEvent::Error(error.to_string()));
    }
    result.map(|_| ())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UiOutcome {
    abort_scan: bool,
}

fn run_app_with_events<B, P, R>(
    terminal: &mut Terminal<B>,
    mut app: App,
    receiver: UnboundedReceiver<RunEvent>,
    command_sender: UnboundedSender<String>,
    mut poll_event: P,
    mut read_event: R,
) -> Result<UiOutcome>
where
    B: Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
    P: FnMut(Duration) -> std::io::Result<bool>,
    R: FnMut() -> std::io::Result<Event>,
{
    let mut receiver = receiver;
    let outcome = loop {
        while let Ok(event) = receiver.try_recv() {
            app.apply_run_event(event);
        }

        terminal.draw(|frame| render::render(frame, &app))?;

        if app.should_quit {
            break UiOutcome {
                abort_scan: app.has_active_scan(),
            };
        }

        if poll_event(Duration::from_millis(100))? {
            match read_event()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if let Some(AppAction::StartScan(url)) = app.handle_key(key) {
                        let _ = command_sender.send(url);
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    };

    Ok(outcome)
}

fn spawn_update_check(event_sender: UnboundedSender<RunEvent>) {
    spawn_update_check_with(event_sender, update::check_for_update);
}

#[cfg(test)]
type TestUiLaunchFuture = Pin<Box<dyn Future<Output = Result<UiOutcome>>>>;

#[cfg(test)]
type TestUiLaunchOverride =
    fn(RuntimeOptions, UnboundedReceiver<RunEvent>, UnboundedSender<String>) -> TestUiLaunchFuture;

#[cfg(test)]
static TEST_UI_LAUNCH_OVERRIDE: std::sync::Mutex<Option<TestUiLaunchOverride>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
fn set_test_ui_launch_override(launch: TestUiLaunchOverride) {
    *TEST_UI_LAUNCH_OVERRIDE.lock().unwrap() = Some(launch);
}

#[cfg(test)]
fn take_test_ui_launch_override() -> Option<TestUiLaunchOverride> {
    TEST_UI_LAUNCH_OVERRIDE.lock().unwrap().take()
}

fn spawn_update_check_with<F, Fut>(event_sender: UnboundedSender<RunEvent>, update_check: F)
where
    F: FnOnce() -> Fut + 'static,
    Fut: Future<Output = Option<crate::update::UpdateNotice>> + 'static,
{
    tokio::task::spawn_local(async move {
        dispatch_update_check(event_sender, update_check).await;
    });
}

async fn dispatch_update_check<F, Fut>(event_sender: UnboundedSender<RunEvent>, update_check: F)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Option<crate::update::UpdateNotice>>,
{
    if let Some(notice) = update_check().await {
        let _ = event_sender.send(RunEvent::UpdateAvailable(notice));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use serial_test::serial;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use crate::tui::app::UiMode;

    fn runtime(url: Option<&str>) -> RuntimeOptions {
        RuntimeOptions {
            url: url.map(str::to_string),
            depth: 1,
            max_pages: 1,
            output: None,
            save: None,
            cli: false,
            external: false,
            verbose: false,
            ignore_redirects: false,
            keep_fragments: false,
            rate_limit: None,
            concurrency: 1,
            respect_robots_txt: false,
            tui: false,
            config: None,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    #[serial]
    async fn startup_update_check_sends_event_without_initial_url() {
        let local = tokio::task::LocalSet::new();
        let (sender, mut receiver) = unbounded_channel();
        let event = local
            .run_until(async move {
                dispatch_update_check(sender, || async {
                    Some(crate::update::UpdateNotice {
                        latest_version: "9.9.9".to_string(),
                        release_url: "https://example.com/releases/v9.9.9".to_string(),
                    })
                })
                .await;
                tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                    .await
                    .expect("update event should arrive")
                    .expect("sender should stay alive")
            })
            .await;

        match event {
            RunEvent::UpdateAvailable(notice) => {
                assert_eq!(notice.latest_version, "9.9.9");
            }
            other => panic!("expected update event, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    #[serial]
    async fn spawn_update_check_uses_local_task_and_sends_notice() {
        let local = tokio::task::LocalSet::new();
        let (sender, mut receiver) = unbounded_channel();
        let event = local
            .run_until(async move {
                spawn_update_check_with(sender, || async {
                    Some(crate::update::UpdateNotice {
                        latest_version: "1.2.3".to_string(),
                        release_url: "https://example.com/releases/v1.2.3".to_string(),
                    })
                });
                tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                    .await
                    .expect("update event should arrive")
                    .expect("sender should stay alive")
            })
            .await;

        assert!(matches!(event, RunEvent::UpdateAvailable(_)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_with_ui_launcher_aborts_scan_when_ui_requests_abort() {
        let result = run_with_ui_launcher(runtime(None), |_, _, _| async {
            Ok(UiOutcome { abort_scan: true })
        })
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_with_ui_launcher_propagates_scan_errors() {
        let result = run_with_ui_launcher(runtime(Some("not-a-url")), |_, _, _| async {
            Ok(UiOutcome { abort_scan: false })
        })
        .await;

        assert!(result.is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_with_ui_launcher_waits_for_scan_loop_completion() {
        let result = run_with_ui_launcher(runtime(None), |_, _, command_sender| async move {
            drop(command_sender);
            Ok(UiOutcome { abort_scan: false })
        })
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn launch_default_ui_with_runs_driver_and_restore_hooks() {
        let (_event_sender, event_receiver) = unbounded_channel();
        let (command_sender, _command_receiver) = unbounded_channel();
        let ran_driver = Arc::new(AtomicBool::new(false));
        let restored_terminal = Arc::new(AtomicBool::new(false));
        let ran_driver_in_task = ran_driver.clone();
        let restored_in_task = restored_terminal.clone();

        let outcome = launch_default_ui_with::<TestBackend, _, _, _>(
            runtime(None),
            event_receiver,
            command_sender,
            || Terminal::new(TestBackend::new(80, 24)).expect("test terminal"),
            move || restored_in_task.store(true, Ordering::SeqCst),
            move |_terminal, _app, _receiver, _command_sender| {
                ran_driver_in_task.store(true, Ordering::SeqCst);
                Ok(UiOutcome { abort_scan: false })
            },
        )
        .await
        .expect("test driver should succeed");

        assert_eq!(outcome, UiOutcome { abort_scan: false });
        assert!(ran_driver.load(Ordering::SeqCst));
        assert!(restored_terminal.load(Ordering::SeqCst));
    }

    fn launch_override(
        _runtime: RuntimeOptions,
        _receiver: UnboundedReceiver<RunEvent>,
        command_sender: UnboundedSender<String>,
    ) -> TestUiLaunchFuture {
        Box::pin(async move {
            drop(command_sender);
            Ok(UiOutcome { abort_scan: false })
        })
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_uses_test_launch_override() {
        set_test_ui_launch_override(launch_override);

        let result = run(runtime(None)).await;

        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn scan_loop_covers_initial_url_and_queued_commands() {
        let (sender, _receiver) = unbounded_channel();
        let (_unused_sender, command_receiver) = unbounded_channel();
        let result = scan_loop(
            Some("not-a-url".to_string()),
            runtime(None),
            sender,
            command_receiver,
        )
        .await;
        assert!(result.is_err());

        let (sender, _receiver) = unbounded_channel();
        let (command_sender, command_receiver) = unbounded_channel();
        command_sender.send("not-a-url".to_string()).unwrap();
        drop(command_sender);
        let result = scan_loop(None, runtime(None), sender, command_receiver).await;
        assert!(result.is_err());

        let (sender, _receiver) = unbounded_channel();
        let (command_sender, command_receiver) = unbounded_channel();
        drop(command_sender);
        let result = scan_loop(None, runtime(None), sender, command_receiver).await;
        assert!(result.is_ok());
    }

    #[test]
    fn run_app_with_events_handles_enter_then_quit_and_sends_scan_command() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(runtime(None));
        app.url_input = "https://example.com".to_string();
        let (_event_sender, event_receiver) = unbounded_channel();
        let (command_sender, mut command_receiver) = unbounded_channel();

        let mut poll_count = 0usize;
        let mut read_count = 0usize;
        let outcome = run_app_with_events(
            &mut terminal,
            app,
            event_receiver,
            command_sender,
            |_| {
                poll_count += 1;
                Ok(true)
            },
            || {
                read_count += 1;
                Ok(match read_count {
                    1 => Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                    _ => Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
                })
            },
        )
        .expect("app should quit cleanly");

        assert_eq!(poll_count, 2);
        assert_eq!(outcome, UiOutcome { abort_scan: true });
        assert_eq!(
            command_receiver.try_recv().ok(),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn run_app_with_events_handles_resize_and_non_key_events() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(runtime(Some("https://example.com")));
        app.mode = UiMode::Normal;
        let (_event_sender, event_receiver) = unbounded_channel();
        let (command_sender, _command_receiver) = unbounded_channel();
        let mut read_count = 0usize;

        let outcome = run_app_with_events(
            &mut terminal,
            app,
            event_receiver,
            command_sender,
            |_| Ok(true),
            || {
                read_count += 1;
                match read_count {
                    1 => Ok(Event::Resize(100, 40)),
                    2 => Ok(Event::FocusGained),
                    _ => Err(std::io::Error::other("scripted reader exhausted")),
                }
            },
        )
        .expect_err(
            "non-key events should keep the loop running until the scripted reader is exhausted",
        );

        assert!(outcome.to_string().contains("scripted reader"));
    }

    #[test]
    fn run_app_with_events_applies_buffered_events_before_quit() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(runtime(Some("https://example.com")));
        app.should_quit = true;
        let (event_sender, event_receiver) = unbounded_channel();
        event_sender
            .send(RunEvent::Error("boom".to_string()))
            .unwrap();
        let (command_sender, _command_receiver) = unbounded_channel();

        let outcome = run_app_with_events(
            &mut terminal,
            app,
            event_receiver,
            command_sender,
            |_| Ok(false),
            || Err(std::io::Error::other("unused")),
        )
        .expect("app should quit after processing buffered event");

        assert_eq!(outcome, UiOutcome { abort_scan: false });
    }
}
