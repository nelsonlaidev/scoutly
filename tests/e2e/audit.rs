use std::env;
use std::fs;
use std::path::PathBuf;

use crate::common::server::TestServer;
use crate::common::{AUDITED_AT_PLACEHOLDER, ORIGIN_PLACEHOLDER, fixture};
use crate::{assert_command_succeeded, run};

fn render_audit_config(directory: &tempfile::TempDir, origin: &str, name: &str) -> PathBuf {
    let template =
        fs::read_to_string(fixture("config/audit.yaml")).expect("read audit config template");

    let path = directory.path().join(name);

    fs::write(&path, template.replace(ORIGIN_PLACEHOLDER, origin))
        .expect("write rendered audit config");

    path
}

fn normalize_json_report(output: &[u8], origin: &str) -> serde_json::Value {
    let mut report: serde_json::Value =
        serde_json::from_slice(output).expect("JSON report is valid JSON");

    normalize_json_strings(&mut report, origin);

    let audited_at = report
        .get_mut("audited_at")
        .expect("JSON report includes audited_at");

    *audited_at = serde_json::Value::String(AUDITED_AT_PLACEHOLDER.to_owned());

    report
}

fn normalize_json_strings(value: &mut serde_json::Value, origin: &str) {
    match value {
        serde_json::Value::String(value) => {
            *value = value.replace(origin, ORIGIN_PLACEHOLDER);
        }
        serde_json::Value::Array(values) => {
            for value in values {
                normalize_json_strings(value, origin);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                normalize_json_strings(value, origin);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

fn normalize_text_report(output: &[u8], origin: &str) -> String {
    let text = String::from_utf8(output.to_owned()).expect("text report is UTF-8");

    text.replace(origin, ORIGIN_PLACEHOLDER)
        .lines()
        .map(|line| {
            if line.starts_with("Audited: ") {
                format!("Audited: {AUDITED_AT_PLACEHOLDER}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn assert_fixture(relative: &str, actual: &str) {
    if env::var_os("SCOUTLY_DUMP_FIXTURES").is_some() {
        println!("--- {relative} ---\n{actual}--- end {relative} ---");
        return;
    }

    let expected = fs::read_to_string(fixture(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"));

    assert_eq!(actual, expected, "fixture {relative} drifted");
}

fn assert_json_fixture(relative: &str, actual: &serde_json::Value) {
    if env::var_os("SCOUTLY_DUMP_FIXTURES").is_some() {
        println!(
            "--- {relative} ---\n{}\n--- end {relative} ---",
            serde_json::to_string_pretty(actual).expect("serialize normalized JSON")
        );
        return;
    }

    let expected: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(fixture(relative))
            .unwrap_or_else(|error| panic!("read {relative}: {error}")),
    )
    .unwrap_or_else(|error| panic!("parse {relative}: {error}"));

    assert_eq!(*actual, expected, "fixture {relative} drifted");
}

#[test]
fn audit_outputs_match_fixtures() {
    let directory = tempfile::tempdir().expect("create test temp directory");

    let json_server = TestServer::start();
    let json_config = render_audit_config(&directory, json_server.origin(), "audit-json.yaml");
    let json_output = run(&[
        "--config",
        json_config.to_str().expect("audit config path is UTF-8"),
        "--format=json",
        json_server.origin(),
    ]);

    assert_command_succeeded("JSON audit", &json_output);
    assert_json_fixture(
        "output/report.json",
        &normalize_json_report(&json_output.stdout, json_server.origin()),
    );
    assert_fixture(
        "output/progress.txt",
        &String::from_utf8(json_output.stderr.clone())
            .expect("progress output is UTF-8")
            .replace(json_server.origin(), ORIGIN_PLACEHOLDER),
    );
    assert_fixture("http/trace.json", &json_server.trace_json());

    let text_server = TestServer::start();
    let text_config = render_audit_config(&directory, text_server.origin(), "audit-text.yaml");
    let text_output = run(&[
        "--config",
        text_config.to_str().expect("audit config path is UTF-8"),
        "--format=text",
        "--progress=never",
        text_server.origin(),
    ]);

    assert_command_succeeded("text audit", &text_output);
    assert_fixture(
        "output/report.txt",
        &normalize_text_report(&text_output.stdout, text_server.origin()),
    );

    assert!(text_output.stderr.is_empty());
}
