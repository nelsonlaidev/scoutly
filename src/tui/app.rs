use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

use crate::config::RuntimeOptions;
use crate::models::{CrawlReport, IssueSeverity, PageInfo};
use crate::runtime::{ProgressSnapshot, RunEvent, RunStage};
use crate::update::UpdateNotice;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    UrlInput,
    Normal,
    Search,
}

impl UiMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::UrlInput => "URL",
            Self::Normal => "NORMAL",
            Self::Search => "SEARCH",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppAction {
    StartScan(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeverityFilter {
    All,
    Error,
    Warning,
    Info,
}

impl SeverityFilter {
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All severities",
            Self::Error => "Errors only",
            Self::Warning => "Warnings only",
            Self::Info => "Infos only",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::All => Self::Error,
            Self::Error => Self::Warning,
            Self::Warning => Self::Info,
            Self::Info => Self::All,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Severity,
    Issues,
    Status,
    Depth,
    Url,
}

impl SortMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Severity => "Severity",
            Self::Issues => "Issue count",
            Self::Status => "HTTP status",
            Self::Depth => "Crawl depth",
            Self::Url => "URL",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Severity => Self::Issues,
            Self::Issues => Self::Status,
            Self::Status => Self::Depth,
            Self::Depth => Self::Url,
            Self::Url => Self::Severity,
        }
    }
}

pub struct App {
    pub url: Option<String>,
    pub url_input: String,
    pub depth: usize,
    pub max_pages: usize,
    pub mode: UiMode,
    pub progress: ProgressSnapshot,
    pub report: Option<CrawlReport>,
    pub search_query: String,
    pub search_input: String,
    pub severity_filter: SeverityFilter,
    pub sort_mode: SortMode,
    pub selected_index: usize,
    pub show_details: bool,
    pub error: Option<String>,
    pub update_notice: Option<UpdateNotice>,
    pub should_quit: bool,
    pub scan_in_progress: bool,
    pub scan_started_at: Option<Instant>,
}

impl App {
    pub fn new(runtime: RuntimeOptions) -> Self {
        let initial_url = runtime.url.clone();
        let has_initial_url = initial_url.is_some();
        let mode = if has_initial_url {
            UiMode::Normal
        } else {
            UiMode::UrlInput
        };
        let message = if let Some(url) = initial_url.as_deref() {
            format!("Preparing scan for {url}")
        } else {
            "Enter a URL to start crawling".to_string()
        };

        Self {
            url: initial_url.clone(),
            url_input: initial_url.clone().unwrap_or_default(),
            depth: runtime.depth,
            max_pages: runtime.max_pages,
            mode,
            progress: ProgressSnapshot::new(RunStage::LoadingConfig, message),
            report: None,
            search_query: String::new(),
            search_input: String::new(),
            severity_filter: SeverityFilter::All,
            sort_mode: SortMode::Severity,
            selected_index: 0,
            show_details: true,
            error: None,
            update_notice: None,
            should_quit: false,
            scan_in_progress: has_initial_url,
            scan_started_at: has_initial_url.then(Instant::now),
        }
    }

    pub fn apply_run_event(&mut self, event: RunEvent) {
        match event {
            RunEvent::Progress(snapshot) => {
                self.scan_in_progress =
                    !matches!(snapshot.stage, RunStage::Completed | RunStage::Failed);
                self.scan_started_at = if self.scan_in_progress {
                    self.scan_started_at.or(Some(Instant::now()))
                } else {
                    None
                };
                self.progress = snapshot;
            }
            RunEvent::ReportReady(report) => {
                self.scan_in_progress = false;
                self.scan_started_at = None;
                self.progress.stage = RunStage::Completed;
                self.progress.summary = report.summary.clone();
                self.progress.message = "Report ready".to_string();
                self.url = Some(report.start_url.clone());
                self.url_input = report.start_url.clone();
                self.report = Some(report);
                self.mode = UiMode::Normal;
                self.error = None;
                self.clamp_selection();
            }
            RunEvent::UpdateAvailable(notice) => {
                self.update_notice = Some(notice);
            }
            RunEvent::Error(error) => {
                self.scan_in_progress = false;
                self.scan_started_at = None;
                self.error = Some(error.clone());
                self.progress.stage = RunStage::Failed;
                self.progress.message = error;
                if self.report.is_none() {
                    self.mode = UiMode::UrlInput;
                }
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<AppAction> {
        let action = match self.mode {
            UiMode::UrlInput => self.handle_url_input_key(key),
            UiMode::Normal => self.handle_normal_key(key),
            UiMode::Search => self.handle_search_key(key),
        };

        self.clamp_selection();
        action
    }

    pub fn visible_pages(&self) -> Vec<&PageInfo> {
        let Some(report) = &self.report else {
            return Vec::new();
        };

        let query = self.search_query.trim().to_lowercase();
        let mut pages: Vec<&PageInfo> = report
            .pages
            .values()
            .filter(|page| self.matches_severity(page) && self.matches_query(page, &query))
            .collect();

        pages.sort_by(|left, right| self.compare_pages(left, right));
        pages
    }

    pub fn selected_page<'a>(&'a self, pages: &'a [&'a PageInfo]) -> Option<&'a PageInfo> {
        pages.get(self.selected_index).copied()
    }

    pub fn status_label(&self) -> &'static str {
        if self.mode == UiMode::UrlInput
            && !self.scan_in_progress
            && self.report.is_none()
            && self.error.is_none()
        {
            return "READY";
        }

        if self.error.is_some() {
            "FAILED"
        } else if self.report.is_some() && !self.scan_in_progress {
            "COMPLETE"
        } else {
            match self.progress.stage {
                RunStage::LoadingConfig => "LOADING",
                RunStage::Crawling => "CRAWLING",
                RunStage::CheckingLinks => "CHECKING",
                RunStage::AnalyzingSeo => "ANALYZING",
                RunStage::GeneratingReport => "REPORTING",
                RunStage::Completed => "COMPLETE",
                RunStage::Failed => "FAILED",
            }
        }
    }

    pub fn is_finished(&self) -> bool {
        self.report.is_some() || self.error.is_some()
    }

    pub const fn has_active_scan(&self) -> bool {
        self.scan_in_progress
    }

    pub fn elapsed_scan_time(&self) -> Option<Duration> {
        self.scan_started_at.map(|started_at| started_at.elapsed())
    }

    fn handle_url_input_key(&mut self, key: KeyEvent) -> Option<AppAction> {
        match key.code {
            KeyCode::Esc => {
                if self.report.is_some() {
                    self.mode = UiMode::Normal;
                } else {
                    self.should_quit = true;
                }
                None
            }
            KeyCode::Enter => {
                let url = self.url_input.trim();
                if url.is_empty() {
                    self.error = Some("Enter a URL before starting a crawl".to_string());
                    None
                } else {
                    self.start_scan(url.to_string())
                }
            }
            KeyCode::Backspace => {
                self.url_input.pop();
                None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.url_input.clear();
                None
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.url_input.push(c);
                None
            }
            _ => None,
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Option<AppAction> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('u') => {
                self.mode = UiMode::UrlInput;
                self.error = None;
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Char('g') => self.selected_index = 0,
            KeyCode::Char('G') => {
                let len = self.visible_pages().len();
                self.selected_index = len.saturating_sub(1);
            }
            KeyCode::Char('/') if self.report.is_some() => {
                self.mode = UiMode::Search;
                self.search_input = self.search_query.clone();
            }
            KeyCode::Char('f') if self.report.is_some() => {
                self.severity_filter = self.severity_filter.next();
                self.selected_index = 0;
            }
            KeyCode::Char('s') if self.report.is_some() => {
                self.sort_mode = self.sort_mode.next();
                self.selected_index = 0;
            }
            KeyCode::Enter if self.report.is_some() => {
                self.show_details = !self.show_details;
            }
            _ => {}
        }

        None
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> Option<AppAction> {
        match key.code {
            KeyCode::Esc => {
                self.search_input = self.search_query.clone();
                self.mode = UiMode::Normal;
            }
            KeyCode::Enter => {
                self.search_query = self.search_input.clone();
                self.mode = UiMode::Normal;
                self.selected_index = 0;
            }
            KeyCode::Backspace => {
                self.search_input.pop();
                self.search_query = self.search_input.clone();
                self.selected_index = 0;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_input.clear();
                self.search_query.clear();
                self.selected_index = 0;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_input.push(c);
                self.search_query = self.search_input.clone();
                self.selected_index = 0;
            }
            _ => {}
        }

        None
    }

    fn start_scan(&mut self, url: String) -> Option<AppAction> {
        self.url = Some(url.clone());
        self.url_input = url.clone();
        self.progress =
            ProgressSnapshot::new(RunStage::LoadingConfig, format!("Preparing scan for {url}"));
        self.report = None;
        self.error = None;
        self.scan_in_progress = true;
        self.scan_started_at = Some(Instant::now());
        self.search_query.clear();
        self.search_input.clear();
        self.selected_index = 0;
        self.show_details = true;
        self.mode = UiMode::Normal;
        Some(AppAction::StartScan(url))
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.visible_pages().len();
        if len == 0 {
            self.selected_index = 0;
            return;
        }

        let next = self.selected_index as isize + delta;
        self.selected_index = next.clamp(0, len.saturating_sub(1) as isize) as usize;
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_pages().len();
        if len == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= len {
            self.selected_index = len - 1;
        }
    }

    fn matches_severity(&self, page: &PageInfo) -> bool {
        match self.severity_filter {
            SeverityFilter::All => true,
            SeverityFilter::Error => page
                .issues
                .iter()
                .any(|issue| issue.severity == IssueSeverity::Error),
            SeverityFilter::Warning => page
                .issues
                .iter()
                .any(|issue| issue.severity == IssueSeverity::Warning),
            SeverityFilter::Info => page
                .issues
                .iter()
                .any(|issue| issue.severity == IssueSeverity::Info),
        }
    }

    fn matches_query(&self, page: &PageInfo, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }

        let in_url = page.url.to_lowercase().contains(query);
        let in_title = page
            .title
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(query);
        let in_issues = page
            .issues
            .iter()
            .any(|issue| issue.message.to_lowercase().contains(query));

        in_url || in_title || in_issues
    }

    fn compare_pages(&self, left: &PageInfo, right: &PageInfo) -> std::cmp::Ordering {
        use std::cmp::Reverse;

        match self.sort_mode {
            SortMode::Severity => (
                Reverse(Self::severity_rank(left)),
                Reverse(left.issues.len()),
                &left.url,
            )
                .cmp(&(
                    Reverse(Self::severity_rank(right)),
                    Reverse(right.issues.len()),
                    &right.url,
                )),
            SortMode::Issues => (
                Reverse(left.issues.len()),
                Reverse(Self::severity_rank(left)),
                &left.url,
            )
                .cmp(&(
                    Reverse(right.issues.len()),
                    Reverse(Self::severity_rank(right)),
                    &right.url,
                )),
            SortMode::Status => (
                Reverse(left.status_code.unwrap_or_default()),
                Reverse(Self::severity_rank(left)),
                &left.url,
            )
                .cmp(&(
                    Reverse(right.status_code.unwrap_or_default()),
                    Reverse(Self::severity_rank(right)),
                    &right.url,
                )),
            SortMode::Depth => (
                left.crawl_depth,
                Reverse(Self::severity_rank(left)),
                &left.url,
            )
                .cmp(&(
                    right.crawl_depth,
                    Reverse(Self::severity_rank(right)),
                    &right.url,
                )),
            SortMode::Url => left.url.cmp(&right.url),
        }
    }

    fn severity_rank(page: &PageInfo) -> u8 {
        if page
            .issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Error)
        {
            3
        } else if page
            .issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Warning)
        {
            2
        } else if page
            .issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Info)
        {
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CrawlSummary, IssueType, OpenGraphTags, SeoIssue};
    use std::collections::HashMap;

    fn page(url: &str, issues: Vec<SeoIssue>) -> PageInfo {
        PageInfo {
            url: url.to_string(),
            status_code: Some(200),
            content_type: Some("text/html".to_string()),
            title: Some(url.to_string()),
            meta_description: None,
            h1_tags: vec![],
            links: vec![],
            images: vec![],
            open_graph: OpenGraphTags::default(),
            issues,
            crawl_depth: 0,
        }
    }

    fn issue(severity: IssueSeverity, message: &str) -> SeoIssue {
        SeoIssue {
            severity,
            issue_type: IssueType::BrokenLink,
            message: message.to_string(),
        }
    }

    fn app_with_report() -> App {
        let runtime = RuntimeOptions {
            url: Some("https://example.com".to_string()),
            depth: 5,
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

        let mut pages = HashMap::new();
        pages.insert(
            "https://example.com/error".to_string(),
            page(
                "https://example.com/error",
                vec![issue(IssueSeverity::Error, "broken")],
            ),
        );
        pages.insert(
            "https://example.com/warn".to_string(),
            page(
                "https://example.com/warn",
                vec![issue(IssueSeverity::Warning, "missing description")],
            ),
        );

        let report = CrawlReport {
            start_url: runtime.url.clone().unwrap(),
            pages,
            summary: CrawlSummary {
                total_pages: 2,
                total_links: 0,
                broken_links: 1,
                errors: 1,
                warnings: 1,
                infos: 0,
            },
            timestamp: "2026-04-02T00:00:00Z".to_string(),
        };

        let mut app = App::new(runtime);
        app.report = Some(report);
        app.mode = UiMode::Normal;
        app
    }

    #[test]
    fn empty_initial_url_starts_in_url_input_mode() {
        let app = App::new(RuntimeOptions {
            url: None,
            depth: 5,
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
        });

        assert_eq!(app.mode, UiMode::UrlInput);
        assert_eq!(app.status_label(), "READY");
        assert!(!app.has_active_scan());
    }

    #[test]
    fn url_input_enter_starts_scan() {
        let mut app = App::new(RuntimeOptions {
            url: None,
            depth: 5,
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
        });
        app.url_input = "https://example.com".to_string();

        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(
            action,
            Some(AppAction::StartScan("https://example.com".to_string()))
        );
        assert_eq!(app.mode, UiMode::Normal);
        assert!(app.has_active_scan());
        assert!(app.elapsed_scan_time().is_some());
    }

    #[test]
    fn initial_url_marks_scan_as_active() {
        let app = App::new(RuntimeOptions {
            url: Some("https://example.com".to_string()),
            depth: 5,
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
        });

        assert_eq!(app.mode, UiMode::Normal);
        assert!(app.has_active_scan());
        assert!(app.elapsed_scan_time().is_some());
    }

    #[test]
    fn severity_filter_limits_visible_pages() {
        let mut app = app_with_report();
        app.severity_filter = SeverityFilter::Error;

        let pages = app.visible_pages();
        assert_eq!(pages.len(), 1);
        assert!(pages[0].url.contains("error"));
    }

    #[test]
    fn search_filters_pages_by_url_and_issue_text() {
        let mut app = app_with_report();
        app.search_query = "description".to_string();

        let pages = app.visible_pages();
        assert_eq!(pages.len(), 1);
        assert!(pages[0].url.contains("warn"));
    }

    #[test]
    fn run_events_update_report_and_failure_state() {
        let mut app = app_with_report();
        let report = app.report.clone().unwrap();

        app.apply_run_event(RunEvent::UpdateAvailable(UpdateNotice {
            latest_version: "0.4.0".to_string(),
            release_url: "https://github.com/nelsonlaidev/scoutly/releases/tag/v0.4.0".to_string(),
        }));
        assert_eq!(
            app.update_notice
                .as_ref()
                .map(|notice| notice.latest_version.as_str()),
            Some("0.4.0")
        );

        app.apply_run_event(RunEvent::Error("boom".to_string()));
        assert_eq!(app.status_label(), "FAILED");

        app.apply_run_event(RunEvent::ReportReady(report));
        assert!(app.report.is_some());
    }
}
