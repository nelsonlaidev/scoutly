use std::collections::HashMap;

use time::OffsetDateTime;

use crate::checker::{CheckedImage, CheckedLink, image_reference_key};
use crate::crawler::CrawledPage;
use crate::html::{ParsedPage, is_html_content_type};
use crate::resource::ResourceResult;
use crate::url_compat::normalize_url;
use crate::{
    FailureReason, Image, ImageOccurrence, ImageResult, Issue, IssueCode, IssueTarget, Link,
    LinkOccurrence, LinkResult, Options, Page, PageImage, Report, ResultKind, Severity, Summary,
    TargetType,
};

pub(crate) fn build_report(
    audited_url: &url::Url,
    crawled_pages: &[CrawledPage],
    checked_links: Vec<CheckedLink>,
    checked_images: Vec<CheckedImage>,
    images_checked: bool,
    options: &Options,
    audited_at: OffsetDateTime,
) -> Report {
    let mut link_occurrences = collect_link_occurrences(crawled_pages);
    let mut image_occurrences = if images_checked {
        collect_image_occurrences(crawled_pages)
    } else {
        HashMap::new()
    };

    let links = checked_links
        .into_iter()
        .map(|checked| {
            let occurrences = link_occurrences.remove(&checked.url).unwrap_or_default();
            public_link(checked, occurrences)
        })
        .collect::<Vec<_>>();
    let images = checked_images
        .into_iter()
        .map(|checked| {
            let occurrences = image_occurrences.remove(&checked.key).unwrap_or_default();
            public_image(checked, occurrences)
        })
        .collect::<Vec<_>>();

    debug_assert!(link_occurrences.is_empty());
    debug_assert!(image_occurrences.is_empty());

    let mut pages = Vec::with_capacity(crawled_pages.len());
    let mut issues = Vec::new();

    for crawled in crawled_pages {
        let content_type = crawled.content_type.as_deref().unwrap_or_default();
        let is_html = !crawled.outside_scope && is_html_content_type(content_type);

        if is_html {
            pages.push(public_page(crawled));
        }

        let target = IssueTarget {
            target_type: TargetType::Page,
            url: crawled.url.to_string(),
        };

        match crawled.status_code {
            None => issues.push(Issue {
                code: IssueCode::PageCrawlFailed,
                severity: Severity::Error,
                message: "Page crawl failed".to_owned(),
                target,
            }),
            Some(status) if status >= 400 => issues.push(Issue {
                code: IssueCode::PageHttpError,
                severity: Severity::Error,
                message: format!("Page returned HTTP {status}"),
                target,
            }),
            Some(_) if is_html => {
                issues.extend(analyze_page(&crawled.page, target));
            }
            Some(_) => {}
        }
    }

    for link in &links {
        issues.extend(analyze_link(link));
    }

    if images_checked {
        for image in &images {
            issues.extend(analyze_image(image));
        }
    }

    let issues = options.apply_rule_policy(issues);
    let summary = Summary::calculate(&pages, &links, &images, &issues);

    Report {
        url: audited_url.to_string(),
        audited_at,
        summary,
        issues,
        pages,
        links,
        images,
    }
}

fn collect_link_occurrences(pages: &[CrawledPage]) -> HashMap<String, Vec<LinkOccurrence>> {
    let mut occurrences: HashMap<String, Vec<LinkOccurrence>> = HashMap::new();

    for crawled in pages {
        for link in &crawled.page.links {
            let key = normalize_url(&link.url, false).to_string();

            occurrences.entry(key).or_default().push(LinkOccurrence {
                page_url: crawled.url.to_string(),
                original_url: link.original_url.clone(),
                element: link.element.as_str().to_owned(),
                text: link.text.clone(),
            });
        }
    }

    occurrences
}

fn collect_image_occurrences(pages: &[CrawledPage]) -> HashMap<String, Vec<ImageOccurrence>> {
    let mut occurrences: HashMap<String, Vec<ImageOccurrence>> = HashMap::new();

    for crawled in pages {
        for reference in &crawled.page.image_references {
            occurrences
                .entry(image_reference_key(reference))
                .or_default()
                .push(ImageOccurrence {
                    page_url: crawled.url.to_string(),
                    original_url: reference.original_url.clone(),
                    element: reference.element.as_str().to_owned(),
                    attribute: reference.attribute.as_str().to_owned(),
                    descriptor: reference.descriptor.clone(),
                    alt: reference.alt.clone(),
                });
        }
    }

    occurrences
}

fn public_page(crawled: &CrawledPage) -> Page {
    Page {
        url: crawled.url.to_string(),
        depth: crawled.depth,
        status_code: crawled.status_code,
        content_type: crawled.content_type.clone(),
        title: crawled.page.title.clone(),
        description: crawled.page.description.clone(),
        headings: crawled.page.headings.clone(),
        images: crawled
            .page
            .images
            .iter()
            .map(|image| PageImage {
                url: image.src.to_string(),
                alt: image.alt.clone(),
            })
            .collect(),
        open_graph: crawled.page.open_graph.clone(),
    }
}

fn public_link(checked: CheckedLink, found_on: Vec<LinkOccurrence>) -> Link {
    let resource = checked.result;
    let (status_code, final_url) = reachable_fields(&resource, &checked.url);

    Link {
        url: checked.url.clone(),
        result: LinkResult {
            kind: resource.kind,
            status_code,
            final_url,
            reason: match resource.kind {
                ResultKind::Blocked | ResultKind::Failed | ResultKind::Skipped => resource.reason,
                ResultKind::Response | ResultKind::Invalid => None,
            },
        },
        found_on,
    }
}

fn public_image(checked: CheckedImage, found_on: Vec<ImageOccurrence>) -> Image {
    let resource = checked.result;
    let successful = matches!(resource.kind, ResultKind::Response | ResultKind::Blocked);
    let (status_code, final_url) = reachable_fields(&resource, &checked.url);

    Image {
        url: checked.url.clone(),
        result: ImageResult {
            kind: resource.kind,
            status_code,
            final_url,
            content_type: if successful {
                resource.content_type
            } else {
                None
            },
            reason: match resource.kind {
                ResultKind::Blocked
                | ResultKind::Failed
                | ResultKind::Skipped
                | ResultKind::Invalid => resource.reason,
                ResultKind::Response => None,
            },
        },
        found_on,
    }
}

fn reachable_fields(
    resource: &ResourceResult,
    fallback_url: &str,
) -> (Option<u16>, Option<String>) {
    if !matches!(resource.kind, ResultKind::Response | ResultKind::Blocked) {
        return (None, None);
    }

    (
        resource.status_code,
        Some(
            resource
                .final_url
                .as_ref()
                .map_or_else(|| fallback_url.to_owned(), url::Url::to_string),
        ),
    )
}

fn analyze_page(page: &ParsedPage, target: IssueTarget) -> Vec<Issue> {
    let mut issues = Vec::new();

    issues.extend(validate_length(
        page.title.as_deref(),
        &target,
        LengthRule {
            min: 50,
            max: 60,
            missing_code: IssueCode::MissingTitle,
            missing_message: "Page is missing a title tag",
            short_code: IssueCode::TitleTooShort,
            short_label: "Title is too short",
            long_code: IssueCode::TitleTooLong,
            long_label: "Title is too long",
        },
    ));
    issues.extend(validate_length(
        page.description.as_deref(),
        &target,
        LengthRule {
            min: 150,
            max: 160,
            missing_code: IssueCode::MissingMetaDescription,
            missing_message: "Page is missing a meta description",
            short_code: IssueCode::MetaDescriptionTooShort,
            short_label: "Meta description is too short",
            long_code: IssueCode::MetaDescriptionTooLong,
            long_label: "Meta description is too long",
        },
    ));

    match page.headings.h1.len() {
        0 => issues.push(issue(
            &target,
            IssueCode::MissingH1,
            Severity::Warning,
            "Page is missing an H1 tag",
        )),
        1 => {}
        count => issues.push(issue(
            &target,
            IssueCode::MultipleH1,
            Severity::Warning,
            format!("Page has multiple H1 tags ({count})"),
        )),
    }

    let missing_alt = page
        .image_alt_texts
        .iter()
        .filter(|alt| alt.is_none())
        .count();
    if missing_alt > 0 {
        issues.push(issue(
            &target,
            IssueCode::MissingImageAlt,
            Severity::Warning,
            format!("{missing_alt} image(s) missing alt text"),
        ));
    }
    if page.headings.h1.len() + page.links.len() + page.images.len() < 5 {
        issues.push(issue(
            &target,
            IssueCode::ThinContent,
            Severity::Warning,
            "Page may have thin content (few elements found)",
        ));
    }

    for (present, code, tag) in [
        (
            page.open_graph
                .title
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            IssueCode::MissingOgTitle,
            "og:title",
        ),
        (
            page.open_graph
                .description
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            IssueCode::MissingOgDescription,
            "og:description",
        ),
        (
            page.open_graph
                .image
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            IssueCode::MissingOgImage,
            "og:image",
        ),
        (
            page.open_graph
                .url
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            IssueCode::MissingOgUrl,
            "og:url",
        ),
        (
            page.open_graph
                .object_type
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            IssueCode::MissingOgType,
            "og:type",
        ),
    ] {
        if !present {
            issues.push(issue(
                &target,
                code,
                Severity::Info,
                format!("Page is missing {tag} tag"),
            ));
        }
    }

    issues
}

#[derive(Clone, Copy)]
struct LengthRule {
    min: usize,
    max: usize,
    missing_code: IssueCode,
    missing_message: &'static str,
    short_code: IssueCode,
    short_label: &'static str,
    long_code: IssueCode,
    long_label: &'static str,
}

fn validate_length(value: Option<&str>, target: &IssueTarget, rule: LengthRule) -> Vec<Issue> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return vec![issue(
            target,
            rule.missing_code,
            Severity::Error,
            rule.missing_message,
        )];
    };

    let length = value.chars().count();

    if length < rule.min {
        vec![issue(
            target,
            rule.short_code,
            Severity::Warning,
            format!(
                "{} ({} chars, recommended: {}-{})",
                rule.short_label, length, rule.min, rule.max
            ),
        )]
    } else if length > rule.max {
        vec![issue(
            target,
            rule.long_code,
            Severity::Warning,
            format!(
                "{} ({} chars, recommended: {}-{})",
                rule.long_label, length, rule.min, rule.max
            ),
        )]
    } else {
        Vec::new()
    }
}

fn analyze_link(link: &Link) -> Vec<Issue> {
    let target = IssueTarget {
        target_type: TargetType::Link,
        url: link.url.clone(),
    };

    match link.result.kind {
        ResultKind::Failed => {
            return vec![Issue {
                code: IssueCode::BrokenLink,
                severity: Severity::Error,
                message: format!(
                    "Link check failed: {} ({})",
                    link.url,
                    reason_words(link.result.reason)
                ),
                target,
            }];
        }
        ResultKind::Skipped => return Vec::new(),
        ResultKind::Response | ResultKind::Blocked | ResultKind::Invalid => {}
    }

    let mut issues = Vec::new();

    if link.result.kind == ResultKind::Blocked {
        issues.push(Issue {
            code: IssueCode::LinkCheckBlocked,
            severity: Severity::Warning,
            message: format!(
                "Link check blocked by anti-bot challenge: {} (HTTP {})",
                link.url,
                link.result.status_code.unwrap_or_default()
            ),
            target: target.clone(),
        });
    }

    if link.is_redirected() {
        issues.push(Issue {
            code: IssueCode::Redirect,
            severity: Severity::Info,
            message: format!(
                "Link redirected: {} -> {}",
                link.url,
                link.result.final_url.as_deref().unwrap_or_default()
            ),
            target: target.clone(),
        });
    }

    if link.is_broken() {
        issues.push(Issue {
            code: IssueCode::BrokenLink,
            severity: Severity::Error,
            message: format!(
                "Broken link: {} (HTTP {})",
                link.url,
                link.result.status_code.unwrap_or_default()
            ),
            target,
        });
    }

    issues
}

fn analyze_image(image: &Image) -> Vec<Issue> {
    let target = IssueTarget {
        target_type: TargetType::Image,
        url: image.url.clone(),
    };

    match image.result.kind {
        ResultKind::Invalid => {
            return vec![Issue {
                code: IssueCode::InvalidImageUrl,
                severity: Severity::Error,
                message: format!("Invalid image URL: {}", image.url),
                target,
            }];
        }
        ResultKind::Failed => {
            return vec![Issue {
                code: IssueCode::BrokenImage,
                severity: Severity::Error,
                message: format!(
                    "Image check failed: {} ({})",
                    image.url,
                    reason_words(image.result.reason)
                ),
                target,
            }];
        }
        ResultKind::Skipped => return Vec::new(),
        ResultKind::Response | ResultKind::Blocked => {}
    }

    let mut issues = Vec::new();

    if image.result.kind == ResultKind::Blocked {
        issues.push(Issue {
            code: IssueCode::ImageCheckBlocked,
            severity: Severity::Warning,
            message: format!(
                "Image check blocked by anti-bot challenge: {} (HTTP {})",
                image.url,
                image.result.status_code.unwrap_or_default()
            ),
            target: target.clone(),
        });
    } else if image.is_broken() {
        issues.push(Issue {
            code: IssueCode::BrokenImage,
            severity: Severity::Error,
            message: format!(
                "Broken image: {} (HTTP {})",
                image.url,
                image.result.status_code.unwrap_or_default()
            ),
            target: target.clone(),
        });
    } else if image.is_invalid() {
        issues.push(Issue {
            code: IssueCode::InvalidImageContentType,
            severity: Severity::Error,
            message: format!(
                "Image has invalid Content-Type: {} ({})",
                image.url,
                image.result.content_type.as_deref().unwrap_or("missing")
            ),
            target: target.clone(),
        });
    }

    if image.is_redirected() {
        issues.push(Issue {
            code: IssueCode::ImageRedirect,
            severity: Severity::Info,
            message: format!(
                "Image redirected: {} -> {}",
                image.url,
                image.result.final_url.as_deref().unwrap_or_default()
            ),
            target,
        });
    }

    issues
}

fn reason_words(reason: Option<FailureReason>) -> String {
    reason.map_or_else(String::new, |reason| reason.as_str().replace('-', " "))
}

fn issue(
    target: &IssueTarget,
    code: IssueCode,
    severity: Severity,
    message: impl Into<String>,
) -> Issue {
    Issue {
        code,
        severity,
        message: message.into(),
        target: target.clone(),
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::{analyze_page, reachable_fields};
    use crate::html::ParsedPage;
    use crate::resource::ResourceResult;
    use crate::{FailureReason, IssueTarget, ResultKind, TargetType};

    #[test]
    fn page_analysis_constructs_every_issue_with_the_real_target() {
        let target = IssueTarget {
            target_type: TargetType::Page,
            url: "https://example.com/page".to_owned(),
        };

        let issues = analyze_page(&ParsedPage::default(), target.clone());

        assert!(!issues.is_empty());
        assert!(issues.iter().all(|issue| issue.target == target));
    }

    #[test]
    fn reachable_fields_are_only_exposed_for_completed_requests() {
        let response = ResourceResult {
            kind: ResultKind::Response,
            status_code: Some(204),
            final_url: Some(Url::parse("https://example.com/final").unwrap()),
            content_type: None,
            reason: None,
        };

        assert_eq!(
            reachable_fields(&response, "https://example.com/original"),
            (Some(204), Some("https://example.com/final".to_owned()))
        );

        let failed = ResourceResult {
            kind: ResultKind::Failed,
            status_code: Some(500),
            final_url: Some(Url::parse("https://example.com/final").unwrap()),
            content_type: None,
            reason: Some(FailureReason::RequestFailed),
        };

        assert_eq!(
            reachable_fields(&failed, "https://example.com/original"),
            (None, None)
        );
    }
}
