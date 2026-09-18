use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{IssueCode, Severity};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleLevel {
    Off,
    Info,
    Warning,
    Error,
}

impl RuleLevel {
    #[must_use]
    pub const fn severity(self) -> Option<Severity> {
        match self {
            Self::Off => None,
            Self::Info => Some(Severity::Info),
            Self::Warning => Some(Severity::Warning),
            Self::Error => Some(Severity::Error),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
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

impl Rule {
    pub const ALL: [Self; 25] = [
        Self::MissingTitle,
        Self::TitleTooShort,
        Self::TitleTooLong,
        Self::MissingMetaDescription,
        Self::MetaDescriptionTooShort,
        Self::MetaDescriptionTooLong,
        Self::MissingImageAlt,
        Self::MissingH1,
        Self::MultipleH1,
        Self::ThinContent,
        Self::MissingOgTitle,
        Self::MissingOgDescription,
        Self::MissingOgImage,
        Self::MissingOgUrl,
        Self::MissingOgType,
        Self::PageCrawlFailed,
        Self::PageHttpError,
        Self::BrokenLink,
        Self::LinkCheckBlocked,
        Self::Redirect,
        Self::InvalidImageUrl,
        Self::BrokenImage,
        Self::ImageCheckBlocked,
        Self::InvalidImageContentType,
        Self::ImageRedirect,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingTitle => "missing_title",
            Self::TitleTooShort => "title_too_short",
            Self::TitleTooLong => "title_too_long",
            Self::MissingMetaDescription => "missing_meta_description",
            Self::MetaDescriptionTooShort => "meta_description_too_short",
            Self::MetaDescriptionTooLong => "meta_description_too_long",
            Self::MissingImageAlt => "missing_image_alt",
            Self::MissingH1 => "missing_h1",
            Self::MultipleH1 => "multiple_h1",
            Self::ThinContent => "thin_content",
            Self::MissingOgTitle => "missing_og_title",
            Self::MissingOgDescription => "missing_og_description",
            Self::MissingOgImage => "missing_og_image",
            Self::MissingOgUrl => "missing_og_url",
            Self::MissingOgType => "missing_og_type",
            Self::PageCrawlFailed => "page_crawl_failed",
            Self::PageHttpError => "page_http_error",
            Self::BrokenLink => "broken_link",
            Self::LinkCheckBlocked => "link_check_blocked",
            Self::Redirect => "redirect",
            Self::InvalidImageUrl => "invalid_image_url",
            Self::BrokenImage => "broken_image",
            Self::ImageCheckBlocked => "image_check_blocked",
            Self::InvalidImageContentType => "invalid_image_content_type",
            Self::ImageRedirect => "image_redirect",
        }
    }

    #[must_use]
    pub const fn default_level(self) -> RuleLevel {
        match self {
            Self::MissingTitle
            | Self::MissingMetaDescription
            | Self::PageCrawlFailed
            | Self::PageHttpError
            | Self::BrokenLink
            | Self::InvalidImageUrl
            | Self::BrokenImage
            | Self::InvalidImageContentType => RuleLevel::Error,
            Self::MissingImageAlt
            | Self::MissingH1
            | Self::MultipleH1
            | Self::LinkCheckBlocked
            | Self::ImageCheckBlocked => RuleLevel::Warning,
            Self::Redirect | Self::ImageRedirect => RuleLevel::Info,
            Self::TitleTooShort
            | Self::TitleTooLong
            | Self::MetaDescriptionTooShort
            | Self::MetaDescriptionTooLong
            | Self::ThinContent
            | Self::MissingOgTitle
            | Self::MissingOgDescription
            | Self::MissingOgImage
            | Self::MissingOgUrl
            | Self::MissingOgType => RuleLevel::Off,
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<IssueCode> for Rule {
    fn from(code: IssueCode) -> Self {
        match code {
            IssueCode::MissingTitle => Self::MissingTitle,
            IssueCode::TitleTooShort => Self::TitleTooShort,
            IssueCode::TitleTooLong => Self::TitleTooLong,
            IssueCode::MissingMetaDescription => Self::MissingMetaDescription,
            IssueCode::MetaDescriptionTooShort => Self::MetaDescriptionTooShort,
            IssueCode::MetaDescriptionTooLong => Self::MetaDescriptionTooLong,
            IssueCode::MissingImageAlt => Self::MissingImageAlt,
            IssueCode::MissingH1 => Self::MissingH1,
            IssueCode::MultipleH1 => Self::MultipleH1,
            IssueCode::ThinContent => Self::ThinContent,
            IssueCode::MissingOgTitle => Self::MissingOgTitle,
            IssueCode::MissingOgDescription => Self::MissingOgDescription,
            IssueCode::MissingOgImage => Self::MissingOgImage,
            IssueCode::MissingOgUrl => Self::MissingOgUrl,
            IssueCode::MissingOgType => Self::MissingOgType,
            IssueCode::PageCrawlFailed => Self::PageCrawlFailed,
            IssueCode::PageHttpError => Self::PageHttpError,
            IssueCode::BrokenLink => Self::BrokenLink,
            IssueCode::LinkCheckBlocked => Self::LinkCheckBlocked,
            IssueCode::Redirect => Self::Redirect,
            IssueCode::InvalidImageUrl => Self::InvalidImageUrl,
            IssueCode::BrokenImage => Self::BrokenImage,
            IssueCode::ImageCheckBlocked => Self::ImageCheckBlocked,
            IssueCode::InvalidImageContentType => Self::InvalidImageContentType,
            IssueCode::ImageRedirect => Self::ImageRedirect,
        }
    }
}

impl From<Rule> for IssueCode {
    fn from(rule: Rule) -> Self {
        match rule {
            Rule::MissingTitle => Self::MissingTitle,
            Rule::TitleTooShort => Self::TitleTooShort,
            Rule::TitleTooLong => Self::TitleTooLong,
            Rule::MissingMetaDescription => Self::MissingMetaDescription,
            Rule::MetaDescriptionTooShort => Self::MetaDescriptionTooShort,
            Rule::MetaDescriptionTooLong => Self::MetaDescriptionTooLong,
            Rule::MissingImageAlt => Self::MissingImageAlt,
            Rule::MissingH1 => Self::MissingH1,
            Rule::MultipleH1 => Self::MultipleH1,
            Rule::ThinContent => Self::ThinContent,
            Rule::MissingOgTitle => Self::MissingOgTitle,
            Rule::MissingOgDescription => Self::MissingOgDescription,
            Rule::MissingOgImage => Self::MissingOgImage,
            Rule::MissingOgUrl => Self::MissingOgUrl,
            Rule::MissingOgType => Self::MissingOgType,
            Rule::PageCrawlFailed => Self::PageCrawlFailed,
            Rule::PageHttpError => Self::PageHttpError,
            Rule::BrokenLink => Self::BrokenLink,
            Rule::LinkCheckBlocked => Self::LinkCheckBlocked,
            Rule::Redirect => Self::Redirect,
            Rule::InvalidImageUrl => Self::InvalidImageUrl,
            Rule::BrokenImage => Self::BrokenImage,
            Rule::ImageCheckBlocked => Self::ImageCheckBlocked,
            Rule::InvalidImageContentType => Self::InvalidImageContentType,
            Rule::ImageRedirect => Self::ImageRedirect,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuleIgnore {
    pub url_prefix: String,
    pub rules: Vec<Rule>,
}

#[cfg(test)]
mod tests {
    use super::{Rule, RuleLevel};
    use crate::IssueCode;

    #[test]
    fn every_rule_has_the_expected_wire_name_and_default() {
        assert_eq!(Rule::ALL.len(), 25);

        for rule in Rule::ALL {
            assert_eq!(
                serde_json::to_string(&rule).unwrap(),
                format!("\"{}\"", rule.as_str())
            );

            let code = IssueCode::from(rule);

            assert_eq!(Rule::from(code), rule);
        }

        assert_eq!(Rule::MissingTitle.default_level(), RuleLevel::Error);
        assert_eq!(Rule::MissingH1.default_level(), RuleLevel::Warning);
        assert_eq!(Rule::Redirect.default_level(), RuleLevel::Info);
        assert_eq!(Rule::ThinContent.default_level(), RuleLevel::Off);

        let counts = Rule::ALL.iter().fold([0; 4], |mut counts, rule| {
            let index = match rule.default_level() {
                RuleLevel::Off => 0,
                RuleLevel::Info => 1,
                RuleLevel::Warning => 2,
                RuleLevel::Error => 3,
            };
            counts[index] += 1;
            counts
        });

        assert_eq!(counts, [10, 2, 5, 8]);
    }
}
