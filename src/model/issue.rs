use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum IssueCode {
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
    MissingOgTitle,
    MissingOgDescription,
    MissingOgImage,
    MissingOgUrl,
    MissingOgType,
    PageCrawlFailed,
    PageHttpError,
    BrokenLink,
    LinkCheckBlocked,
    Redirect,
    InvalidImageUrl,
    BrokenImage,
    ImageCheckBlocked,
    InvalidImageContentType,
    ImageRedirect,
}

impl IssueCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingTitle => "missing-title",
            Self::TitleTooShort => "title-too-short",
            Self::TitleTooLong => "title-too-long",
            Self::MissingMetaDescription => "missing-meta-description",
            Self::MetaDescriptionTooShort => "meta-description-too-short",
            Self::MetaDescriptionTooLong => "meta-description-too-long",
            Self::MissingImageAlt => "missing-image-alt",
            Self::MissingH1 => "missing-h1",
            Self::MultipleH1 => "multiple-h1",
            Self::ThinContent => "thin-content",
            Self::MissingOgTitle => "missing-og-title",
            Self::MissingOgDescription => "missing-og-description",
            Self::MissingOgImage => "missing-og-image",
            Self::MissingOgUrl => "missing-og-url",
            Self::MissingOgType => "missing-og-type",
            Self::PageCrawlFailed => "page-crawl-failed",
            Self::PageHttpError => "page-http-error",
            Self::BrokenLink => "broken-link",
            Self::LinkCheckBlocked => "link-check-blocked",
            Self::Redirect => "redirect",
            Self::InvalidImageUrl => "invalid-image-url",
            Self::BrokenImage => "broken-image",
            Self::ImageCheckBlocked => "image-check-blocked",
            Self::InvalidImageContentType => "invalid-image-content-type",
            Self::ImageRedirect => "image-redirect",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetType {
    Page,
    Link,
    Image,
}

impl TargetType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Link => "link",
            Self::Image => "image",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueTarget {
    #[serde(rename = "type")]
    pub target_type: TargetType,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Issue {
    pub code: IssueCode,
    pub severity: Severity,
    pub message: String,
    pub target: IssueTarget,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueSummary {
    pub total: usize,
    pub error: usize,
    pub warning: usize,
    pub info: usize,
}

#[cfg(test)]
mod tests {
    use super::{IssueCode, Severity, TargetType};

    #[test]
    fn display_names_match_serialized_enum_values() {
        for (severity, expected) in [
            (Severity::Error, "error"),
            (Severity::Warning, "warning"),
            (Severity::Info, "info"),
        ] {
            assert_eq!(severity.as_str(), expected);
            assert_eq!(
                serde_json::to_string(&severity).unwrap(),
                format!("\"{expected}\"")
            );
        }

        for (target, expected) in [
            (TargetType::Page, "page"),
            (TargetType::Link, "link"),
            (TargetType::Image, "image"),
        ] {
            assert_eq!(target.as_str(), expected);
            assert_eq!(
                serde_json::to_string(&target).unwrap(),
                format!("\"{expected}\"")
            );
        }

        for code in [
            IssueCode::MissingTitle,
            IssueCode::TitleTooShort,
            IssueCode::TitleTooLong,
            IssueCode::MissingMetaDescription,
            IssueCode::MetaDescriptionTooShort,
            IssueCode::MetaDescriptionTooLong,
            IssueCode::MissingImageAlt,
            IssueCode::MissingH1,
            IssueCode::MultipleH1,
            IssueCode::ThinContent,
            IssueCode::MissingOgTitle,
            IssueCode::MissingOgDescription,
            IssueCode::MissingOgImage,
            IssueCode::MissingOgUrl,
            IssueCode::MissingOgType,
            IssueCode::PageCrawlFailed,
            IssueCode::PageHttpError,
            IssueCode::BrokenLink,
            IssueCode::LinkCheckBlocked,
            IssueCode::Redirect,
            IssueCode::InvalidImageUrl,
            IssueCode::BrokenImage,
            IssueCode::ImageCheckBlocked,
            IssueCode::InvalidImageContentType,
            IssueCode::ImageRedirect,
        ] {
            assert_eq!(
                serde_json::to_string(&code).unwrap(),
                format!("\"{}\"", code.as_str())
            );
        }
    }
}
