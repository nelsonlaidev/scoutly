use crate::http_client::build_http_client;
use crate::models::{IssueSeverity, IssueType, Link, PageInfo, SeoIssue};
use crate::reporter::Reporter;
use crate::runtime::{ProgressSnapshot, RunEvent, RunEventSender, RunStage};
use anyhow::Result;
use futures::{
    pin_mut,
    stream::{self, StreamExt},
};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use url::Url;

const DEFAULT_CONCURRENT_CHECKS: usize = 20;

#[derive(Clone)]
enum LinkCheckOutcome {
    Reachable {
        status_code: u16,
        redirected_url: Option<String>,
    },
    SkippedUnsupportedScheme,
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
            client: build_http_client(10).unwrap_or_else(|_| reqwest::Client::new()),
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
        let total_links = link_urls.len();

        // Initialize progress bar if enabled
        if let Some(ref pb) = self.progress_bar {
            pb.set_position(0);
        }

        let pending_checks = stream::iter(link_urls.iter().cloned())
            .map(|url| async move {
                let outcome = self.check_link(&url).await;
                (url, outcome)
            })
            .buffer_unordered(self.concurrent_checks);
        pin_mut!(pending_checks);

        let mut completed = 0usize;

        while let Some((url, outcome)) = pending_checks.next().await {
            if let Some(locations) = all_links.get(&url) {
                for (page_url, link_idx) in locations {
                    if let Some(page) = pages.get_mut(page_url) {
                        let issues = page
                            .links
                            .get_mut(*link_idx)
                            .map(|link| Self::apply_outcome(link, &outcome, ignore_redirects))
                            .unwrap_or_default();

                        if !issues.is_empty() {
                            page.issues.extend(issues);
                        }
                    }
                }
            }

            completed += 1;

            // Update progress bar
            if let Some(ref pb) = self.progress_bar {
                pb.set_position(completed as u64);
                pb.set_message(format!("Checking {}", url));
            }

            if let Some(sender) = &self.progress_sender {
                let mut snapshot = ProgressSnapshot::new(
                    RunStage::CheckingLinks,
                    format!("Checking link {}/{}: {}", completed, total_links, url),
                );
                snapshot.pages_crawled = pages.len();
                snapshot.links_discovered = total_links;
                snapshot.links_checked = completed;
                snapshot.total_links = total_links;
                snapshot.summary = Reporter::summarize_pages(pages);

                let _ = sender.send(RunEvent::Progress(snapshot));
            }
        }

        // Finish progress bar
        if let Some(ref pb) = self.progress_bar {
            pb.finish_with_message(format!("Checked {} links", total_links));
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
            LinkCheckOutcome::SkippedUnsupportedScheme => {
                link.status_code = None;
                link.redirected_url = None;
                link.check_error = None;
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
        if let Ok(parsed_url) = Url::parse(url)
            && !matches!(parsed_url.scheme(), "http" | "https")
        {
            return LinkCheckOutcome::SkippedUnsupportedScheme;
        }

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
        } else {
            "request failed".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, HttpResponse, HttpServer, web};
    use std::net::TcpListener;
    use std::time::Duration;

    fn link(url: &str) -> Link {
        Link {
            url: url.to_string(),
            text: "Link".to_string(),
            is_external: false,
            status_code: None,
            redirected_url: None,
            check_error: None,
        }
    }

    async fn start_slow_server(delay: Duration) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(delay))
                .route(
                    "/slow",
                    web::get().to(|delay: web::Data<Duration>| async move {
                        tokio::time::sleep(*delay.get_ref()).await;
                        HttpResponse::Ok().finish()
                    }),
                )
                .route(
                    "/ok",
                    web::get().to(|| async { HttpResponse::Ok().finish() }),
                )
        })
        .workers(1)
        .listen(listener)
        .expect("listen test server")
        .run();

        tokio::spawn(async move {
            let _ = server.await;
        });

        for _ in 0..20 {
            if reqwest::get(format!("{base_url}/ok")).await.is_ok() {
                return base_url;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        panic!("link checker server failed to start at {base_url}");
    }

    #[test]
    fn apply_outcome_covers_redirect_broken_and_transport_failures() {
        let mut redirect_link = link("https://example.com/source");
        let redirect_issues = LinkChecker::apply_outcome(
            &mut redirect_link,
            &LinkCheckOutcome::Reachable {
                status_code: 301,
                redirected_url: Some("https://example.com/final".to_string()),
            },
            false,
        );
        assert_eq!(redirect_link.status_code, Some(301));
        assert_eq!(
            redirect_link.redirected_url.as_deref(),
            Some("https://example.com/final")
        );
        assert_eq!(redirect_issues.len(), 1);
        assert_eq!(redirect_issues[0].issue_type, IssueType::Redirect);

        let mut broken_link = link("https://example.com/missing");
        let broken_issues = LinkChecker::apply_outcome(
            &mut broken_link,
            &LinkCheckOutcome::Reachable {
                status_code: 404,
                redirected_url: None,
            },
            false,
        );
        assert_eq!(broken_issues.len(), 1);
        assert_eq!(broken_issues[0].issue_type, IssueType::BrokenLink);

        let mut ignored_redirect = link("https://example.com/ignore");
        let no_issues = LinkChecker::apply_outcome(
            &mut ignored_redirect,
            &LinkCheckOutcome::Reachable {
                status_code: 302,
                redirected_url: Some("https://example.com/final".to_string()),
            },
            true,
        );
        assert!(no_issues.is_empty());

        let mut unsupported = link("mailto:test@example.com");
        let unsupported_issues = LinkChecker::apply_outcome(
            &mut unsupported,
            &LinkCheckOutcome::SkippedUnsupportedScheme,
            false,
        );
        assert!(unsupported_issues.is_empty());
        assert!(unsupported.status_code.is_none());
        assert!(unsupported.check_error.is_none());

        let mut failed = link("https://example.com/fail");
        let failure_issues = LinkChecker::apply_outcome(
            &mut failed,
            &LinkCheckOutcome::TransportFailure {
                error: "request timed out".to_string(),
            },
            false,
        );
        assert_eq!(failed.check_error.as_deref(), Some("request timed out"));
        assert_eq!(failure_issues.len(), 1);
        assert_eq!(failure_issues[0].issue_type, IssueType::BrokenLink);
    }

    #[tokio::test]
    async fn check_link_and_classify_request_error_cover_scheme_connect_request_and_timeout() {
        let checker = LinkChecker::with_concurrency(1);
        assert!(matches!(
            checker.check_link("mailto:test@example.com").await,
            LinkCheckOutcome::SkippedUnsupportedScheme
        ));
        assert!(matches!(
            checker.check_link("http://127.0.0.1:9").await,
            LinkCheckOutcome::TransportFailure { error } if error == "connection failed"
        ));
        let request_error = reqwest::Client::new()
            .get("http://")
            .build()
            .expect_err("invalid url should fail before send");
        assert_eq!(
            LinkChecker::classify_request_error(&request_error),
            "request failed"
        );

        let base_url = start_slow_server(Duration::from_millis(200)).await;
        let timeout_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(50))
            .build()
            .unwrap();
        let checker = LinkChecker {
            client: timeout_client,
            progress_bar: None,
            concurrent_checks: 1,
            progress_sender: None,
        };

        assert!(matches!(
            checker.check_link(&format!("{base_url}/slow")).await,
            LinkCheckOutcome::TransportFailure { error } if error == "request timed out"
        ));
    }
}
