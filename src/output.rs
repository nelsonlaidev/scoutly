use std::collections::{HashMap, HashSet};

use scoutly::{Image, Issue, Link, OutputFormat, Report, Severity, TargetType};
use thiserror::Error;
use time::UtcOffset;

#[derive(Debug, Error)]
pub(crate) enum OutputError {
    #[error("encode JSON report: {0}")]
    Json(#[from] serde_json::Error),
}

pub(crate) fn format_report(report: &Report, format: OutputFormat) -> Result<String, OutputError> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(report)?),
        OutputFormat::Text => Ok(format_text_report(report)),
    }
}

fn format_text_report(report: &Report) -> String {
    let summary = &report.summary;

    let mut lines = vec![
        "Scoutly Audit Report".to_owned(),
        format!("URL: {}", report.url),
        format!("Audited: {}", audited_timestamp(report.audited_at)),
        String::new(),
        "Summary".to_owned(),
        format!("Pages: {}", summary.pages),
        format!(
            "Links: {} total, {} checked, {} broken, {} blocked, {} redirected",
            summary.links.total,
            summary.links.checked,
            summary.links.broken,
            summary.links.blocked,
            summary.links.redirected,
        ),
        format!(
            "Images: {} total, {} checked, {} broken, {} blocked, {} invalid, {} redirected",
            summary.images.total,
            summary.images.checked,
            summary.images.broken,
            summary.images.blocked,
            summary.images.invalid,
            summary.images.redirected,
        ),
        format!(
            "Issues: {} total, {}, {}, {} info",
            summary.issues.total,
            format_count(summary.issues.error, "error", "errors"),
            format_count(summary.issues.warning, "warning", "warnings"),
            summary.issues.info,
        ),
    ];

    if report.issues.is_empty() {
        lines.extend([String::new(), "No issues found.".to_owned()]);
        return lines.join("\n");
    }

    let links_by_url = if report
        .issues
        .iter()
        .any(|issue| issue.target.target_type == TargetType::Link)
    {
        report
            .links
            .iter()
            .map(|link| (link.url.as_str(), link))
            .collect::<HashMap<_, _>>()
    } else {
        HashMap::new()
    };

    let images_by_url = if report
        .issues
        .iter()
        .any(|issue| issue.target.target_type == TargetType::Image)
    {
        report
            .images
            .iter()
            .map(|image| (image.url.as_str(), image))
            .collect::<HashMap<_, _>>()
    } else {
        HashMap::new()
    };

    let mut grouped = [Vec::new(), Vec::new(), Vec::new()];

    for issue in &report.issues {
        grouped[match issue.severity {
            Severity::Error => 0,
            Severity::Warning => 1,
            Severity::Info => 2,
        }]
        .push(issue);
    }

    for (issues, heading) in grouped.into_iter().zip(["Errors", "Warnings", "Info"]) {
        if issues.is_empty() {
            continue;
        }

        lines.extend([String::new(), format!("{heading} ({})", issues.len())]);

        for issue in issues {
            append_issue(&mut lines, issue, &links_by_url, &images_by_url);
        }
    }

    lines.join("\n")
}

fn format_count(count: usize, singular: &str, plural: &str) -> String {
    let label = if count == 1 { singular } else { plural };
    format!("{count} {label}")
}

fn append_issue(
    lines: &mut Vec<String>,
    issue: &Issue,
    links_by_url: &HashMap<&str, &Link>,
    images_by_url: &HashMap<&str, &Image>,
) {
    let target_label = match issue.target.target_type {
        TargetType::Page => "Page",
        TargetType::Link => "Link",
        TargetType::Image => "Image",
    };

    lines.extend([
        format!("- {} [{}]", issue.message, issue.code.as_str()),
        format!("  {target_label}: {}", issue.target.url),
    ]);

    match issue.target.target_type {
        TargetType::Link => append_occurrences(
            lines,
            links_by_url
                .get(issue.target.url.as_str())
                .into_iter()
                .flat_map(|link| link.found_on.iter())
                .map(|occurrence| occurrence.page_url.as_str()),
        ),
        TargetType::Image => append_occurrences(
            lines,
            images_by_url
                .get(issue.target.url.as_str())
                .into_iter()
                .flat_map(|image| image.found_on.iter())
                .map(|occurrence| occurrence.page_url.as_str()),
        ),
        TargetType::Page => {}
    }
}

fn append_occurrences<'a>(lines: &mut Vec<String>, page_urls: impl Iterator<Item = &'a str>) {
    let mut seen = HashSet::new();

    for page_url in page_urls {
        if seen.insert(page_url) {
            lines.push(format!("  Found on: {page_url}"));
        }
    }
}

fn audited_timestamp(value: time::OffsetDateTime) -> String {
    let offset = value.offset();
    let suffix = if offset == UtcOffset::UTC {
        "Z".to_owned()
    } else {
        let minutes = offset.whole_minutes();
        let sign = if minutes < 0 { '-' } else { '+' };
        let minutes = minutes.unsigned_abs();
        format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60)
    };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}{suffix}",
        value.year(),
        u8::from(value.month()),
        value.day(),
        value.hour(),
        value.minute(),
        value.second(),
        value.nanosecond() / 1_000_000,
    )
}

#[cfg(test)]
mod tests {
    use scoutly::{
        Headings, Image, ImageOccurrence, ImageSummary, Issue, IssueCode, IssueSummary,
        IssueTarget, Link, LinkOccurrence, LinkSummary, OpenGraph, OutputFormat, Page, Report,
        Severity, Summary, TargetType,
    };
    use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

    use super::{audited_timestamp, format_report, format_text_report};

    fn timestamp() -> time::OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(2026, Month::August, 2).unwrap(),
            Time::from_hms_milli(12, 0, 0, 123).unwrap(),
        )
        .assume_utc()
    }

    #[test]
    fn json_is_pretty_and_does_not_escape_html() {
        let report = Report {
            url: "https://example.com/?q=<value>".into(),
            audited_at: timestamp(),
            summary: Summary::default(),
            issues: Vec::new(),
            pages: Vec::new(),
            links: Vec::new(),
            images: Vec::new(),
        };

        let output = format_report(&report, OutputFormat::Json).unwrap();

        assert!(output.starts_with("{\n  \"url\""));
        assert!(output.contains("<value>"));
        assert!(!output.ends_with('\n'));
    }

    #[test]
    fn text_groups_issues_and_deduplicates_occurrence_pages() {
        let link_url = "https://example.com/broken".to_owned();
        let image_url = "https://example.com/image.png".to_owned();
        let report = Report {
            url: "https://example.com/".into(),
            audited_at: timestamp(),
            summary: Summary {
                pages: 1,
                links: LinkSummary {
                    total: 1,
                    checked: 1,
                    broken: 1,
                    ..LinkSummary::default()
                },
                images: ImageSummary {
                    total: 1,
                    checked: 1,
                    invalid: 1,
                    ..ImageSummary::default()
                },
                issues: IssueSummary {
                    total: 3,
                    error: 1,
                    warning: 1,
                    info: 1,
                },
            },
            issues: vec![
                Issue {
                    code: IssueCode::BrokenLink,
                    severity: Severity::Error,
                    message: "Broken link".into(),
                    target: IssueTarget {
                        target_type: TargetType::Link,
                        url: link_url.clone(),
                    },
                },
                Issue {
                    code: IssueCode::TitleTooShort,
                    severity: Severity::Warning,
                    message: "Short title".into(),
                    target: IssueTarget {
                        target_type: TargetType::Page,
                        url: "https://example.com/".into(),
                    },
                },
                Issue {
                    code: IssueCode::ImageRedirect,
                    severity: Severity::Info,
                    message: "Image redirect".into(),
                    target: IssueTarget {
                        target_type: TargetType::Image,
                        url: image_url.clone(),
                    },
                },
            ],
            pages: vec![Page {
                url: "https://example.com/".into(),
                depth: 0,
                status_code: Some(200),
                content_type: Some("text/html".into()),
                title: None,
                description: None,
                headings: Headings::default(),
                images: Vec::new(),
                open_graph: OpenGraph::default(),
            }],
            links: vec![Link {
                url: link_url,
                result: scoutly::LinkResult {
                    kind: scoutly::ResultKind::Response,
                    status_code: Some(404),
                    final_url: None,
                    reason: None,
                },
                found_on: vec![
                    LinkOccurrence {
                        page_url: "https://example.com/".into(),
                        original_url: "/broken".into(),
                        element: "a".into(),
                        text: String::new(),
                    },
                    LinkOccurrence {
                        page_url: "https://example.com/".into(),
                        original_url: "/broken".into(),
                        element: "a".into(),
                        text: String::new(),
                    },
                ],
            }],
            images: vec![Image {
                url: image_url,
                result: scoutly::ImageResult {
                    kind: scoutly::ResultKind::Response,
                    status_code: Some(200),
                    final_url: None,
                    content_type: Some("image/png".into()),
                    reason: None,
                },
                found_on: vec![ImageOccurrence {
                    page_url: "https://example.com/".into(),
                    original_url: "/image.png".into(),
                    element: "img".into(),
                    attribute: "src".into(),
                    descriptor: None,
                    alt: None,
                }],
            }],
        };

        let output = format_text_report(&report);

        for expected in [
            "Audited: 2026-08-02T12:00:00.123Z",
            "Issues: 3 total, 1 error, 1 warning, 1 info",
            "Errors (1)",
            "Warnings (1)",
            "Info (1)",
        ] {
            assert!(output.contains(expected), "missing {expected:?}:\n{output}");
        }

        assert_eq!(output.matches("Found on: https://example.com/").count(), 2);
    }

    #[test]
    fn timestamp_uses_milliseconds_and_numeric_offsets() {
        let offset = UtcOffset::from_hms(-5, -30, 0).unwrap();

        assert_eq!(
            audited_timestamp(timestamp().to_offset(offset)),
            "2026-08-02T06:30:00.123-05:30"
        );
    }
}
