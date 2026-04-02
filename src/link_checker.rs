use crate::http_client::build_http_client;
use crate::models::{IssueSeverity, IssueType, Link, PageInfo, SeoIssue};
use crate::reporter::Reporter;
use crate::runtime::{ProgressSnapshot, RunEvent, RunEventSender, RunStage};
use anyhow::Result;
use futures::stream::{self, StreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;

const DEFAULT_CONCURRENT_CHECKS: usize = 20;

#[derive(Clone)]
enum LinkCheckOutcome {
    Reachable {
        status_code: u16,
        redirected_url: Option<String>,
    },
    TransportFailure {
        error: String,
    },
}

pub struct LinkChecker {
    client: reqwest::Client,
    progress_bar: Option<ProgressBar>,
    concurrent_checks: usize,
    progress_sender: Option<RunEventSender>,
}

impl Default for LinkChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl LinkChecker {
    pub fn new() -> Self {
        Self::with_concurrency(DEFAULT_CONCURRENT_CHECKS)
    }

    pub fn with_concurrency(concurrent_checks: usize) -> Self {
        Self {
            client: build_http_client(10).expect("Failed to build HTTP client"),
            progress_bar: None,
            concurrent_checks: concurrent_checks.max(1),
            progress_sender: None,
        }
    }

    /// Enable progress bar for link checking
    pub fn enable_progress_bar(&mut self, total_links: usize) {
        let pb = ProgressBar::new(total_links as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} links ({eta})")
                .expect("Progress bar template should be valid")
                .progress_chars("=>-"),
        );
        pb.set_message("Checking links");
        self.progress_bar = Some(pb);
    }

    pub fn set_progress_sender(&mut self, sender: RunEventSender) {
        self.progress_sender = Some(sender);
    }

    pub async fn check_all_links(
        &self,
        pages: &mut HashMap<String, PageInfo>,
        ignore_redirects: bool,
    ) -> Result<()> {
        // Collect all unique links
        let mut all_links: HashMap<String, Vec<(String, usize)>> = HashMap::new();

        for (page_url, page_info) in pages.iter() {
            for (idx, link) in page_info.links.iter().enumerate() {
                all_links
                    .entry(link.url.clone())
                    .or_default()
                    .push((page_url.clone(), idx));
            }
        }

        let link_urls: Vec<String> = all_links.keys().cloned().collect();
        let results: HashMap<String, LinkCheckOutcome> = stream::iter(link_urls.iter().cloned())
            .map(|url| async move {
                let outcome = self.check_link(&url).await;
                (url, outcome)
            })
            .buffer_unordered(self.concurrent_checks)
            .collect()
            .await;

        // Initialize progress bar if enabled
        if let Some(ref pb) = self.progress_bar {
            pb.set_position(0);
        }

        for (idx, url) in link_urls.iter().enumerate() {
            if let Some(locations) = all_links.get(url) {
                for (page_url, link_idx) in locations {
                    if let Some(page) = pages.get_mut(page_url)
                        && let Some(outcome) = results.get(url)
                    {
                        let issues = if let Some(link) = page.links.get_mut(*link_idx) {
                            Self::apply_outcome(link, outcome, ignore_redirects)
                        } else {
                            Vec::new()
                        };

                        if !issues.is_empty() {
                            page.issues.extend(issues);
                        }
                    }
                }
            }

            // Update progress bar
            if let Some(ref pb) = self.progress_bar {
                pb.set_position((idx + 1) as u64);
            }

            if let Some(sender) = &self.progress_sender {
                let mut snapshot = ProgressSnapshot::new(
                    RunStage::CheckingLinks,
                    format!("Checked {}/{} unique link(s)", idx + 1, link_urls.len()),
                );
                snapshot.pages_crawled = pages.len();
                snapshot.links_discovered = link_urls.len();
                snapshot.links_checked = idx + 1;
                snapshot.total_links = link_urls.len();
                snapshot.summary = Reporter::summarize_pages(pages);

                let _ = sender.send(RunEvent::Progress(snapshot));
            }
        }

        // Finish progress bar
        if let Some(ref pb) = self.progress_bar {
            pb.finish_with_message(format!("Checked {} links", link_urls.len()));
        }

        Ok(())
    }

    fn apply_outcome(
        link: &mut Link,
        outcome: &LinkCheckOutcome,
        ignore_redirects: bool,
    ) -> Vec<SeoIssue> {
        let mut issues = Vec::new();

        match outcome {
            LinkCheckOutcome::Reachable {
                status_code,
                redirected_url,
            } => {
                link.status_code = Some(*status_code);
                link.redirected_url = redirected_url.clone();
                link.check_error = None;

                if !ignore_redirects && let Some(redirect_to) = redirected_url {
                    issues.push(SeoIssue {
                        severity: IssueSeverity::Info,
                        issue_type: IssueType::Redirect,
                        message: format!("Link redirected: {} -> {}", link.url, redirect_to),
                    });
                }

                if *status_code >= 400 {
                    issues.push(SeoIssue {
                        severity: IssueSeverity::Error,
                        issue_type: IssueType::BrokenLink,
                        message: format!("Broken link: {} (HTTP {})", link.url, status_code),
                    });
                }
            }
            LinkCheckOutcome::TransportFailure { error } => {
                link.status_code = None;
                link.redirected_url = None;
                link.check_error = Some(error.clone());

                issues.push(SeoIssue {
                    severity: IssueSeverity::Error,
                    issue_type: IssueType::BrokenLink,
                    message: format!("Link check failed: {} ({})", link.url, error),
                });
            }
        }

        issues
    }

    async fn check_link(&self, url: &str) -> LinkCheckOutcome {
        match self.client.get(url).send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                let final_url = response.url().to_string();

                // Check if URL was redirected (ignoring fragment differences)
                let url_without_fragment = url.split('#').next().unwrap_or(url);
                let final_url_without_fragment = final_url.split('#').next().unwrap_or(&final_url);

                let redirected_url = if final_url_without_fragment != url_without_fragment {
                    Some(final_url)
                } else {
                    None
                };

                LinkCheckOutcome::Reachable {
                    status_code: status,
                    redirected_url,
                }
            }
            Err(error) => LinkCheckOutcome::TransportFailure {
                error: Self::classify_request_error(&error),
            },
        }
    }

    fn classify_request_error(error: &reqwest::Error) -> String {
        if error.is_timeout() {
            "request timed out".to_string()
        } else if error.is_connect() {
            "connection failed".to_string()
        } else if error.is_request() {
            "request failed".to_string()
        } else {
            "unexpected request error".to_string()
        }
    }
}
