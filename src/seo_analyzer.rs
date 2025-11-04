use crate::models::{IssueSeverity, IssueType, PageInfo, SeoIssue};
use std::collections::HashMap;

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
        // Check for missing title
        if page.title.is_none() || page.title.as_ref().unwrap().is_empty() {
            page.issues.push(SeoIssue {
                severity: IssueSeverity::Error,
                issue_type: IssueType::MissingTitle,
                message: "Page is missing a title tag".to_string(),
            });
        } else if let Some(title) = &page.title {
            // Check title length
            if title.len() < 50 {
                page.issues.push(SeoIssue {
                    severity: IssueSeverity::Warning,
                    issue_type: IssueType::TitleTooShort,
                    message: format!(
                        "Title is too short ({} chars, recommended: 50-60)",
                        title.len()
                    ),
                });
            } else if title.len() > 60 {
                page.issues.push(SeoIssue {
                    severity: IssueSeverity::Warning,
                    issue_type: IssueType::TitleTooLong,
                    message: format!(
                        "Title is too long ({} chars, recommended: 50-60)",
                        title.len()
                    ),
                });
            }
        }

        // Check for missing meta description
        if page.meta_description.is_none() || page.meta_description.as_ref().unwrap().is_empty() {
            page.issues.push(SeoIssue {
                severity: IssueSeverity::Error,
                issue_type: IssueType::MissingMetaDescription,
                message: "Page is missing a meta description".to_string(),
            });
        } else if let Some(desc) = &page.meta_description {
            // Check meta description length
            if desc.len() < 150 {
                page.issues.push(SeoIssue {
                    severity: IssueSeverity::Warning,
                    issue_type: IssueType::MetaDescriptionTooShort,
                    message: format!(
                        "Meta description is too short ({} chars, recommended: 150-160)",
                        desc.len()
                    ),
                });
            } else if desc.len() > 160 {
                page.issues.push(SeoIssue {
                    severity: IssueSeverity::Warning,
                    issue_type: IssueType::MetaDescriptionTooLong,
                    message: format!(
                        "Meta description is too long ({} chars, recommended: 150-160)",
                        desc.len()
                    ),
                });
            }
        }

        // Check H1 tags
        if page.h1_tags.is_empty() {
            page.issues.push(SeoIssue {
                severity: IssueSeverity::Warning,
                issue_type: IssueType::MissingH1,
                message: "Page is missing an H1 tag".to_string(),
            });
        } else if page.h1_tags.len() > 1 {
            page.issues.push(SeoIssue {
                severity: IssueSeverity::Warning,
                issue_type: IssueType::MultipleH1,
                message: format!("Page has multiple H1 tags ({})", page.h1_tags.len()),
            });
        }

        // Check for images without alt text
        let missing_alt_count = page.images.iter().filter(|img| img.alt.is_none()).count();
        if missing_alt_count > 0 {
            page.issues.push(SeoIssue {
                severity: IssueSeverity::Warning,
                issue_type: IssueType::MissingImageAlt,
                message: format!("{} image(s) missing alt text", missing_alt_count),
            });
        }

        // Check for thin content (basic check based on extracted elements)
        let content_indicators = page.h1_tags.len() + page.links.len() + page.images.len();
        if content_indicators < 5 {
            page.issues.push(SeoIssue {
                severity: IssueSeverity::Warning,
                issue_type: IssueType::ThinContent,
                message: "Page may have thin content (few elements found)".to_string(),
            });
        }
    }
}
