use reqwest::header::{CONTENT_TYPE, HeaderName};
use url::Url;

use crate::coalescing_cache::CoalescingCache;
use crate::transport::{
    AllowRedirects, BodyMode, GetRequest, Transport, TransportError, joined_header,
};
use crate::url_compat::normalize_url;
use crate::{FailureReason, ResultKind};

const RESOURCE_DRAIN_LIMIT: usize = 32 * 1024;
const CF_MITIGATED: HeaderName = HeaderName::from_static("cf-mitigated");

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceResult {
    pub(crate) kind: ResultKind,
    pub(crate) status_code: Option<u16>,
    pub(crate) final_url: Option<Url>,
    pub(crate) content_type: Option<String>,
    pub(crate) reason: Option<FailureReason>,
}

impl ResourceResult {
    fn invalid(reason: FailureReason) -> Self {
        Self {
            kind: ResultKind::Invalid,
            status_code: None,
            final_url: None,
            content_type: None,
            reason: Some(reason),
        }
    }

    fn skipped(reason: FailureReason) -> Self {
        Self {
            kind: ResultKind::Skipped,
            status_code: None,
            final_url: None,
            content_type: None,
            reason: Some(reason),
        }
    }

    fn failed(reason: FailureReason) -> Self {
        Self {
            kind: ResultKind::Failed,
            status_code: None,
            final_url: None,
            content_type: None,
            reason: Some(reason),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ResourceChecker {
    transport: Transport,
    cache: CoalescingCache<String, ResourceResult>,
}

impl ResourceChecker {
    pub(crate) fn new(transport: Transport) -> Self {
        Self {
            transport,
            cache: CoalescingCache::default(),
        }
    }

    pub(crate) async fn check(&self, input: Option<&Url>) -> ResourceResult {
        let Some(input) = input else {
            return ResourceResult::invalid(FailureReason::InvalidUrl);
        };

        if !matches!(input.scheme(), "http" | "https") || input.host().is_none() {
            return ResourceResult::skipped(FailureReason::UnsupportedProtocol);
        }

        let request_url = normalize_url(input, false);
        let key = request_url.to_string();

        self.cache
            .get_or_init(key, || self.check_uncached(request_url))
            .await
    }

    async fn check_uncached(&self, request_url: Url) -> ResourceResult {
        let request = GetRequest::new(
            request_url,
            BodyMode::Drain {
                max_bytes: RESOURCE_DRAIN_LIMIT,
            },
        );

        match self.transport.get(request, &AllowRedirects).await {
            Ok(response) => {
                let blocked = response
                    .headers
                    .get(CF_MITIGATED)
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.trim().eq_ignore_ascii_case("challenge"));

                ResourceResult {
                    kind: if blocked {
                        ResultKind::Blocked
                    } else {
                        ResultKind::Response
                    },
                    status_code: Some(response.status.as_u16()),
                    final_url: Some(normalize_url(&response.final_url, false)),
                    content_type: joined_header(&response.headers, CONTENT_TYPE),
                    reason: blocked.then_some(FailureReason::AntiBotChallenge),
                }
            }
            Err(error) => classify_transport_error(error),
        }
    }
}

fn classify_transport_error(error: TransportError) -> ResourceResult {
    match error {
        TransportError::TimedOut { .. } => ResourceResult::failed(FailureReason::RequestTimedOut),
        TransportError::ConnectionFailed { .. } => {
            ResourceResult::failed(FailureReason::ConnectionFailed)
        }
        TransportError::UnsupportedProtocol { .. } => {
            ResourceResult::skipped(FailureReason::UnsupportedProtocol)
        }
        TransportError::InvalidRedirectLocation { .. }
        | TransportError::TooManyRedirects { .. }
        | TransportError::RedirectRejected { .. }
        | TransportError::BodyTooLarge { .. }
        | TransportError::RequestFailed { .. } => {
            ResourceResult::failed(FailureReason::RequestFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use url::Url;

    use super::{ResourceChecker, ResourceResult};
    use crate::test_server::{TestResponse, TestServer};
    use crate::transport::Transport;
    use crate::{FailureReason, Options, ResultKind};

    fn checker(options: &Options) -> ResourceChecker {
        ResourceChecker::new(Transport::new(options).unwrap())
    }

    #[tokio::test]
    async fn invalid_and_unsupported_resources_are_not_requested() {
        let checker = checker(&Options::default());

        assert_eq!(
            checker.check(None).await,
            ResourceResult::invalid(FailureReason::InvalidUrl)
        );
        assert_eq!(
            checker
                .check(Some(&Url::parse("mailto:test@example.com").unwrap()))
                .await,
            ResourceResult::skipped(FailureReason::UnsupportedProtocol)
        );
    }

    #[tokio::test]
    async fn reports_response_metadata_and_anti_bot_challenges() {
        let server = TestServer::start(|request| {
            if request.target == "/blocked" {
                TestResponse::new(403)
                    .header("Content-Type", "text/html")
                    .header("Content-Type", "charset=utf-8")
                    .header("Cf-Mitigated", " ChAlLeNgE ")
            } else {
                TestResponse::new(204).header("Content-Type", "image/png")
            }
        });
        let checker = checker(&Options::default());

        let response = checker.check(Some(&server.url("/image"))).await;

        assert_eq!(response.kind, ResultKind::Response);
        assert_eq!(response.status_code, Some(204));
        assert_eq!(response.final_url, Some(server.url("/image")));
        assert_eq!(response.content_type.as_deref(), Some("image/png"));
        assert_eq!(response.reason, None);

        let blocked = checker.check(Some(&server.url("/blocked"))).await;

        assert_eq!(blocked.kind, ResultKind::Blocked);
        assert_eq!(blocked.status_code, Some(403));
        assert_eq!(
            blocked.content_type.as_deref(),
            Some("text/html, charset=utf-8")
        );
        assert_eq!(blocked.reason, Some(FailureReason::AntiBotChallenge));
    }

    #[tokio::test]
    async fn coalesces_concurrent_fragment_variants() {
        let requests = Arc::new(AtomicUsize::new(0));
        let server_requests = Arc::clone(&requests);
        let server = TestServer::start(move |_| {
            server_requests.fetch_add(1, Ordering::SeqCst);
            TestResponse::new(200)
                .body_delay(Duration::from_millis(25))
                .body(vec![0; 64 * 1024])
        });
        let checker = Arc::new(checker(&Options::default()));

        let mut first_url = server.url("/asset");
        first_url.set_fragment(Some("one"));

        let mut second_url = server.url("/asset");
        second_url.set_fragment(Some("two"));

        let first_checker = Arc::clone(&checker);
        let first = tokio::spawn(async move { first_checker.check(Some(&first_url)).await });
        let second_checker = Arc::clone(&checker);
        let second = tokio::spawn(async move { second_checker.check(Some(&second_url)).await });

        assert_eq!(first.await.unwrap().kind, ResultKind::Response);
        assert_eq!(second.await.unwrap().kind, ResultKind::Response);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn classifies_timeout_and_connection_failure() {
        let slow =
            TestServer::start(|_| TestResponse::new(200).header_delay(Duration::from_millis(500)));
        let options = Options {
            timeout: Duration::from_millis(100),
            ..Options::default()
        };
        let result = checker(&options).check(Some(&slow.url("/"))).await;

        assert_eq!(result.kind, ResultKind::Failed);
        assert_eq!(result.reason, Some(FailureReason::RequestTimedOut));

        let slow_body = TestServer::start(|_| {
            TestResponse::new(200)
                .body_delay(Duration::from_millis(500))
                .body("late")
        });
        let result = checker(&options).check(Some(&slow_body.url("/"))).await;

        assert_eq!(result.kind, ResultKind::Failed);
        assert_eq!(result.reason, Some(FailureReason::RequestTimedOut));

        let unavailable = TestServer::unused_url();
        let result = checker(&Options::default()).check(Some(&unavailable)).await;

        assert_eq!(result.kind, ResultKind::Failed);
        assert_eq!(result.reason, Some(FailureReason::ConnectionFailed));
    }
}
