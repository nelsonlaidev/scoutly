use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use reqwest::header::CONTENT_TYPE;
use url::Url;

use crate::concurrent::run_ordered;
use crate::html::{LinkElement, ParsedPage, is_html_content_type, parse_html};
use crate::progress::{PhaseStart, ProgressReporter};
use crate::robots::{RobotsCache, RobotsRedirectPolicy};
use crate::sitemap::{
    MAX_COMPRESSED_BYTES, ParsedSitemap, SitemapFrontier, SitemapKind, parse_sitemap_document,
};
use crate::transport::{AllowRedirects, BodyMode, GetRequest, Transport, joined_header};
use crate::url_compat::{normalize_url, same_origin};
use crate::{AuditError, Options};

const MAX_HTML_BYTES: usize = 10 * 1024 * 1024;
const NON_HTML_DRAIN_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CrawledPage {
    pub(crate) outside_scope: bool,
    pub(crate) page: ParsedPage,
    pub(crate) url: Url,
    pub(crate) final_url: Url,
    pub(crate) depth: i64,
    pub(crate) status_code: Option<u16>,
    pub(crate) content_type: Option<String>,
}

#[derive(Clone, Debug)]
struct QueuedPage {
    url: Url,
    key: String,
    depth: i64,
}

#[derive(Clone, Debug)]
struct CrawledQueuedPage {
    queued: QueuedPage,
    crawled: CrawledPage,
}

pub(crate) async fn crawl(
    start_url: &Url,
    transport: Transport,
    robots: Arc<RobotsCache>,
    options: Arc<Options>,
    reporter: &mut ProgressReporter,
) -> Result<Vec<CrawledPage>, AuditError> {
    let initial = queued_page(start_url, 0, options.keep_fragments);
    let mut pages = Vec::with_capacity(options.max_pages.min(128));

    if !robots.allows(&initial.url) || !options.allows_page(&initial.url) {
        reporter.start_phase(PhaseStart::Sitemaps).await?;
        return Ok(pages);
    }

    reporter.page_discovered(&initial.url).await?;

    let mut frontier = VecDeque::from([initial.clone()]);
    let mut enqueued = HashSet::from([initial.key.clone()]);
    let mut crawl_scope_url = initial.url.clone();
    let redirect_policy = RobotsRedirectPolicy::new(Arc::clone(&robots), transport.clone());

    while !frontier.is_empty() && pages.len() < options.max_pages {
        let batch_size = frontier.len().min(options.concurrency);
        let batch = frontier.drain(..batch_size).collect();
        let crawled = crawl_batch(batch, &transport, &redirect_policy, &options, reporter).await?;

        for result in &crawled {
            if result.queued.key == initial.key && !options.allows_page(&result.crawled.final_url) {
                return Err(AuditError::StartRedirectOutsideScope {
                    start: start_url.to_string(),
                    destination: result.crawled.final_url.to_string(),
                });
            }

            if result.queued.key == initial.key {
                crawl_scope_url = result.crawled.final_url.clone();
            }
        }

        for result in &crawled {
            if result.queued.depth >= i64::from(options.max_depth) {
                continue;
            }

            for link in &result.crawled.page.links {
                if enqueued.len() >= options.max_pages
                    || !matches!(link.element, LinkElement::Anchor | LinkElement::Iframe)
                    || !matches!(link.url.scheme(), "http" | "https")
                    || link.url.host().is_none()
                    || !same_origin(&link.url, &crawl_scope_url)
                {
                    continue;
                }

                let next = queued_page(&link.url, result.queued.depth + 1, options.keep_fragments);
                if enqueued.contains(&next.key)
                    || !options.allows_page(&next.url)
                    || !robots.allows(&next.url)
                {
                    continue;
                }

                enqueued.insert(next.key.clone());
                reporter.page_discovered(&next.url).await?;
                frontier.push_back(next);
            }
        }

        pages.extend(crawled.into_iter().map(|result| result.crawled));
    }

    reporter.start_phase(PhaseStart::Sitemaps).await?;

    if !options.sitemaps || options.max_depth == 0 || pages.len() >= options.max_pages {
        return Ok(pages);
    }

    let mut initial_sitemaps = robots.sitemap_urls(&crawl_scope_url);

    if initial_sitemaps.is_empty() {
        initial_sitemaps.push(
            origin_path(&crawl_scope_url, "/sitemap.xml")
                .expect("crawl scope is an HTTP URL with a host"),
        );
    }

    let mut allowed_initial = Vec::new();

    for sitemap in initial_sitemaps {
        if robots.allows(&sitemap) {
            allowed_initial.push(sitemap);
        }
    }

    let sitemap_urls = crawl_sitemaps(
        allowed_initial,
        &crawl_scope_url,
        &transport,
        &robots,
        &options,
        &mut enqueued,
        reporter,
        options.max_pages - pages.len(),
    )
    .await?;

    let mut sitemap_queue = VecDeque::with_capacity(sitemap_urls.len());

    for url in sitemap_urls {
        let queued = queued_page(&url, -1, options.keep_fragments);
        reporter.page_discovered(&queued.url).await?;
        sitemap_queue.push_back(queued);
    }

    while !sitemap_queue.is_empty() {
        let batch_size = sitemap_queue.len().min(options.concurrency);
        let batch = sitemap_queue.drain(..batch_size).collect();
        let crawled = crawl_batch(batch, &transport, &redirect_policy, &options, reporter).await?;

        pages.extend(crawled.into_iter().map(|result| result.crawled));
    }

    Ok(pages)
}

async fn crawl_batch(
    batch: Vec<QueuedPage>,
    transport: &Transport,
    redirect_policy: &RobotsRedirectPolicy,
    options: &Arc<Options>,
    reporter: &mut ProgressReporter,
) -> Result<Vec<CrawledQueuedPage>, AuditError> {
    let batch_transport = transport.clone();
    let batch_options = Arc::clone(options);
    let batch_policy = redirect_policy.clone();
    let crawled = reporter
        .with_receiver_open(run_ordered(batch, options.concurrency, move |queued| {
            let transport = batch_transport.clone();
            let options = Arc::clone(&batch_options);
            let policy = batch_policy.clone();

            async move { crawl_page(queued, &transport, &policy, &options).await }
        }))
        .await?
        .map_err(AuditError::WorkerTask)?;

    for result in &crawled {
        reporter.page_crawled(&result.queued.url).await?;
    }

    Ok(crawled)
}

async fn crawl_page(
    queued: QueuedPage,
    transport: &Transport,
    redirect_policy: &RobotsRedirectPolicy,
    options: &Options,
) -> CrawledQueuedPage {
    let mut crawled = CrawledPage {
        outside_scope: false,
        page: ParsedPage::default(),
        url: queued.url.clone(),
        final_url: queued.url.clone(),
        depth: queued.depth,
        status_code: None,
        content_type: None,
    };
    let request = GetRequest::new(
        queued.url.clone(),
        BodyMode::CollectHtml {
            max_bytes: MAX_HTML_BYTES,
            non_html_drain_bytes: NON_HTML_DRAIN_BYTES,
        },
    );

    let Ok(response) = transport.get(request, redirect_policy).await else {
        return CrawledQueuedPage { queued, crawled };
    };

    crawled.final_url = normalize_url(&response.final_url, options.keep_fragments);
    crawled.status_code = Some(response.status.as_u16());
    crawled.content_type = joined_header(&response.headers, CONTENT_TYPE);
    crawled.outside_scope = !options.allows_page(&response.final_url);

    if crawled.outside_scope {
        return CrawledQueuedPage { queued, crawled };
    }

    let content_type = crawled.content_type.clone().unwrap_or_default();

    if is_html_content_type(&content_type) {
        crawled.page = parse_html(&response.body, &response.final_url);
    }

    CrawledQueuedPage { queued, crawled }
}

#[allow(clippy::too_many_arguments)]
async fn crawl_sitemaps(
    initial: Vec<Url>,
    site_url: &Url,
    transport: &Transport,
    robots: &RobotsCache,
    options: &Options,
    excluded: &mut HashSet<String>,
    reporter: &mut ProgressReporter,
    max_pages: usize,
) -> Result<Vec<Url>, AuditError> {
    if max_pages == 0 || options.max_sitemap_documents == 0 {
        return Ok(Vec::new());
    }

    let mut frontier = SitemapFrontier::new(&initial, options.max_sitemap_documents);
    let mut pages = Vec::with_capacity(max_pages.min(256));

    while let Some(document_url) = frontier.next_document() {
        if pages.len() >= max_pages {
            break;
        }

        let fetched = reporter
            .with_receiver_open(transport.get(
                GetRequest::new(
                    document_url.clone(),
                    BodyMode::Collect {
                        max_bytes: MAX_COMPRESSED_BYTES + 1,
                    },
                ),
                &AllowRedirects,
            ))
            .await?;

        reporter.sitemap_fetched(&document_url).await?;

        let Ok(response) = fetched else {
            continue;
        };

        if !response.status.is_success() {
            continue;
        }

        let Ok(parsed) = parse_sitemap_document(&response.body) else {
            continue;
        };

        if parsed.kind == SitemapKind::Index {
            let mut documents = Vec::new();

            for candidate in parsed.urls {
                if robots.allows(&candidate) {
                    documents.push(candidate);
                }
            }

            frontier.extend_from(&ParsedSitemap {
                kind: SitemapKind::Index,
                urls: documents,
            });

            continue;
        }

        for candidate in parsed.urls {
            if pages.len() >= max_pages
                || !same_origin(&candidate, site_url)
                || !options.allows_page(&candidate)
                || !robots.allows(&candidate)
            {
                continue;
            }

            let normalized = normalize_url(&candidate, options.keep_fragments);

            if excluded.insert(normalized.to_string()) {
                pages.push(normalized);
            }
        }
    }

    Ok(pages)
}

fn queued_page(input: &Url, depth: i64, keep_fragments: bool) -> QueuedPage {
    let url = normalize_url(input, keep_fragments);

    QueuedPage {
        key: url.to_string(),
        url,
        depth,
    }
}

fn origin_path(input: &Url, path: &str) -> Option<Url> {
    let mut origin = input.clone();

    origin.set_username("").ok()?;
    origin.set_password(None).ok()?;
    origin.set_path(path);
    origin.set_query(None);
    origin.set_fragment(None);

    Some(origin)
}
