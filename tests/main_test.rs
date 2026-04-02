mod server;

use scoutly::cli::{Cli, OutputFormat};
use scoutly::run;
use server::{get_test_server_url, start_link_test_server};
use std::fs;
use std::process::Command;

#[tokio::test]
#[serial_test::serial]
async fn test_invalid_url_no_protocol() {
    let args = Cli {
        url: Some("example.com".to_string()),
        depth: Some(2),
        max_pages: Some(10),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_err(),
        "Should return error for URL without protocol"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("URL must start with http:// or https://"),
        "Error message should mention URL protocol requirement"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_invalid_url_missing_https() {
    let args = Cli {
        url: Some("ftp://example.com".to_string()),
        depth: Some(2),
        max_pages: Some(10),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_err(),
        "Should return error for non-HTTP(S) protocol"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_valid_http_url() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),
        max_pages: Some(5),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(result.is_ok(), "Should accept http:// URLs");
}

#[tokio::test]
#[serial_test::serial]
async fn test_valid_https_url() {
    let args = Cli {
        url: Some("https://example.com".to_string()),
        depth: Some(1),
        max_pages: Some(1),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(
            !error_msg.contains("URL must start with http:// or https://"),
            "Error should not be about URL protocol"
        );
    }
}

#[tokio::test]
#[serial_test::serial]
async fn test_full_crawl_with_text_output() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: Some(base_url),
        depth: Some(2),
        max_pages: Some(10),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(result.is_ok(), "Should successfully crawl with text output");
}

#[tokio::test]
#[serial_test::serial]
async fn test_full_crawl_with_json_output() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: Some(base_url),
        depth: Some(2),
        max_pages: Some(10),
        output: Some(OutputFormat::Json),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(result.is_ok(), "Should successfully crawl with JSON output");
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_save_file() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;
    let test_filename = "test_report.json";

    let _ = fs::remove_file(test_filename);

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),
        max_pages: Some(5),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: Some(test_filename.to_string()),
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(result.is_ok(), "Should successfully crawl and save file");

    assert!(
        fs::metadata(test_filename).is_ok(),
        "Report file should be created"
    );

    let file_content = fs::read_to_string(test_filename).expect("Failed to read test file");
    let json_result: Result<serde_json::Value, _> = serde_json::from_str(&file_content);
    assert!(json_result.is_ok(), "Saved file should contain valid JSON");

    let _ = fs::remove_file(test_filename);
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_verbose_flag() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),
        max_pages: Some(3),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: true,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with verbose output"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_external_flag() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),
        max_pages: Some(5),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: true,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with external links enabled"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_ignore_redirects_flag() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),
        max_pages: Some(5),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: true,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with ignore_redirects enabled"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_keep_fragments_flag() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),
        max_pages: Some(5),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: true,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with keep_fragments enabled"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_custom_depth_and_max_pages() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: Some(base_url),
        depth: Some(3),
        max_pages: Some(15),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with custom depth and max_pages"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_all_flags_combined() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;
    let test_filename = "test_report_combined.json";

    let _ = fs::remove_file(test_filename);

    let args = Cli {
        url: Some(base_url),
        depth: Some(2),
        max_pages: Some(8),
        output: Some(OutputFormat::Json),
        cli: false,
        tui: false,
        save: Some(test_filename.to_string()),
        external: true,
        verbose: true,
        ignore_redirects: true,
        keep_fragments: true,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with all flags enabled"
    );

    assert!(
        fs::metadata(test_filename).is_ok(),
        "Report file should be created with combined flags"
    );

    let _ = fs::remove_file(test_filename);
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_default_text_output() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),
        max_pages: Some(3),
        output: None,
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully fall back to classic text output in a non-interactive test context"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_cli_flag() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),
        max_pages: Some(3),
        output: None,
        cli: true,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully run in classic CLI mode"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_explicit_tui_requires_interactive_terminal() {
    let args = Cli {
        url: None,
        depth: Some(1),
        max_pages: Some(1),
        output: None,
        cli: false,
        tui: true,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let error = run(args)
        .await
        .expect_err("TUI should fail in non-interactive tests");
    assert!(error.to_string().contains("interactive terminal"));
}

#[tokio::test]
#[serial_test::serial]
async fn test_classic_mode_without_url_errors() {
    let args = Cli {
        url: None,
        depth: Some(1),
        max_pages: Some(1),
        output: None,
        cli: true,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let error = run(args)
        .await
        .expect_err("Classic CLI mode should require a URL");
    assert!(error.to_string().contains("URL is required"));
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_save_and_json_output() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;
    let test_filename = "test_report_json_save.json";

    let _ = fs::remove_file(test_filename);

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),
        max_pages: Some(5),
        output: Some(OutputFormat::Json),
        cli: false,
        tui: false,
        save: Some(test_filename.to_string()),
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with both JSON output and file save"
    );

    assert!(
        fs::metadata(test_filename).is_ok(),
        "Report file should be created"
    );
    let file_content = fs::read_to_string(test_filename).expect("Failed to read test file");
    let json_result: Result<serde_json::Value, _> = serde_json::from_str(&file_content);
    assert!(json_result.is_ok(), "Saved file should contain valid JSON");

    let _ = fs::remove_file(test_filename);
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_verbose_and_json_output() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),
        max_pages: Some(3),
        output: Some(OutputFormat::Json),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: true,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with verbose and JSON output"
    );
}

#[test]
#[serial_test::serial]
fn test_binary_with_invalid_url() {
    let output = Command::new("cargo")
        .args(["run", "--", "example.com"])
        .output()
        .expect("Failed to run binary");

    assert!(!output.status.success(), "Should exit with error code");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("URL must start with http:// or https://"),
        "Error message should mention URL protocol requirement"
    );
}

#[test]
#[serial_test::serial]
fn test_binary_with_valid_url() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "https://example.com",
            "--depth",
            "1",
            "--max-pages",
            "1",
        ])
        .output()
        .expect("Failed to run binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("URL must start with http:// or https://"),
        "Should not fail URL validation for valid URL"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_binary_json_output_is_valid_json() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let output = tokio::task::spawn_blocking(move || {
        Command::new("cargo")
            .args([
                "run",
                "--",
                base_url.as_str(),
                "--depth",
                "1",
                "--max-pages",
                "1",
                "--output",
                "json",
            ])
            .output()
            .expect("Failed to run binary")
    })
    .await
    .expect("Binary execution task should complete");

    assert!(
        output.status.success(),
        "JSON mode should exit successfully"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_result: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(
        json_result.is_ok(),
        "stdout should contain valid JSON only, got: {stdout}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_crawl_with_config_file_verbose() {
    use tempfile::tempdir;

    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let dir = tempdir().unwrap();
    let config_path = dir.path().join("test_config.json");

    let json_content = r#"{
        "depth": 1,
        "max_pages": 3,
        "verbose": true
    }"#;

    fs::write(&config_path, json_content).unwrap();

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),
        max_pages: Some(3),
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: true,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: Some(config_path.to_str().unwrap().to_string()),
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with config file and verbose flag"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_config_merge_with_cli() {
    use tempfile::tempdir;

    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let dir = tempdir().unwrap();
    let config_path = dir.path().join("test_config.json");

    // Config file sets depth to 5, but CLI will override it
    let json_content = r#"{
        "depth": 5,
        "max_pages": 10,
        "verbose": false
    }"#;

    fs::write(&config_path, json_content).unwrap();

    let args = Cli {
        url: Some(base_url),
        depth: Some(1),     // This should override config's depth of 5
        max_pages: Some(3), // This should override config's max_pages of 10
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: Some(config_path.to_str().unwrap().to_string()),
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully merge config with CLI args"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_load_default_config_with_verbose() {
    use std::env;
    use tempfile::tempdir;

    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    // Create a temporary directory and set it as current directory
    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(&temp_dir).unwrap();

    // Create a default config file (scoutly.json)
    let config_path = temp_dir.path().join("scoutly.json");
    let json_content = r#"{
        "depth": 1,
        "max_pages": 3
    }"#;
    fs::write(&config_path, json_content).unwrap();

    let args = Cli {
        url: Some(base_url),
        depth: None,     // Allow default-path config to supply the value
        max_pages: None, // Allow default-path config to supply the value
        output: Some(OutputFormat::Text),
        cli: false,
        tui: false,
        save: None,
        external: false,
        verbose: true, // Enable verbose to trigger the println
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: Some(5),
        respect_robots_txt: Some(false),
        config: None, // No config specified, should load from default path
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully load default config with verbose"
    );

    // Restore original directory
    env::set_current_dir(&original_dir).ok();
}
