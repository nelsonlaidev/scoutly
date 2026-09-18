use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::time::Duration;

use url::Url;

use crate::url_compat::{
    decode_path, decoded_path, matches_path_prefix, parse_target, same_origin, validate_path_prefix,
};
use crate::{Issue, Rule, RuleIgnore, RuleLevel};

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub max_depth: u32,
    pub max_pages: usize,
    pub keep_fragments: bool,
    pub ignore_redirects: bool,
    pub rate_limit: f64,
    pub respect_robots: bool,
    pub sitemaps: bool,
    pub images: bool,
    pub max_sitemap_documents: usize,
    pub timeout: Duration,
    pub max_redirects: u32,
    pub user_agent: String,
    pub concurrency: usize,
    pub rules: BTreeMap<Rule, RuleLevel>,
    pub include_paths: Vec<String>,
    pub exclude_paths: Vec<String>,
    pub ignore_rules: Vec<RuleIgnore>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_depth: 10,
            max_pages: 500,
            keep_fragments: false,
            ignore_redirects: false,
            rate_limit: 0.0,
            respect_robots: true,
            sitemaps: true,
            images: true,
            max_sitemap_documents: 1_000,
            timeout: Duration::from_secs(30),
            max_redirects: 10,
            user_agent: "scoutly/dev".to_owned(),
            concurrency: 20,
            rules: BTreeMap::new(),
            include_paths: Vec::new(),
            exclude_paths: Vec::new(),
            ignore_rules: Vec::new(),
        }
    }
}

impl Options {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut fields = Vec::new();

        if self.max_pages == 0 {
            fields.push(FieldError::new("max_pages", "must be greater than 0"));
        }

        if !self.rate_limit.is_finite() || self.rate_limit < 0.0 {
            fields.push(FieldError::new(
                "rate_limit",
                "must be finite and greater than or equal to 0",
            ));
        }

        if self.max_sitemap_documents == 0 {
            fields.push(FieldError::new(
                "max_sitemap_documents",
                "must be greater than 0",
            ));
        }

        if self.timeout.is_zero() {
            fields.push(FieldError::new("timeout", "must be greater than 0"));
        }

        if self.user_agent.is_empty() {
            fields.push(FieldError::new("user_agent", "is required"));
        } else if self.user_agent.contains(['\r', '\n']) {
            fields.push(FieldError::new("user_agent", "must not contain a newline"));
        }

        if self.concurrency == 0 {
            fields.push(FieldError::new("concurrency", "must be greater than 0"));
        }

        validate_path_list("include_paths", &self.include_paths, &mut fields);
        validate_path_list("exclude_paths", &self.exclude_paths, &mut fields);
        validate_rule_ignores(&self.ignore_rules, &mut fields);

        if fields.is_empty() {
            Ok(())
        } else {
            Err(ValidationError { fields })
        }
    }

    #[must_use]
    pub fn allows_page(&self, url: &Url) -> bool {
        if self.include_paths.is_empty() && self.exclude_paths.is_empty() {
            return true;
        }

        let path = decoded_path(url);
        if self
            .exclude_paths
            .iter()
            .any(|prefix| matches_path_prefix(&path, &decode_path(prefix)))
        {
            return false;
        }

        self.include_paths.is_empty()
            || self
                .include_paths
                .iter()
                .any(|prefix| matches_path_prefix(&path, &decode_path(prefix)))
    }

    #[must_use]
    pub fn apply_rule_policy(&self, issues: impl IntoIterator<Item = Issue>) -> Vec<Issue> {
        issues
            .into_iter()
            .filter_map(|mut issue| {
                let rule = Rule::from(issue.code);
                let mut level = self
                    .rules
                    .get(&rule)
                    .copied()
                    .unwrap_or_else(|| rule.default_level());

                if self.ignore_redirects && matches!(rule, Rule::Redirect | Rule::ImageRedirect) {
                    level = RuleLevel::Off;
                }

                let severity = level.severity()?;

                if self.rule_is_ignored(&issue, rule) {
                    return None;
                }

                issue.severity = severity;
                Some(issue)
            })
            .collect()
    }

    fn rule_is_ignored(&self, issue: &Issue, rule: Rule) -> bool {
        let Ok(target) = Url::parse(&issue.target.url) else {
            return false;
        };

        self.ignore_rules.iter().any(|ignore| {
            if !ignore.rules.contains(&rule) {
                return false;
            }

            let Ok(prefix) = parse_target(&ignore.url_prefix) else {
                return false;
            };

            same_origin(&target, &prefix)
                && matches_path_prefix(&decoded_path(&target), &decoded_path(&prefix))
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl FieldError {
    fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError {
    pub fields: Vec<FieldError>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid options:")?;
        for field in &self.fields {
            write!(formatter, "\n- {} {}", field.field, field.message)?;
        }
        Ok(())
    }
}

impl Error for ValidationError {}

fn validate_path_list(name: &str, paths: &[String], fields: &mut Vec<FieldError>) {
    for (index, prefix) in paths.iter().enumerate() {
        if let Err(message) = validate_path_prefix(prefix) {
            fields.push(FieldError::new(format!("{name}[{index}]"), message));
        }
    }
}

fn validate_rule_ignores(ignores: &[RuleIgnore], fields: &mut Vec<FieldError>) {
    for (index, ignore) in ignores.iter().enumerate() {
        let field = format!("ignore_rules[{index}]");

        if ignore.rules.is_empty() {
            fields.push(FieldError::new(
                format!("{field}.rules"),
                "must contain at least one rule",
            ));
        }

        match Url::parse(&ignore.url_prefix) {
            Ok(url)
                if matches!(url.scheme(), "http" | "https")
                    && url.host().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none()
                    && !ignore.url_prefix.contains('*')
                    && !decoded_path(&url).contains(['[', ']'])
                    && parse_target(&ignore.url_prefix).is_ok() => {}
            _ => fields.push(FieldError::new(
                format!("{field}.url_prefix"),
                "must be an HTTP(S) URL without credentials, query, fragment or wildcards",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use url::Url;

    use super::Options;
    use crate::{Issue, IssueCode, IssueTarget, Rule, RuleIgnore, RuleLevel, Severity, TargetType};

    #[test]
    fn defaults_define_the_public_audit_policy() {
        let options = Options::default();

        assert_eq!(options.max_depth, 10);
        assert_eq!(options.max_pages, 500);
        assert_eq!(options.timeout.as_secs(), 30);
        assert_eq!(options.concurrency, 20);
        assert!(options.respect_robots && options.sitemaps && options.images);
        assert!(options.validate().is_ok());
    }

    #[test]
    fn validation_collects_all_field_errors() {
        let options = Options {
            max_pages: 0,
            rate_limit: f64::NAN,
            max_sitemap_documents: 0,
            timeout: Duration::ZERO,
            user_agent: String::new(),
            concurrency: 0,
            ..Options::default()
        };

        let error = options.validate().unwrap_err();

        assert_eq!(error.fields.len(), 6);
        assert!(
            error
                .to_string()
                .starts_with("invalid options:\n- max_pages")
        );
    }

    #[test]
    fn page_scope_uses_decoded_segment_paths() {
        let options = Options {
            include_paths: vec!["/docs".into(), "/caf%C3%A9".into()],
            exclude_paths: vec!["/docs/private".into()],
            ..Options::default()
        };

        assert!(options.allows_page(&Url::parse("https://example.com/docs%2Fstart").unwrap()));
        assert!(!options.allows_page(&Url::parse("https://example.com/docs-old").unwrap()));
        assert!(!options.allows_page(&Url::parse("https://example.com/docs/private/a").unwrap()));
        assert!(!options.allows_page(&Url::parse("https://example.com/Docs").unwrap()));
        assert!(options.allows_page(&Url::parse("https://example.com/caf%C3%A9/menu").unwrap()));
    }

    #[test]
    fn ignore_prefixes_require_absolute_urls_and_allow_ipv6_hosts() {
        let valid = Options {
            ignore_rules: vec![RuleIgnore {
                url_prefix: "https://[::1]/docs".into(),
                rules: vec![Rule::MissingTitle],
            }],
            ..Options::default()
        };
        assert!(valid.validate().is_ok());

        for prefix in ["/docs", "example.com/docs", "https://example.com/?"] {
            let options = Options {
                ignore_rules: vec![RuleIgnore {
                    url_prefix: prefix.into(),
                    rules: vec![Rule::MissingTitle],
                }],
                ..Options::default()
            };

            assert!(options.validate().is_err(), "{prefix}");
        }
    }

    #[test]
    fn invalid_scope_paths_are_rejected() {
        for path in [
            "",
            "docs",
            "https://example.com/docs",
            "//example.com/docs",
            "/docs?",
            "/docs#",
            "/docs/**",
            "/[ab]",
            "/bad%",
            "/bad\n",
        ] {
            let options = Options {
                include_paths: vec![path.into()],
                exclude_paths: vec![path.into()],
                ..Options::default()
            };
            assert!(options.validate().is_err(), "{path:?}");
        }
    }

    #[test]
    fn user_agent_rejects_header_injection() {
        let options = Options {
            user_agent: "scoutly/test\r\nX-Injected: true".into(),
            ..Options::default()
        };

        let error = options.validate().unwrap_err();

        assert_eq!(error.fields.len(), 1);
        assert_eq!(error.fields[0].field, "user_agent");
    }

    #[test]
    fn typed_rules_override_and_local_ignores_are_independent() {
        let mut rules = BTreeMap::new();
        rules.insert(Rule::ThinContent, RuleLevel::Warning);
        let options = Options {
            rules,
            ignore_rules: vec![RuleIgnore {
                url_prefix: "https://example.com/docs".into(),
                rules: vec![Rule::MissingTitle],
            }],
            ..Options::default()
        };
        let issues = vec![
            issue(IssueCode::MissingTitle, "https://example.com/docs/page"),
            issue(IssueCode::MissingTitle, "https://example.com/other"),
            issue(IssueCode::ThinContent, "https://example.com/docs/page"),
        ];

        let filtered = options.apply_rule_policy(issues);

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].severity, Severity::Error);
        assert_eq!(filtered[1].severity, Severity::Warning);
    }

    fn issue(code: IssueCode, url: &str) -> Issue {
        Issue {
            code,
            severity: Severity::Info,
            message: "test".into(),
            target: IssueTarget {
                target_type: TargetType::Page,
                url: url.into(),
            },
        }
    }
}
