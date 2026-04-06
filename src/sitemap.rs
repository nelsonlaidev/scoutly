use crate::http_client::build_http_client;
use crate::models::{PageInfo, SitemapEntry};
use crate::robots::RobotsTxt;
use anyhow::Result;
use scraper::{ElementRef, Html, Selector};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::LazyLock;
use url::Url;

static URL_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("url").expect("url selector should be valid"));
static SITEMAP_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("sitemap").expect("sitemap selector should be valid"));
static LOC_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("loc").expect("loc selector should be valid"));
static PRIORITY_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("priority").expect("priority selector should be valid"));
static CHANGEFREQ_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("changefreq").expect("changefreq selector should be valid"));

pub async fn collect_sitemap_entries(
    start_url: &str,
    pages: &HashMap<String, PageInfo>,
    keep_fragments: bool,
) -> Result<Vec<SitemapEntry>> {
    let client = build_http_client(30)?;
    let base_url = Url::parse(start_url)?;
    let mut robots = RobotsTxt::new();

    let _ = robots.fetch(&client, &base_url).await;

    let mut sitemap_queue = VecDeque::from(discover_sitemap_urls(&base_url, &robots));
    let mut visited_sitemaps = HashSet::new();
    let mut seen_entry_urls = HashSet::new();
    let page_titles = page_title_lookup(pages, keep_fragments);
    let mut entries = Vec::new();

    while let Some(sitemap_url) = sitemap_queue.pop_front() {
        let sitemap_key = canonical_url_key(&sitemap_url, keep_fragments);
        if !visited_sitemaps.insert(sitemap_key) {
            continue;
        }

        let Some(document) = fetch_document(&client, &sitemap_url).await? else {
            continue;
        };

        for nested_sitemap in document.nested_sitemaps {
            sitemap_queue.push_back(nested_sitemap);
        }

        for url in document.urls {
            let entry_key = canonical_url_key(&url.loc, keep_fragments);
            if !seen_entry_urls.insert(entry_key) {
                continue;
            }

            let title = lookup_title(&page_titles, &url.loc, keep_fragments);
            entries.push(SitemapEntry {
                url: url.loc,
                title,
                priority: url.priority,
                change_frequency: url.change_frequency,
            });
        }
    }

    Ok(entries)
}

#[derive(Debug, Default)]
struct ParsedSitemapDocument {
    urls: Vec<ParsedUrlEntry>,
    nested_sitemaps: Vec<String>,
}

#[derive(Debug)]
struct ParsedUrlEntry {
    loc: String,
    priority: Option<String>,
    change_frequency: Option<String>,
}

fn discover_sitemap_urls(base_url: &Url, robots: &RobotsTxt) -> Vec<String> {
    let mut urls = robots.sitemap_urls(base_url);
    if urls.is_empty()
        && let Ok(default_sitemap) = base_url.join("/sitemap.xml")
    {
        urls.push(default_sitemap.to_string());
    }
    urls
}

async fn fetch_document(
    client: &reqwest::Client,
    sitemap_url: &str,
) -> Result<Option<ParsedSitemapDocument>> {
    let response = match client.get(sitemap_url).send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(error = %error, sitemap = %sitemap_url, "Failed to fetch sitemap");
            return Ok(None);
        }
    };

    if !response.status().is_success() {
        tracing::info!(status = %response.status(), sitemap = %sitemap_url, "Skipping unavailable sitemap");
        return Ok(None);
    }

    let content = response.text().await?;
    Ok(Some(parse_sitemap_document(sitemap_url, &content)))
}

fn parse_sitemap_document(base_sitemap_url: &str, content: &str) -> ParsedSitemapDocument {
    let document = Html::parse_document(content);
    let mut parsed = ParsedSitemapDocument::default();

    for sitemap in document.select(&SITEMAP_SELECTOR) {
        if let Some(loc) = selected_text(&sitemap, &LOC_SELECTOR) {
            let resolved_loc = match resolve_url(base_sitemap_url, &loc) {
                Some(resolved) => resolved,
                None => {
                    tracing::warn!(loc = %loc, "Failed to resolve relative URL in sitemap");
                    loc
                }
            };
            parsed.nested_sitemaps.push(resolved_loc);
        }
    }

    for url in document.select(&URL_SELECTOR) {
        let Some(loc) = selected_text(&url, &LOC_SELECTOR) else {
            continue;
        };

        let resolved_loc = match resolve_url(base_sitemap_url, &loc) {
            Some(resolved) => resolved,
            None => {
                tracing::warn!(loc = %loc, "Failed to resolve relative URL in sitemap");
                loc
            }
        };

        parsed.urls.push(ParsedUrlEntry {
            loc: resolved_loc,
            priority: selected_text(&url, &PRIORITY_SELECTOR),
            change_frequency: selected_text(&url, &CHANGEFREQ_SELECTOR),
        });
    }

    parsed
}

fn selected_text(element: &ElementRef<'_>, selector: &Selector) -> Option<String> {
    element
        .select(selector)
        .next()
        .map(|node| node.text().collect::<String>().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_url(base: &str, value: &str) -> Option<String> {
    let base = Url::parse(base).ok()?;
    base.join(value).ok().map(|url| url.to_string())
}

fn lookup_title(
    page_titles: &HashMap<String, String>,
    sitemap_url: &str,
    keep_fragments: bool,
) -> String {
    for candidate in url_lookup_variants(sitemap_url, keep_fragments) {
        if let Some(title) = page_titles.get(&candidate) {
            return title.clone();
        }
    }

    PageInfo::title_fallback_from_url(sitemap_url).unwrap_or_else(|| "(untitled)".to_string())
}

fn page_title_lookup(
    pages: &HashMap<String, PageInfo>,
    keep_fragments: bool,
) -> HashMap<String, String> {
    let mut lookup = HashMap::new();

    for page in pages.values() {
        let title = page.display_title();
        for candidate in url_lookup_variants(&page.url, keep_fragments) {
            lookup.entry(candidate).or_insert_with(|| title.clone());
        }
    }

    lookup
}

fn canonical_url_key(url: &str, keep_fragments: bool) -> String {
    url_lookup_variants(url, keep_fragments)
        .into_iter()
        .next()
        .unwrap_or_else(|| url.to_string())
}

fn url_lookup_variants(url: &str, keep_fragments: bool) -> Vec<String> {
    let base = if keep_fragments {
        url.to_string()
    } else {
        url.split('#').next().unwrap_or(url).to_string()
    };

    let mut variants = vec![base.clone()];

    if let Ok(parsed) = Url::parse(&base)
        && parsed.path() == "/"
        && let Some(without_root_slash) = origin_with_optional_suffix(&parsed, false)
    {
        push_unique(&mut variants, without_root_slash);
    }

    variants
}

fn origin_with_optional_suffix(url: &Url, include_root_slash: bool) -> Option<String> {
    let mut value = format!("{}://{}", url.scheme(), url.host_str()?);
    if let Some(port) = url.port() {
        value.push_str(&format!(":{port}"));
    }

    if include_root_slash {
        value.push('/');
    }

    if let Some(query) = url.query() {
        value.push('?');
        value.push_str(query);
    }

    Some(value)
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Image, Link, OpenGraphTags, SeoIssue};

    fn page(url: &str, title: Option<&str>) -> PageInfo {
        PageInfo {
            url: url.to_string(),
            status_code: Some(200),
            content_type: Some("text/html".to_string()),
            title: title.map(str::to_string),
            meta_description: None,
            h1_tags: vec![],
            links: Vec::<Link>::new(),
            images: Vec::<Image>::new(),
            open_graph: OpenGraphTags::default(),
            issues: Vec::<SeoIssue>::new(),
            crawl_depth: 0,
        }
    }

    #[test]
    fn parse_sitemap_document_extracts_url_entries() {
        let parsed = parse_sitemap_document(
            "https://example.com/sitemap.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
              <url>
                <loc>https://example.com/about</loc>
                <priority>0.8</priority>
                <changefreq>weekly</changefreq>
              </url>
            </urlset>"#,
        );

        assert_eq!(parsed.urls.len(), 1);
        assert_eq!(parsed.urls[0].loc, "https://example.com/about");
        assert_eq!(parsed.urls[0].priority.as_deref(), Some("0.8"));
        assert_eq!(parsed.urls[0].change_frequency.as_deref(), Some("weekly"));
    }

    #[test]
    fn parse_sitemap_document_extracts_nested_sitemaps() {
        let parsed = parse_sitemap_document(
            "https://example.com/sitemap.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
              <sitemap>
                <loc>/posts.xml</loc>
              </sitemap>
            </sitemapindex>"#,
        );

        assert_eq!(parsed.urls.len(), 0);
        assert_eq!(
            parsed.nested_sitemaps,
            vec!["https://example.com/posts.xml"]
        );
    }

    #[test]
    fn lookup_title_matches_root_url_variants() {
        let lookup = page_title_lookup(
            &HashMap::from([(
                String::from("https://example.com"),
                page("https://example.com", Some("Home")),
            )]),
            false,
        );

        assert_eq!(
            lookup_title(&lookup, "https://example.com/", false),
            "Home".to_string()
        );
    }

    #[test]
    fn parse_sitemap_document_keeps_unresolved_locations() {
        let parsed = parse_sitemap_document(
            "not-a-valid-base",
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
              <sitemap><loc>/posts.xml</loc></sitemap>
            </sitemapindex>
            <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
              <url><loc>/about</loc></url>
            </urlset>"#,
        );

        assert_eq!(parsed.nested_sitemaps, vec!["/posts.xml"]);
        assert_eq!(parsed.urls[0].loc, "/about");
    }

    #[test]
    fn lookup_variants_cover_root_slash_query_and_fragment_modes() {
        assert_eq!(
            url_lookup_variants("https://example.com/", false),
            vec![
                "https://example.com/".to_string(),
                "https://example.com".to_string(),
            ]
        );
        assert_eq!(
            url_lookup_variants("https://example.com", false),
            vec!["https://example.com".to_string()]
        );
        assert_eq!(
            url_lookup_variants("https://example.com?a=1", false),
            vec!["https://example.com?a=1".to_string()]
        );
        assert_eq!(
            canonical_url_key("https://example.com/about#team", false),
            "https://example.com/about".to_string()
        );
        assert_eq!(
            canonical_url_key("https://example.com/about#team", true),
            "https://example.com/about#team".to_string()
        );
    }

    #[test]
    fn discover_default_sitemap_and_title_fallback_paths() {
        let robots = RobotsTxt::new();
        let base = Url::parse("https://example.com").unwrap();
        assert_eq!(
            discover_sitemap_urls(&base, &robots),
            vec!["https://example.com/sitemap.xml".to_string()]
        );

        let title = lookup_title(&HashMap::new(), "https://example.com/blog/post", false);
        assert!(title.contains("post"));
    }

    #[test]
    fn parse_sitemap_document_skips_url_entries_without_loc() {
        let parsed = parse_sitemap_document(
            "https://example.com/sitemap.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
              <url><priority>0.7</priority></url>
            </urlset>"#,
        );

        assert!(parsed.urls.is_empty());
    }

    #[test]
    fn origin_with_optional_suffix_can_add_root_slash_and_query() {
        let url = Url::parse("https://example.com?lang=en").unwrap();
        assert_eq!(
            origin_with_optional_suffix(&url, true),
            Some("https://example.com/?lang=en".to_string())
        );
    }
}
