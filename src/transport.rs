use std::error::Error as StdError;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{
    ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, CONTENT_TYPE, COOKIE, HeaderMap, HeaderName,
    HeaderValue, InvalidHeaderValue, LOCATION, PROXY_AUTHORIZATION, USER_AGENT,
};
use reqwest::{Client, StatusCode};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep_until, timeout};
use url::Url;

use crate::Options;
use crate::url_compat::same_origin;

const DEFAULT_ACCEPT: &str = "*/*";
const DEFAULT_ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";
const REDIRECT_DRAIN_LIMIT: usize = 2 * 1024;
const MAX_RATE_INTERVAL: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BodyMode {
    Collect {
        max_bytes: usize,
    },
    Drain {
        max_bytes: usize,
    },
    CollectHtml {
        max_bytes: usize,
        non_html_drain_bytes: usize,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct GetRequest {
    pub(crate) url: Url,
    pub(crate) headers: HeaderMap,
    pub(crate) body_mode: BodyMode,
}

impl GetRequest {
    pub(crate) fn new(url: Url, body_mode: BodyMode) -> Self {
        Self {
            url,
            headers: HeaderMap::new(),
            body_mode,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HttpResponse {
    pub(crate) status: StatusCode,
    pub(crate) headers: HeaderMap,
    pub(crate) final_url: Url,
    pub(crate) body: Vec<u8>,
}

#[derive(Debug, Error)]
pub(crate) enum TransportBuildError {
    #[error("invalid HTTP user agent: {0}")]
    InvalidUserAgent(#[source] InvalidHeaderValue),

    #[error("build HTTP client: {0}")]
    Build(#[source] reqwest::Error),
}

#[derive(Debug, Error)]
pub(crate) enum TransportError {
    #[error("unsupported URL protocol for {url}")]
    UnsupportedProtocol { url: String },

    #[error("HTTP operation timed out for {url}")]
    TimedOut { url: String },

    #[error("HTTP connection failed for {url}: {source}")]
    ConnectionFailed {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("HTTP request failed for {url}: {source}")]
    RequestFailed {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("invalid redirect location {location:?} from {url}")]
    InvalidRedirectLocation { url: String, location: String },

    #[error("too many redirects (limit {limit}) at {url}")]
    TooManyRedirects { limit: u32, url: String },

    #[error("redirect to {url} rejected: {source}")]
    RedirectRejected {
        url: String,
        #[source]
        source: RedirectError,
    },

    #[error("response body from {url} exceeds {limit}-byte limit")]
    BodyTooLarge { url: String, limit: usize },
}

pub(crate) type RedirectError = Box<dyn StdError + Send + Sync>;

pub(crate) trait RedirectPolicy: Send + Sync {
    async fn check(&self, destination: &Url) -> Result<(), RedirectError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AllowRedirects;

impl RedirectPolicy for AllowRedirects {
    async fn check(&self, _destination: &Url) -> Result<(), RedirectError> {
        Ok(())
    }
}

#[derive(Debug)]
struct RequestSchedule {
    interval: Option<Duration>,
    next: Mutex<Option<Instant>>,
}

impl RequestSchedule {
    fn new(rate_limit: f64) -> Self {
        let interval = (rate_limit > 0.0)
            .then(|| Duration::try_from_secs_f64(1.0 / rate_limit).unwrap_or(MAX_RATE_INTERVAL));

        Self {
            interval,
            next: Mutex::new(None),
        }
    }

    async fn wait(&self) {
        let Some(interval) = self.interval else {
            return;
        };

        // Holding the mutex while sleeping makes a canceled reservation
        // disappear instead of leaving an unused future slot in the schedule.
        let mut next = self.next.lock().await;

        if let Some(scheduled) = *next
            && scheduled > Instant::now()
        {
            sleep_until(scheduled).await;
        }

        *next = Some(
            Instant::now()
                .checked_add(interval)
                .unwrap_or_else(|| Instant::now() + MAX_RATE_INTERVAL),
        );
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Transport {
    client: Client,
    timeout: Duration,
    max_redirects: u32,
    user_agent: HeaderValue,
    schedule: Arc<RequestSchedule>,
}

impl Transport {
    /// Builds a transport from options that have passed [`Options::validate`].
    pub(crate) fn new(options: &Options) -> Result<Self, TransportBuildError> {
        let user_agent = HeaderValue::from_str(&options.user_agent)
            .map_err(TransportBuildError::InvalidUserAgent)?;
        // Preserve a provider selected by an embedding application; otherwise use ring.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .referer(false)
            .gzip(true)
            .build()
            .map_err(TransportBuildError::Build)?;

        Ok(Self {
            client,
            timeout: options.timeout,
            max_redirects: options.max_redirects,
            user_agent,
            schedule: Arc::new(RequestSchedule::new(options.rate_limit)),
        })
    }

    pub(crate) async fn get(
        &self,
        request: GetRequest,
        redirect_policy: &impl RedirectPolicy,
    ) -> Result<HttpResponse, TransportError> {
        let operation_url = request.url.clone();

        if self.timeout.is_zero() {
            return self.get_inner(request, redirect_policy).await;
        }

        timeout(self.timeout, self.get_inner(request, redirect_policy))
            .await
            .map_err(|_| TransportError::TimedOut {
                url: operation_url.to_string(),
            })?
    }

    async fn get_inner(
        &self,
        mut request: GetRequest,
        redirect_policy: &impl RedirectPolicy,
    ) -> Result<HttpResponse, TransportError> {
        validate_http_url(&request.url)?;
        apply_default_headers(&mut request.headers, &self.user_agent);

        let mut redirects = 0_u32;

        loop {
            self.schedule.wait().await;

            let current_url = request.url.clone();
            let mut response = self
                .client
                .get(current_url.clone())
                .headers(request.headers.clone())
                .send()
                .await
                .map_err(|error| classify_reqwest_error(error, &current_url))?;

            let status = response.status();
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);

            if is_redirect(status)
                && let Some(location) = location
            {
                let next_url = current_url.join(&location).map_err(|_| {
                    TransportError::InvalidRedirectLocation {
                        url: current_url.to_string(),
                        location: location.clone(),
                    }
                })?;

                validate_http_url(&next_url)?;

                // Draining is best-effort: redirect policy and limits still apply
                // when the peer closes or the bounded drain cannot finish.
                drain_body(&mut response, REDIRECT_DRAIN_LIMIT).await;

                if redirects >= self.max_redirects {
                    return Err(TransportError::TooManyRedirects {
                        limit: self.max_redirects,
                        url: next_url.to_string(),
                    });
                }

                if let Err(source) = redirect_policy.check(&next_url).await {
                    return Err(TransportError::RedirectRejected {
                        url: next_url.to_string(),
                        source,
                    });
                }

                if !same_origin(&current_url, &next_url) {
                    request.headers.remove(AUTHORIZATION);
                    request.headers.remove(COOKIE);
                    request.headers.remove(PROXY_AUTHORIZATION);
                }

                request.url = next_url;
                redirects += 1;
                continue;
            }

            let headers = response.headers().clone();
            let body = consume_body(&mut response, request.body_mode, &current_url).await?;

            return Ok(HttpResponse {
                status,
                headers,
                final_url: current_url,
                body,
            });
        }
    }
}

fn validate_http_url(url: &Url) -> Result<(), TransportError> {
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return Err(TransportError::UnsupportedProtocol {
            url: url.to_string(),
        });
    }

    Ok(())
}

fn apply_default_headers(headers: &mut HeaderMap, user_agent: &HeaderValue) {
    if headers.get(ACCEPT).is_none_or(HeaderValue::is_empty) {
        headers.insert(ACCEPT, HeaderValue::from_static(DEFAULT_ACCEPT));
    }

    if headers
        .get(ACCEPT_LANGUAGE)
        .is_none_or(HeaderValue::is_empty)
    {
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static(DEFAULT_ACCEPT_LANGUAGE),
        );
    }

    if headers.get(USER_AGENT).is_none_or(HeaderValue::is_empty) {
        headers.insert(USER_AGENT, user_agent.clone());
    }
}

const fn is_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

async fn consume_body(
    response: &mut reqwest::Response,
    mode: BodyMode,
    url: &Url,
) -> Result<Vec<u8>, TransportError> {
    match mode {
        BodyMode::Collect { max_bytes } => collect_body(response, max_bytes, url).await,
        BodyMode::Drain { max_bytes } => {
            drain_body(response, max_bytes).await;
            Ok(Vec::new())
        }
        BodyMode::CollectHtml {
            max_bytes,
            non_html_drain_bytes,
        } => {
            let content_type = joined_header(response.headers(), CONTENT_TYPE);

            if crate::html::is_html_content_type(content_type.as_deref().unwrap_or_default()) {
                collect_body(response, max_bytes, url).await
            } else {
                drain_body(response, non_html_drain_bytes).await;
                Ok(Vec::new())
            }
        }
    }
}

async fn collect_body(
    response: &mut reqwest::Response,
    max_bytes: usize,
    url: &Url,
) -> Result<Vec<u8>, TransportError> {
    let mut body = Vec::with_capacity(
        response
            .content_length()
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or_default()
            .min(max_bytes),
    );

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| classify_reqwest_error(error, url))?
    {
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Err(TransportError::BodyTooLarge {
                url: url.to_string(),
                limit: max_bytes,
            });
        }

        body.extend_from_slice(&chunk);
    }

    Ok(body)
}

async fn drain_body(response: &mut reqwest::Response, max_bytes: usize) {
    let mut remaining = max_bytes;

    while remaining > 0 {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) | Err(_) => break,
        };

        remaining = remaining.saturating_sub(chunk.len());
    }
}

pub(crate) fn joined_header(headers: &HeaderMap, name: HeaderName) -> Option<String> {
    let values = headers
        .get_all(name)
        .iter()
        .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
        .collect::<Vec<_>>();

    (!values.is_empty()).then(|| values.join(", "))
}

fn classify_reqwest_error(error: reqwest::Error, url: &Url) -> TransportError {
    if error.is_timeout() {
        TransportError::TimedOut {
            url: url.to_string(),
        }
    } else if error.is_connect() {
        TransportError::ConnectionFailed {
            url: url.to_string(),
            source: error,
        }
    } else {
        TransportError::RequestFailed {
            url: url.to_string(),
            source: error,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use reqwest::header::{
        ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, COOKIE, HeaderValue, PROXY_AUTHORIZATION,
        USER_AGENT,
    };

    use super::{AllowRedirects, BodyMode, GetRequest, RedirectPolicy, Transport, TransportError};
    use crate::Options;
    use crate::test_server::{TestResponse, TestServer};

    #[test]
    fn preserves_transport_build_error_sources() {
        let options = Options {
            user_agent: "invalid\0agent".to_owned(),
            ..Options::default()
        };

        let error = Transport::new(&options).expect_err("invalid header value must fail");

        assert!(error.source().is_some());
        assert!(error.to_string().contains("invalid HTTP user agent"));
    }

    #[tokio::test]
    async fn applies_default_headers_and_only_decodes_gzip() {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());

        encoder.write_all(b"decoded body").unwrap();

        let compressed = encoder.finish().unwrap();
        let server = TestServer::start(move |_| {
            TestResponse::new(200)
                .header("Content-Encoding", "gzip")
                .header("Content-Type", "text/plain")
                .body(compressed.clone())
        });
        let options = Options {
            user_agent: "scoutly-test/1.0".to_owned(),
            ..Options::default()
        };
        let transport = Transport::new(&options).unwrap();

        let response = transport
            .get(
                GetRequest::new(server.url("/gzip"), BodyMode::Collect { max_bytes: 100 }),
                &AllowRedirects,
            )
            .await
            .unwrap();

        assert_eq!(response.body, b"decoded body");

        let requests = server.requests();

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].header("Accept"), Some("*/*"));
        assert_eq!(
            requests[0].header("Accept-Language"),
            Some("en-US,en;q=0.9")
        );
        assert_eq!(requests[0].header("User-Agent"), Some("scoutly-test/1.0"));
        assert_eq!(requests[0].header("Accept-Encoding"), Some("gzip"));
    }

    #[tokio::test]
    async fn preserves_explicit_headers() {
        let server = TestServer::start(|_| TestResponse::new(204));
        let transport = Transport::new(&Options::default()).unwrap();
        let mut request = GetRequest::new(server.url("/headers"), BodyMode::Drain { max_bytes: 0 });

        request
            .headers
            .insert(ACCEPT, HeaderValue::from_static("text/html"));
        request
            .headers
            .insert(ACCEPT_LANGUAGE, HeaderValue::from_static("zh-TW"));
        request
            .headers
            .insert(USER_AGENT, HeaderValue::from_static("request-agent"));

        transport.get(request, &AllowRedirects).await.unwrap();

        let requests = server.requests();

        assert_eq!(requests[0].header("Accept"), Some("text/html"));
        assert_eq!(requests[0].header("Accept-Language"), Some("zh-TW"));
        assert_eq!(requests[0].header("User-Agent"), Some("request-agent"));
    }

    #[tokio::test]
    async fn follows_all_supported_redirect_statuses_in_order() {
        for status in [301, 302, 303, 307, 308] {
            let server = TestServer::start(move |request| {
                if request.target == "/start" {
                    TestResponse::new(status)
                        .header("Location", "final")
                        .body("redirect")
                } else {
                    TestResponse::new(200).body("done")
                }
            });
            let transport = Transport::new(&Options::default()).unwrap();

            let response = transport
                .get(
                    GetRequest::new(server.url("/start"), BodyMode::Collect { max_bytes: 100 }),
                    &AllowRedirects,
                )
                .await
                .unwrap();

            assert_eq!(response.status.as_u16(), 200, "redirect status {status}");
            assert_eq!(response.final_url, server.url("/final"));
            assert_eq!(response.body, b"done");
            assert_eq!(
                server
                    .requests()
                    .into_iter()
                    .map(|request| request.target)
                    .collect::<Vec<_>>(),
                ["/start", "/final"],
                "redirect status {status}"
            );
        }
    }

    #[tokio::test]
    async fn returns_redirect_without_location_and_rejects_invalid_location() {
        let no_location = TestServer::start(|_| TestResponse::new(302).body("redirect"));
        let transport = Transport::new(&Options::default()).unwrap();
        let response = transport
            .get(
                GetRequest::new(no_location.url("/"), BodyMode::Collect { max_bytes: 100 }),
                &AllowRedirects,
            )
            .await
            .unwrap();

        assert_eq!(response.status.as_u16(), 302);
        assert_eq!(response.body, b"redirect");

        let invalid =
            TestServer::start(|_| TestResponse::new(302).header("Location", "http://[::1"));
        let error = transport
            .get(
                GetRequest::new(invalid.url("/"), BodyMode::Collect { max_bytes: 100 }),
                &AllowRedirects,
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            TransportError::InvalidRedirectLocation { .. }
        ));
    }

    #[derive(Debug)]
    struct RejectRedirect {
        calls: Arc<AtomicUsize>,
    }

    impl RedirectPolicy for RejectRedirect {
        async fn check(&self, destination: &url::Url) -> Result<(), super::RedirectError> {
            assert_eq!(destination.path(), "/private");
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(Box::new(std::io::Error::other("not authorized")))
        }
    }

    #[tokio::test]
    async fn checks_policy_and_limit_before_redirect_request() {
        let server = TestServer::start(|request| {
            if request.target == "/start" {
                TestResponse::new(302).header("Location", "/private")
            } else {
                TestResponse::new(204)
            }
        });
        let transport = Transport::new(&Options::default()).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let policy = RejectRedirect {
            calls: Arc::clone(&calls),
        };
        let error = transport
            .get(
                GetRequest::new(server.url("/start"), BodyMode::Drain { max_bytes: 100 }),
                &policy,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, TransportError::RedirectRejected { .. }));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(server.requests().len(), 1);

        let no_redirects = Options {
            max_redirects: 0,
            ..Options::default()
        };
        let error = Transport::new(&no_redirects)
            .unwrap()
            .get(
                GetRequest::new(server.url("/start"), BodyMode::Drain { max_bytes: 100 }),
                &AllowRedirects,
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            TransportError::TooManyRedirects { limit: 0, .. }
        ));
        assert_eq!(server.requests().len(), 2);
    }

    #[tokio::test]
    async fn strips_sensitive_headers_only_across_origins() {
        let destination = TestServer::start(|_| TestResponse::new(204));
        let destination_url = destination.url("/final").to_string();
        let source =
            TestServer::start(move |_| TestResponse::new(302).header("Location", &destination_url));
        let transport = Transport::new(&Options::default()).unwrap();
        let mut request = GetRequest::new(source.url("/start"), BodyMode::Drain { max_bytes: 100 });

        request
            .headers
            .insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        request
            .headers
            .insert(COOKIE, HeaderValue::from_static("session=secret"));
        request.headers.insert(
            PROXY_AUTHORIZATION,
            HeaderValue::from_static("Basic secret"),
        );
        request
            .headers
            .insert("x-trace", HeaderValue::from_static("keep-me"));

        transport.get(request, &AllowRedirects).await.unwrap();

        let requests = destination.requests();

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].header("Authorization"), None);
        assert_eq!(requests[0].header("Cookie"), None);
        assert_eq!(requests[0].header("Proxy-Authorization"), None);
        assert_eq!(requests[0].header("X-Trace"), Some("keep-me"));
    }

    #[tokio::test]
    async fn timeout_covers_headers_body_and_redirect_rate_wait() {
        let options = Options {
            timeout: Duration::from_millis(100),
            ..Options::default()
        };
        let slow_headers =
            TestServer::start(|_| TestResponse::new(200).header_delay(Duration::from_millis(500)));
        let error = Transport::new(&options)
            .unwrap()
            .get(
                GetRequest::new(slow_headers.url("/"), BodyMode::Collect { max_bytes: 100 }),
                &AllowRedirects,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, TransportError::TimedOut { .. }));

        let slow_body = TestServer::start(|_| {
            TestResponse::new(200)
                .body_delay(Duration::from_millis(500))
                .body("late")
        });
        let error = Transport::new(&options)
            .unwrap()
            .get(
                GetRequest::new(slow_body.url("/"), BodyMode::Collect { max_bytes: 100 }),
                &AllowRedirects,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, TransportError::TimedOut { .. }));

        let redirect = TestServer::start(|request| {
            if request.target == "/start" {
                TestResponse::new(302).header("Location", "/final")
            } else {
                TestResponse::new(200)
            }
        });
        let rate_limited = Options {
            timeout: Duration::from_millis(100),
            max_redirects: 1,
            rate_limit: 0.1,
            ..Options::default()
        };
        let error = Transport::new(&rate_limited)
            .unwrap()
            .get(
                GetRequest::new(redirect.url("/start"), BodyMode::Collect { max_bytes: 100 }),
                &AllowRedirects,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, TransportError::TimedOut { .. }));
        assert_eq!(redirect.requests().len(), 1);
    }

    #[tokio::test]
    async fn one_schedule_rate_limits_all_concurrent_calls() {
        let server = TestServer::start(|_| TestResponse::new(200).body("ok"));
        let options = Options {
            timeout: Duration::from_millis(100),
            rate_limit: 0.1,
            ..Options::default()
        };
        let transport = Transport::new(&options).unwrap();
        let first_transport = transport.clone();
        let first_url = server.url("/first");
        let first = tokio::spawn(async move {
            first_transport
                .get(
                    GetRequest::new(first_url, BodyMode::Collect { max_bytes: 100 }),
                    &AllowRedirects,
                )
                .await
        });
        let second_url = server.url("/second");
        let second = tokio::spawn(async move {
            transport
                .get(
                    GetRequest::new(second_url, BodyMode::Collect { max_bytes: 100 }),
                    &AllowRedirects,
                )
                .await
        });

        let results = [first.await.unwrap(), second.await.unwrap()];

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(TransportError::TimedOut { .. })))
                .count(),
            1
        );
        assert_eq!(server.requests().len(), 1);
    }

    #[tokio::test]
    async fn rejects_oversized_collected_body() {
        let server = TestServer::start(|_| TestResponse::new(200).body("too large"));
        let error = Transport::new(&Options::default())
            .unwrap()
            .get(
                GetRequest::new(server.url("/"), BodyMode::Collect { max_bytes: 3 }),
                &AllowRedirects,
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            TransportError::BodyTooLarge { limit: 3, .. }
        ));
    }

    #[tokio::test]
    async fn rejects_unsupported_protocol_without_a_request() {
        let error = Transport::new(&Options::default())
            .unwrap()
            .get(
                GetRequest::new(
                    url::Url::parse("ftp://example.com/file").unwrap(),
                    BodyMode::Drain { max_bytes: 100 },
                ),
                &AllowRedirects,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, TransportError::UnsupportedProtocol { .. }));
    }

    #[tokio::test]
    async fn preserves_request_error_sources() {
        let error = Transport::new(&Options::default())
            .unwrap()
            .get(
                GetRequest::new(
                    TestServer::unused_url(),
                    BodyMode::Collect { max_bytes: 100 },
                ),
                &AllowRedirects,
            )
            .await
            .expect_err("unused loopback address must refuse the connection");

        assert!(matches!(error, TransportError::ConnectionFailed { .. }));
        assert!(error.source().is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn aborting_operation_cancels_redirect_chain_without_new_requests() {
        let server = TestServer::start(|request| {
            if request.target == "/start" {
                TestResponse::new(302)
                    .header("Location", "/final")
                    .body_delay(Duration::from_secs(5))
                    .body("redirect")
            } else {
                TestResponse::new(204)
            }
        });
        let transport = Transport::new(&Options::default()).unwrap();
        let url = server.url("/start");
        let task = tokio::spawn(async move {
            transport
                .get(
                    GetRequest::new(url, BodyMode::Drain { max_bytes: 100 }),
                    &AllowRedirects,
                )
                .await
        });

        server.wait_for_requests(1);
        task.abort();

        assert!(task.await.unwrap_err().is_cancelled());

        tokio::time::sleep(Duration::from_secs(1)).await;

        assert_eq!(
            server
                .requests()
                .into_iter()
                .map(|request| request.target)
                .collect::<Vec<_>>(),
            ["/start"]
        );
    }
}
