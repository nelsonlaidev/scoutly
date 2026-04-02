use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Cell, Paragraph, Row, Table, Wrap},
};

use super::app::{App, UiMode};
use crate::models::{IssueSeverity, PageInfo};

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

    let pages = app.visible_pages();

    if pages.is_empty() && app.report.is_none() {
        let waiting = Paragraph::new(Text::from(vec![
            Line::raw("The crawl is running. Live counts update above."),
            Line::raw("Once the report is ready, pages and issue details will appear here."),
            Line::raw("Press q to quit at any time."),
        ]))
        .block(Block::bordered().title("Waiting for report"))
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
        for issue in &page.issues {
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
    let help = if matches!(app.mode, UiMode::UrlInput) {
        "mode=URL · type a target URL · Enter start crawl · Ctrl-U clear · Esc quit/back"
            .to_string()
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
}
