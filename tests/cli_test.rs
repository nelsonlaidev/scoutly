use assert_cmd::cargo;
use predicates::prelude::*;

#[tokio::test]
async fn test_cli_help() {
    let mut cmd = cargo::cargo_bin_cmd!("scoutly");
    let assert = cmd.arg("--help").assert();

    // On Windows, the binary name in help might be "scoutly.exe"
    let expected_pattern = if cfg!(windows) {
        "scoutly.exe [OPTIONS] <URL>"
    } else {
        "scoutly [OPTIONS] <URL>"
    };

    assert
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains(expected_pattern));
}
