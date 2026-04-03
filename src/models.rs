use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    pub url: String,
    pub status_code: Option<u16>,
    pub content_type: Option<String>,
    pub title: Option<String>,
    pub meta_description: Option<String>,
    pub h1_tags: Vec<String>,
    pub links: Vec<Link>,
    pub images: Vec<Image>,
    pub open_graph: OpenGraphTags,
    pub issues: Vec<SeoIssue>,
    pub crawl_depth: usize,
}

impl PageInfo {
    pub fn display_title(&self) -> String {
        self.title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                (!Self::is_html_content_type(self.content_type.as_deref()))
                    .then(|| Self::resource_name_from_url(&self.url))
                    .flatten()
            })
            .unwrap_or_else(|| "(untitled)".to_string())
    }

    pub fn is_html_content_type(content_type: Option<&str>) -> bool {
        content_type.is_none_or(|ct| {
            let ct_lower = ct.to_lowercase();
            ct_lower.contains("text/html") || ct_lower.contains("application/xhtml")
        })
    }

    fn resource_name_from_url(url: &str) -> Option<String> {
        Url::parse(url)
            .ok()?
            .path_segments()?
            .filter(|segment| !segment.is_empty())
            .next_back()
            .map(|segment| segment.to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenGraphTags {
    pub og_title: Option<String>,
    pub og_description: Option<String>,
    pub og_image: Option<String>,
    pub og_url: Option<String>,
    pub og_type: Option<String>,
    pub og_site_name: Option<String>,
    pub og_locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub url: String,
    pub text: String,
    pub is_external: bool,
    pub status_code: Option<u16>,
    pub redirected_url: Option<String>,
    pub check_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub src: String,
    pub alt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeoIssue {
    pub severity: IssueSeverity,
    pub issue_type: IssueType,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueType {
    MissingTitle,
    TitleTooShort,
    TitleTooLong,
    MissingMetaDescription,
    MetaDescriptionTooShort,
    MetaDescriptionTooLong,
    MissingImageAlt,
    MissingH1,
    MultipleH1,
    ThinContent,
    BrokenLink,
    Redirect,
    MissingOgTitle,
    MissingOgDescription,
    MissingOgImage,
    MissingOgUrl,
    MissingOgType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlReport {
    pub start_url: String,
    pub pages: HashMap<String, PageInfo>,
    pub summary: CrawlSummary,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrawlSummary {
    pub total_pages: usize,
    pub total_links: usize,
    pub broken_links: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(url: &str, content_type: Option<&str>, title: Option<&str>) -> PageInfo {
        PageInfo {
            url: url.to_string(),
            status_code: Some(200),
            content_type: content_type.map(str::to_string),
            title: title.map(str::to_string),
            meta_description: None,
            h1_tags: vec![],
            links: vec![],
            images: vec![],
            open_graph: OpenGraphTags::default(),
            issues: vec![],
            crawl_depth: 0,
        }
    }

    #[test]
    fn display_title_uses_filename_for_non_html_resources() {
        let page = page(
            "https://example.com/media/video1.mp4",
            Some("video/mp4"),
            None,
        );

        assert_eq!(page.display_title(), "video1.mp4");
    }

    #[test]
    fn display_title_keeps_html_pages_without_title_untitled() {
        let page = page(
            "https://example.com/missing-title.html",
            Some("text/html"),
            None,
        );

        assert_eq!(page.display_title(), "(untitled)");
    }
}
