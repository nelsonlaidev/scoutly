use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlSummary {
    pub total_pages: usize,
    pub total_links: usize,
    pub broken_links: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}
