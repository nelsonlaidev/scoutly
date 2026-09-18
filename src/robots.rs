use std::collections::HashSet;
use std::sync::Arc;

use robotstxt::{DefaultMatcher, RobotsParseHandler, parse_robotstxt};
use thiserror::Error;
use url::Url;

use crate::coalescing_cache::CoalescingCache;
use crate::transport::{
    AllowRedirects, BodyMode, GetRequest, RedirectError, RedirectPolicy, Transport, TransportError,
};
use crate::url_compat::{normalize_url, same_origin};

const DEFAULT_USER_AGENT: &str = "scoutly";
const MAX_BODY_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub(crate) enum RobotsError {
    #[error("robots.txt document exceeds {limit}-byte limit")]
    TooLarge { limit: usize },

    #[error("robots.txt is not valid UTF-8")]
    InvalidUtf8,

    #[error("robots.txt rule appears before a user-agent directive on line {line}")]
    RuleBeforeUserAgent { line: usize },
}

#[derive(Debug)]
pub(crate) struct RobotsPolicy {
    body: PolicyBody,
    origin: Url,
    sitemap_urls: Vec<Url>,
}

#[derive(Debug)]
enum PolicyBody {
    AllowAll,
    Rules { body: String, user_agent: String },
}

impl RobotsPolicy {
    pub(crate) fn allows(&self, candidate: &Url) -> bool {
        if !same_origin(&self.origin, candidate) {
            return true;
        }

        match &self.body {
            PolicyBody::AllowAll => true,
            PolicyBody::Rules { body, user_agent } => {
                let mut matcher = DefaultMatcher::default();
                matcher.one_agent_allowed_by_robots(body, user_agent, candidate.as_str())
            }
        }
    }

    fn sitemap_urls(&self) -> &[Url] {
        &self.sitemap_urls
    }

    fn allow_all(origin: Url) -> Self {
        Self {
            body: PolicyBody::AllowAll,
            origin,
            sitemap_urls: Vec::new(),
        }
    }
}

pub(crate) fn parse_robots(
    body: &[u8],
    origin: &Url,
    user_agent: &str,
) -> Result<RobotsPolicy, RobotsError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(RobotsError::TooLarge {
            limit: MAX_BODY_BYTES,
        });
    }
    let body = std::str::from_utf8(body).map_err(|_| RobotsError::InvalidUtf8)?;
    let mut metadata = RobotsMetadata::default();
    parse_robotstxt(body, &mut metadata);
    if let Some(line) = metadata.rule_before_user_agent {
        return Err(RobotsError::RuleBeforeUserAgent { line });
    }

    Ok(RobotsPolicy {
        body: PolicyBody::Rules {
            body: body.to_owned(),
            user_agent: matchable_user_agent(user_agent),
        },
        origin: origin.clone(),
        sitemap_urls: metadata.sitemap_urls,
    })
}

#[derive(Debug, Default)]
struct RobotsMetadata {
    seen_user_agent: bool,
    rule_before_user_agent: Option<usize>,
    sitemap_urls: Vec<Url>,
    seen_sitemaps: HashSet<Url>,
}

impl RobotsMetadata {
    fn record_rule(&mut self, line: u32) {
        if !self.seen_user_agent && self.rule_before_user_agent.is_none() {
            self.rule_before_user_agent = Some(line as usize);
        }
    }
}

impl RobotsParseHandler for RobotsMetadata {
    fn handle_robots_start(&mut self) {
        *self = Self::default();
    }

    fn handle_robots_end(&mut self) {}

    fn handle_user_agent(&mut self, _line_num: u32, _user_agent: &str) {
        self.seen_user_agent = true;
    }

    fn handle_allow(&mut self, line_num: u32, _value: &str) {
        self.record_rule(line_num);
    }

    fn handle_disallow(&mut self, line_num: u32, _value: &str) {
        self.record_rule(line_num);
    }

    fn handle_sitemap(&mut self, _line_num: u32, value: &str) {
        let Ok(candidate) = Url::parse(value) else {
            return;
        };

        if !matches!(candidate.scheme(), "http" | "https") || candidate.host().is_none() {
            return;
        }

        let normalized = normalize_url(&candidate, true);
        if self.seen_sitemaps.insert(normalized.clone()) {
            self.sitemap_urls.push(normalized);
        }
    }

    fn handle_unknown_action(&mut self, _line_num: u32, _action: &str, _value: &str) {}
}

#[derive(Debug, Error)]
pub(crate) enum RobotsLoadError {
    #[error("fetch robots.txt {url}: {source}")]
    Fetch {
        url: String,
        #[source]
        source: TransportError,
    },

    #[error("fetch robots.txt {url}: unexpected HTTP status {status}")]
    UnexpectedStatus { url: String, status: u16 },

    #[error("parse robots.txt {url}: {source}")]
    Parse {
        url: String,
        #[source]
        source: RobotsError,
    },
}

#[derive(Debug)]
pub(crate) struct RobotsCache {
    respect: bool,
    user_agent: String,
    entries: CoalescingCache<String, Arc<RobotsPolicy>>,
}

impl RobotsCache {
    pub(crate) fn new(respect: bool, user_agent: String) -> Self {
        Self {
            respect,
            user_agent,
            entries: CoalescingCache::default(),
        }
    }

    pub(crate) async fn load(
        &self,
        base_url: &Url,
        transport: &Transport,
    ) -> Result<Arc<RobotsPolicy>, RobotsLoadError> {
        let origin = origin_url(base_url);
        let key = origin.to_string();

        self.entries
            .get_or_try_init(key, || async move {
                if !self.respect {
                    return Ok(Arc::new(RobotsPolicy::allow_all(origin)));
                }

                let robots_url = robots_url(&origin);
                let response = transport
                    .get(
                        GetRequest::new(
                            robots_url.clone(),
                            BodyMode::Collect {
                                max_bytes: MAX_BODY_BYTES + 1,
                            },
                        ),
                        &AllowRedirects,
                    )
                    .await
                    .map_err(|source| RobotsLoadError::Fetch {
                        url: robots_url.to_string(),
                        source,
                    })?;

                if (400..500).contains(&response.status.as_u16()) {
                    return Ok(Arc::new(RobotsPolicy::allow_all(origin)));
                }

                if !response.status.is_success() {
                    return Err(RobotsLoadError::UnexpectedStatus {
                        url: robots_url.to_string(),
                        status: response.status.as_u16(),
                    });
                }

                parse_robots(&response.body, &origin, &self.user_agent)
                    .map(Arc::new)
                    .map_err(|source| RobotsLoadError::Parse {
                        url: robots_url.to_string(),
                        source,
                    })
            })
            .await
    }

    pub(crate) fn allows(&self, candidate: &Url) -> bool {
        if !self.respect {
            return true;
        }

        self.entries
            .get_if_ready(&origin_url(candidate).to_string())
            .is_none_or(|policy| policy.allows(candidate))
    }

    pub(crate) fn sitemap_urls(&self, candidate: &Url) -> Vec<Url> {
        self.entries
            .get_if_ready(&origin_url(candidate).to_string())
            .map_or_else(Vec::new, |policy| policy.sitemap_urls().to_vec())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RobotsRedirectPolicy {
    cache: Arc<RobotsCache>,
    transport: Transport,
}

impl RobotsRedirectPolicy {
    pub(crate) fn new(cache: Arc<RobotsCache>, transport: Transport) -> Self {
        Self { cache, transport }
    }
}

impl RedirectPolicy for RobotsRedirectPolicy {
    async fn check(&self, destination: &Url) -> Result<(), RedirectError> {
        let policy = self
            .cache
            .load(destination, &self.transport)
            .await
            .map_err(|error| -> RedirectError { Box::new(error) })?;

        if policy.allows(destination) {
            Ok(())
        } else {
            Err(Box::new(RobotsDisallowed(destination.to_string())))
        }
    }
}

#[derive(Debug, Error)]
#[error("robots.txt disallows {0}")]
struct RobotsDisallowed(String);

pub(crate) fn robots_url(base_url: &Url) -> Url {
    let mut url = origin_url(base_url);
    url.set_path("/robots.txt");
    url
}

fn origin_url(input: &Url) -> Url {
    let mut origin = input.clone();
    let _ = origin.set_username("");
    let _ = origin.set_password(None);
    origin.set_path("/");
    origin.set_query(None);
    origin.set_fragment(None);
    origin
}

fn matchable_user_agent(user_agent: &str) -> String {
    let value = user_agent.trim();
    let token = value
        .find(|character: char| {
            !(character.is_ascii_alphabetic() || matches!(character, '-' | '_'))
        })
        .map_or(value, |end| &value[..end]);

    if token.is_empty() {
        DEFAULT_USER_AGENT.to_owned()
    } else {
        token.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_BODY_BYTES, RobotsError, parse_robots, robots_url};
    use url::Url;

    #[test]
    fn selected_agent_rules_only_restrict_the_loaded_origin() {
        let body = b"User-agent: *\nDisallow: /public\n\nUser-agent: scoutly\nDisallow: /private\nAllow: /private/open\n";
        let origin = Url::parse("https://example.com/start").unwrap();
        let policy = parse_robots(body, &origin, "scoutly/1.0").unwrap();

        assert!(!policy.allows(&Url::parse("https://example.com/private").unwrap()));
        assert!(policy.allows(&Url::parse("https://example.com/private/open").unwrap()));
        assert!(policy.allows(&Url::parse("https://example.com/public").unwrap()));
        assert!(policy.allows(&Url::parse("https://other.example/private").unwrap()));
        assert!(policy.allows(&Url::parse("http://example.com/private").unwrap()));
    }

    #[test]
    fn empty_user_agent_uses_scoutly() {
        let origin = Url::parse("https://example.com/").unwrap();
        let policy =
            parse_robots(b"User-agent: scoutly\nDisallow: /private\n", &origin, "  ").unwrap();
        assert!(!policy.allows(&Url::parse("https://example.com/private?key=value").unwrap()));
    }

    #[test]
    fn sitemap_scanner_normalizes_and_deduplicates_in_source_order() {
        let body = concat!(
            "User-agent *\n",
            "Allow: /\n",
            "Sitemap: https://example.com/sitemap.xml\n",
            "sitemap: HTTPS://EXAMPLE.COM:443/sitemap.xml\n",
            "Site-map: http://sitemaps.example/pages.xml # comment\n",
            "Sitemap: https://例え.テスト/地図.xml\n",
            "Sitemap: /relative.xml\n",
            "Sitemap: ftp://example.com/files.xml\n",
        )
        .as_bytes();
        let origin = Url::parse("https://example.com/").unwrap();
        let policy = parse_robots(body, &origin, "").unwrap();
        assert_eq!(
            policy
                .sitemap_urls()
                .iter()
                .map(Url::as_str)
                .collect::<Vec<_>>(),
            [
                "https://example.com/sitemap.xml",
                "http://sitemaps.example/pages.xml",
                "https://xn--r8jz45g.xn--zckzah/%E5%9C%B0%E5%9B%B3.xml",
            ]
        );
    }

    #[test]
    fn malformed_and_oversized_documents_are_rejected() {
        let origin = Url::parse("https://example.com/").unwrap();
        assert_eq!(
            parse_robots(b"Disallow: /\nUser-agent: bot", &origin, "bot").unwrap_err(),
            RobotsError::RuleBeforeUserAgent { line: 1 }
        );
        assert_eq!(
            parse_robots(b"# comment\nDisalow /\nUser-agent bot", &origin, "bot").unwrap_err(),
            RobotsError::RuleBeforeUserAgent { line: 2 }
        );
        assert_eq!(
            parse_robots(b"Allow: /\nUser-agent: bot", &origin, "bot").unwrap_err(),
            RobotsError::RuleBeforeUserAgent { line: 1 }
        );
        assert_eq!(
            parse_robots(b"User-agent: \xff", &origin, "bot").unwrap_err(),
            RobotsError::InvalidUtf8
        );
        assert_eq!(
            parse_robots(&vec![b'x'; MAX_BODY_BYTES + 1], &origin, "bot").unwrap_err(),
            RobotsError::TooLarge {
                limit: MAX_BODY_BYTES
            }
        );
    }

    #[test]
    fn robots_url_keeps_only_origin_components() {
        let base = Url::parse("https://user:secret@example.com:8443/path?q=1#fragment").unwrap();

        assert_eq!(
            robots_url(&base).as_str(),
            "https://example.com:8443/robots.txt"
        );
    }
}
