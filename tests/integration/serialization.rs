use std::fs;

use scoutly::{Progress, Report};

use crate::common::fixture;

const ORIGIN: &str = "https://example.test";
const AUDITED_AT: &str = "2026-07-31T12:00:00Z";

#[test]
fn report_json_round_trips_fixture_exactly() {
    let expected = fs::read_to_string(fixture("output/report.json"))
        .expect("read report fixture")
        .replace("{{ORIGIN}}", ORIGIN)
        .replace("{{AUDITED_AT}}", AUDITED_AT);
    let report: Report = serde_json::from_str(&expected).expect("deserialize report fixture");

    let actual = format!(
        "{}\n",
        serde_json::to_string_pretty(&report).expect("serialize Rust report")
    );

    assert_eq!(actual, expected);
    assert_eq!(report.summary.pages, report.pages.len());
    assert!(!report.pages.is_empty());
}

#[test]
fn progress_omits_only_an_empty_current_url_and_keeps_unknown_totals_null() {
    let progress: Progress = serde_json::from_str(
        r#"{
          "phase":"robots",
          "pages":{"discovered":0,"crawled":0},
          "sitemaps":{"fetched":0},
          "links":{"checked":0,"total":null},
          "images":{"checked":0,"total":null}
        }"#,
    )
    .expect("deserialize progress");

    let encoded = serde_json::to_value(progress).expect("serialize progress");

    assert!(encoded.get("current_url").is_none());
    assert!(encoded["links"]["total"].is_null());
    assert!(encoded["images"]["total"].is_null());
}
