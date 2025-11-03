use crate::http_client::build_http_client;
use crate::models::{IssueSeverity, IssueType, PageInfo, SeoIssue};
use anyhow::Result;
use futures::future::join_all;
use std::collections::HashMap;

pub struct LinkChecker {
    client: reqwest::Client,
}

impl LinkChecker {
    pub fn new() -> Self {
        Self {
            client: build_http_client(10).expect("Failed to build HTTP client"),
        }
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
                    .or_insert_with(Vec::new)
                    .push((page_url.clone(), idx));
            }
        }

        // Check links in batches
        let link_urls: Vec<String> = all_links.keys().cloned().collect();
        let mut futures = Vec::new();

        for url in &link_urls {
            futures.push(self.check_link(url));
        }

        let results = join_all(futures).await;

        // Update page info with link status codes and redirects
        for (url, (status_code, redirected_url)) in link_urls.iter().zip(results.iter()) {
            if let Some(locations) = all_links.get(url) {
                for (page_url, link_idx) in locations {
                    if let Some(page) = pages.get_mut(page_url) {
                        if let Some(link) = page.links.get_mut(*link_idx) {
                            link.status_code = *status_code;
                            link.redirected_url = redirected_url.clone();

                            // Add redirect issue if applicable (unless ignored)
                            if !ignore_redirects {
                                if let Some(redirect_to) = redirected_url {
                                    page.issues.push(SeoIssue {
                                        severity: IssueSeverity::Info,
                                        issue_type: IssueType::Redirect,
                                        message: format!(
                                            "Link redirected: {} -> {}",
                                            link.url, redirect_to
                                        ),
                                    });
                                }
                            }

                            // Add broken link issue if applicable
                            if let Some(code) = status_code {
                                if *code >= 400 {
                                    page.issues.push(SeoIssue {
                                        severity: IssueSeverity::Error,
                                        issue_type: IssueType::BrokenLink,
                                        message: format!(
                                            "Broken link: {} (HTTP {})",
                                            link.url, code
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn check_link(&self, url: &str) -> (Option<u16>, Option<String>) {
        // Use GET with full browser-like headers (many sites block HEAD requests)
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

                (Some(status), redirected_url)
            }
            Err(_) => (None, None),
        }
    }
}
