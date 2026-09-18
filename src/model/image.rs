use serde::{Deserialize, Serialize};

use crate::model::result::{FailureReason, ResultKind};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImageResult {
    pub kind: ResultKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<FailureReason>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImageOccurrence {
    pub page_url: String,
    pub original_url: String,
    pub element: String,
    pub attribute: String,
    pub descriptor: Option<String>,
    pub alt: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Image {
    pub url: String,
    pub result: ImageResult,
    pub found_on: Vec<ImageOccurrence>,
}

impl Image {
    #[must_use]
    pub fn is_broken(&self) -> bool {
        self.result.kind == ResultKind::Failed
            || (self.result.kind == ResultKind::Response
                && self.result.status_code.is_some_and(|status| status >= 400))
    }

    #[must_use]
    pub fn is_invalid(&self) -> bool {
        if self.result.kind == ResultKind::Invalid {
            return true;
        }

        if self.result.kind != ResultKind::Response
            || self.result.status_code.is_some_and(|status| status >= 400)
        {
            return false;
        }

        !self
            .result
            .content_type
            .as_deref()
            .is_some_and(|value| value.trim().to_ascii_lowercase().starts_with("image/"))
    }

    #[must_use]
    pub fn is_redirected(&self) -> bool {
        matches!(self.result.kind, ResultKind::Response | ResultKind::Blocked)
            && self
                .result
                .final_url
                .as_deref()
                .is_some_and(|final_url| final_url != self.url)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImageSummary {
    pub total: usize,
    pub checked: usize,
    pub broken: usize,
    pub blocked: usize,
    pub redirected: usize,
    pub invalid: usize,
}

#[cfg(test)]
mod tests {
    use super::{Image, ImageResult};
    use crate::model::result::ResultKind;

    #[test]
    fn redirect_detection_matches_image_fragment_rules() {
        let image = Image {
            url: "https://example.com/image.png#source".into(),
            result: ImageResult {
                kind: ResultKind::Response,
                status_code: Some(200),
                final_url: Some("https://example.com/image.png".into()),
                content_type: Some("image/png".into()),
                reason: None,
            },
            found_on: Vec::new(),
        };

        assert!(image.is_redirected());
    }
}
