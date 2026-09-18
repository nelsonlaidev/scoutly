use std::fs;

use serde::Deserialize;

use crate::common::fixture;
use crate::common::server::TestServer;
use crate::{assert_command_succeeded, run};

#[derive(Debug, Deserialize)]
struct ConfigCase {
    file: String,
    outcome: ConfigOutcome,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ConfigOutcome {
    Success,
    Failure,
}

#[test]
fn config_files_produce_expected_cli_results() {
    let cases: Vec<ConfigCase> = serde_json::from_str(
        &fs::read_to_string(fixture("config/cases.json")).expect("read config cases"),
    )
    .expect("decode config cases");

    for case in cases {
        let server = TestServer::start();
        let config = fixture(&format!("config/{}", case.file));
        let output = run(&[
            "--config",
            config.to_str().expect("config path is UTF-8"),
            server.origin(),
        ]);

        match case.outcome {
            ConfigOutcome::Success => assert_command_succeeded(&case.file, &output),
            ConfigOutcome::Failure => {
                assert!(
                    !output.status.success(),
                    "{} unexpectedly succeeded",
                    case.file
                );
                let stderr = String::from_utf8(output.stderr).expect("config error is UTF-8");
                assert!(
                    stderr.contains("decode config"),
                    "{} stderr did not identify a config decode error:\n{stderr}",
                    case.file
                );
            }
        }
    }
}
