use anyhow::Result;
use reqwest::{Client, ClientBuilder, header};
use std::time::Duration;

/// Common HTTP headers used for all requests
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.3 Safari/605.1.15";
const ACCEPT: &str = "*/*";
const ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";
const CONNECTION: &str = "keep-alive";

/// Creates a reqwest client with standard browser-like headers and configuration
pub fn build_http_client(timeout_secs: u64) -> Result<Client> {
    let mut headers = header::HeaderMap::new();
    headers.insert(header::ACCEPT, ACCEPT.parse().unwrap());
    headers.insert(header::ACCEPT_LANGUAGE, ACCEPT_LANGUAGE.parse().unwrap());
    headers.insert(header::CONNECTION, CONNECTION.parse().unwrap());

    let client = ClientBuilder::new()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(reqwest::redirect::Policy::limited(10))
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .build()?;

    Ok(client)
}
