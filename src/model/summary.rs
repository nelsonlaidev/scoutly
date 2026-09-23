use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::model::image::{Image, ImageSummary};
use crate::model::issue::{Issue, IssueSummary, Severity};
use crate::model::link::{Link, LinkSummary};
use crate::model::page::Page;
use crate::model::result::ResultKind;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Summary {
    pub pages: usize,
    pub links: LinkSummary,
    pub images: ImageSummary,
    pub issues: IssueSummary,
}

impl Summary {
    #[must_use]
    pub fn calculate(pages: &[Page], links: &[Link], images: &[Image], issues: &[Issue]) -> Self {
        let mut summary = Self {
            pages: pages.len(),
            links: LinkSummary {
                total: links.len(),
                ..LinkSummary::default()
            },
            images: ImageSummary {
                total: images.len(),
                ..ImageSummary::default()
            },
            issues: IssueSummary {
                total: issues.len(),
                ..IssueSummary::default()
            },
        };

        for link in links {
            if link.result.kind != ResultKind::Skipped {
                summary.links.checked += 1;
            }

            if link.result.kind == ResultKind::Blocked {
                summary.links.blocked += 1;
            }

            if link.is_broken() {
                summary.links.broken += 1;
            }

            if link.is_redirected() {
                summary.links.redirected += 1;
            }
        }

        for image in images {
            if matches!(
                image.result.kind,
                ResultKind::Response | ResultKind::Blocked | ResultKind::Failed
            ) {
                summary.images.checked += 1;
            }

            if image.result.kind == ResultKind::Blocked {
                summary.images.blocked += 1;
            }

            if image.is_broken() {
                summary.images.broken += 1;
            }

            if image.is_redirected() {
                summary.images.redirected += 1;
            }

            if image.is_invalid() {
                summary.images.invalid += 1;
            }
        }

        for issue in issues {
            match issue.severity {
                Severity::Error => summary.issues.error += 1,
                Severity::Warning => summary.issues.warning += 1,
                Severity::Info => summary.issues.info += 1,
            }
        }

        summary
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Report {
    pub url: String,
    #[serde(with = "time::serde::rfc3339")]
    pub audited_at: OffsetDateTime,
    pub summary: Summary,
    pub issues: Vec<Issue>,
    pub pages: Vec<Page>,
    pub links: Vec<Link>,
    pub images: Vec<Image>,
}

impl Report {
    pub fn refresh_summary(&mut self) {
        self.summary = Summary::calculate(&self.pages, &self.links, &self.images, &self.issues);
    }
}

#[cfg(test)]
mod tests {
    use super::Summary;
    use crate::model::image::{Image, ImageResult};
    use crate::model::issue::{Issue, IssueCode, IssueTarget, Severity, TargetType};
    use crate::model::link::{Link, LinkResult};
    use crate::model::result::ResultKind;

    #[test]
    fn summary_uses_resource_result_semantics() {
        let links = vec![
            Link {
                url: "https://example.com/old#source".into(),
                result: LinkResult {
                    kind: ResultKind::Response,
                    status_code: Some(301),
                    final_url: Some("https://example.com/new#target".into()),
                    reason: None,
                },
                found_on: Vec::new(),
            },
            Link {
                url: "mailto:hello@example.com".into(),
                result: LinkResult {
                    kind: ResultKind::Skipped,
                    status_code: None,
                    final_url: None,
                    reason: None,
                },
                found_on: Vec::new(),
            },
        ];

        let images = vec![Image {
            url: "https://example.com/not-image".into(),
            result: ImageResult {
                kind: ResultKind::Response,
                status_code: Some(200),
                final_url: None,
                content_type: Some("text/html; charset=utf-8".into()),
                reason: None,
            },
            found_on: Vec::new(),
        }];

        let issues = vec![Issue {
            code: IssueCode::Redirect,
            severity: Severity::Info,
            message: "redirected".into(),
            target: IssueTarget {
                target_type: TargetType::Link,
                url: links[0].url.clone(),
            },
        }];

        let summary = Summary::calculate(&[], &links, &images, &issues);

        assert_eq!(summary.links.total, 2);
        assert_eq!(summary.links.checked, 1);
        assert_eq!(summary.links.redirected, 1);
        assert_eq!(summary.images.checked, 1);
        assert_eq!(summary.images.invalid, 1);
        assert_eq!(summary.issues.info, 1);
    }
}
