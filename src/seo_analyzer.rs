use crate::models::{IssueSeverity, IssueType, PageInfo, SeoIssue};
use std::collections::HashMap;

struct LengthRule<'a> {
    min_length: usize,
    max_length: usize,
    missing_type: IssueType,
    missing_message: &'a str,
    too_short_type: IssueType,
    too_short_label: &'a str,
    too_long_type: IssueType,
    too_long_label: &'a str,
}

pub struct SeoAnalyzer;

impl SeoAnalyzer {
    pub fn analyze_pages(pages: &mut HashMap<String, PageInfo>) {
        for page in pages.values_mut() {
            // Only analyze SEO for HTML pages
            if let Some(content_type) = &page.content_type
                && content_type.to_lowercase().contains("text/html")
            {
                Self::analyze_page(page);
            }
        }
    }

    fn analyze_page(page: &mut PageInfo) {
        page.issues
            .extend(Self::validate_title(page.title.as_deref()));
        page.issues.extend(Self::validate_meta_description(
            page.meta_description.as_deref(),
        ));
        page.issues.extend(Self::validate_h1_tags(&page.h1_tags));
        page.issues.extend(Self::validate_images(page));
        page.issues.extend(Self::validate_thin_content(page));
        page.issues.extend(Self::validate_open_graph(page));
    }

    fn validate_title(title: Option<&str>) -> Vec<SeoIssue> {
        Self::validate_length(
            title,
            LengthRule {
                min_length: 50,
                max_length: 60,
                missing_type: IssueType::MissingTitle,
                missing_message: "Page is missing a title tag",
                too_short_type: IssueType::TitleTooShort,
                too_short_label: "Title is too short",
                too_long_type: IssueType::TitleTooLong,
                too_long_label: "Title is too long",
            },
        )
    }

    fn validate_meta_description(description: Option<&str>) -> Vec<SeoIssue> {
        Self::validate_length(
            description,
            LengthRule {
                min_length: 150,
                max_length: 160,
                missing_type: IssueType::MissingMetaDescription,
                missing_message: "Page is missing a meta description",
                too_short_type: IssueType::MetaDescriptionTooShort,
                too_short_label: "Meta description is too short",
                too_long_type: IssueType::MetaDescriptionTooLong,
                too_long_label: "Meta description is too long",
            },
        )
    }

    fn validate_length(value: Option<&str>, rule: LengthRule<'_>) -> Vec<SeoIssue> {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            return vec![Self::issue(
                IssueSeverity::Error,
                rule.missing_type,
                rule.missing_message.to_string(),
            )];
        };

        let value_len = value.len();
        if value_len < rule.min_length {
            return vec![Self::issue(
                IssueSeverity::Warning,
                rule.too_short_type,
                format!(
                    "{} ({} chars, recommended: {}-{})",
                    rule.too_short_label, value_len, rule.min_length, rule.max_length
                ),
            )];
        }

        if value_len > rule.max_length {
            return vec![Self::issue(
                IssueSeverity::Warning,
                rule.too_long_type,
                format!(
                    "{} ({} chars, recommended: {}-{})",
                    rule.too_long_label, value_len, rule.min_length, rule.max_length
                ),
            )];
        }

        Vec::new()
    }

    fn validate_h1_tags(h1_tags: &[String]) -> Vec<SeoIssue> {
        if h1_tags.is_empty() {
            return vec![Self::issue(
                IssueSeverity::Warning,
                IssueType::MissingH1,
                "Page is missing an H1 tag".to_string(),
            )];
        }

        if h1_tags.len() > 1 {
            return vec![Self::issue(
                IssueSeverity::Warning,
                IssueType::MultipleH1,
                format!("Page has multiple H1 tags ({})", h1_tags.len()),
            )];
        }

        Vec::new()
    }

    fn validate_images(page: &PageInfo) -> Vec<SeoIssue> {
        let missing_alt_count = page.images.iter().filter(|img| img.alt.is_none()).count();
        if missing_alt_count == 0 {
            return Vec::new();
        }

        vec![Self::issue(
            IssueSeverity::Warning,
            IssueType::MissingImageAlt,
            format!("{} image(s) missing alt text", missing_alt_count),
        )]
    }

    fn validate_thin_content(page: &PageInfo) -> Vec<SeoIssue> {
        let content_indicators = page.h1_tags.len() + page.links.len() + page.images.len();
        if content_indicators >= 5 {
            return Vec::new();
        }

        vec![Self::issue(
            IssueSeverity::Warning,
            IssueType::ThinContent,
            "Page may have thin content (few elements found)".to_string(),
        )]
    }

    fn validate_open_graph(page: &PageInfo) -> Vec<SeoIssue> {
        let mut issues = Vec::new();

        for (value, issue_type, tag_name) in [
            (
                page.open_graph.og_title.as_deref(),
                IssueType::MissingOgTitle,
                "og:title",
            ),
            (
                page.open_graph.og_description.as_deref(),
                IssueType::MissingOgDescription,
                "og:description",
            ),
            (
                page.open_graph.og_image.as_deref(),
                IssueType::MissingOgImage,
                "og:image",
            ),
            (
                page.open_graph.og_url.as_deref(),
                IssueType::MissingOgUrl,
                "og:url",
            ),
            (
                page.open_graph.og_type.as_deref(),
                IssueType::MissingOgType,
                "og:type",
            ),
        ] {
            if value.map(str::trim).is_none_or(str::is_empty) {
                issues.push(Self::issue(
                    IssueSeverity::Info,
                    issue_type,
                    format!("Page is missing {tag_name} tag"),
                ));
            }
        }

        issues
    }

    fn issue(severity: IssueSeverity, issue_type: IssueType, message: String) -> SeoIssue {
        SeoIssue {
            severity,
            issue_type,
            message,
        }
    }
}
