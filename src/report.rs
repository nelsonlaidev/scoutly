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
    use time::OffsetDateTime;
    use url::Url;

    use super::{analyze_image, analyze_link, analyze_page, build_report, reachable_fields};
    use crate::checker::{CheckedImage, CheckedLink};
    use crate::crawler::CrawledPage;
    use crate::html::{
        ImageReference, ImageReferenceAttribute, ImageReferenceElement, LinkElement, ParsedImage,
        ParsedLink, ParsedPage,
    };
    use crate::resource::ResourceResult;
    use crate::{
        FailureReason, Headings, Image, ImageResult, IssueCode, IssueTarget, Link, LinkResult,
        OpenGraph, Options, ResultKind, TargetType,
    };

    fn page_target() -> IssueTarget {
        IssueTarget {
            target_type: TargetType::Page,
            url: "https://example.com/page".to_owned(),
        }
    }

    fn link(kind: ResultKind, status_code: Option<u16>, final_url: Option<&str>) -> Link {
        Link {
            url: "https://example.com/original".to_owned(),
            result: LinkResult {
                kind,
                status_code,
                final_url: final_url.map(str::to_owned),
                reason: Some(FailureReason::RequestTimedOut),
            },
            found_on: Vec::new(),
        }
    }

    fn image(
        kind: ResultKind,
        status_code: Option<u16>,
        final_url: Option<&str>,
        content_type: Option<&str>,
    ) -> Image {
        Image {
            url: "https://example.com/image.png".to_owned(),
            result: ImageResult {
                kind,
                status_code,
                final_url: final_url.map(str::to_owned),
                content_type: content_type.map(str::to_owned),
                reason: Some(FailureReason::ConnectionFailed),
            },
            found_on: Vec::new(),
        }
    }

    #[test]
    fn page_analysis_constructs_every_issue_with_the_real_target() {
        let target = page_target();

        let issues = analyze_page(&ParsedPage::default(), target.clone());

        assert!(!issues.is_empty());
        assert!(issues.iter().all(|issue| issue.target == target));
    }

    #[test]
    fn page_analysis_covers_length_heading_alt_content_and_open_graph_boundaries() {
        let healthy = ParsedPage {
            title: Some("t".repeat(50)),
            description: Some("d".repeat(160)),
            headings: Headings {
                h1: vec!["Primary".to_owned()],
            },
            links: (0..3)
                .map(|index| ParsedLink {
                    element: LinkElement::Anchor,
                    url: Url::parse(&format!("https://example.com/{index}")).unwrap(),
                    original_url: format!("/{index}"),
                    text: format!("Link {index}"),
                    is_external: false,
                })
                .collect(),
            images: vec![ParsedImage {
                src: Url::parse("https://example.com/image.png").unwrap(),
                alt: Some("Image".to_owned()),
            }],
            image_alt_texts: vec![Some("Image".to_owned())],
            image_references: Vec::new(),
            open_graph: OpenGraph {
                title: Some("Title".to_owned()),
                description: Some("Description".to_owned()),
                image: Some("https://example.com/image.png".to_owned()),
                url: Some("https://example.com/page".to_owned()),
                object_type: Some("website".to_owned()),
                ..OpenGraph::default()
            },
        };

        assert!(analyze_page(&healthy, page_target()).is_empty());

        let mut unhealthy = healthy;
        unhealthy.title = Some("t".repeat(61));
        unhealthy.description = Some("d".repeat(149));
        unhealthy.headings.h1.push("Secondary".to_owned());
        unhealthy.image_alt_texts = vec![None, None];
        unhealthy.links.clear();
        unhealthy.images.clear();
        unhealthy.open_graph = OpenGraph {
            title: Some("  ".to_owned()),
            ..OpenGraph::default()
        };

        let issues = analyze_page(&unhealthy, page_target());
        let codes = issues.iter().map(|issue| issue.code).collect::<Vec<_>>();

        for code in [
            IssueCode::TitleTooLong,
            IssueCode::MetaDescriptionTooShort,
            IssueCode::MultipleH1,
            IssueCode::MissingImageAlt,
            IssueCode::ThinContent,
            IssueCode::MissingOgTitle,
            IssueCode::MissingOgDescription,
            IssueCode::MissingOgImage,
            IssueCode::MissingOgUrl,
            IssueCode::MissingOgType,
        ] {
            assert!(codes.contains(&code), "missing issue {code:?}");
        }
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("2 image(s)"))
        );
    }

    #[test]
    fn page_analysis_treats_blank_metadata_as_missing_and_accepts_upper_boundaries() {
        let blank = ParsedPage {
            title: Some(" \n ".to_owned()),
            description: Some(String::new()),
            ..ParsedPage::default()
        };
        let blank_codes = analyze_page(&blank, page_target())
            .into_iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>();

        assert!(blank_codes.contains(&IssueCode::MissingTitle));
        assert!(blank_codes.contains(&IssueCode::MissingMetaDescription));

        let boundaries = ParsedPage {
            title: Some("t".repeat(60)),
            description: Some("d".repeat(150)),
            headings: Headings {
                h1: vec!["Primary".to_owned()],
            },
            links: (0..4)
                .map(|index| ParsedLink {
                    element: LinkElement::Anchor,
                    url: Url::parse(&format!("https://example.com/{index}")).unwrap(),
                    original_url: format!("/{index}"),
                    text: String::new(),
                    is_external: false,
                })
                .collect(),
            open_graph: OpenGraph {
                title: Some("title".to_owned()),
                description: Some("description".to_owned()),
                image: Some("image".to_owned()),
                url: Some("url".to_owned()),
                object_type: Some("website".to_owned()),
                ..OpenGraph::default()
            },
            ..ParsedPage::default()
        };

        assert!(analyze_page(&boundaries, page_target()).is_empty());
    }

    #[test]
    fn link_analysis_handles_every_result_kind() {
        let failed = analyze_link(&link(ResultKind::Failed, None, None));
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].code, IssueCode::BrokenLink);
        assert!(failed[0].message.contains("request timed out"));

        assert!(analyze_link(&link(ResultKind::Skipped, None, None)).is_empty());
        assert!(analyze_link(&link(ResultKind::Invalid, None, None)).is_empty());

        let blocked = analyze_link(&link(
            ResultKind::Blocked,
            Some(403),
            Some("https://example.com/challenge"),
        ));
        assert_eq!(
            blocked.iter().map(|issue| issue.code).collect::<Vec<_>>(),
            vec![IssueCode::LinkCheckBlocked, IssueCode::Redirect]
        );

        let broken_redirect = analyze_link(&link(
            ResultKind::Response,
            Some(404),
            Some("https://example.com/missing"),
        ));
        assert_eq!(
            broken_redirect
                .iter()
                .map(|issue| issue.code)
                .collect::<Vec<_>>(),
            vec![IssueCode::Redirect, IssueCode::BrokenLink]
        );

        assert!(
            analyze_link(&link(
                ResultKind::Response,
                Some(200),
                Some("https://example.com/original")
            ))
            .is_empty()
        );
    }

    #[test]
    fn image_analysis_handles_every_result_kind_and_combined_outcomes() {
        let invalid_url = analyze_image(&image(ResultKind::Invalid, None, None, None));
        assert_eq!(invalid_url[0].code, IssueCode::InvalidImageUrl);

        let failed = analyze_image(&image(ResultKind::Failed, None, None, None));
        assert_eq!(failed[0].code, IssueCode::BrokenImage);
        assert!(failed[0].message.contains("connection failed"));

        assert!(analyze_image(&image(ResultKind::Skipped, None, None, None)).is_empty());

        let blocked = analyze_image(&image(
            ResultKind::Blocked,
            Some(403),
            Some("https://example.com/protected.png"),
            Some("image/png"),
        ));
        assert_eq!(
            blocked.iter().map(|issue| issue.code).collect::<Vec<_>>(),
            vec![IssueCode::ImageCheckBlocked, IssueCode::ImageRedirect]
        );

        let broken = analyze_image(&image(
            ResultKind::Response,
            Some(500),
            Some("https://example.com/image.png"),
            Some("text/html"),
        ));
        assert_eq!(
            broken.iter().map(|issue| issue.code).collect::<Vec<_>>(),
            vec![IssueCode::BrokenImage]
        );

        let invalid_content = analyze_image(&image(
            ResultKind::Response,
            Some(200),
            Some("https://example.com/final.png"),
            None,
        ));
        assert_eq!(
            invalid_content
                .iter()
                .map(|issue| issue.code)
                .collect::<Vec<_>>(),
            vec![IssueCode::InvalidImageContentType, IssueCode::ImageRedirect]
        );

        assert!(
            analyze_image(&image(
                ResultKind::Response,
                Some(200),
                Some("https://example.com/image.png"),
                Some("image/png")
            ))
            .is_empty()
        );
    }

    #[test]
    fn report_building_preserves_occurrences_and_classifies_page_results() {
        let page_url = Url::parse("https://example.com/page").unwrap();
        let link_url = Url::parse("https://example.com/next").unwrap();
        let image_url = Url::parse("https://example.com/image.png").unwrap();
        let parsed = ParsedPage {
            title: Some("t".repeat(50)),
            description: Some("d".repeat(150)),
            headings: Headings {
                h1: vec!["Primary".to_owned()],
            },
            links: vec![ParsedLink {
                element: LinkElement::Anchor,
                url: link_url.clone(),
                original_url: "/next".to_owned(),
                text: "Next".to_owned(),
                is_external: false,
            }],
            images: vec![ParsedImage {
                src: image_url.clone(),
                alt: Some("Image".to_owned()),
            }],
            image_alt_texts: vec![Some("Image".to_owned())],
            image_references: vec![ImageReference {
                url: Some(image_url.clone()),
                original_url: "/image.png".to_owned(),
                element: ImageReferenceElement::Image,
                attribute: ImageReferenceAttribute::Src,
                descriptor: None,
                alt: Some("Image".to_owned()),
            }],
            open_graph: OpenGraph::default(),
        };
        let crawled = vec![
            CrawledPage {
                outside_scope: false,
                page: parsed,
                url: page_url.clone(),
                final_url: page_url.clone(),
                depth: 1,
                status_code: Some(200),
                content_type: Some("text/html".to_owned()),
            },
            CrawledPage {
                outside_scope: false,
                page: ParsedPage::default(),
                url: Url::parse("https://example.com/failed").unwrap(),
                final_url: Url::parse("https://example.com/failed").unwrap(),
                depth: 0,
                status_code: None,
                content_type: None,
            },
            CrawledPage {
                outside_scope: false,
                page: ParsedPage::default(),
                url: Url::parse("https://example.com/server-error").unwrap(),
                final_url: Url::parse("https://example.com/server-error").unwrap(),
                depth: 0,
                status_code: Some(503),
                content_type: Some("text/html".to_owned()),
            },
            CrawledPage {
                outside_scope: false,
                page: ParsedPage::default(),
                url: Url::parse("https://example.com/file.pdf").unwrap(),
                final_url: Url::parse("https://example.com/file.pdf").unwrap(),
                depth: 0,
                status_code: Some(200),
                content_type: Some("application/pdf".to_owned()),
            },
            CrawledPage {
                outside_scope: true,
                page: ParsedPage::default(),
                url: Url::parse("https://other.example/page").unwrap(),
                final_url: Url::parse("https://other.example/page").unwrap(),
                depth: 0,
                status_code: Some(200),
                content_type: Some("text/html".to_owned()),
            },
        ];
        let links = vec![CheckedLink {
            url: link_url.to_string(),
            result: ResourceResult {
                kind: ResultKind::Response,
                status_code: Some(200),
                final_url: None,
                content_type: Some("text/html".to_owned()),
                reason: None,
            },
        }];
        let images = vec![CheckedImage {
            key: image_url.to_string(),
            url: image_url.to_string(),
            result: ResourceResult {
                kind: ResultKind::Response,
                status_code: Some(200),
                final_url: None,
                content_type: Some("image/png".to_owned()),
                reason: None,
            },
        }];

        let report = build_report(
            &page_url,
            &crawled,
            links,
            images,
            true,
            &Options::default(),
            OffsetDateTime::UNIX_EPOCH,
        );

        assert_eq!(report.pages.len(), 3);
        assert_eq!(report.links[0].found_on[0].original_url, "/next");
        assert_eq!(report.images[0].found_on[0].attribute, "src");
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == IssueCode::PageCrawlFailed)
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == IssueCode::PageHttpError)
        );
    }

    #[test]
    fn report_building_skips_image_analysis_when_disabled() {
        let page_url = Url::parse("https://example.com/").unwrap();
        let report = build_report(
            &page_url,
            &[],
            Vec::new(),
            vec![CheckedImage {
                key: "invalid:image".to_owned(),
                url: "image".to_owned(),
                result: ResourceResult {
                    kind: ResultKind::Invalid,
                    status_code: None,
                    final_url: None,
                    content_type: None,
                    reason: Some(FailureReason::InvalidUrl),
                },
            }],
            false,
            &Options::default(),
            OffsetDateTime::UNIX_EPOCH,
        );

        assert_eq!(report.images.len(), 1);
        assert!(
            !report
                .issues
                .iter()
                .any(|issue| issue.code == IssueCode::InvalidImageUrl)
        );
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

        let blocked_without_redirect = ResourceResult {
            kind: ResultKind::Blocked,
            status_code: Some(403),
            final_url: None,
            content_type: None,
            reason: Some(FailureReason::AntiBotChallenge),
        };
        assert_eq!(
            reachable_fields(&blocked_without_redirect, "https://example.com/original"),
            (Some(403), Some("https://example.com/original".to_owned()))
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
