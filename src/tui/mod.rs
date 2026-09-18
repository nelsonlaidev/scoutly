mod results;
mod running;
mod setup;

use std::future::Future;
use std::io::{self, Stdout, stdout};
use std::pin::Pin;
use std::time::{Duration, Instant};

use crossterm::cursor::Show;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{
    Modifier, Style,
    palette::tailwind::{AMBER, GREEN, NEUTRAL, RED},
};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use scoutly::{AuditError, Options, Progress, Report, ValidationError, audit};
use thiserror::Error;
use tokio::sync::mpsc;
use tui_scrollbar::{GlyphSet, ScrollBar, ScrollBarInteraction, ScrollCommand, ScrollLengths};

use self::results::{ResultsAction, ResultsState};
use self::running::RunningState;
use self::setup::{SetupAction, SetupState};

const DEFAULT_WIDTH: u16 = 80;
const DEFAULT_HEIGHT: u16 = 24;
const MINIMUM_WIDTH: u16 = 70;
const MINIMUM_HEIGHT: u16 = 20;
const HEADER_ROWS: u16 = 3;
const FOOTER_ROWS: u16 = 1;
const EVENT_TICK: Duration = Duration::from_millis(50);
const RUNNING_FRAME_TICK: Duration = Duration::from_millis(250);
const PROGRESS_CHANNEL_CAPACITY: usize = 64;
pub(super) const MOUSE_WHEEL_DELTA: usize = 3;

type AuditFuture = Pin<Box<dyn Future<Output = Result<Report, AuditError>>>>;

struct ActiveAudit {
    future: AuditFuture,
    progress: Option<mpsc::Receiver<Progress>>,
}

enum LoopEvent {
    Tick,
    Progress(Option<Progress>),
    Finished(Result<Report, AuditError>),
    Interrupted(Result<(), io::Error>),
}

#[derive(Debug, Error)]
pub(crate) enum TuiError {
    #[error("validate TUI options: {0}")]
    InvalidOptions(#[from] ValidationError),

    #[error("{operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },

    #[error("audit canceled")]
    Cancelled,
}

impl TuiError {
    fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Screen {
    Setup,
    Running,
    Results,
    Canceled,
    Failed,
}

impl Screen {
    const fn label(self) -> &'static str {
        match self {
            Self::Setup => "Setup",
            Self::Running => "Running",
            Self::Results => "Results",
            Self::Canceled => "Canceled",
            Self::Failed => "Failed",
        }
    }
}

#[derive(Debug)]
enum AppAction {
    None,
    StartAudit {
        target: String,
        options: Box<Options>,
    },
    CancelAudit,
    Quit,
}

#[derive(Debug)]
struct AppAreas {
    header: Rect,
    body: Rect,
    footer: Rect,
}

#[derive(Debug)]
struct App {
    area: Rect,
    screen: Screen,
    setup: SetupState,
    running: Option<RunningState>,
    results: Option<ResultsState>,
    report: Option<Report>,
    failure_message: String,
}

impl App {
    fn new(options: Options) -> Self {
        Self {
            area: Rect::new(0, 0, DEFAULT_WIDTH, DEFAULT_HEIGHT),
            screen: Screen::Setup,
            setup: SetupState::new(options),
            running: None,
            results: None,
            report: None,
            failure_message: String::new(),
        }
    }

    fn handle_event(&mut self, event: Event) -> AppAction {
        if let Event::Resize(width, height) = event {
            self.area.width = width.max(1);
            self.area.height = height.max(1);
            return AppAction::None;
        }

        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Release {
                return AppAction::None;
            }

            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                if self.screen == Screen::Running {
                    if self
                        .running
                        .as_mut()
                        .is_some_and(RunningState::begin_cancel)
                    {
                        return AppAction::CancelAudit;
                    }
                    return AppAction::None;
                }
                return AppAction::Quit;
            }

            if self.too_small() {
                return AppAction::None;
            }

            return self.handle_key(key);
        }

        if let Event::Mouse(mouse) = event {
            if self.too_small() {
                return AppAction::None;
            }

            let body = self.areas().body;

            match self.screen {
                Screen::Setup => match self.setup.handle_mouse(mouse, body) {
                    SetupAction::None => {}
                    SetupAction::Start { target, options } => {
                        return self.start_audit(target, options);
                    }
                },
                Screen::Running => {
                    if let Some(running) = &mut self.running {
                        running.handle_mouse(mouse, body);
                    }
                }
                Screen::Results => {
                    if let (Some(results), Some(report)) = (&mut self.results, &self.report) {
                        results.handle_mouse(mouse, report, body);
                    }
                }
                Screen::Canceled | Screen::Failed => {}
            }
        }

        AppAction::None
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> AppAction {
        let body = self.areas().body;
        match self.screen {
            Screen::Setup => match self.setup.handle_key(key, body.height) {
                SetupAction::None => AppAction::None,
                SetupAction::Start { target, options } => self.start_audit(target, options),
            },
            Screen::Running => AppAction::None,
            Screen::Results => {
                let (Some(results), Some(report)) = (&mut self.results, &self.report) else {
                    return AppAction::None;
                };

                match results.handle_key(key, report, body) {
                    ResultsAction::None => AppAction::None,
                    ResultsAction::Quit => AppAction::Quit,
                    ResultsAction::NewAudit => {
                        self.new_audit();
                        AppAction::None
                    }
                }
            }
            Screen::Canceled | Screen::Failed => match key.code {
                KeyCode::Enter => {
                    self.new_audit();
                    AppAction::None
                }
                KeyCode::Char('q') => AppAction::Quit,
                _ => AppAction::None,
            },
        }
    }

    fn start_audit(&mut self, target: String, options: Box<Options>) -> AppAction {
        self.screen = Screen::Running;
        self.report = None;
        self.results = None;
        self.failure_message.clear();
        self.running = Some(RunningState::new(target.clone()));

        AppAction::StartAudit { target, options }
    }

    fn receive_progress(&mut self, progress: Progress) {
        if self.screen == Screen::Running
            && let Some(running) = &mut self.running
        {
            running.receive_progress(progress);
        }
    }

    fn finish_audit(&mut self, result: Result<Report, AuditError>) {
        self.running = None;

        match result {
            Ok(report) => {
                self.results = Some(ResultsState::new(&report));
                self.report = Some(report);
                self.screen = Screen::Results;
                self.failure_message.clear();
            }
            Err(error) => {
                self.report = None;
                self.results = None;
                self.screen = Screen::Failed;
                self.failure_message = error.to_string();
            }
        }
    }

    fn cancel_audit(&mut self) {
        self.running = None;
        self.results = None;
        self.report = None;

        self.screen = Screen::Canceled;
        self.failure_message.clear();
    }

    fn new_audit(&mut self) {
        self.screen = Screen::Setup;
        self.running = None;
        self.results = None;
        self.report = None;
        self.failure_message.clear();

        self.setup.reset();
    }

    fn tick(&mut self) {
        if let Some(running) = &mut self.running {
            running.tick();
        }
    }

    fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        self.area = Rect::new(area.x, area.y, area.width.max(1), area.height.max(1));
        let areas = self.areas();

        self.render_header(frame, areas.header);
        self.render_footer(frame, areas.footer);

        let body = areas.body;

        if self.too_small() {
            render_centered_status(
                frame,
                body,
                "Terminal is too small",
                &format!(
                    "Minimum size: {MINIMUM_WIDTH} x {MINIMUM_HEIGHT}\nCurrent size: {} x {}",
                    self.area.width, self.area.height
                ),
                Style::new().fg(AMBER.c400).bold(),
            );
            return;
        }

        match self.screen {
            Screen::Setup => self.setup.render(frame, body),
            Screen::Running => {
                if let Some(running) = &mut self.running {
                    running.render(frame, body);
                }
            }
            Screen::Results => {
                if let (Some(results), Some(report)) = (&mut self.results, &self.report) {
                    results.render(frame, body, report);
                }
            }
            Screen::Canceled => render_centered_status(
                frame,
                body,
                "Audit canceled",
                "No partial report was saved.",
                Style::new().fg(AMBER.c400).bold(),
            ),
            Screen::Failed => render_centered_status(
                frame,
                body,
                "Audit failed",
                &self.failure_message,
                Style::new().fg(RED.c400).bold(),
            ),
        }
    }

    fn render_header(&self, frame: &mut Frame<'_>, header: Rect) {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(NEUTRAL.c700)
            .padding(Padding::horizontal(1));
        let inner = block.inner(header);

        frame.render_widget(block, header);

        if inner.is_empty() {
            return;
        }

        let status_style = match self.screen {
            Screen::Setup => Style::new().fg(NEUTRAL.c500),
            Screen::Running => Style::new().fg(NEUTRAL.c50).bold(),
            Screen::Results => Style::new().fg(GREEN.c400).bold(),
            Screen::Canceled => Style::new().fg(AMBER.c400).bold(),
            Screen::Failed => Style::new().fg(RED.c400).bold(),
        };
        let status = self.screen.label();

        let [brand_area, target_area, status_area] = Layout::horizontal([
            Constraint::Length("Scoutly".len() as u16),
            Constraint::Min(0),
            Constraint::Length(status.len() as u16),
        ])
        .areas(inner);

        frame.render_widget(
            Paragraph::new(Span::styled("Scoutly", (NEUTRAL.c50, Modifier::BOLD))),
            brand_area,
        );
        frame.render_widget(
            Paragraph::new(self.target())
                .style(NEUTRAL.c500)
                .alignment(Alignment::Center),
            target_area,
        );
        frame.render_widget(
            Paragraph::new(Span::styled(status, status_style)).alignment(Alignment::Right),
            status_area,
        );
    }

    fn render_footer(&self, frame: &mut Frame<'_>, footer: Rect) {
        if footer.is_empty() {
            return;
        }

        let help = if self.too_small() {
            "Ctrl+C quit"
        } else {
            match self.screen {
                Screen::Setup => "Tab/Enter move   Shift+Tab back   Space toggle   Ctrl+C quit",
                Screen::Running => "Ctrl+C cancel audit",
                Screen::Results => {
                    "1-5 tabs   / search   f filter   Tab pane   Arrows move   n new   q quit"
                }
                Screen::Canceled | Screen::Failed => "Enter edit and retry   q quit",
            }
        };
        frame.render_widget(Paragraph::new(help).style(NEUTRAL.c500), footer);
    }

    fn target(&self) -> &str {
        match self.screen {
            Screen::Running => self
                .running
                .as_ref()
                .map_or("Website audit", |running| running.target.as_str()),
            Screen::Results => self
                .report
                .as_ref()
                .map_or("Website audit", |report| report.url.as_str()),
            Screen::Setup | Screen::Canceled | Screen::Failed => {
                if self.setup.target().is_empty() {
                    "Website audit"
                } else {
                    self.setup.target()
                }
            }
        }
    }

    fn areas(&self) -> AppAreas {
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(HEADER_ROWS),
            Constraint::Fill(1),
            Constraint::Length(FOOTER_ROWS),
        ])
        .areas(self.area);

        AppAreas {
            header,
            body,
            footer,
        }
    }

    const fn too_small(&self) -> bool {
        self.area.width < MINIMUM_WIDTH || self.area.height < MINIMUM_HEIGHT
    }
}

pub(crate) async fn run(options: Options) -> Result<(), TuiError> {
    options.validate()?;

    let mut session = TerminalSession::enter()
        .map_err(|source| TuiError::io("initialize TUI terminal", source))?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal =
        Terminal::new(backend).map_err(|source| TuiError::io("create TUI terminal", source))?;
    terminal
        .clear()
        .map_err(|source| TuiError::io("clear TUI terminal", source))?;

    let result = run_event_loop(&mut terminal, options).await;
    drop(terminal);
    let restore_result = session.restore();
    result?;

    restore_result.map_err(|source| TuiError::io("restore TUI terminal", source))
}

async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    options: Options,
) -> Result<(), TuiError> {
    let mut app = App::new(options);
    let mut active_audit: Option<ActiveAudit> = None;
    let mut ticker = tokio::time::interval(EVENT_TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);

    let mut redraw = true;
    let mut last_running_frame = Instant::now();

    loop {
        if redraw {
            terminal
                .draw(|frame| app.render(frame))
                .map_err(|source| TuiError::io("draw TUI frame", source))?;
            redraw = false;
        }

        let loop_event = match &mut active_audit {
            Some(active) => {
                let progress = &mut active.progress;
                tokio::select! {
                    result = active.future.as_mut() => LoopEvent::Finished(result),
                    snapshot = async {
                        progress
                            .as_mut()
                            .expect("guarded progress receiver exists")
                            .recv()
                            .await
                    }, if progress.is_some() => LoopEvent::Progress(snapshot),
                    _ = ticker.tick() => LoopEvent::Tick,
                    signal = &mut interrupt => LoopEvent::Interrupted(signal),
                }
            }
            None => {
                tokio::select! {
                    _ = ticker.tick() => LoopEvent::Tick,
                    signal = &mut interrupt => LoopEvent::Interrupted(signal),
                }
            }
        };

        match loop_event {
            LoopEvent::Tick => {
                if app.screen == Screen::Running
                    && last_running_frame.elapsed() >= RUNNING_FRAME_TICK
                {
                    app.tick();
                    last_running_frame = Instant::now();
                    redraw = true;
                }
            }
            LoopEvent::Progress(Some(progress)) => {
                app.receive_progress(progress);
                redraw = true;
            }
            LoopEvent::Progress(None) => {
                if let Some(active) = &mut active_audit {
                    active.progress = None;
                }
            }
            LoopEvent::Finished(result) => {
                active_audit = None;
                app.finish_audit(result);
                redraw = true;
            }
            LoopEvent::Interrupted(Ok(())) => return Err(TuiError::Cancelled),
            LoopEvent::Interrupted(Err(source)) => {
                return Err(TuiError::io("listen for TUI interrupt", source));
            }
        }

        while event::poll(Duration::ZERO)
            .map_err(|source| TuiError::io("poll TUI input", source))?
        {
            let input = event::read().map_err(|source| TuiError::io("read TUI input", source))?;
            redraw = true;

            match app.handle_event(input) {
                AppAction::None => {}
                AppAction::Quit => return Ok(()),
                AppAction::CancelAudit => {
                    active_audit = None;
                    app.cancel_audit();
                }
                AppAction::StartAudit { target, options } => {
                    let (sender, receiver) = mpsc::channel(PROGRESS_CHANNEL_CAPACITY);
                    active_audit = Some(ActiveAudit {
                        future: Box::pin(
                            async move { audit(&target, *options, Some(sender)).await },
                        ),
                        progress: Some(receiver),
                    });
                }
            }
        }
    }
}

struct TerminalSession {
    raw_mode: bool,
    alternate_screen: bool,
    mouse_capture: bool,
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        let mut session = Self {
            raw_mode: false,
            alternate_screen: false,
            mouse_capture: false,
        };

        enable_raw_mode()?;
        session.raw_mode = true;
        execute!(stdout(), EnterAlternateScreen)?;
        session.alternate_screen = true;
        execute!(stdout(), EnableMouseCapture)?;
        session.mouse_capture = true;

        Ok(session)
    }

    fn restore(&mut self) -> io::Result<()> {
        let mut first_error = None;

        if self.mouse_capture {
            match execute!(stdout(), DisableMouseCapture) {
                Ok(()) => self.mouse_capture = false,
                Err(error) => first_error = Some(error),
            }
        }

        if self.alternate_screen {
            match execute!(stdout(), LeaveAlternateScreen, Show) {
                Ok(()) => self.alternate_screen = false,
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }

        if self.raw_mode {
            match disable_raw_mode() {
                Ok(()) => self.raw_mode = false,
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }

        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn render_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    focused: bool,
    render_content: impl FnOnce(&mut Frame<'_>, Rect),
) {
    if area.is_empty() {
        return;
    }

    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if focused {
            Style::new().fg(NEUTRAL.c50).bold()
        } else {
            Style::new().fg(NEUTRAL.c700)
        })
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            title.to_owned(),
            if focused {
                Style::new().fg(NEUTRAL.c50).bold()
            } else {
                Style::new().fg(NEUTRAL.c500)
            },
        )));
    let inner = pane_inner(area);

    frame.render_widget(block, area);
    render_content(frame, inner);
}

fn pane_inner(area: Rect) -> Rect {
    Block::new()
        .borders(Borders::ALL)
        .padding(Padding::horizontal(1))
        .inner(area)
}

fn scrollbar_area(area: Rect) -> Rect {
    Rect::new(
        area.right().saturating_sub(1),
        area.y.saturating_add(1),
        u16::from(area.width > 0),
        area.height.saturating_sub(2),
    )
}

fn scrollbar(
    content_length: usize,
    visible_length: usize,
    position: usize,
    focused: bool,
) -> ScrollBar {
    ScrollBar::vertical(ScrollLengths {
        content_len: content_length,
        viewport_len: visible_length,
    })
    .offset(position)
    .glyph_set(GlyphSet::box_drawing())
    .track_style(Style::new().fg(NEUTRAL.c700))
    .thumb_style(if focused {
        Style::new().fg(NEUTRAL.c50).bold()
    } else {
        Style::new().fg(NEUTRAL.c500)
    })
}

fn handle_scrollbar_mouse(
    mouse: MouseEvent,
    area: Rect,
    content_length: usize,
    visible_length: usize,
    position: &mut usize,
    interaction: &mut ScrollBarInteraction,
) -> bool {
    let scrollbar_area = scrollbar_area(area);

    if scrollbar_area.is_empty() {
        return false;
    }

    let handled = match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            content_length > visible_length
                && scrollbar_area.contains((mouse.column, mouse.row).into())
        }
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => true,
        _ => false,
    };

    if !handled {
        return false;
    }

    if let Some(ScrollCommand::SetOffset(next)) =
        scrollbar(content_length, visible_length, *position, false).handle_mouse_event(
            scrollbar_area,
            mouse,
            interaction,
        )
    {
        *position = next;
    }

    true
}

fn render_scrollbar(
    frame: &mut Frame<'_>,
    area: Rect,
    content_length: usize,
    visible_length: usize,
    position: usize,
    focused: bool,
) {
    if content_length <= visible_length || area.height <= 2 || area.width == 0 {
        return;
    }

    frame.render_widget(
        &scrollbar(content_length, visible_length, position, focused),
        scrollbar_area(area),
    );
}

fn render_centered_status(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    message: &str,
    style: ratatui::style::Style,
) {
    let message_lines = message.lines().collect::<Vec<_>>();
    let width = usize::from(area.width).max(1);
    let message_line_count = message_lines
        .iter()
        .map(|line| Line::from(*line).width().max(1).div_ceil(width))
        .sum::<usize>();
    let line_count = u16::try_from(message_line_count.saturating_add(2)).unwrap_or(u16::MAX);
    let height = line_count.min(area.height).max(1);

    let centered = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(height) / 2,
        area.width,
        height,
    );
    let mut lines = vec![
        Line::from(Span::styled(title.to_owned(), style)),
        Line::from(""),
    ];

    lines.extend(
        message_lines
            .into_iter()
            .map(|line| Line::from(line.to_owned())),
    );

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        centered,
    );
}

pub(super) fn text_byte_index(value: &str, character: usize) -> usize {
    value
        .char_indices()
        .nth(character)
        .map_or(value.len(), |(index, _)| index)
}

pub(super) fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

#[cfg(test)]
pub(super) mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use scoutly::{
        Headings, Image, ImageResult, ImageSummary, Issue, IssueCode, IssueSummary, IssueTarget,
        Link, LinkResult, LinkSummary, OpenGraph, Page, Report, ResultKind, Severity, Summary,
        TargetType,
    };

    use super::{App, AppAction, MINIMUM_HEIGHT, Screen};
    use crate::tui::results::ResultsState;
    use crate::tui::running::RunningState;

    pub(super) fn sample_report() -> Report {
        let page_url = "https://example.com/".to_owned();
        let broken_url = "https://example.com/broken".to_owned();
        let image_url = "https://example.com/image.png".to_owned();

        Report {
            url: page_url.clone(),
            audited_at: time::OffsetDateTime::UNIX_EPOCH,
            summary: Summary {
                pages: 1,
                links: LinkSummary {
                    total: 1,
                    checked: 1,
                    broken: 1,
                    ..LinkSummary::default()
                },
                images: ImageSummary {
                    total: 1,
                    checked: 1,
                    invalid: 1,
                    ..ImageSummary::default()
                },
                issues: IssueSummary {
                    total: 2,
                    error: 1,
                    warning: 1,
                    info: 0,
                },
            },
            issues: vec![
                Issue {
                    code: IssueCode::BrokenLink,
                    severity: Severity::Error,
                    message: "Link returned HTTP 404".into(),
                    target: IssueTarget {
                        target_type: TargetType::Link,
                        url: broken_url.clone(),
                    },
                },
                Issue {
                    code: IssueCode::InvalidImageContentType,
                    severity: Severity::Warning,
                    message: "Image has an invalid Content-Type".into(),
                    target: IssueTarget {
                        target_type: TargetType::Image,
                        url: image_url.clone(),
                    },
                },
            ],
            pages: vec![Page {
                url: page_url.clone(),
                depth: 0,
                status_code: Some(200),
                content_type: Some("text/html".into()),
                title: Some("Example".into()),
                description: Some("Example website".into()),
                headings: Headings {
                    h1: vec!["Example".into()],
                },
                images: Vec::new(),
                open_graph: OpenGraph::default(),
            }],
            links: vec![Link {
                url: broken_url,
                result: LinkResult {
                    kind: ResultKind::Response,
                    status_code: Some(404),
                    final_url: None,
                    reason: None,
                },
                found_on: vec![scoutly::LinkOccurrence {
                    page_url: page_url.clone(),
                    original_url: "/broken".into(),
                    element: "a".into(),
                    text: "Broken".into(),
                }],
            }],
            images: vec![Image {
                url: image_url,
                result: ImageResult {
                    kind: ResultKind::Response,
                    status_code: Some(200),
                    final_url: None,
                    content_type: Some("text/plain".into()),
                    reason: None,
                },
                found_on: vec![scoutly::ImageOccurrence {
                    page_url,
                    original_url: "/image.png".into(),
                    element: "img".into(),
                    attribute: "src".into(),
                    descriptor: None,
                    alt: None,
                }],
            }],
        }
    }

    pub(super) fn large_report(count: usize) -> Report {
        let mut report = sample_report();
        report.links = (0..count)
            .map(|index| Link {
                url: format!("https://example.com/link-{index:02}"),
                result: LinkResult {
                    kind: ResultKind::Response,
                    status_code: Some(200),
                    final_url: None,
                    reason: None,
                },
                found_on: Vec::new(),
            })
            .collect();
        report.refresh_summary();

        report
    }

    fn empty_report() -> Report {
        Report {
            url: "https://example.com/".into(),
            audited_at: time::OffsetDateTime::UNIX_EPOCH,
            summary: Summary::default(),
            issues: Vec::new(),
            pages: Vec::new(),
            links: Vec::new(),
            images: Vec::new(),
        }
    }

    fn snapshot(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.content().iter().all(|cell| cell.bg == Color::Reset));
        let mut output = String::new();
        for row in buffer.content().chunks(width as usize) {
            let line = row.iter().map(|cell| cell.symbol()).collect::<String>();
            output.push_str(line.trim_end());
            output.push('\n');
        }

        output
    }

    #[test]
    fn snapshot_setup_at_minimum_size() {
        let mut app = App::new(scoutly::Options::default());
        assert_snapshot!("setup_70x20", snapshot(&mut app, 70, 20));
    }

    #[test]
    fn snapshot_resize_warning_below_minimum() {
        let mut app = App::new(scoutly::Options::default());
        assert_snapshot!(
            "resize_warning_69x20",
            snapshot(&mut app, 69, MINIMUM_HEIGHT)
        );
    }

    #[test]
    fn snapshot_empty_results_narrow() {
        let report = empty_report();
        let mut app = App::new(scoutly::Options::default());
        app.screen = Screen::Results;
        app.results = Some(ResultsState::new(&report));
        app.report = Some(report);
        assert_snapshot!("results_empty_99x30", snapshot(&mut app, 99, 30));
    }

    #[test]
    fn snapshot_large_results_split() {
        let report = large_report(40);
        let mut results = ResultsState::new(&report);
        results.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('4'),
                crossterm::event::KeyModifiers::NONE,
            ),
            &report,
            ratatui::layout::Rect::new(0, 0, 98, 26),
        );
        let mut app = App::new(scoutly::Options::default());
        app.screen = Screen::Results;
        app.results = Some(results);
        app.report = Some(report);
        assert_snapshot!("results_large_100x30", snapshot(&mut app, 100, 30));
    }

    #[test]
    fn snapshot_running_screen() {
        let mut app = App::new(scoutly::Options::default());
        app.screen = Screen::Running;
        app.running = Some(RunningState::new("https://example.com/".into()));
        assert_snapshot!("running_70x20", snapshot(&mut app, 70, 20));
    }

    #[test]
    fn snapshot_canceled_and_failed_screens() {
        let mut canceled = App::new(scoutly::Options::default());
        canceled.screen = Screen::Canceled;
        assert_snapshot!("canceled_70x20", snapshot(&mut canceled, 70, 20));

        let mut failed = App::new(scoutly::Options::default());
        failed.screen = Screen::Failed;
        failed.failure_message = "connection failed".into();
        assert_snapshot!("failed_70x20", snapshot(&mut failed, 70, 20));
    }

    #[test]
    fn application_transitions_and_minimum_size_input_gate() {
        let mut app = App::new(scoutly::Options::default());

        app.area.width = 69;
        let ignored = app.handle_event(Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        assert!(matches!(ignored, AppAction::None));
        assert_eq!(app.screen, Screen::Setup);

        app.area.width = 70;
        app.area.height = 20;
        app.screen = Screen::Running;
        app.running = Some(RunningState::new("https://example.com".into()));
        let cancel = app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));
        assert!(matches!(cancel, AppAction::CancelAudit));
        app.cancel_audit();
        assert_eq!(app.screen, Screen::Canceled);

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert_eq!(app.screen, Screen::Setup);
    }

    #[test]
    fn root_areas_preserve_the_frame_origin() {
        let mut app = App::new(scoutly::Options::default());
        app.area = ratatui::layout::Rect::new(10, 5, 70, 20);

        let areas = app.areas();

        assert_eq!(areas.header, ratatui::layout::Rect::new(10, 5, 70, 3));
        assert_eq!(areas.body, ratatui::layout::Rect::new(10, 8, 70, 16));
        assert_eq!(areas.footer, ratatui::layout::Rect::new(10, 24, 70, 1));
    }

    #[test]
    fn setup_starts_results_and_failed_transitions_without_partial_reports() {
        let mut app = App::new(scoutly::Options::default());

        app.area.width = 70;
        app.area.height = 20;

        for character in "example.com".chars() {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(character),
                KeyModifiers::NONE,
            )));
        }

        for _ in 0..14 {
            app.handle_event(Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        }

        let start = app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(matches!(start, AppAction::StartAudit { .. }));
        assert_eq!(app.screen, Screen::Running);

        app.finish_audit(Ok(sample_report()));
        assert_eq!(app.screen, Screen::Results);
        assert!(app.report.is_some());

        app.new_audit();
        app.start_audit("invalid".into(), Box::new(scoutly::Options::default()));
        app.finish_audit(Err(scoutly::AuditError::InvalidTarget(
            scoutly::TargetUrlError::Invalid {
                input: "invalid".into(),
            },
        )));
        assert_eq!(app.screen, Screen::Failed);
        assert!(app.report.is_none());
        assert!(app.failure_message.contains("absolute HTTP"));
    }
}
