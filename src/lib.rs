//! Scoutly's asynchronous website audit library.
//!
//! The asynchronous library, CLI, and terminal interface share this crate's
//! deterministic audit pipeline.

mod checker;
mod coalescing_cache;
mod concurrent;
mod config;
mod crawler;
mod html;
mod model;
mod options;
mod progress;
mod report;
mod resource;
mod robots;
mod rules;
mod sitemap;
mod srcset;
mod transport;
mod url_compat;

#[cfg(test)]
mod test_server;

pub use config::{
    AmbiguousConfigError, CONFIG_CANDIDATE_NAMES, Config, ConfigDecodeError, ConfigError,
    ConfigLoadOptions, ConfigLoadResult, ConfigOverrides, ConfigValidationError, OutputFormat,
    ProgressMode, load_config, resolve_config,
};
pub use model::{
    FailureReason, Headings, Image, ImageOccurrence, ImageResult, ImageSummary, Issue, IssueCode,
    IssueSummary, IssueTarget, Link, LinkOccurrence, LinkResult, LinkSummary, OpenGraph, Page,
    PageImage, Report, ResultKind, Severity, Summary, TargetType,
};
pub use options::{FieldError, Options, ValidationError};
pub use progress::{PageProgress, Phase, Progress, ResourceProgress, SitemapProgress};
pub use rules::{Rule, RuleIgnore, RuleLevel};
pub use url::Url;
pub use url_compat::{TargetUrlError, parse_target};

use std::error::Error as StdError;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;

/// Sender used to receive audit progress snapshots.
pub type ProgressSender = mpsc::Sender<Progress>;

type BoxError = Box<dyn StdError + Send + Sync>;

/// An error returned by [`audit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AuditError {
    /// The supplied options do not form a valid audit policy.
    #[error(transparent)]
    InvalidOptions(#[from] ValidationError),

    /// The audit target is not a supported absolute HTTP(S) URL.
    #[error(transparent)]
    InvalidTarget(#[from] TargetUrlError),

    /// The progress receiver was closed before the audit completed.
    #[error("progress receiver closed")]
    ProgressClosed,

    /// The configured start URL is excluded by page scope.
    #[error(
        "start URL {start} is outside page scope (include_paths={include_paths:?}, exclude_paths={exclude_paths:?}); choose a start URL within scope"
    )]
    StartOutsideScope {
        start: String,
        include_paths: Vec<String>,
        exclude_paths: Vec<String>,
    },

    /// The start URL redirected to a path excluded by page scope.
    #[error(
        "start URL {start} redirects outside page scope to {destination}; choose a start URL within scope"
    )]
    StartRedirectOutsideScope { start: String, destination: String },

    /// The HTTP client could not be initialized.
    #[error("create HTTP transport: {source}")]
    TransportInitialization {
        #[source]
        source: BoxError,
    },

    /// The robots policy could not be determined safely.
    #[error("determine robots policy: {source}")]
    Robots {
        #[source]
        source: BoxError,
    },

    /// A background audit worker panicked or was terminated unexpectedly.
    #[error("audit worker failed")]
    WorkerTask(#[source] tokio::task::JoinError),
}

/// Audits a website.
///
/// The report is returned only after all phases complete. Dropping this future
/// cancels its in-flight tasks, and closing the progress receiver aborts the audit.
/// A bounded progress channel applies backpressure when its receiver is slow.
pub async fn audit(
    target: &str,
    options: Options,
    progress: Option<ProgressSender>,
) -> Result<Report, AuditError> {
    options.validate()?;

    let start_url = url_compat::parse_target(target)?;

    if !options.allows_page(&start_url) {
        return Err(AuditError::StartOutsideScope {
            start: start_url.to_string(),
            include_paths: options.include_paths.clone(),
            exclude_paths: options.exclude_paths.clone(),
        });
    }

    let transport = transport::Transport::new(&options).map_err(|source| {
        AuditError::TransportInitialization {
            source: Box::new(source),
        }
    })?;
    let mut reporter = progress::ProgressReporter::new(progress);
    let robots = Arc::new(robots::RobotsCache::new(
        options.respect_robots,
        options.user_agent.clone(),
    ));
    let options = Arc::new(options);

    let robots_url = robots::robots_url(&start_url);

    reporter
        .start_phase(progress::PhaseStart::Robots(&robots_url))
        .await?;
    reporter
        .with_receiver_open(robots.load(&start_url, &transport))
        .await?
        .map_err(|source| AuditError::Robots {
            source: Box::new(source),
        })?;

    reporter
        .start_phase(progress::PhaseStart::Crawl(&start_url))
        .await?;
    let crawled_pages = crawler::crawl(
        &start_url,
        transport.clone(),
        Arc::clone(&robots),
        Arc::clone(&options),
        &mut reporter,
    )
    .await?;

    let resource_checker = Arc::new(resource::ResourceChecker::new(transport));
    let link_urls = checker::collect_link_urls(&crawled_pages);

    reporter
        .start_phase(progress::PhaseStart::Links {
            total: link_urls.len(),
        })
        .await?;
    let checked_links = reporter
        .with_receiver_open(checker::check_links(
            link_urls,
            Arc::clone(&resource_checker),
            options.concurrency,
        ))
        .await?
        .map_err(AuditError::WorkerTask)?;
    for link in &checked_links {
        reporter.link_checked(&link.url).await?;
    }

    let checked_images = if options.images {
        let image_inputs = checker::collect_image_inputs(&crawled_pages);
        reporter
            .start_phase(progress::PhaseStart::Images {
                total: image_inputs.len(),
            })
            .await?;
        let images = reporter
            .with_receiver_open(checker::check_images(
                image_inputs,
                resource_checker,
                options.concurrency,
            ))
            .await?
            .map_err(AuditError::WorkerTask)?;
        for image in &images {
            reporter.image_checked(&image.url).await?;
        }
        images
    } else {
        reporter
            .start_phase(progress::PhaseStart::Images { total: 0 })
            .await?;
        Vec::new()
    };

    reporter.start_phase(progress::PhaseStart::Report).await?;
    // Let a progress consumer observe the final phase and close before the
    // synchronous report builder starts.
    tokio::task::yield_now().await;
    reporter.ensure_receiver_open()?;
    let report = report::build_report(
        &start_url,
        &crawled_pages,
        checked_links,
        checked_images,
        options.images,
        &options,
        time::OffsetDateTime::now_utc(),
    );

    reporter.ensure_receiver_open()?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::Duration;

    use super::{AuditError, IssueCode, Options, Phase, Rule, RuleLevel, audit};
    use crate::test_server::{TestResponse, TestServer};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn closed_progress_receiver_is_reported() {
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);

        let error = audit("https://example.com", Options::default(), Some(sender))
            .await
            .expect_err("closed progress receiver must stop the audit");

        assert!(matches!(error, AuditError::ProgressClosed));
    }

    #[tokio::test]
    async fn invalid_inputs_are_rejected_before_engine_start() {
        let error = audit("/relative", Options::default(), None)
            .await
            .expect_err("relative targets must be rejected");

        assert!(matches!(error, AuditError::InvalidTarget(_)));
    }

    #[tokio::test]
    async fn page_and_resource_order_is_independent_of_concurrency() {
        let server = TestServer::start(|request| match request.target.as_str() {
            "/" => TestResponse::new(200)
                .header("Content-Type", "text/html")
                .body(r#"<a href="/slow">slow</a><a href="/fast">fast</a>"#),
            "/slow" => TestResponse::new(200)
                .header("Content-Type", "text/html")
                .body_delay(Duration::from_millis(75))
                .body("<h1>slow</h1>"),
            "/fast" => TestResponse::new(200)
                .header("Content-Type", "text/html")
                .body("<h1>fast</h1>"),
            _ => TestResponse::new(404),
        });
        let target = server.url("/").to_string();
        let sequential_options = Options {
            max_depth: 1,
            max_pages: 3,
            respect_robots: false,
            sitemaps: false,
            images: false,
            concurrency: 1,
            ..Options::default()
        };
        let mut concurrent_options = sequential_options.clone();
        concurrent_options.concurrency = 3;

        let mut sequential = audit(&target, sequential_options, None).await.unwrap();
        let mut concurrent = audit(&target, concurrent_options, None).await.unwrap();
        sequential.audited_at = time::OffsetDateTime::UNIX_EPOCH;
        concurrent.audited_at = time::OffsetDateTime::UNIX_EPOCH;

        assert_eq!(sequential, concurrent);

        let page_urls = sequential
            .pages
            .iter()
            .map(|page| page.url.clone())
            .collect::<Vec<_>>();
        let expected_urls = vec![
            server.url("/").to_string(),
            server.url("/slow").to_string(),
            server.url("/fast").to_string(),
        ];

        assert_eq!(page_urls, expected_urls);
    }

    #[tokio::test]
    async fn initial_redirect_loads_destination_robots_and_sets_crawl_origin() {
        let destination = TestServer::start(|request| match request.target.as_str() {
            "/robots.txt" => TestResponse::new(200).body("User-agent: *\nDisallow: /blocked\n"),
            "/" => TestResponse::new(200)
                .header("Content-Type", "text/html")
                .body(r#"<a href="/child">child</a><a href="/blocked">blocked</a>"#),
            "/child" | "/blocked" => TestResponse::new(200)
                .header("Content-Type", "text/html")
                .body("<h1>page</h1>"),
            _ => TestResponse::new(404),
        });
        let destination_url = destination.url("/").to_string();
        let source = TestServer::start(move |request| match request.target.as_str() {
            "/robots.txt" => TestResponse::new(200).body("User-agent: *\nAllow: /\n"),
            "/" => TestResponse::new(302).header("Location", &destination_url),
            _ => TestResponse::new(404),
        });
        let options = Options {
            max_depth: 1,
            max_pages: 2,
            sitemaps: false,
            images: false,
            concurrency: 1,
            ..Options::default()
        };

        let source_url = source.url("/").to_string();
        let report = audit(&source_url, options, None).await.unwrap();

        let page_urls = report
            .pages
            .iter()
            .map(|page| page.url.clone())
            .collect::<Vec<_>>();
        let expected_urls = vec![source_url, destination.url("/child").to_string()];

        assert_eq!(page_urls, expected_urls);
        assert_eq!(
            destination
                .requests()
                .into_iter()
                .map(|request| request.target)
                .collect::<Vec<_>>(),
            ["/robots.txt", "/", "/child", "/child", "/blocked"]
        );
    }

    #[tokio::test]
    async fn links_and_images_share_fragment_free_results_without_losing_occurrences() {
        let resources = TestServer::start(|request| match request.target.as_str() {
            "/asset" => TestResponse::new(404).header("Content-Type", "image/png"),
            _ => TestResponse::new(404),
        });
        let resource_url = resources.url("/asset").to_string();
        let first_link = format!("{resource_url}#one");
        let second_link = format!("{resource_url}#two");
        let image = format!("{resource_url}#image");
        let body = format!(
            r#"<a href="{first_link}">one</a><iframe src="{second_link}"></iframe><img src="{image}" alt="asset">"#
        );
        let server = TestServer::start(move |request| match request.target.as_str() {
            "/" => TestResponse::new(200)
                .header("Content-Type", "text/html")
                .body(body.clone()),
            _ => TestResponse::new(404),
        });
        let options = Options {
            max_depth: 0,
            max_pages: 1,
            respect_robots: false,
            sitemaps: false,
            rules: [
                (Rule::BrokenLink, RuleLevel::Off),
                (Rule::BrokenImage, RuleLevel::Off),
            ]
            .into(),
            ..Options::default()
        };
        let target = server.url("/").to_string();

        let report = audit(&target, options, None).await.unwrap();

        assert_eq!(
            report.links.len(),
            1,
            "report: {report:#?}; requests: {:#?}",
            server.requests()
        );
        assert_eq!(report.links[0].result.status_code, Some(404));
        assert_eq!(report.links[0].found_on.len(), 2);
        assert_eq!(report.links[0].found_on[0].original_url, first_link);
        assert_eq!(report.links[0].found_on[1].original_url, second_link);
        assert_eq!(report.images.len(), 1);
        assert_eq!(report.images[0].result.status_code, Some(404));
        assert_eq!(report.images[0].found_on.len(), 1);
        assert!(
            report
                .issues
                .iter()
                .all(|issue| !matches!(issue.code, IssueCode::BrokenLink | IssueCode::BrokenImage))
        );
        assert_eq!(
            server
                .requests()
                .into_iter()
                .map(|request| request.target)
                .collect::<Vec<_>>(),
            ["/"]
        );
        assert_eq!(
            resources
                .requests()
                .into_iter()
                .map(|request| request.target)
                .collect::<Vec<_>>(),
            ["/asset"]
        );
    }

    #[tokio::test]
    async fn out_of_scope_start_is_rejected_before_network_access() {
        let server = TestServer::start(|_| TestResponse::new(200));
        let options = Options {
            include_paths: vec!["/docs".to_owned()],
            ..Options::default()
        };

        let target = server.url("/").to_string();
        let error = audit(&target, options, None).await.unwrap_err();

        assert!(matches!(error, AuditError::StartOutsideScope { .. }));
        assert!(server.requests().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn closing_progress_mid_crawl_aborts_without_a_report() {
        let server = TestServer::start(|request| {
            if request.target == "/" {
                TestResponse::new(200)
                    .header("Content-Type", "text/html")
                    .body("<a href=\"/slow\">slow</a>")
            } else {
                TestResponse::new(200)
                    .body_delay(Duration::from_secs(5))
                    .body("late")
            }
        });
        let options = Options {
            max_depth: 1,
            max_pages: 2,
            respect_robots: false,
            sitemaps: false,
            images: false,
            ..Options::default()
        };
        let (sender, mut receiver) = mpsc::channel(2);
        let target = server.url("/").to_string();
        let audit_task = tokio::spawn(async move { audit(&target, options, Some(sender)).await });
        while let Some(progress) = receiver.recv().await {
            if progress.phase == Phase::Crawl {
                break;
            }
        }
        drop(receiver);

        let error = audit_task.await.unwrap().unwrap_err();
        assert!(matches!(error, AuditError::ProgressClosed));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn aborting_audit_future_stops_launching_page_requests() {
        let server = TestServer::start(|request| {
            if request.target == "/" {
                let links = (0..6)
                    .map(|index| format!(r#"<a href="/slow-{index}">slow</a>"#))
                    .collect::<String>();
                TestResponse::new(200)
                    .header("Content-Type", "text/html")
                    .body(links)
            } else {
                TestResponse::new(200)
                    .header("Content-Type", "text/html")
                    .body_delay(Duration::from_secs(5))
                    .body("late")
            }
        });
        let options = Options {
            max_depth: 1,
            max_pages: 7,
            respect_robots: false,
            sitemaps: false,
            images: false,
            concurrency: 3,
            ..Options::default()
        };
        let target = server.url("/").to_string();
        let task = tokio::spawn(async move { audit(&target, options, None).await });

        server.wait_for_requests(4);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let count_after_abort = server.requests().len();
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(server.requests().len(), count_after_abort);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "phase 7 stress gate"]
    async fn repeated_audits_cancellation_and_bounded_bodies_do_not_leak() {
        let descriptors_before = open_file_descriptor_count();
        let stable_server = TestServer::start(|request| match request.target.as_str() {
            "/" => TestResponse::new(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(
                    "<title>Stable audit fixture title</title><meta name=\"description\" \
                     content=\"A deterministic local audit fixture description.\"><h1>Fixture</h1>",
                ),
            _ => TestResponse::new(404),
        });
        let stable_options = Options {
            max_depth: 0,
            max_pages: 1,
            respect_robots: false,
            sitemaps: false,
            images: false,
            timeout: Duration::from_secs(2),
            ..Options::default()
        };
        let stable_target = stable_server.url("/").to_string();
        let mut expected_output = None;
        let mut expected_hash = None;

        for _ in 0..100 {
            let mut report = audit(&stable_target, stable_options.clone(), None)
                .await
                .unwrap();
            report.audited_at = time::OffsetDateTime::UNIX_EPOCH;
            let output = serde_json::to_vec(&report).unwrap();
            let mut hasher = DefaultHasher::new();
            output.hash(&mut hasher);
            let output_hash = hasher.finish();

            assert_eq!(
                expected_output.get_or_insert_with(|| output.clone()),
                &output
            );
            assert_eq!(*expected_hash.get_or_insert(output_hash), output_hash);
        }
        stable_server.wait_for_idle();
        assert_eq!(stable_server.requests().len(), 100);
        drop(stable_server);

        let cancellation_server = TestServer::start(|_| {
            TestResponse::new(200)
                .header("Content-Type", "text/html")
                .body(vec![b'x'; 64 * 1024])
                .body_chunks(1024, Duration::from_millis(10))
        });
        let cancellation_target = cancellation_server.url("/").to_string();
        for request_count in 1..=25 {
            let target = cancellation_target.clone();
            let options = stable_options.clone();
            let task = tokio::spawn(async move { audit(&target, options, None).await });
            let request_started = tokio::time::timeout(Duration::from_secs(5), async {
                while cancellation_server.requests().len() < request_count {
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            })
            .await;
            assert!(
                request_started.is_ok(),
                "canceled audit did not start request {request_count}; observed {}",
                cancellation_server.requests().len()
            );
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
            cancellation_server.wait_for_idle();
        }
        let requests_after_cancellation = cancellation_server.requests().len();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            cancellation_server.requests().len(),
            requests_after_cancellation
        );
        drop(cancellation_server);

        let slowloris_server = TestServer::start(|_| {
            TestResponse::new(200)
                .header("Content-Type", "text/html")
                .body(vec![b'x'; 32])
                .body_chunks(1, Duration::from_millis(50))
        });
        let slowloris_report = audit(
            slowloris_server.url("/").as_str(),
            Options {
                timeout: Duration::from_millis(25),
                ..stable_options.clone()
            },
            None,
        )
        .await
        .unwrap();
        assert!(
            slowloris_report
                .issues
                .iter()
                .any(|issue| issue.code == IssueCode::PageCrawlFailed)
        );
        slowloris_server.wait_for_idle();
        drop(slowloris_server);

        let oversized_body = vec![b'x'; 10 * 1024 * 1024 + 1];
        let oversized_server = TestServer::start(move |_| {
            TestResponse::new(200)
                .header("Content-Type", "text/html")
                .body(oversized_body.clone())
        });
        let oversized_report = audit(oversized_server.url("/").as_str(), stable_options, None)
            .await
            .unwrap();
        assert!(
            oversized_report
                .issues
                .iter()
                .any(|issue| issue.code == IssueCode::PageCrawlFailed)
        );
        oversized_server.wait_for_idle();
        drop(oversized_server);

        tokio::time::sleep(Duration::from_millis(100)).await;
        if let (Some(before), Some(after)) = (descriptors_before, open_file_descriptor_count()) {
            assert!(
                after <= before + 4,
                "open file descriptors grew from {before} to {after}"
            );
        }
    }

    #[cfg(unix)]
    fn open_file_descriptor_count() -> Option<usize> {
        std::fs::read_dir("/dev/fd").ok().map(Iterator::count)
    }

    #[cfg(not(unix))]
    const fn open_file_descriptor_count() -> Option<usize> {
        None
    }
}
