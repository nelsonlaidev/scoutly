use std::fs;

use scoutly::{
    ConfigError, ConfigLoadOptions, ConfigOverrides, OutputFormat, ProgressMode, load_config,
    resolve_config,
};
use serde::Deserialize;

use crate::common::fixture;

#[test]
fn config_fixtures_have_expected_outcomes_and_errors() {
    let cases: Vec<ConfigCase> = serde_json::from_str(
        &fs::read_to_string(fixture("config/cases.json")).expect("read config cases"),
    )
    .expect("decode config cases");

    for case in cases {
        let result = load_config(ConfigLoadOptions {
            config_path: Some(fixture(&format!("config/{}", case.file))),
            version: "test".into(),
            ..ConfigLoadOptions::default()
        });

        match case.outcome {
            ConfigOutcome::Success => {
                assert!(case.classification.is_none(), "{}", case.file);

                let loaded = result.unwrap_or_else(|error| panic!("{}: {error}", case.file));

                assert_eq!(loaded.base.max_depth, 0, "{}", case.file);
                assert_eq!(loaded.base.max_pages, 1, "{}", case.file);
                assert_eq!(loaded.base.rate_limit, 0.0, "{}", case.file);
                assert!(!loaded.base.respect_robots, "{}", case.file);
                assert!(!loaded.base.sitemaps, "{}", case.file);
                assert!(!loaded.base.images, "{}", case.file);
                assert_eq!(loaded.base.timeout, 5_000, "{}", case.file);
                assert_eq!(loaded.base.concurrency, 1, "{}", case.file);
                assert_eq!(loaded.base.format, OutputFormat::Json, "{}", case.file);
                assert_eq!(loaded.base.progress, ProgressMode::Never, "{}", case.file);

                resolve_config(loaded.base, ConfigOverrides::default())
                    .unwrap_or_else(|error| panic!("{}: {error}", case.file));
            }
            ConfigOutcome::Failure => {
                let error = result.unwrap_err();
                let expected_path = fixture(&format!("config/{}", case.file));
                let ConfigError::Decode { path, source } = error else {
                    panic!("{}: expected decode failure", case.file);
                };

                assert_eq!(path, expected_path, "{}", case.file);

                let class = classify_decode_error(&source.to_string());

                assert_eq!(Some(class), case.classification, "{}", case.file);
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ConfigCase {
    file: String,
    outcome: ConfigOutcome,
    classification: Option<DecodeClass>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ConfigOutcome {
    Success,
    Failure,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
enum DecodeClass {
    #[serde(rename = "cannot be null")]
    CannotBeNull,
    #[serde(rename = "duplicate field")]
    DuplicateField,
    #[serde(rename = "strict mode")]
    StrictMode,
}

fn classify_decode_error(message: &str) -> DecodeClass {
    let lowercase = message.to_ascii_lowercase();

    if lowercase.contains("cannot be null") {
        DecodeClass::CannotBeNull
    } else if lowercase.contains("duplicate field") || lowercase.contains("duplicate mapping key") {
        DecodeClass::DuplicateField
    } else if lowercase.contains("unknown field") {
        DecodeClass::StrictMode
    } else {
        panic!("unclassified decode error: {message}")
    }
}
