use assert_cmd::cargo;
use predicates::prelude::*;

#[tokio::test]
async fn test_cli_help() {
    let mut cmd = cargo::cargo_bin_cmd!("scoutly");
    let assert = cmd.arg("--help").assert();

    assert
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("scoutly [OPTIONS] <URL>"));
}
