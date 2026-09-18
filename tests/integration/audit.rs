use std::fs;
use std::time::Duration;

use scoutly::{Options, Phase, Rule, RuleIgnore, RuleLevel};

use crate::common::server::TestServer;
use crate::common::{AUDITED_AT_PLACEHOLDER, ORIGIN_PLACEHOLDER, fixture};

#[tokio::test]
async fn audit_matches_expected_report_and_progress() {
    let server = TestServer::start();
    let options = Options {
        max_depth: 1,
        max_pages: 4,
        max_sitemap_documents: 2,
        timeout: Duration::from_secs(5),
        max_redirects: 3,
        user_agent: "scoutly-test/0.6".to_owned(),
        concurrency: 1,
        rules: [(Rule::MissingTitle, RuleLevel::Error)].into(),
        include_paths: vec!["/".to_owned()],
        exclude_paths: vec!["/private".to_owned()],
        ignore_rules: vec![RuleIgnore {
            url_prefix: format!("{}/about", server.origin()),
            rules: vec![Rule::MissingTitle],
        }],
        ..Options::default()
    };

    let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
    let collect_progress = async move {
        let mut snapshots = Vec::new();
        while let Some(progress) = receiver.recv().await {
            snapshots.push(progress);
        }
        snapshots
    };

    let (report, progress) = tokio::join!(
        scoutly::audit(server.origin(), options, Some(sender)),
        collect_progress
    );
    let report = report.expect("library audit succeeds");

    let actual = serde_json::to_value(&report).expect("serialize Rust report");
    let audited_at = actual["audited_at"]
        .as_str()
        .expect("audited_at is a JSON string");
    let expected: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(fixture("output/report.json"))
            .expect("read report fixture")
            .replace(ORIGIN_PLACEHOLDER, server.origin())
            .replace(AUDITED_AT_PLACEHOLDER, audited_at),
    )
    .expect("parse normalized report fixture");

    assert_eq!(actual, expected, "report differs from fixture");

    assert_eq!(
        server.trace_json(),
        fs::read_to_string(fixture("http/trace.json")).expect("read HTTP trace fixture"),
        "HTTP trace differs from fixture"
    );

    let phases = progress.iter().fold(Vec::new(), |mut phases, snapshot| {
        if phases.last() != Some(&snapshot.phase) {
            phases.push(snapshot.phase);
        }
        phases
    });

    assert_eq!(
        phases,
        [
            Phase::Robots,
            Phase::Crawl,
            Phase::Sitemaps,
            Phase::Links,
            Phase::Images,
            Phase::Report,
        ]
    );
    let final_progress = progress.last().expect("report progress snapshot");

    assert_eq!(final_progress.pages.discovered, 4);
    assert_eq!(final_progress.pages.crawled, 4);
    assert_eq!(final_progress.sitemaps.fetched, 1);
    assert_eq!(final_progress.links.checked, 3);
    assert_eq!(final_progress.links.total, Some(3));
    assert_eq!(final_progress.images.checked, 1);
    assert_eq!(final_progress.images.total, Some(1));
}
