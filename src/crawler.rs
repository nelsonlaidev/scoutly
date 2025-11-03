use crate::http_client::build_http_client;
use crate::models::{Image, Link, PageInfo};
use anyhow::{Context, Result};
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet, VecDeque};
use url::Url;

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
}

impl Crawler {
    pub fn new(
        start_url: &str,
        max_depth: usize,
        max_pages: usize,
        follow_external: bool,
        keep_fragments: bool,
    ) -> Result<Self> {
        let base_url = Url::parse(start_url).context("Invalid URL")?;
        let mut to_visit = VecDeque::new();
        to_visit.push_back((start_url.to_string(), 0));

        Ok(Self {
            client: build_http_client(30)?,
            base_url,
            max_depth,
            max_pages,
            follow_external,
            keep_fragments,
            visited: HashSet::new(),
            to_visit,
            pages: HashMap::new(),
        })
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

    pub async fn crawl(&mut self) -> Result<()> {
        while let Some((url, depth)) = self.to_visit.pop_front() {
            if self.visited.len() >= self.max_pages {
                break;
            }

            let normalized_url = self.normalize_url(&url);

            if self.visited.contains(&normalized_url) || depth > self.max_depth {
                continue;
            }

            self.visited.insert(normalized_url.clone());

            match self.fetch_page(&url, depth).await {
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

                    self.pages.insert(normalized_url.clone(), page_info);
                }
                Err(e) => {
                    eprintln!("Error crawling {}: {}", url, e);
                    // Still insert a minimal page info for failed pages
                    self.pages.insert(
                        normalized_url.clone(),
                        PageInfo {
                            url: url.clone(),
                            status_code: None,
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

        Ok(())
    }

    async fn fetch_page(&self, url: &str, depth: usize) -> Result<PageInfo> {
        let response = self.client.get(url).send().await?;
        let status_code = response.status().as_u16();
        let html_content = response.text().await?;
        let document = Html::parse_document(&html_content);

        // Extract title
        let title = Self::extract_title(&document);

        // Extract meta description
        let meta_description = Self::extract_meta_description(&document);

        // Extract H1 tags
        let h1_tags = Self::extract_h1_tags(&document);

        // Extract links
        let links = self.extract_links(&document, url)?;

        // Extract images
        let images = self.extract_images(&document, url)?;

        Ok(PageInfo {
            url: url.to_string(),
            status_code: Some(status_code),
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
        let selector = Selector::parse("title").ok()?;
        document
            .select(&selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
    }

    fn extract_meta_description(document: &Html) -> Option<String> {
        let selector = Selector::parse("meta[name='description']").ok()?;
        document
            .select(&selector)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.to_string())
    }

    fn extract_h1_tags(document: &Html) -> Vec<String> {
        let selector = Selector::parse("h1").unwrap();
        document
            .select(&selector)
            .map(|el| el.text().collect::<String>().trim().to_string())
            .collect()
    }

    fn extract_links(&self, document: &Html, page_url: &str) -> Result<Vec<Link>> {
        let page_url_parsed = Url::parse(page_url)?;
        let mut links = Vec::new();

        // Extract from <a href> tags
        let a_selector = Selector::parse("a[href]").unwrap();
        for element in document.select(&a_selector) {
            if let Some(href) = element.value().attr("href") {
                if let Ok(absolute_url) = page_url_parsed.join(href) {
                    let url_str = absolute_url.to_string();
                    let is_external = absolute_url.host_str() != self.base_url.host_str();
                    let text = element.text().collect::<String>().trim().to_string();

                    links.push(Link {
                        url: url_str,
                        text,
                        is_external,
                        status_code: None,
                        redirected_url: None,
                    });
                }
            }
        }

        // Extract from <iframe src> tags
        let iframe_selector = Selector::parse("iframe[src]").unwrap();
        for element in document.select(&iframe_selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(absolute_url) = page_url_parsed.join(src) {
                    let url_str = absolute_url.to_string();
                    let is_external = absolute_url.host_str() != self.base_url.host_str();
                    let title = element.value().attr("title").unwrap_or("").to_string();

                    links.push(Link {
                        url: url_str,
                        text: format!("[iframe] {}", title),
                        is_external,
                        status_code: None,
                        redirected_url: None,
                    });
                }
            }
        }

        // Extract from <video src> and <source src> tags
        let video_selector = Selector::parse("video[src]").unwrap();
        for element in document.select(&video_selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(absolute_url) = page_url_parsed.join(src) {
                    let url_str = absolute_url.to_string();
                    let is_external = absolute_url.host_str() != self.base_url.host_str();

                    links.push(Link {
                        url: url_str,
                        text: "[video]".to_string(),
                        is_external,
                        status_code: None,
                        redirected_url: None,
                    });
                }
            }
        }

        let source_selector = Selector::parse("source[src]").unwrap();
        for element in document.select(&source_selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(absolute_url) = page_url_parsed.join(src) {
                    let url_str = absolute_url.to_string();
                    let is_external = absolute_url.host_str() != self.base_url.host_str();
                    let media_type = element.value().attr("type").unwrap_or("").to_string();

                    links.push(Link {
                        url: url_str,
                        text: format!("[source type={}]", media_type),
                        is_external,
                        status_code: None,
                        redirected_url: None,
                    });
                }
            }
        }

        // Extract from <audio src> tags
        let audio_selector = Selector::parse("audio[src]").unwrap();
        for element in document.select(&audio_selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(absolute_url) = page_url_parsed.join(src) {
                    let url_str = absolute_url.to_string();
                    let is_external = absolute_url.host_str() != self.base_url.host_str();

                    links.push(Link {
                        url: url_str,
                        text: "[audio]".to_string(),
                        is_external,
                        status_code: None,
                        redirected_url: None,
                    });
                }
            }
        }

        // Extract from <embed src> tags
        let embed_selector = Selector::parse("embed[src]").unwrap();
        for element in document.select(&embed_selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(absolute_url) = page_url_parsed.join(src) {
                    let url_str = absolute_url.to_string();
                    let is_external = absolute_url.host_str() != self.base_url.host_str();

                    links.push(Link {
                        url: url_str,
                        text: "[embed]".to_string(),
                        is_external,
                        status_code: None,
                        redirected_url: None,
                    });
                }
            }
        }

        // Extract from <object data> tags
        let object_selector = Selector::parse("object[data]").unwrap();
        for element in document.select(&object_selector) {
            if let Some(data) = element.value().attr("data") {
                if let Ok(absolute_url) = page_url_parsed.join(data) {
                    let url_str = absolute_url.to_string();
                    let is_external = absolute_url.host_str() != self.base_url.host_str();

                    links.push(Link {
                        url: url_str,
                        text: "[object]".to_string(),
                        is_external,
                        status_code: None,
                        redirected_url: None,
                    });
                }
            }
        }

        Ok(links)
    }

    fn extract_images(&self, document: &Html, page_url: &str) -> Result<Vec<Image>> {
        let selector = Selector::parse("img[src]").unwrap();
        let page_url_parsed = Url::parse(page_url)?;
        let mut images = Vec::new();

        for element in document.select(&selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(absolute_url) = page_url_parsed.join(src) {
                    let alt = element.value().attr("alt").map(|s| s.to_string());
                    images.push(Image {
                        src: absolute_url.to_string(),
                        alt,
                    });
                }
            }
        }

        Ok(images)
    }
}
