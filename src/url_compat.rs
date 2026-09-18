use std::borrow::Cow;

use percent_encoding::percent_decode_str;
use thiserror::Error;
use url::{Host, Url};

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TargetUrlError {
    #[error("target {input:?} must be an absolute HTTP(S) URL")]
    Invalid { input: String },

    #[error("target {input:?} has an invalid port")]
    InvalidPort { input: String },
}

/// Parses and normalizes an audit target using Scoutly's URL compatibility rules.
///
/// Targets without a scheme default to HTTPS. Only absolute HTTP(S) URLs with
/// a host and a valid non-zero port are accepted.
pub fn parse_target(input: &str) -> Result<Url, TargetUrlError> {
    let trimmed = input.trim();
    if trimmed.is_empty()
        || (trimmed.starts_with('/') && !trimmed.starts_with("//"))
        || !valid_percent_encoding(trimmed)
    {
        return Err(TargetUrlError::Invalid {
            input: input.to_owned(),
        });
    }

    let explicit_scheme = has_explicit_scheme(trimmed);
    let authority = authority(trimmed);
    let candidate = if explicit_scheme {
        trimmed.to_owned()
    } else if trimmed.starts_with("//") {
        format!("https:{trimmed}")
    } else {
        format!("https://{trimmed}")
    };

    let mut url = Url::parse(&candidate).map_err(|error| {
        if matches!(error, url::ParseError::InvalidPort) {
            TargetUrlError::InvalidPort {
                input: input.to_owned(),
            }
        } else {
            TargetUrlError::Invalid {
                input: input.to_owned(),
            }
        }
    })?;

    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || (!explicit_scheme && (authority.contains('@') || authority.ends_with(':')))
    {
        return Err(TargetUrlError::Invalid {
            input: input.to_owned(),
        });
    }

    if url.port() == Some(0) {
        return Err(TargetUrlError::InvalidPort {
            input: input.to_owned(),
        });
    }

    url = normalize_url(&url, true);

    Ok(url)
}

pub(crate) fn normalize_url(url: &Url, keep_fragment: bool) -> Url {
    let mut normalized = url.clone();
    if !keep_fragment {
        normalized.set_fragment(None);
    }

    normalized
}

pub(crate) fn canonical_host(url: &Url) -> Option<String> {
    let host = match url.host()? {
        Host::Domain(domain) => domain.to_ascii_lowercase(),
        Host::Ipv4(address) => address.to_string(),
        Host::Ipv6(address) => format!("[{address}]"),
    };

    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

pub(crate) fn same_host(left: &Url, right: &Url) -> bool {
    match (canonical_host(left), canonical_host(right)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

pub(crate) fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme().eq_ignore_ascii_case(right.scheme()) && same_host(left, right)
}

pub(crate) fn decoded_path(url: &Url) -> Cow<'_, str> {
    decode_path(url.path())
}

pub(crate) fn decode_path(path: &str) -> Cow<'_, str> {
    percent_decode_str(path).decode_utf8_lossy()
}

pub(crate) fn matches_path_prefix(path: &str, prefix: &str) -> bool {
    let prefix = prefix.strip_suffix('/').unwrap_or(prefix);
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub(crate) fn validate_path_prefix(prefix: &str) -> Result<(), &'static str> {
    if !prefix.starts_with('/') || prefix.starts_with("//") {
        return Err("must be an absolute path beginning with one slash");
    }

    if prefix.contains(['?', '#', '*', '[', ']']) {
        return Err("must not contain a query, fragment, wildcard, or bracket");
    }

    if prefix.chars().any(char::is_control) || !valid_percent_encoding(prefix) {
        return Err("must be a valid URL path");
    }

    Ok(())
}

fn has_explicit_scheme(value: &str) -> bool {
    value.find("://").is_some_and(|scheme_end| {
        value
            .find(['/', '?', '#'])
            .is_none_or(|separator| scheme_end < separator)
    })
}

fn authority(value: &str) -> &str {
    value
        .split_once("://")
        .map_or(value, |(_, remainder)| remainder)
        .trim_start_matches("//")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
}

fn valid_percent_encoding(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }

        if index + 2 >= bytes.len()
            || !bytes[index + 1].is_ascii_hexdigit()
            || !bytes[index + 2].is_ascii_hexdigit()
        {
            return false;
        }

        index += 3;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{canonical_host, matches_path_prefix, parse_target, same_host, same_origin};
    use url::Url;

    #[test]
    fn target_table_covers_normalization_rules() {
        let valid = [
            (
                "HTTPS://EXAMPLE.COM:443/path#fragment",
                "https://example.com/path#fragment",
            ),
            ("example.com", "https://example.com/"),
            ("  example.com/path  ", "https://example.com/path"),
            ("localhost:8080/path", "https://localhost:8080/path"),
            ("127.0.0.1:8080", "https://127.0.0.1:8080/"),
            ("[::1]:8080/path", "https://[::1]:8080/path"),
            ("//example.com/path", "https://example.com/path"),
            ("https://例え.テスト/", "https://xn--r8jz45g.xn--zckzah/"),
            (
                "https://user:pass@example.com/path",
                "https://user:pass@example.com/path",
            ),
        ];

        for (input, expected) in valid {
            assert_eq!(
                parse_target(input).expect(input).as_str(),
                expected,
                "{input}"
            );
        }

        for input in [
            "",
            "://bad",
            "/relative",
            "mailto:person@example.com",
            "file:/tmp",
            "ftp://example.com",
            "localhost:invalid",
            "localhost:0",
            "localhost:65536",
            "https://example.com/%zz",
            "@example.com",
            ":@example.com",
        ] {
            assert!(parse_target(input).is_err(), "{input}");
        }
    }

    #[test]
    fn origin_and_host_comparisons_require_hosts_and_follow_default_port_rules() {
        let https = parse_target("https://EXAMPLE.com:443/path").unwrap();
        let http = parse_target("http://example.com/elsewhere").unwrap();
        let non_default = parse_target("http://example.com:443").unwrap();

        assert_eq!(canonical_host(&https).as_deref(), Some("example.com"));
        assert!(same_host(&https, &http));
        assert!(!same_origin(&https, &http));
        assert!(!same_host(&https, &non_default));

        let first_hostless = Url::parse("mailto:first@example.com").unwrap();
        let second_hostless = Url::parse("mailto:second@example.com").unwrap();

        assert!(!same_host(&first_hostless, &second_hostless));
        assert!(!same_origin(&first_hostless, &second_hostless));
    }

    #[test]
    fn path_prefixes_are_case_sensitive_and_segment_aware() {
        assert!(matches_path_prefix("/docs/start", "/docs"));
        assert!(matches_path_prefix("/docs", "/docs/"));
        assert!(!matches_path_prefix("/docs-old", "/docs"));
        assert!(!matches_path_prefix("/Docs", "/docs"));
        assert!(matches_path_prefix("/anything", "/"));
    }
}
