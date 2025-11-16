mod server;

use scoutly::cli::Cli;
use scoutly::run;
use server::{get_test_server_url, start_link_test_server};
use std::fs;
use std::process::Command;

#[tokio::test]
async fn test_invalid_url_no_protocol() {
    let args = Cli {
        url: "example.com".to_string(),
        depth: 2,
        max_pages: 10,
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
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
async fn test_invalid_url_missing_https() {
    let args = Cli {
        url: "ftp://example.com".to_string(),
        depth: 2,
        max_pages: 10,
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_err(),
        "Should return error for non-HTTP(S) protocol"
    );
}

#[tokio::test]
async fn test_valid_http_url() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: base_url,
        depth: 1,
        max_pages: 5,
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: None,
    };

    let result = run(args).await;
    assert!(result.is_ok(), "Should accept http:// URLs");
}

#[tokio::test]
async fn test_valid_https_url() {
    let args = Cli {
        url: "https://example.com".to_string(),
        depth: 1,
        max_pages: 1,
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
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
async fn test_full_crawl_with_text_output() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: base_url,
        depth: 2,
        max_pages: 10,
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: None,
    };

    let result = run(args).await;
    assert!(result.is_ok(), "Should successfully crawl with text output");
}

#[tokio::test]
async fn test_full_crawl_with_json_output() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: base_url,
        depth: 2,
        max_pages: 10,
        output: "json".to_string(),
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: None,
    };

    let result = run(args).await;
    assert!(result.is_ok(), "Should successfully crawl with JSON output");
}

#[tokio::test]
async fn test_crawl_with_save_file() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;
    let test_filename = "test_report.json";

    let _ = fs::remove_file(test_filename);

    let args = Cli {
        url: base_url,
        depth: 1,
        max_pages: 5,
        output: "text".to_string(),
        save: Some(test_filename.to_string()),
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
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
async fn test_crawl_with_verbose_flag() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: base_url,
        depth: 1,
        max_pages: 3,
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: true,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with verbose output"
    );
}

#[tokio::test]
async fn test_crawl_with_external_flag() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: base_url,
        depth: 1,
        max_pages: 5,
        output: "text".to_string(),
        save: None,
        external: true,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with external links enabled"
    );
}

#[tokio::test]
async fn test_crawl_with_ignore_redirects_flag() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: base_url,
        depth: 1,
        max_pages: 5,
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: true,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with ignore_redirects enabled"
    );
}

#[tokio::test]
async fn test_crawl_with_keep_fragments_flag() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: base_url,
        depth: 1,
        max_pages: 5,
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: true,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with keep_fragments enabled"
    );
}

#[tokio::test]
async fn test_crawl_with_custom_depth_and_max_pages() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: base_url,
        depth: 3,
        max_pages: 15,
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with custom depth and max_pages"
    );
}

#[tokio::test]
async fn test_crawl_with_all_flags_combined() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;
    let test_filename = "test_report_combined.json";

    let _ = fs::remove_file(test_filename);

    let args = Cli {
        url: base_url,
        depth: 2,
        max_pages: 8,
        output: "json".to_string(),
        save: Some(test_filename.to_string()),
        external: true,
        verbose: true,
        ignore_redirects: true,
        keep_fragments: true,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
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
async fn test_crawl_with_default_text_output() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: base_url,
        depth: 1,
        max_pages: 3,
        output: "anything_else".to_string(),
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with non-json output (defaults to text)"
    );
}

#[tokio::test]
async fn test_crawl_with_save_and_json_output() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;
    let test_filename = "test_report_json_save.json";

    let _ = fs::remove_file(test_filename);

    let args = Cli {
        url: base_url,
        depth: 1,
        max_pages: 5,
        output: "json".to_string(),
        save: Some(test_filename.to_string()),
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
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
async fn test_crawl_with_verbose_and_json_output() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    let args = Cli {
        url: base_url,
        depth: 1,
        max_pages: 3,
        output: "json".to_string(),
        save: None,
        external: false,
        verbose: true,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: None,
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with verbose and JSON output"
    );
}

#[test]
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
        url: base_url,
        depth: 1,
        max_pages: 3,
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: true,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
        config: Some(config_path.to_str().unwrap().to_string()),
    };

    let result = run(args).await;
    assert!(
        result.is_ok(),
        "Should successfully crawl with config file and verbose flag"
    );
}

#[tokio::test]
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
        url: base_url,
        depth: 1,     // This should override config's depth of 5
        max_pages: 3, // This should override config's max_pages of 10
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: false,
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
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
        url: base_url,
        depth: 5,       // Default value
        max_pages: 200, // Default value
        output: "text".to_string(),
        save: None,
        external: false,
        verbose: true, // Enable verbose to trigger the println
        ignore_redirects: false,
        keep_fragments: false,
        rate_limit: None,
        concurrency: 5,
        respect_robots_txt: false,
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
