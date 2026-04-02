pub mod app;
pub mod render;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::LocalSet;

use crate::config::RuntimeOptions;
use crate::execute_scan;
use crate::runtime::RunEvent;

use self::app::{App, AppAction};

pub async fn run(runtime: RuntimeOptions) -> Result<()> {
    LocalSet::new()
        .run_until(async move {
            let (event_sender, event_receiver) = unbounded_channel();
            let (command_sender, mut command_receiver) = unbounded_channel::<String>();
            let scan_template = runtime.clone();
            let initial_url = scan_template.url.clone();
            let event_sender_for_scans = event_sender.clone();

            let scan_handle = tokio::task::spawn_local(async move {
                if let Some(url) = initial_url {
                    run_scan(url, &scan_template, &event_sender_for_scans).await?;
                }

                while let Some(url) = command_receiver.recv().await {
                    run_scan(url, &scan_template, &event_sender_for_scans).await?;
                }

                Ok::<(), anyhow::Error>(())
            });

            let ui_outcome = tokio::task::spawn_blocking(move || {
                run_tui(runtime, event_receiver, command_sender)
            })
            .await??;

            if ui_outcome.abort_scan {
                scan_handle.abort();
                return Ok(());
            }

            match scan_handle.await {
                Ok(result) => result,
                Err(error) if error.is_cancelled() => Ok(()),
                Err(error) => Err(error.into()),
            }
        })
        .await
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

struct UiOutcome {
    abort_scan: bool,
}

fn run_tui(
    runtime: RuntimeOptions,
    receiver: UnboundedReceiver<RunEvent>,
    command_sender: UnboundedSender<String>,
) -> Result<UiOutcome> {
    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, App::new(runtime), receiver, command_sender);
    ratatui::restore();
    result
}

fn run_app(
    terminal: &mut DefaultTerminal,
    mut app: App,
    mut receiver: UnboundedReceiver<RunEvent>,
    command_sender: UnboundedSender<String>,
) -> Result<UiOutcome> {
    loop {
        while let Ok(event) = receiver.try_recv() {
            app.apply_run_event(event);
        }

        terminal.draw(|frame| render::render(frame, &app))?;

        if app.should_quit {
            return Ok(UiOutcome {
                abort_scan: app.has_active_scan(),
            });
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if let Some(AppAction::StartScan(url)) = app.handle_key(key) {
                        let _ = command_sender.send(url);
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
}
