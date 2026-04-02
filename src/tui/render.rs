use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Cell, Paragraph, Row, Table, Wrap},
};
use std::time::Duration;

use super::app::{App, UiMode};
use crate::models::{IssueSeverity, PageInfo, SeoIssue};
use crate::update::format_tui_update_message;

pub fn render(frame: &mut Frame, app: &App) {
    let [header_area, metrics_area, main_area, footer_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(3),
    ])
    .areas(frame.area());

    render_header(frame, app, header_area);
    render_metrics(frame, app, metrics_area);
    render_main(frame, app, main_area);
    render_footer(frame, app, footer_area);
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let title = Line::from(vec![
        Span::styled(
            " Scoutly ",
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::raw("  default TUI · classic CLI via --cli"),
    ]);
    let subtitle = Line::from(vec![
        Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
        status_span(app),
        Span::raw(format!(
            "   URL: {}   depth={}   max_pages={}",
            app.url.as_deref().unwrap_or("(enter in TUI)"),
            app.depth,
            app.max_pages
        )),
    ]);

    let header = Paragraph::new(Text::from(vec![title, subtitle]))
        .block(Block::bordered().title("Run"))
        .wrap(Wrap { trim: true });
    frame.render_widget(header, area);
}

fn render_metrics(frame: &mut Frame, app: &App, area: Rect) {
    let summary = &app.progress.summary;
    let metrics = Line::from(vec![
        metric_span("Pages", app.progress.pages_crawled.to_string(), Color::Cyan),
        Span::raw("  "),
        metric_span(
            "Links",
            format!(
                "{}/{}",
                app.progress.links_checked,
                app.progress.total_links.max(app.progress.links_discovered)
            ),
            Color::Blue,
        ),
        Span::raw("  "),
        metric_span("Errors", summary.errors.to_string(), Color::Red),
        Span::raw("  "),
        metric_span("Warnings", summary.warnings.to_string(), Color::Yellow),
        Span::raw("  "),
        metric_span("Info", summary.infos.to_string(), Color::Green),
        Span::raw("  "),
        metric_span("Broken", summary.broken_links.to_string(), Color::Magenta),
    ]);

    let message = Paragraph::new(Text::from(vec![
        metrics,
        Line::raw(app.progress.message.clone()),
    ]))
    .block(Block::bordered().title("Live summary"))
    .wrap(Wrap { trim: true });
    frame.render_widget(message, area);
}

fn render_main(frame: &mut Frame, app: &App, area: Rect) {
    if matches!(app.mode, UiMode::UrlInput) {
        render_url_input(frame, app, area);
        return;
    }

    if app.has_active_scan() {
        render_scan_in_progress(frame, app, area);
        return;
    }

    let pages = app.visible_pages();

    if pages.is_empty() && app.report.is_none() {
        let waiting = Paragraph::new(Text::from(vec![
            Line::raw("No crawl report is loaded yet."),
            Line::raw("Press u to enter a URL and start a crawl."),
        ]))
        .block(Block::bordered().title("Ready to crawl"))
        .wrap(Wrap { trim: true });
        frame.render_widget(waiting, area);
        return;
    }

    if app.show_details {
        let [table_area, detail_area] =
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
                .areas(area);
        render_pages_table(frame, app, &pages, table_area);
        render_detail_pane(frame, app, &pages, detail_area);
    } else {
        render_pages_table(frame, app, &pages, area);
    }
}

fn render_scan_in_progress(frame: &mut Frame, app: &App, area: Rect) {
    let elapsed = app.elapsed_scan_time().unwrap_or_default();
    let spinner = spinner_frame(elapsed);
    let summary = &app.progress.summary;
    let progress = Text::from(vec![
        Line::from(vec![Span::styled(
            format!("{spinner} {}", stage_label(app)),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::raw(""),
        Line::raw(app.progress.message.clone()),
        Line::raw(""),
        Line::raw(format!(
            "Elapsed: {}   Pages: {}   Links: {}/{}",
            format_duration(elapsed),
            app.progress.pages_crawled,
            app.progress.links_checked,
            app.progress.total_links.max(app.progress.links_discovered)
        )),
        Line::raw(format!(
            "Issues so far: {} errors, {} warnings, {} info, {} broken links",
            summary.errors, summary.warnings, summary.infos, summary.broken_links
        )),
        Line::raw(""),
        Line::raw("Scoutly is actively scanning the site."),
        Line::raw("Results will appear automatically as soon as the report is ready."),
    ]);

    let loading = Paragraph::new(progress)
        .block(Block::bordered().title("Crawl in progress"))
        .wrap(Wrap { trim: true });
    frame.render_widget(loading, area);
}

fn render_url_input(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![
        Line::raw("Enter the starting URL for your crawl, then press Enter."),
        Line::raw(""),
        Line::from(vec![
            Span::styled("URL: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(if app.url_input.is_empty() {
                "https://example.com".to_string()
            } else {
                app.url_input.clone()
            }),
        ]),
    ];

    if let Some(error) = &app.error {
        lines.push(Line::raw(""));
        lines.push(Line::styled(error.clone(), Style::default().fg(Color::Red)));
    }

    if app.report.is_some() {
        lines.push(Line::raw(""));
        lines.push(Line::raw(
            "Press Esc to return to the current report without starting a new crawl.",
        ));
    }

    let input = Paragraph::new(Text::from(lines))
        .block(Block::bordered().title("Start crawl"))
        .wrap(Wrap { trim: true });
    frame.render_widget(input, area);
}

fn render_pages_table(frame: &mut Frame, app: &App, pages: &[&PageInfo], area: Rect) {
    let rows = pages.iter().map(|page| {
        Row::new(vec![
            Cell::from(page.crawl_depth.to_string()),
            Cell::from(
                page.status_code
                    .map(|status| status.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
            Cell::from(issue_summary(page)),
            Cell::from(trimmed(page.title.as_deref().unwrap_or("(untitled)"), 24)),
            Cell::from(trimmed(&page.url, 48)),
        ])
    });

    let widths = [
        Constraint::Length(5),
        Constraint::Length(7),
        Constraint::Length(12),
        Constraint::Length(26),
        Constraint::Min(20),
    ];

    let table = Table::new(rows, widths)
        .header(
            Row::new(["Depth", "Status", "Issues", "Title", "URL"]).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ")
        .block(Block::bordered().title(format!(
            "Pages · {} · filter={} · sort={}{}",
            pages.len(),
            app.severity_filter.label(),
            app.sort_mode.label(),
            if app.search_query.is_empty() {
                String::new()
            } else {
                format!(" · search=\"{}\"", app.search_query)
            }
        )));

    let mut state = ratatui::widgets::TableState::default().with_selected(Some(app.selected_index));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_detail_pane(frame: &mut Frame, app: &App, pages: &[&PageInfo], area: Rect) {
    let Some(page) = app.selected_page(pages) else {
        let empty = Paragraph::new("No page matches the current search/filter.")
            .block(Block::bordered().title("Details"));
        frame.render_widget(empty, area);
        return;
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("URL: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(page.url.clone()),
        ]),
        Line::from(vec![
            Span::styled("Title: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(
                page.title
                    .clone()
                    .unwrap_or_else(|| "(untitled)".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(
                page.status_code
                    .map(|status| status.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled("Depth: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(page.crawl_depth.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Issues: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(page.issues.len().to_string()),
        ]),
        Line::raw(""),
    ];

    if let Some(error) = &app.error {
        lines.push(Line::styled(error.clone(), Style::default().fg(Color::Red)));
        lines.push(Line::raw(""));
    }

    if page.issues.is_empty() {
        lines.push(Line::styled(
            "No issues on this page.",
            Style::default().fg(Color::Green),
        ));
    } else {
        lines.push(Line::styled(
            "Issues",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        for issue in ordered_issues(page) {
            lines.push(Line::from(vec![
                severity_span(issue.severity),
                Span::raw(" "),
                Span::raw(issue.message.clone()),
            ]));
        }
    }

    let details = Paragraph::new(Text::from(lines))
        .block(Block::bordered().title("Details"))
        .wrap(Wrap { trim: true });
    frame.render_widget(details, area);
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let mut help = if matches!(app.mode, UiMode::UrlInput) {
        "mode=URL · type a target URL · Enter start crawl · Ctrl-U clear · Esc quit/back"
            .to_string()
    } else if app.has_active_scan() {
        format!(
            "mode={} · crawl running · q quit · live progress updates above",
            app.mode.label()
        )
    } else if matches!(app.mode, UiMode::Search) {
        format!(
            "mode={} · search={} · type to filter · Enter accept · Esc close · Ctrl-U clear",
            app.mode.label(),
            app.search_input
        )
    } else {
        format!(
            "mode={} · j/k or ↑/↓ move · / search · f severity · s sort · u URL input · Enter details · q quit",
            app.mode.label()
        )
    };

    if let Some(notice) = &app.update_notice {
        help.push_str(" · ");
        help.push_str(&format_tui_update_message(notice));
    }

    let footer = Paragraph::new(help)
        .block(Block::bordered().title("Keys"))
        .wrap(Wrap { trim: true });
    frame.render_widget(footer, area);
}

fn metric_span(label: &str, value: String, color: Color) -> Span<'static> {
    Span::styled(
        format!("{label}: {value}"),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

fn status_span(app: &App) -> Span<'static> {
    let color = match app.status_label() {
        "COMPLETE" => Color::Green,
        "FAILED" => Color::Red,
        "CHECKING" => Color::Yellow,
        "ANALYZING" => Color::Magenta,
        _ => Color::Cyan,
    };

    Span::styled(
        app.status_label().to_string(),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

fn stage_label(app: &App) -> &'static str {
    match app.progress.stage {
        crate::runtime::RunStage::LoadingConfig => "Setting up crawl",
        crate::runtime::RunStage::Crawling => "Crawling pages",
        crate::runtime::RunStage::CheckingLinks => "Checking links",
        crate::runtime::RunStage::AnalyzingSeo => "Analyzing SEO",
        crate::runtime::RunStage::GeneratingReport => "Generating report",
        crate::runtime::RunStage::Completed => "Report ready",
        crate::runtime::RunStage::Failed => "Crawl failed",
    }
}

fn spinner_frame(elapsed: Duration) -> &'static str {
    const FRAMES: [&str; 4] = ["-", "\\", "|", "/"];
    let frame = ((elapsed.as_millis() / 120) as usize) % FRAMES.len();
    FRAMES[frame]
}

fn format_duration(elapsed: Duration) -> String {
    let total_seconds = elapsed.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn severity_span(severity: IssueSeverity) -> Span<'static> {
    let (label, color) = match severity {
        IssueSeverity::Error => ("[ERROR]", Color::Red),
        IssueSeverity::Warning => ("[WARN]", Color::Yellow),
        IssueSeverity::Info => ("[INFO]", Color::Green),
    };

    Span::styled(
        label.to_string(),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

fn ordered_issues(page: &PageInfo) -> Vec<&SeoIssue> {
    let mut issues = page.issues.iter().collect::<Vec<_>>();
    issues.sort_by_key(|issue| issue_severity_rank(issue.severity));
    issues
}

fn issue_severity_rank(severity: IssueSeverity) -> u8 {
    match severity {
        IssueSeverity::Error => 0,
        IssueSeverity::Warning => 1,
        IssueSeverity::Info => 2,
    }
}

fn issue_summary(page: &PageInfo) -> String {
    let errors = page
        .issues
        .iter()
        .filter(|issue| issue.severity == IssueSeverity::Error)
        .count();
    let warnings = page
        .issues
        .iter()
        .filter(|issue| issue.severity == IssueSeverity::Warning)
        .count();
    let infos = page
        .issues
        .iter()
        .filter(|issue| issue.severity == IssueSeverity::Info)
        .count();

    format!("E:{errors} W:{warnings} I:{infos}")
}

fn trimmed(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        return value.to_string();
    }

    let mut truncated = value
        .chars()
        .take(max_len.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CrawlReport, CrawlSummary, IssueType, OpenGraphTags, SeoIssue};
    use crate::runtime::ProgressSnapshot;
    use ratatui::{Terminal, backend::TestBackend};
    use std::collections::HashMap;

    fn sample_page() -> PageInfo {
        PageInfo {
            url: "https://example.com/about".to_string(),
            status_code: Some(200),
            content_type: Some("text/html".to_string()),
            title: Some("About".to_string()),
            meta_description: None,
            h1_tags: vec!["About".to_string()],
            links: vec![],
            images: vec![],
            open_graph: OpenGraphTags::default(),
            issues: vec![SeoIssue {
                severity: IssueSeverity::Warning,
                issue_type: IssueType::MissingMetaDescription,
                message: "Missing meta description".to_string(),
            }],
            crawl_depth: 1,
        }
    }

    #[test]
    fn ordered_issues_prioritize_errors_then_warnings_then_infos() {
        let mut page = sample_page();
        page.issues = vec![
            SeoIssue {
                severity: IssueSeverity::Info,
                issue_type: IssueType::Redirect,
                message: "redirected".to_string(),
            },
            SeoIssue {
                severity: IssueSeverity::Warning,
                issue_type: IssueType::MissingMetaDescription,
                message: "missing description".to_string(),
            },
            SeoIssue {
                severity: IssueSeverity::Error,
                issue_type: IssueType::BrokenLink,
                message: "broken link".to_string(),
            },
            SeoIssue {
                severity: IssueSeverity::Info,
                issue_type: IssueType::Redirect,
                message: "another redirect".to_string(),
            },
        ];

        let severities = ordered_issues(&page)
            .into_iter()
            .map(|issue| issue.severity)
            .collect::<Vec<_>>();

        assert_eq!(
            severities,
            vec![
                IssueSeverity::Error,
                IssueSeverity::Warning,
                IssueSeverity::Info,
                IssueSeverity::Info,
            ]
        );
    }

    #[test]
    fn render_outputs_core_labels_to_test_backend() {
        let runtime = crate::config::RuntimeOptions {
            url: Some("https://example.com".to_string()),
            depth: 2,
            max_pages: 10,
            output: None,
            save: None,
            cli: false,
            external: false,
            verbose: false,
            ignore_redirects: false,
            keep_fragments: false,
            rate_limit: None,
            concurrency: 5,
            respect_robots_txt: true,
            tui: false,
            config: None,
        };
        let mut app = App::new(runtime);
        let page = sample_page();
        let mut pages = HashMap::new();
        pages.insert(page.url.clone(), page);
        app.progress = ProgressSnapshot::new(crate::runtime::RunStage::Completed, "Report ready");
        app.report = Some(CrawlReport {
            start_url: "https://example.com".to_string(),
            pages,
            summary: CrawlSummary {
                total_pages: 1,
                total_links: 2,
                broken_links: 0,
                errors: 0,
                warnings: 1,
                infos: 0,
            },
            timestamp: "2026-04-02T00:00:00Z".to_string(),
        });
        app.scan_in_progress = false;
        app.scan_started_at = None;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let content = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(content.contains("Scoutly"));
        assert!(content.contains("Pages"));
        assert!(content.contains("Details"));
        assert!(content.contains("About"));
    }

    #[test]
    fn render_shows_explicit_loading_state_while_scan_is_running() {
        let runtime = crate::config::RuntimeOptions {
            url: Some("https://example.com".to_string()),
            depth: 2,
            max_pages: 10,
            output: None,
            save: None,
            cli: false,
            external: false,
            verbose: false,
            ignore_redirects: false,
            keep_fragments: false,
            rate_limit: None,
            concurrency: 5,
            respect_robots_txt: true,
            tui: false,
            config: None,
        };
        let mut app = App::new(runtime);
        app.progress = ProgressSnapshot::new(
            crate::runtime::RunStage::Crawling,
            "Crawling https://example.com".to_string(),
        );
        app.progress.pages_crawled = 3;
        app.progress.links_discovered = 12;
        app.progress.total_links = 12;
        app.scan_in_progress = true;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let content = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(content.contains("Crawl in progress"));
        assert!(content.contains("Crawling pages"));
        assert!(content.contains("Scoutly is actively scanning the site."));
        assert!(content.contains("Elapsed:"));
    }

    #[test]
    fn render_footer_shows_update_notice_when_available() {
        let runtime = crate::config::RuntimeOptions {
            url: None,
            depth: 2,
            max_pages: 10,
            output: None,
            save: None,
            cli: false,
            external: false,
            verbose: false,
            ignore_redirects: false,
            keep_fragments: false,
            rate_limit: None,
            concurrency: 5,
            respect_robots_txt: true,
            tui: false,
            config: None,
        };
        let mut app = App::new(runtime);
        app.update_notice = Some(crate::update::UpdateNotice {
            latest_version: "0.4.0".to_string(),
            release_url: "https://github.com/nelsonlaidev/scoutly/releases/tag/v0.4.0".to_string(),
        });

        let backend = TestBackend::new(140, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let content = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(content.contains("update v0.4.0 available"));
    }
}
