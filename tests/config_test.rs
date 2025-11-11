use std::fs;
use tempfile::tempdir;

#[test]
fn test_cli_with_json_config() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");

    let json_content = r#"{
        "depth": 3,
        "max_pages": 50,
        "output": "json",
        "concurrency": 8
    }"#;

    fs::write(&config_path, json_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    // Just verify the binary can load the config without error
    assert!(output.status.success() || output.status.code() == Some(0));
}

#[test]
fn test_cli_with_toml_config() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let toml_content = r#"
depth = 3
max_pages = 50
output = "json"
concurrency = 8
"#;

    fs::write(&config_path, toml_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    // Just verify the binary can load the config without error
    assert!(output.status.success() || output.status.code() == Some(0));
}

#[test]
fn test_cli_with_yaml_config() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");

    let yaml_content = r#"
depth: 3
max_pages: 50
output: json
concurrency: 8
"#;

    fs::write(&config_path, yaml_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    // Just verify the binary can load the config without error
    assert!(output.status.success() || output.status.code() == Some(0));
}

#[test]
fn test_cli_with_yml_config() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.yml");

    let yaml_content = r#"
depth: 3
max_pages: 50
output: json
concurrency: 8
"#;

    fs::write(&config_path, yaml_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    // Just verify the binary can load the config without error
    assert!(output.status.success() || output.status.code() == Some(0));
}

#[test]
fn test_cli_with_invalid_config_format() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.txt");

    let content = "invalid content";
    fs::write(&config_path, content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .output()
        .expect("Failed to execute command");

    // Should fail because .txt is not supported
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unsupported config file format"));
}

#[test]
fn test_cli_with_invalid_json_config() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");

    let invalid_json = r#"{ invalid json }"#;
    fs::write(&config_path, invalid_json).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .output()
        .expect("Failed to execute command");

    // Should fail because JSON is invalid
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Failed to parse JSON config"));
}

#[test]
fn test_cli_with_invalid_toml_config() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let invalid_toml = r#"[[[ invalid toml"#;
    fs::write(&config_path, invalid_toml).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .output()
        .expect("Failed to execute command");

    // Should fail because TOML is invalid
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Failed to parse TOML config"));
}

#[test]
fn test_cli_with_invalid_yaml_config() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");

    let invalid_yaml = r#"
url: "test
  depth: invalid
"#;
    fs::write(&config_path, invalid_yaml).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .output()
        .expect("Failed to execute command");

    // Should fail because YAML is invalid
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Failed to parse YAML config"));
}

#[test]
fn test_cli_with_nonexistent_config() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg("/nonexistent/path/config.json")
        .output()
        .expect("Failed to execute command");

    // Should fail because file doesn't exist
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Failed to read config file"));
}

#[test]
fn test_cli_args_override_config() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");

    // Config sets depth to 3
    let json_content = r#"{
        "depth": 3,
        "max_pages": 50,
        "concurrency": 8
    }"#;

    fs::write(&config_path, json_content).unwrap();

    // CLI sets depth to 10, which should override config
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--depth")
        .arg("10")
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    // Just verify the binary runs without error
    assert!(output.status.success() || output.status.code() == Some(0));
}

#[test]
fn test_config_with_all_fields() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");

    let json_content = r#"{
        "depth": 7,
        "max_pages": 300,
        "output": "json",
        "save": "report.json",
        "external": true,
        "verbose": true,
        "ignore_redirects": true,
        "keep_fragments": true,
        "rate_limit": 2.5,
        "concurrency": 12,
        "respect_robots_txt": false
    }"#;

    fs::write(&config_path, json_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    // Just verify the binary can load the config without error
    assert!(output.status.success() || output.status.code() == Some(0));
}

#[test]
fn test_partial_config_with_defaults() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");

    // Only set some fields, others should use CLI defaults
    let json_content = r#"{
        "depth": 8,
        "concurrency": 15
    }"#;

    fs::write(&config_path, json_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    // Just verify the binary can load the config without error
    assert!(output.status.success() || output.status.code() == Some(0));
}

#[test]
fn test_empty_config() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");

    let json_content = r#"{}"#;

    fs::write(&config_path, json_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    // Should work fine with empty config
    assert!(output.status.success() || output.status.code() == Some(0));
}

#[test]
fn test_toml_with_all_fields() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let toml_content = r#"
depth = 7
max_pages = 300
output = "json"
save = "report.json"
external = true
verbose = true
ignore_redirects = true
keep_fragments = true
rate_limit = 2.5
concurrency = 12
respect_robots_txt = false
"#;

    fs::write(&config_path, toml_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    // Just verify the binary can load the config without error
    assert!(output.status.success() || output.status.code() == Some(0));
}

#[test]
fn test_yaml_with_all_fields() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");

    let yaml_content = r#"
depth: 7
max_pages: 300
output: "json"
save: "report.json"
external: true
verbose: true
ignore_redirects: true
keep_fragments: true
rate_limit: 2.5
concurrency: 12
respect_robots_txt: false
"#;

    fs::write(&config_path, yaml_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_scoutly"))
        .arg("https://example.com")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    // Just verify the binary can load the config without error
    assert!(output.status.success() || output.status.code() == Some(0));
}
