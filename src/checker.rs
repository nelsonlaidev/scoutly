use std::collections::HashSet;
use std::sync::Arc;

use tokio::task::JoinError;
use url::Url;

use crate::concurrent::run_ordered;
use crate::crawler::CrawledPage;
use crate::html::ImageReference;
use crate::resource::{ResourceChecker, ResourceResult};
use crate::url_compat::normalize_url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedLink {
    pub(crate) url: String,
    pub(crate) result: ResourceResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedImage {
    pub(crate) key: String,
    pub(crate) url: String,
    pub(crate) result: ResourceResult,
}

#[derive(Debug)]
pub(crate) struct ImageInput {
    key: String,
    display_url: String,
    url: Option<Url>,
}

pub(crate) fn collect_link_urls(pages: &[CrawledPage]) -> Vec<Url> {
    let mut seen = HashSet::new();
    let mut urls = Vec::new();

    for crawled in pages {
        for link in &crawled.page.links {
            let request_url = normalize_url(&link.url, false);

            if seen.insert(request_url.to_string()) {
                urls.push(request_url);
            }
        }
    }

    urls
}

pub(crate) async fn check_links(
    urls: Vec<Url>,
    checker: Arc<ResourceChecker>,
    concurrency: usize,
) -> Result<Vec<CheckedLink>, JoinError> {
    run_ordered(urls, concurrency, move |url| {
        let checker = Arc::clone(&checker);
        async move {
            let result = checker.check(Some(&url)).await;
            CheckedLink {
                url: url.to_string(),
                result,
            }
        }
    })
    .await
}

pub(crate) async fn check_images(
    inputs: Vec<ImageInput>,
    checker: Arc<ResourceChecker>,
    concurrency: usize,
) -> Result<Vec<CheckedImage>, JoinError> {
    run_ordered(inputs, concurrency, move |input| {
        let checker = Arc::clone(&checker);
        async move {
            let result = checker.check(input.url.as_ref()).await;
            CheckedImage {
                key: input.key,
                url: input.display_url,
                result,
            }
        }
    })
    .await
}

pub(crate) fn image_reference_key(reference: &ImageReference) -> String {
    reference.url.as_ref().map_or_else(
        || format!("invalid:{}", reference.original_url),
        |url| normalize_url(url, false).to_string(),
    )
}

pub(crate) fn collect_image_inputs(pages: &[CrawledPage]) -> Vec<ImageInput> {
    let mut seen = HashSet::new();
    let mut inputs = Vec::new();

    for crawled in pages {
        for reference in &crawled.page.image_references {
            let url = reference.url.as_ref().map(|url| normalize_url(url, false));
            let key = url.as_ref().map_or_else(
                || format!("invalid:{}", reference.original_url),
                Url::to_string,
            );
            if !seen.insert(key.clone()) {
                continue;
            }
            let display_url = url
                .as_ref()
                .map_or_else(|| reference.original_url.clone(), Url::to_string);
            inputs.push(ImageInput {
                key,
                display_url,
                url,
            });
        }
    }

    inputs
}
