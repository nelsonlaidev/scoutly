use serde::{Deserialize, Serialize};

use crate::model::result::{FailureReason, ResultKind};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LinkResult {
    pub kind: ResultKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<FailureReason>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LinkOccurrence {
    pub page_url: String,
    pub original_url: String,
    pub element: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Link {
    pub url: String,
    pub result: LinkResult,
    pub found_on: Vec<LinkOccurrence>,
}

impl Link {
    #[must_use]
    pub fn is_broken(&self) -> bool {
        self.result.kind == ResultKind::Failed
            || (self.result.kind == ResultKind::Response
                && self.result.status_code.is_some_and(|status| status >= 400))
    }

    #[must_use]
    pub fn is_redirected(&self) -> bool {
        matches!(self.result.kind, ResultKind::Response | ResultKind::Blocked)
            && self
                .result
                .final_url
                .as_deref()
                .is_some_and(|final_url| final_url != strip_fragment(&self.url))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LinkSummary {
    pub total: usize,
    pub checked: usize,
    pub broken: usize,
    pub blocked: usize,
    pub redirected: usize,
}

fn strip_fragment(value: &str) -> &str {
    value.split_once('#').map_or(value, |(before, _)| before)
}

#[cfg(test)]
mod tests {
    use super::{Link, LinkResult};
    use crate::model::result::ResultKind;

    #[test]
    fn redirect_detection_matches_link_fragment_rules() {
        let link = Link {
            url: "https://example.com/page#section".into(),
            result: LinkResult {
                kind: ResultKind::Response,
                status_code: Some(200),
                final_url: Some("https://example.com/page".into()),
                reason: None,
            },
            found_on: Vec::new(),
        };

        assert!(!link.is_redirected());

        let mut final_fragment = link.clone();

        final_fragment.result.final_url = Some("https://example.com/page#other".into());

        assert!(final_fragment.is_redirected());
    }
}
