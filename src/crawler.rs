use crate::http_client::build_http_client;
use crate::models::{Image, Link, PageInfo};
use crate::robots::RobotsTxt;
use anyhow::{Context, Result, anyhow};
use futures::stream::{self, StreamExt};
use governor::{
    Quota, RateLimiter, clock::DefaultClock, state::InMemoryState, state::direct::NotKeyed,
};
use indicatif::{ProgressBar, ProgressStyle};
use once_cell::sync::Lazy;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZeroU32;
use url::Url;

/// Configuration for the crawler
pub struct CrawlerConfig {
    pub max_depth: usize,
    pub max_pages: usize,
    pub follow_external: bool,
    pub keep_fragments: bool,
    pub requests_per_second: Option<f64>,
    pub concurrent_requests: usize,
    pub respect_robots_txt: bool,
}

// Cached selectors to avoid repeated parsing and eliminate unwrap() calls
static TITLE_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("title").expect("title selector should be valid"));
static META_DESC_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse("meta[name='description']").expect("meta description selector should be valid")
});
static H1_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("h1").expect("h1 selector should be valid"));
static IMG_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("img[src]").expect("img[src] selector should be valid"));

// Unified selector for all link-bearing elements (single DOM pass optimization)
static LINK_ELEMENTS_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse(
        "a[href], iframe[src], video[src], source[src], audio[src], embed[src], object[data]",
    )
    .expect("link elements selector should be valid")
});

pub struct Crawler {
    client: reqwest::Client,
    base_url: Url,
    max_depth: usize,
    max_pages: usize,
    follow_external: bool,
    keep_fragments: bool,
    visited: HashSet<String>,
    to_visit: VecDeque<(String, usize)>,
    pub pages: HashMap<String, PageInfo>,
    rate_limiter: Option<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    concurrent_requests: usize,
    respect_robots_txt: bool,
    robots_txt: RobotsTxt,
    progress_bar: Option<ProgressBar>,
}

impl Crawler {
    pub fn new(start_url: &str, config: CrawlerConfig) -> Result<Self> {
        let base_url = Url::parse(start_url).context("Invalid URL")?;

        // Validate URL scheme - only allow http and https
        match base_url.scheme() {
            "http" | "https" => {}
            scheme => {
                return Err(anyhow!(
                    "Invalid URL scheme '{}': only http and https are supported",
                    scheme
                ));
            }
        }

        let mut to_visit = VecDeque::new();
        to_visit.push_back((start_url.to_string(), 0));

        // Initialize rate limiter if requests_per_second is specified
        let rate_limiter = config.requests_per_second.map(|rps| {
            let quota = Quota::per_second(NonZeroU32::new(rps.ceil() as u32).unwrap());
            RateLimiter::direct(quota)
        });

        Ok(Self {
            client: build_http_client(30)?,
            base_url,
            max_depth: config.max_depth,
            max_pages: config.max_pages,
            follow_external: config.follow_external,
            keep_fragments: config.keep_fragments,
            visited: HashSet::new(),
            to_visit,
            pages: HashMap::new(),
            rate_limiter,
            concurrent_requests: config.concurrent_requests,
            respect_robots_txt: config.respect_robots_txt,
            robots_txt: RobotsTxt::new(),
            progress_bar: None,
        })
    }

    /// Enable progress bar for crawling
    pub fn enable_progress_bar(&mut self) {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("[{elapsed_precise}] {spinner:.cyan} Crawling: {pos} pages")
                .expect("Progress bar template should be valid"),
        );
        self.progress_bar = Some(pb);
    }

    /// Normalizes a URL by optionally removing fragment identifiers
    fn normalize_url(&self, url: &str) -> String {
        if self.keep_fragments {
            url.to_string()
        } else {
            // Strip fragment identifier if present
            if let Some(pos) = url.find('#') {
                url[..pos].to_string()
            } else {
                url.to_string()
            }
        }
    }

    /// Checks if a URL is external by comparing host and port with base_url
    fn is_external_url(&self, url: &Url) -> bool {
        url.host_str() != self.base_url.host_str() || url.port() != self.base_url.port()
    }

    pub async fn crawl(&mut self) -> Result<()> {
        // Fetch robots.txt for the base domain if respect_robots_txt is enabled
        if self.respect_robots_txt
            && let Err(e) = self.robots_txt.fetch(&self.client, &self.base_url).await
        {
            tracing::warn!(error = %e, "Failed to fetch robots.txt, continuing anyway");
        }

        // Initialize progress bar if enabled
        if let Some(ref pb) = self.progress_bar {
            pb.set_position(0);
        }

        while !self.to_visit.is_empty() && self.visited.len() < self.max_pages {
            // Collect up to concurrent_requests URLs to fetch
            let mut batch = Vec::new();
            while let Some((url, depth)) = self.to_visit.pop_front() {
                let normalized_url = self.normalize_url(&url);

                // Check if already visited or depth exceeded before processing
                if self.visited.contains(&normalized_url) || depth > self.max_depth {
                    continue;
                }

                // Check robots.txt if enabled
                if self.respect_robots_txt
                    && let Ok(parsed_url) = Url::parse(&url)
                    && !self.robots_txt.is_allowed(&parsed_url, "scoutly")
                {
                    tracing::info!(url = %url, "Skipping URL disallowed by robots.txt");
                    self.visited.insert(normalized_url.clone());
                    continue;
                }

                // Check if adding this would exceed max_pages
                if self.visited.len() + batch.len() >= self.max_pages {
                    break;
                }

                self.visited.insert(normalized_url.clone());
                batch.push((url, depth, normalized_url));

                // Stop if we've reached the batch size
                if batch.len() >= self.concurrent_requests {
                    break;
                }
            }

            if batch.is_empty() {
                break;
            }

            // Fetch batch concurrently using buffer_unordered
            let results = stream::iter(&batch)
                .map(|(url, depth, _normalized_url)| self.fetch_page(url, *depth))
                .buffer_unordered(self.concurrent_requests)
                .collect::<Vec<_>>()
                .await;

            // Combine results with batch data
            let results: Vec<_> = batch.into_iter().zip(results).collect();

            // Process results and queue new links
            for ((url, depth, normalized_url), result) in results {
                match result {
                    Ok(page_info) => {
                        // Queue internal links for crawling
                        if depth < self.max_depth {
                            for link in &page_info.links {
                                if !link.is_external || self.follow_external {
                                    let normalized_link_url = self.normalize_url(&link.url);
                                    if !self.visited.contains(&normalized_link_url) {
                                        self.to_visit.push_back((link.url.clone(), depth + 1));
                                    }
                                }
                            }
                        }

                        self.pages.insert(normalized_url, page_info);
                    }
                    Err(e) => {
                        tracing::error!(url = %url, error = %e, "Failed to crawl page");
                        // Still insert a minimal page info for failed pages
                        self.pages.insert(
                            normalized_url,
                            PageInfo {
                                url,
                                status_code: None,
                                content_type: None,
                                title: None,
                                meta_description: None,
                                h1_tags: vec![],
                                links: vec![],
                                images: vec![],
                                issues: vec![],
                                crawl_depth: depth,
                            },
                        );
                    }
                }
            }

            // Update progress bar
            if let Some(ref pb) = self.progress_bar {
                pb.set_position(self.pages.len() as u64);
            }
        }

        // Finish progress bar
        if let Some(ref pb) = self.progress_bar {
            pb.finish_with_message(format!("Crawled {} pages", self.pages.len()));
        }

        Ok(())
    }

    async fn fetch_page(&self, url: &str, depth: usize) -> Result<PageInfo> {
        // Wait for rate limiter before making request
        if let Some(limiter) = &self.rate_limiter {
            limiter.until_ready().await;
        }

        let response = self.client.get(url).send().await?;
        let status_code = response.status().as_u16();

        // Extract content type from response headers
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Validate content type before attempting to parse as HTML
        if let Some(ref ct) = content_type {
            let ct_lower = ct.to_lowercase();
            if !ct_lower.contains("text/html") && !ct_lower.contains("application/xhtml") {
                tracing::warn!(
                    url = %url,
                    content_type = %ct,
                    "Non-HTML content type detected, parsing may fail"
                );
            }
        }

        let html_content = response.text().await?;
        let document = Html::parse_document(&html_content);

        // Parse URL once for use in extraction methods
        let page_url = Url::parse(url)?;

        // Extract title
        let title = Self::extract_title(&document);

        // Extract meta description
        let meta_description = Self::extract_meta_description(&document);

        // Extract H1 tags
        let h1_tags = Self::extract_h1_tags(&document);

        // Extract links
        let links = self.extract_links(&document, &page_url)?;

        // Extract images
        let images = self.extract_images(&document, &page_url)?;

        Ok(PageInfo {
            url: url.to_string(),
            status_code: Some(status_code),
            content_type,
            title,
            meta_description,
            h1_tags,
            links,
            images,
            issues: vec![],
            crawl_depth: depth,
        })
    }

    fn extract_title(document: &Html) -> Option<String> {
        document
            .select(&TITLE_SELECTOR)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
    }

    fn extract_meta_description(document: &Html) -> Option<String> {
        document
            .select(&META_DESC_SELECTOR)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.to_string())
    }

    fn extract_h1_tags(document: &Html) -> Vec<String> {
        document
            .select(&H1_SELECTOR)
            .map(|el| el.text().collect::<String>().trim().to_string())
            .collect()
    }

    fn extract_links(&self, document: &Html, page_url: &Url) -> Result<Vec<Link>> {
        let mut links = Vec::new();

        // Single-pass extraction: iterate through all link-bearing elements once
        for element in document.select(&LINK_ELEMENTS_SELECTOR) {
            let element_name = element.value().name();

            // Get the URL attribute based on element type
            let url_attr = match element_name {
                "a" => element.value().attr("href"),
                "object" => element.value().attr("data"),
                _ => element.value().attr("src"), // iframe, video, source, audio, embed
            };

            if let Some(url_value) = url_attr
                && let Ok(absolute_url) = page_url.join(url_value)
            {
                let url_str = absolute_url.to_string();
                let is_external = self.is_external_url(&absolute_url);

                // Generate text based on element type
                let text = match element_name {
                    "a" => element.text().collect::<String>().trim().to_string(),
                    "iframe" => {
                        let title = element.value().attr("title").unwrap_or("");
                        format!("[iframe] {}", title)
                    }
                    "video" => "[video]".to_string(),
                    "source" => {
                        let media_type = element.value().attr("type").unwrap_or("");
                        format!("[source type={}]", media_type)
                    }
                    "audio" => "[audio]".to_string(),
                    "embed" => "[embed]".to_string(),
                    "object" => "[object]".to_string(),
                    _ => continue, // Skip unknown elements
                };

                links.push(Link {
                    url: url_str,
                    text,
                    is_external,
                    status_code: None,
                    redirected_url: None,
                });
            }
        }

        Ok(links)
    }

    fn extract_images(&self, document: &Html, page_url: &Url) -> Result<Vec<Image>> {
        let mut images = Vec::new();

        for element in document.select(&IMG_SELECTOR) {
            if let Some(src) = element.value().attr("src")
                && let Ok(absolute_url) = page_url.join(src)
            {
                let alt = element.value().attr("alt").map(|s| s.to_string());
                images.push(Image {
                    src: absolute_url.to_string(),
                    alt,
                });
            }
        }

        Ok(images)
    }
}
