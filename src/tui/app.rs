use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeMap;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultSection {
    ByPage,
    ByLinkUrl,
    ByStatus,
    AllLinks,
}

impl ResultSection {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ByPage => "By Page",
            Self::ByLinkUrl => "By Link URL",
            Self::ByStatus => "By Status",
            Self::AllLinks => "All Links",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::ByPage => Self::ByLinkUrl,
            Self::ByLinkUrl => Self::ByStatus,
            Self::ByStatus => Self::AllLinks,
            Self::AllLinks => Self::ByPage,
        }
    }

    pub const fn previous(self) -> Self {
        match self {
            Self::ByPage => Self::AllLinks,
            Self::ByLinkUrl => Self::ByPage,
            Self::ByStatus => Self::ByLinkUrl,
            Self::AllLinks => Self::ByStatus,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkOccurrence {
    pub source_page_url: String,
    pub source_page_title: String,
    pub source_page_depth: usize,
    pub destination_url: String,
    pub link_text: String,
    pub is_external: bool,
    pub status_code: Option<u16>,
    pub redirected_url: Option<String>,
    pub check_error: Option<String>,
}

impl LinkOccurrence {
    pub fn status_label(&self) -> String {
        match (&self.check_error, self.status_code) {
            (Some(_), _) => "Check failed".to_string(),
            (None, Some(status)) => status.to_string(),
            (None, None) => "Unknown".to_string(),
        }
    }

    pub fn result_summary(&self) -> String {
        match (&self.check_error, self.status_code, &self.redirected_url) {
            (Some(error), _, _) => format!("Check failed: {error}"),
            (None, Some(status), Some(redirected_url)) => {
                format!("HTTP {status} → {redirected_url}")
            }
            (None, Some(status), None) => format!("HTTP {status}"),
            (None, None, _) => "Status unknown".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkUrlGroup {
    pub destination_url: String,
    pub occurrences: Vec<LinkOccurrence>,
}

impl LinkUrlGroup {
    pub fn occurrence_count(&self) -> usize {
        self.occurrences.len()
    }

    pub fn referring_pages(&self) -> Vec<String> {
        let mut pages = self
            .occurrences
            .iter()
            .map(|occurrence| occurrence.source_page_url.clone())
            .collect::<Vec<_>>();
        pages.sort();
        pages.dedup();
        pages
    }

    pub fn result_label(&self) -> String {
        let mut labels = self
            .occurrences
            .iter()
            .map(LinkOccurrence::status_label)
            .collect::<Vec<_>>();
        labels.sort();
        labels.dedup();

        if labels.len() == 1 {
            labels.pop().unwrap_or_else(|| "Unknown".to_string())
        } else {
            "Mixed".to_string()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LinkStatusBucketKey {
    CheckFailed,
    Unknown,
    Http(u16),
}

impl LinkStatusBucketKey {
    pub fn label(&self) -> String {
        match self {
            Self::CheckFailed => "Check failed".to_string(),
            Self::Unknown => "Unknown".to_string(),
            Self::Http(status) => status.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkStatusBucket {
    pub key: LinkStatusBucketKey,
    pub occurrences: Vec<LinkOccurrence>,
}

impl LinkStatusBucket {
    pub fn label(&self) -> String {
        self.key.label()
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
    pub result_section: ResultSection,
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
            result_section: ResultSection::ByPage,
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

    pub fn visible_link_occurrences(&self) -> Vec<LinkOccurrence> {
        let query = self.search_query.trim().to_lowercase();
        let mut occurrences = self
            .link_occurrences()
            .into_iter()
            .filter(|occurrence| self.matches_link_occurrence(occurrence, &query))
            .collect::<Vec<_>>();

        occurrences.sort_by(|left, right| {
            (
                &left.source_page_url,
                &left.destination_url,
                &left.link_text,
                &left.redirected_url,
            )
                .cmp(&(
                    &right.source_page_url,
                    &right.destination_url,
                    &right.link_text,
                    &right.redirected_url,
                ))
        });
        occurrences
    }

    pub fn visible_link_url_groups(&self) -> Vec<LinkUrlGroup> {
        let query = self.search_query.trim().to_lowercase();
        let mut groups = BTreeMap::<String, Vec<LinkOccurrence>>::new();

        for occurrence in self.link_occurrences() {
            groups
                .entry(occurrence.destination_url.clone())
                .or_default()
                .push(occurrence);
        }

        groups
            .into_iter()
            .map(|(destination_url, occurrences)| LinkUrlGroup {
                destination_url,
                occurrences,
            })
            .filter(|group| self.matches_link_url_group(group, &query))
            .collect()
    }

    pub fn visible_status_buckets(&self) -> Vec<LinkStatusBucket> {
        let query = self.search_query.trim().to_lowercase();
        let mut groups = BTreeMap::<LinkStatusBucketKey, Vec<LinkOccurrence>>::new();

        for occurrence in self.link_occurrences() {
            groups
                .entry(Self::status_bucket_key(&occurrence))
                .or_default()
                .push(occurrence);
        }

        let mut buckets = groups
            .into_iter()
            .map(|(key, occurrences)| LinkStatusBucket { key, occurrences })
            .filter(|bucket| self.matches_status_bucket(bucket, &query))
            .collect::<Vec<_>>();

        buckets.sort_by(|left, right| {
            Self::status_bucket_sort_key(&left.key).cmp(&Self::status_bucket_sort_key(&right.key))
        });
        buckets
    }

    pub fn selected_page<'a>(&'a self, pages: &'a [&'a PageInfo]) -> Option<&'a PageInfo> {
        pages.get(self.selected_index).copied()
    }

    pub fn selected_link_occurrence<'a>(
        &'a self,
        occurrences: &'a [LinkOccurrence],
    ) -> Option<&'a LinkOccurrence> {
        occurrences.get(self.selected_index)
    }

    pub fn selected_link_url_group<'a>(
        &'a self,
        groups: &'a [LinkUrlGroup],
    ) -> Option<&'a LinkUrlGroup> {
        groups.get(self.selected_index)
    }

    pub fn selected_status_bucket<'a>(
        &'a self,
        buckets: &'a [LinkStatusBucket],
    ) -> Option<&'a LinkStatusBucket> {
        buckets.get(self.selected_index)
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

    pub const fn page_controls_enabled(&self) -> bool {
        matches!(self.result_section, ResultSection::ByPage)
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
                let len = self.visible_row_count();
                self.selected_index = len.saturating_sub(1);
            }
            KeyCode::Char('/') if self.report.is_some() => {
                self.mode = UiMode::Search;
                self.search_input = self.search_query.clone();
            }
            KeyCode::Char('f') if self.report.is_some() && self.page_controls_enabled() => {
                self.severity_filter = self.severity_filter.next();
                self.selected_index = 0;
            }
            KeyCode::Char('s') if self.report.is_some() && self.page_controls_enabled() => {
                self.sort_mode = self.sort_mode.next();
                self.selected_index = 0;
            }
            KeyCode::Tab if self.report.is_some() => self.cycle_result_section(true),
            KeyCode::BackTab if self.report.is_some() => self.cycle_result_section(false),
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
        self.result_section = ResultSection::ByPage;
        self.selected_index = 0;
        self.show_details = true;
        self.mode = UiMode::Normal;
        Some(AppAction::StartScan(url))
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.visible_row_count();
        if len == 0 {
            self.selected_index = 0;
            return;
        }

        let next = self.selected_index as isize + delta;
        self.selected_index = next.clamp(0, len.saturating_sub(1) as isize) as usize;
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_row_count();
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
        let in_title = page.display_title().to_lowercase().contains(query);
        let in_issues = page
            .issues
            .iter()
            .any(|issue| issue.message.to_lowercase().contains(query));

        in_url || in_title || in_issues
    }

    fn link_occurrences(&self) -> Vec<LinkOccurrence> {
        let Some(report) = &self.report else {
            return Vec::new();
        };

        let mut occurrences = report
            .pages
            .values()
            .flat_map(|page| {
                page.links.iter().map(|link| LinkOccurrence {
                    source_page_url: page.url.clone(),
                    source_page_title: page.display_title(),
                    source_page_depth: page.crawl_depth,
                    destination_url: link.url.clone(),
                    link_text: link.text.clone(),
                    is_external: link.is_external,
                    status_code: link.status_code,
                    redirected_url: link.redirected_url.clone(),
                    check_error: link.check_error.clone(),
                })
            })
            .collect::<Vec<_>>();

        occurrences.sort_by(|left, right| {
            (
                &left.source_page_url,
                &left.destination_url,
                &left.link_text,
                &left.redirected_url,
            )
                .cmp(&(
                    &right.source_page_url,
                    &right.destination_url,
                    &right.link_text,
                    &right.redirected_url,
                ))
        });
        occurrences
    }

    fn visible_row_count(&self) -> usize {
        match self.result_section {
            ResultSection::ByPage => self.visible_pages().len(),
            ResultSection::ByLinkUrl => self.visible_link_url_groups().len(),
            ResultSection::ByStatus => self.visible_status_buckets().len(),
            ResultSection::AllLinks => self.visible_link_occurrences().len(),
        }
    }

    fn cycle_result_section(&mut self, forward: bool) {
        self.result_section = if forward {
            self.result_section.next()
        } else {
            self.result_section.previous()
        };
        self.selected_index = 0;
    }

    fn matches_link_occurrence(&self, occurrence: &LinkOccurrence, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }

        occurrence.source_page_url.to_lowercase().contains(query)
            || occurrence.source_page_title.to_lowercase().contains(query)
            || occurrence.destination_url.to_lowercase().contains(query)
            || occurrence.link_text.to_lowercase().contains(query)
            || occurrence.status_label().to_lowercase().contains(query)
            || occurrence
                .redirected_url
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(query)
            || occurrence
                .check_error
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(query)
    }

    fn matches_link_url_group(&self, group: &LinkUrlGroup, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }

        group.destination_url.to_lowercase().contains(query)
            || group.result_label().to_lowercase().contains(query)
            || group
                .occurrences
                .iter()
                .any(|occurrence| self.matches_link_occurrence(occurrence, query))
    }

    fn matches_status_bucket(&self, bucket: &LinkStatusBucket, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }

        bucket.label().to_lowercase().contains(query)
            || bucket
                .occurrences
                .iter()
                .any(|occurrence| self.matches_link_occurrence(occurrence, query))
    }

    fn status_bucket_key(occurrence: &LinkOccurrence) -> LinkStatusBucketKey {
        match (&occurrence.check_error, occurrence.status_code) {
            (Some(_), _) => LinkStatusBucketKey::CheckFailed,
            (None, Some(status)) => LinkStatusBucketKey::Http(status),
            (None, None) => LinkStatusBucketKey::Unknown,
        }
    }

    fn status_bucket_sort_key(key: &LinkStatusBucketKey) -> (u8, u16) {
        match key {
            LinkStatusBucketKey::CheckFailed => (0, 0),
            LinkStatusBucketKey::Unknown => (1, 0),
            LinkStatusBucketKey::Http(status) => (2, *status),
        }
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
    use crate::models::{CrawlSummary, IssueType, Link, OpenGraphTags, SeoIssue};
    use std::collections::HashMap;

    fn page_with_links(
        url: &str,
        issues: Vec<SeoIssue>,
        links: Vec<Link>,
        crawl_depth: usize,
    ) -> PageInfo {
        PageInfo {
            url: url.to_string(),
            status_code: Some(200),
            content_type: Some("text/html".to_string()),
            title: Some(url.to_string()),
            meta_description: None,
            h1_tags: vec![],
            links,
            images: vec![],
            open_graph: OpenGraphTags::default(),
            issues,
            crawl_depth,
        }
    }

    fn issue(severity: IssueSeverity, message: &str) -> SeoIssue {
        SeoIssue {
            severity,
            issue_type: IssueType::BrokenLink,
            message: message.to_string(),
        }
    }

    fn link(
        url: &str,
        status_code: Option<u16>,
        redirected_url: Option<&str>,
        check_error: Option<&str>,
    ) -> Link {
        Link {
            url: url.to_string(),
            text: "Link text".to_string(),
            is_external: false,
            status_code,
            redirected_url: redirected_url.map(str::to_string),
            check_error: check_error.map(str::to_string),
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
            page_with_links(
                "https://example.com/error",
                vec![issue(IssueSeverity::Error, "broken")],
                vec![
                    link("https://example.com/shared", Some(200), None, None),
                    link("https://example.com/not-found", Some(404), None, None),
                ],
                0,
            ),
        );
        pages.insert(
            "https://example.com/warn".to_string(),
            page_with_links(
                "https://example.com/warn",
                vec![issue(IssueSeverity::Warning, "missing description")],
                vec![
                    link("https://example.com/shared", Some(200), None, None),
                    link(
                        "https://example.com/timeout",
                        None,
                        None,
                        Some("connection timed out"),
                    ),
                ],
                1,
            ),
        );

        let report = CrawlReport {
            start_url: runtime.url.clone().unwrap(),
            pages,
            summary: CrawlSummary {
                total_pages: 2,
                total_links: 4,
                broken_links: 2,
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
    fn tab_cycles_result_sections() {
        let mut app = app_with_report();

        app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(app.result_section, ResultSection::ByLinkUrl);

        app.handle_key(KeyEvent::from(KeyCode::BackTab));
        assert_eq!(app.result_section, ResultSection::ByPage);
    }

    #[test]
    fn by_link_url_groups_duplicate_destinations() {
        let app = app_with_report();

        let groups = app.visible_link_url_groups();
        let shared = groups
            .iter()
            .find(|group| group.destination_url == "https://example.com/shared")
            .expect("shared URL should be grouped");

        assert_eq!(shared.occurrence_count(), 2);
        assert_eq!(shared.referring_pages().len(), 2);
        assert_eq!(shared.result_label(), "200");
    }

    #[test]
    fn by_status_groups_http_and_failed_links() {
        let app = app_with_report();

        let labels = app
            .visible_status_buckets()
            .into_iter()
            .map(|bucket| bucket.label())
            .collect::<Vec<_>>();

        assert_eq!(labels, vec!["Check failed", "200", "404"]);
    }

    #[test]
    fn all_links_preserve_duplicate_occurrences() {
        let app = app_with_report();

        let shared_count = app
            .visible_link_occurrences()
            .into_iter()
            .filter(|occurrence| occurrence.destination_url == "https://example.com/shared")
            .count();

        assert_eq!(shared_count, 2);
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
