use crate::http_client::build_http_client;
use crate::models::{Image, Link, OpenGraphTags, PageInfo};
use crate::reporter::Reporter;
use crate::robots::RobotsTxt;
use crate::runtime::{ProgressSnapshot, RunEvent, RunEventSender, RunStage};
use anyhow::{Context, Result, anyhow};
use futures::stream::{self, StreamExt};
use governor::{
    Quota, RateLimiter, clock::DefaultClock, state::InMemoryState, state::direct::NotKeyed,
};
use indicatif::{ProgressBar, ProgressStyle};
use scraper::{Html, Selector};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::LazyLock;
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

/// Cached selectors to avoid repeated parsing and eliminate unwrap() calls
static TITLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("title").expect("title selector should be valid"));
static META_DESC_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[name='description']").expect("meta description selector should be valid")
});
static H1_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("h1").expect("h1 selector should be valid"));
static IMG_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("img[src]").expect("img[src] selector should be valid"));

// Open Graph meta tag selectors
static OG_TITLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[property='og:title']").expect("og:title selector should be valid")
});
static OG_DESCRIPTION_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[property='og:description']")
        .expect("og:description selector should be valid")
});
static OG_IMAGE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[property='og:image']").expect("og:image selector should be valid")
});
static OG_URL_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[property='og:url']").expect("og:url selector should be valid")
});
static OG_TYPE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[property='og:type']").expect("og:type selector should be valid")
});
static OG_SITE_NAME_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[property='og:site_name']").expect("og:site_name selector should be valid")
});
static OG_LOCALE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[property='og:locale']").expect("og:locale selector should be valid")
});

// Unified selector for all link-bearing elements (single DOM pass optimization)
static LINK_ELEMENTS_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
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
    rate_limiter: Option<Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>>,
    concurrent_requests: usize,
    respect_robots_txt: bool,
    robots_txt: RobotsTxt,
    progress_bar: Option<ProgressBar>,
    progress_sender: Option<RunEventSender>,
}

impl Crawler {
    pub fn new(start_url: &str, config: CrawlerConfig) -> Result<Self> {
        let base_url = Url::parse(start_url).context("Invalid URL")?;

        // Validate URL scheme - only allow http and https
        if !Self::has_supported_web_scheme(&base_url) {
            return Err(anyhow!(
                "Invalid URL scheme '{}': only http and https are supported",
                base_url.scheme()
            ));
        }

        let mut to_visit = VecDeque::new();
        to_visit.push_back((start_url.to_string(), 0));

        // Initialize rate limiter if requests_per_second is specified
        let rate_limiter = config.requests_per_second.and_then(|rps| {
            let capped = rps.ceil() as u32;
            NonZeroU32::new(capped).map(|nz| {
                let quota = Quota::per_second(nz);
                Arc::new(RateLimiter::direct(quota))
            })
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
            progress_sender: None,
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

    pub fn set_progress_sender(&mut self, sender: RunEventSender) {
        self.progress_sender = Some(sender);
    }

    fn emit_progress(&self) {
        let Some(sender) = &self.progress_sender else {
            return;
        };

        let mut snapshot = ProgressSnapshot::new(
            RunStage::Crawling,
            format!("Crawled {} page(s)", self.pages.len()),
        );
        snapshot.pages_crawled = self.pages.len();
        snapshot.links_discovered = self.pages.values().map(|page| page.links.len()).sum();
        snapshot.total_links = snapshot.links_discovered;
        snapshot.summary = Reporter::summarize_pages(&self.pages);

        if sender.send(RunEvent::Progress(snapshot)).is_err() {
            tracing::debug!("Progress event dropped, receiver disconnected");
        }
    }

    /// Normalizes a URL by optionally removing fragment identifiers
    fn normalize_url<'a>(&self, url: &'a str) -> Cow<'a, str> {
        if self.keep_fragments {
            Cow::Borrowed(url)
        } else {
            Cow::Borrowed(url.split('#').next().unwrap_or(url))
        }
    }

    /// Checks if a URL is external by comparing host and port with base_url
    fn is_external_url(&self, url: &Url) -> bool {
        url.host_str() != self.base_url.host_str() || url.port() != self.base_url.port()
    }

    pub async fn crawl(&mut self) -> Result<()> {
        // Fetch robots.txt for the base domain if respect_robots_txt is enabled
        if self.respect_robots_txt {
            let _ = self.robots_txt.fetch(&self.client, &self.base_url).await;
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
                if self.visited.contains(normalized_url.as_ref()) || depth > self.max_depth {
                    continue;
                }

                // Check robots.txt if enabled
                if self.respect_robots_txt
                    && let Ok(parsed_url) = Url::parse(&url)
                    && !self.robots_txt.is_allowed(&parsed_url, "scoutly")
                {
                    tracing::info!(url = %url, "Skipping URL disallowed by robots.txt");
                    let owned = normalized_url.into_owned();
                    self.visited.insert(owned);
                    continue;
                }

                // Check if adding this would exceed max_pages
                if self.visited.len() + batch.len() >= self.max_pages {
                    break;
                }

                let owned = normalized_url.into_owned();
                self.visited.insert(owned.clone());
                batch.push((url, depth, owned));

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
                                if (link.is_external && !self.follow_external)
                                    || !Self::should_crawl_discovered_url(&link.url)
                                {
                                    continue;
                                }

                                let normalized_link_url = self.normalize_url(&link.url);
                                if !self.visited.contains(normalized_link_url.as_ref()) {
                                    self.to_visit.push_back((link.url.clone(), depth + 1));
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
                                open_graph: OpenGraphTags::default(),
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

            self.emit_progress();
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

        if !PageInfo::is_html_content_type(content_type.as_deref()) {
            if let Some(ref ct) = content_type {
                tracing::info!(url = %url, content_type = %ct, "Skipping HTML extraction for non-HTML response");
            }

            return Ok(PageInfo {
                url: url.to_string(),
                status_code: Some(status_code),
                content_type,
                title: None,
                meta_description: None,
                h1_tags: vec![],
                links: vec![],
                images: vec![],
                open_graph: OpenGraphTags::default(),
                issues: vec![],
                crawl_depth: depth,
            });
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

        // Extract Open Graph tags
        let open_graph = Self::extract_open_graph_tags(&document);

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
            open_graph,
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

    fn extract_open_graph_tags(document: &Html) -> OpenGraphTags {
        OpenGraphTags {
            og_title: document
                .select(&OG_TITLE_SELECTOR)
                .next()
                .and_then(|el| el.value().attr("content"))
                .map(|s| s.to_string()),
            og_description: document
                .select(&OG_DESCRIPTION_SELECTOR)
                .next()
                .and_then(|el| el.value().attr("content"))
                .map(|s| s.to_string()),
            og_image: document
                .select(&OG_IMAGE_SELECTOR)
                .next()
                .and_then(|el| el.value().attr("content"))
                .map(|s| s.to_string()),
            og_url: document
                .select(&OG_URL_SELECTOR)
                .next()
                .and_then(|el| el.value().attr("content"))
                .map(|s| s.to_string()),
            og_type: document
                .select(&OG_TYPE_SELECTOR)
                .next()
                .and_then(|el| el.value().attr("content"))
                .map(|s| s.to_string()),
            og_site_name: document
                .select(&OG_SITE_NAME_SELECTOR)
                .next()
                .and_then(|el| el.value().attr("content"))
                .map(|s| s.to_string()),
            og_locale: document
                .select(&OG_LOCALE_SELECTOR)
                .next()
                .and_then(|el| el.value().attr("content"))
                .map(|s| s.to_string()),
        }
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
                links.push(Link {
                    url: url_str,
                    text: Self::link_text_for_element(element_name, &element),
                    is_external,
                    status_code: None,
                    redirected_url: None,
                    check_error: None,
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

    fn should_crawl_discovered_url(url: &str) -> bool {
        let Ok(parsed_url) = Url::parse(url) else {
            return false;
        };

        Self::has_supported_web_scheme(&parsed_url)
            && !Self::is_known_non_html_resource_url(&parsed_url)
    }

    fn has_supported_web_scheme(url: &Url) -> bool {
        matches!(url.scheme(), "http" | "https")
    }

    fn is_known_non_html_resource_url(url: &Url) -> bool {
        let Some(extension) = url
            .path_segments()
            .and_then(|segments| segments.filter(|segment| !segment.is_empty()).next_back())
            .and_then(|segment| {
                segment
                    .rsplit_once('.')
                    .map(|(_, ext)| ext.to_ascii_lowercase())
            })
        else {
            return false;
        };

        matches!(
            extension.as_str(),
            "7z" | "aac"
                | "avi"
                | "avif"
                | "bin"
                | "bmp"
                | "css"
                | "csv"
                | "doc"
                | "docx"
                | "eot"
                | "flac"
                | "gif"
                | "gz"
                | "ico"
                | "jpeg"
                | "jpg"
                | "js"
                | "json"
                | "m4a"
                | "m4v"
                | "mid"
                | "midi"
                | "mjs"
                | "mov"
                | "mp3"
                | "mp4"
                | "mpeg"
                | "mpg"
                | "ogg"
                | "ogv"
                | "otf"
                | "pdf"
                | "png"
                | "ppt"
                | "pptx"
                | "rar"
                | "svg"
                | "swf"
                | "tar"
                | "tgz"
                | "ttf"
                | "txt"
                | "wav"
                | "webm"
                | "webp"
                | "woff"
                | "woff2"
                | "xls"
                | "xlsx"
                | "xml"
                | "zip"
        )
    }

    fn link_text_for_element(element_name: &str, element: &scraper::ElementRef<'_>) -> String {
        match element_name {
            "a" => element.text().collect::<String>().trim().to_string(),
            "iframe" => format!("[iframe] {}", element.value().attr("title").unwrap_or("")),
            "video" => "[video]".to_string(),
            "source" => format!(
                "[source type={}]",
                element.value().attr("type").unwrap_or("")
            ),
            "audio" => "[audio]".to_string(),
            "embed" => "[embed]".to_string(),
            "object" => "[object]".to_string(),
            _ => unreachable!("selector only yields supported link-bearing elements"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, HttpResponse, HttpServer, web};
    use scraper::{Html, Selector};
    use std::net::TcpListener;
    use std::time::Duration;
    use tokio::sync::mpsc::unbounded_channel;

    fn config() -> CrawlerConfig {
        CrawlerConfig {
            max_depth: 1,
            max_pages: 10,
            follow_external: false,
            keep_fragments: false,
            requests_per_second: None,
            concurrent_requests: 1,
            respect_robots_txt: false,
        }
    }

    async fn start_non_html_server() -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind crawler server");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new(|| {
            App::new()
                .route(
                    "/image",
                    web::get().to(|| async {
                        HttpResponse::Ok()
                            .insert_header(("content-type", "image/png"))
                            .body("not-really-a-png")
                    }),
                )
                .route(
                    "/ok",
                    web::get().to(|| async {
                        HttpResponse::Ok()
                            .insert_header(("content-type", "text/html"))
                            .body("<html><title>ok</title></html>")
                    }),
                )
        })
        .workers(1)
        .listen(listener)
        .expect("listen crawler server")
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

        panic!("crawler server failed to start at {base_url}");
    }

    #[test]
    fn discovered_url_filter_rejects_invalid_and_non_html_resources() {
        assert!(!Crawler::should_crawl_discovered_url("not-a-url"));
        assert!(!Crawler::should_crawl_discovered_url(
            "mailto:test@example.com"
        ));
        assert!(!Crawler::should_crawl_discovered_url(
            "https://example.com/file.pdf"
        ));
        assert!(Crawler::should_crawl_discovered_url(
            "https://example.com/path"
        ));
        assert!(Crawler::is_known_non_html_resource_url(
            &Url::parse("https://example.com/image.png").unwrap()
        ));
        assert!(!Crawler::is_known_non_html_resource_url(
            &Url::parse("https://example.com/page").unwrap()
        ));
    }

    #[tokio::test]
    async fn progress_sender_and_non_html_fetch_paths_are_exercised() {
        let base_url = start_non_html_server().await;
        let mut crawler = Crawler::new(&base_url, config()).unwrap();
        let (sender, mut receiver) = unbounded_channel();
        crawler.set_progress_sender(sender.clone());
        crawler.emit_progress();

        let event = receiver.try_recv().expect("progress event should be sent");
        let RunEvent::Progress(snapshot) = event else {
            panic!("expected progress snapshot");
        };
        assert_eq!(snapshot.stage, RunStage::Crawling);
        assert_eq!(snapshot.message, "Crawled 0 page(s)");

        drop(receiver);
        crawler.emit_progress();

        let page = crawler
            .fetch_page(&format!("{base_url}/image"), 0)
            .await
            .unwrap();
        assert_eq!(page.url, format!("{base_url}/image"));
        assert_eq!(page.status_code, Some(200));
        assert_eq!(page.content_type.as_deref(), Some("image/png"));
        assert!(page.links.is_empty());
    }

    #[test]
    fn link_text_helper_covers_known_and_unknown_elements() {
        let document = Html::parse_document(
            r#"<div><a href="/about">About</a><source src="/movie.mp4" type="video/mp4"></source><div src="/x"></div></div>"#,
        );
        let a_selector = Selector::parse("a").unwrap();
        let source_selector = Selector::parse("source").unwrap();

        let a = document.select(&a_selector).next().unwrap();
        let source = document.select(&source_selector).next().unwrap();

        assert_eq!(Crawler::link_text_for_element("a", &a), "About".to_string());
        assert_eq!(
            Crawler::link_text_for_element("source", &source),
            "[source type=video/mp4]".to_string()
        );
    }

    #[test]
    fn extract_links_covers_supported_embedded_media_elements() {
        let crawler = Crawler::new("https://example.com", config()).unwrap();
        let document = Html::parse_document(
            r#"
            <html>
              <body>
                <iframe src="/frame" title="Frame Title"></iframe>
                <video src="/video.mp4"></video>
                <audio src="/audio.mp3"></audio>
                <embed src="/guide.pdf"></embed>
                <object data="/report.pdf"></object>
              </body>
            </html>
            "#,
        );
        let page_url = Url::parse("https://example.com/start").unwrap();

        let links = crawler.extract_links(&document, &page_url).unwrap();

        assert_eq!(links.len(), 5);
        assert!(links.iter().any(|link| link.text == "[iframe] Frame Title"));
        assert!(links.iter().any(|link| link.text == "[video]"));
        assert!(links.iter().any(|link| link.text == "[audio]"));
        assert!(links.iter().any(|link| link.text == "[embed]"));
        assert!(links.iter().any(|link| link.text == "[object]"));
    }

    #[tokio::test]
    async fn crawl_attempts_robots_fetch_before_failing_request() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind temp listener");
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let mut crawler = Crawler::new(
            &format!("http://{addr}"),
            CrawlerConfig {
                respect_robots_txt: true,
                ..config()
            },
        )
        .unwrap();

        crawler.crawl().await.unwrap();
    }
}
