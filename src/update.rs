use reqwest::Client;
use serde::Deserialize;

use crate::http_client::build_api_client;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY_URL: &str = env!("CARGO_PKG_REPOSITORY");
const UPDATE_CHECK_TIMEOUT_SECS: u64 = 5;
const UPDATE_API_URL_ENV: &str = "SCOUTLY_UPDATE_API_URL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateNotice {
    pub latest_version: String,
    pub release_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

#[derive(Debug, Deserialize)]
struct LatestReleaseResponse {
    tag_name: Option<String>,
    html_url: Option<String>,
}

pub async fn check_for_update() -> Option<UpdateNotice> {
    check_for_update_for_repository(REPOSITORY_URL, CURRENT_VERSION).await
}

async fn check_for_update_for_repository(
    repository_url: &str,
    current_version: &str,
) -> Option<UpdateNotice> {
    if repository_url.is_empty() {
        tracing::debug!("No repository URL configured, skipping update check");
        return None;
    }

    let endpoint = std::env::var(UPDATE_API_URL_ENV)
        .ok()
        .or_else(|| latest_release_api_url(repository_url))?;

    tracing::debug!(endpoint = %endpoint, current_version = %current_version, "Checking for updates");

    check_for_update_with_endpoint(current_version, &endpoint).await
}

#[doc(hidden)]
pub async fn check_for_update_with_endpoint(
    current_version: &str,
    endpoint: &str,
) -> Option<UpdateNotice> {
    let client = build_api_client(UPDATE_CHECK_TIMEOUT_SECS).ok()?;
    tracing::debug!(endpoint = %endpoint, "Fetching latest release info");
    fetch_update_notice(&client, current_version, endpoint)
        .await
        .ok()
        .flatten()
}

pub fn format_cli_update_message(notice: &UpdateNotice) -> String {
    format!(
        "Update available: Scoutly v{} -> v{} ({})",
        CURRENT_VERSION, notice.latest_version, notice.release_url
    )
}

pub fn format_tui_update_message(notice: &UpdateNotice) -> String {
    format!("update v{} available", notice.latest_version)
}

async fn fetch_update_notice(
    client: &Client,
    current_version: &str,
    endpoint: &str,
) -> anyhow::Result<Option<UpdateNotice>> {
    tracing::debug!(endpoint = %endpoint, "Sending request to releases API");
    let response = client.get(endpoint).send().await?.error_for_status()?;
    let release: LatestReleaseResponse = response.json().await?;
    tracing::debug!(tag_name = ?release.tag_name, html_url = ?release.html_url, "Received release response");

    let latest_version = release
        .tag_name
        .as_deref()
        .and_then(normalize_version)
        .filter(|latest_version| is_newer_version(current_version, latest_version))
        .map(str::to_string);

    tracing::debug!(current_version = %current_version, latest_version = ?latest_version, "Version comparison result");

    Ok(match (latest_version, release.html_url) {
        (Some(latest_version), Some(release_url)) => Some(UpdateNotice {
            latest_version,
            release_url,
        }),
        _ => None,
    })
}

fn latest_release_api_url(repository_url: &str) -> Option<String> {
    let repository_url = repository_url.trim_end_matches('/');
    let repository_url = reqwest::Url::parse(repository_url).ok()?;
    let host = repository_url.host_str()?;
    if !host.eq_ignore_ascii_case("github.com") {
        return None;
    }

    let segments = repository_url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() < 2 {
        return None;
    }

    let owner = segments[0];
    let repo = segments[1].trim_end_matches(".git");

    Some(format!(
        "https://api.github.com/repos/{owner}/{repo}/releases/latest"
    ))
}

fn normalize_version(raw_version: &str) -> Option<&str> {
    let normalized = raw_version.trim().trim_start_matches('v');
    parse_version(normalized).map(|_| normalized)
}

fn is_newer_version(current_version: &str, latest_version: &str) -> bool {
    let Some(current_version) = normalize_version(current_version).and_then(parse_version) else {
        return false;
    };
    let Some(latest_version) = parse_version(latest_version) else {
        return false;
    };

    latest_version > current_version
}

fn parse_version(version: &str) -> Option<Version> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;

    if parts.next().is_some() {
        return None;
    }

    Some(Version {
        major,
        minor,
        patch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, HttpResponse, HttpServer, web};
    use serial_test::serial;
    use std::net::TcpListener;
    use std::time::Duration;

    async fn start_update_server() -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind update server");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new(|| {
            App::new().route(
                "/latest",
                web::get().to(|| async {
                    HttpResponse::Ok().json(serde_json::json!({
                        "tag_name": "v9.9.9",
                        "html_url": "https://example.com/releases/v9.9.9"
                    }))
                }),
            )
        })
        .workers(1)
        .listen(listener)
        .expect("listen update server")
        .run();

        tokio::spawn(async move {
            let _ = server.await;
        });

        for _ in 0..20 {
            if reqwest::get(format!("{base_url}/latest")).await.is_ok() {
                return base_url;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        panic!("update server failed to start at {base_url}");
    }

    #[test]
    fn normalize_version_accepts_optional_v_prefix() {
        assert_eq!(normalize_version("v1.2.3"), Some("1.2.3"));
        assert_eq!(normalize_version("1.2.3"), Some("1.2.3"));
    }

    #[test]
    fn normalize_version_rejects_malformed_values() {
        assert_eq!(normalize_version("v1.2"), None);
        assert_eq!(normalize_version("v1.2.3-beta.1"), None);
        assert_eq!(normalize_version("latest"), None);
    }

    #[test]
    fn update_notice_is_skipped_for_same_or_older_versions() {
        assert!(!is_newer_version("0.3.0", "0.3.0"));
        assert!(!is_newer_version("0.3.0", "0.2.9"));
    }

    #[test]
    fn update_notice_is_returned_for_newer_versions() {
        assert!(is_newer_version("0.3.0", "0.3.1"));
        assert!(is_newer_version("0.3.0", "1.0.0"));
    }

    #[test]
    fn latest_release_api_url_uses_github_repo_metadata() {
        assert_eq!(
            latest_release_api_url("https://github.com/nelsonlaidev/scoutly"),
            Some("https://api.github.com/repos/nelsonlaidev/scoutly/releases/latest".to_string())
        );
    }

    #[test]
    fn latest_release_api_url_rejects_non_github_or_short_paths() {
        assert_eq!(
            latest_release_api_url("https://gitlab.com/nelson/scoutly"),
            None
        );
        assert_eq!(
            latest_release_api_url("https://github.com/nelsonlaidev"),
            None
        );
        assert_eq!(latest_release_api_url("not a url"), None);
        assert_eq!(
            latest_release_api_url("https://github.com/nelsonlaidev/scoutly.git/"),
            Some("https://api.github.com/repos/nelsonlaidev/scoutly/releases/latest".to_string())
        );
    }

    #[test]
    fn update_message_formatters_include_expected_versions() {
        let notice = UpdateNotice {
            latest_version: "9.9.9".to_string(),
            release_url: "https://example.com/releases/v9.9.9".to_string(),
        };

        assert!(format_cli_update_message(&notice).contains(CURRENT_VERSION));
        assert!(format_cli_update_message(&notice).contains("9.9.9"));
        assert_eq!(
            format_tui_update_message(&notice),
            "update v9.9.9 available"
        );
    }

    #[test]
    fn parse_and_compare_versions_cover_invalid_inputs() {
        assert_eq!(
            parse_version("1.2.3"),
            Some(Version {
                major: 1,
                minor: 2,
                patch: 3,
            })
        );
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("1.two.3"), None);
        assert!(!is_newer_version("invalid", "1.2.3"));
        assert!(!is_newer_version("1.2.3", "invalid"));
    }

    #[tokio::test]
    async fn empty_repository_url_skips_update_check() {
        assert_eq!(check_for_update_for_repository("", "1.2.3").await, None);
    }

    #[tokio::test]
    #[serial]
    async fn helper_uses_env_override_and_returns_notice() {
        let base_url = start_update_server().await;
        unsafe {
            std::env::set_var(UPDATE_API_URL_ENV, format!("{base_url}/latest"));
        }
        let notice =
            check_for_update_for_repository("https://github.com/example/scoutly", "0.1.0").await;
        unsafe {
            std::env::remove_var(UPDATE_API_URL_ENV);
        }

        assert_eq!(
            notice,
            Some(UpdateNotice {
                latest_version: "9.9.9".to_string(),
                release_url: "https://example.com/releases/v9.9.9".to_string(),
            })
        );
    }
}
