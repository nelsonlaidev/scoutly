use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::Cli;

/// Configuration file structure that mirrors CLI arguments
/// All fields are optional to allow partial configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// The URL to start crawling from
    pub url: Option<String>,

    /// Maximum crawl depth
    pub depth: Option<usize>,

    /// Maximum number of pages to crawl
    pub max_pages: Option<usize>,

    /// Output format: text or json
    pub output: Option<String>,

    /// Save report to file
    pub save: Option<String>,

    /// Follow external links
    pub external: Option<bool>,

    /// Verbose output
    pub verbose: Option<bool>,

    /// Ignore redirect issues in the report
    pub ignore_redirects: Option<bool>,

    /// Treat URLs with fragment identifiers (#) as unique links
    pub keep_fragments: Option<bool>,

    /// Rate limit for requests per second
    pub rate_limit: Option<f64>,

    /// Number of concurrent requests
    pub concurrency: Option<usize>,

    /// Respect robots.txt rules
    pub respect_robots_txt: Option<bool>,
}

/// Configuration file format based on file extension
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Json,
    Toml,
    Yaml,
}

impl ConfigFormat {
    /// Detect format from file extension
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext.to_lowercase().as_str() {
                "json" => Some(ConfigFormat::Json),
                "toml" => Some(ConfigFormat::Toml),
                "yaml" | "yml" => Some(ConfigFormat::Yaml),
                _ => None,
            })
    }

    /// Get file extensions for this format
    pub fn extensions(&self) -> &[&str] {
        match self {
            ConfigFormat::Json => &["json"],
            ConfigFormat::Toml => &["toml"],
            ConfigFormat::Yaml => &["yaml", "yml"],
        }
    }
}

impl Config {
    /// Load configuration from a file
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let format = ConfigFormat::from_path(path)
            .with_context(|| format!("Unsupported config file format: {}", path.display()))?;

        let config = match format {
            ConfigFormat::Json => serde_json::from_str(&contents)
                .with_context(|| format!("Failed to parse JSON config: {}", path.display()))?,
            ConfigFormat::Toml => toml::from_str(&contents)
                .with_context(|| format!("Failed to parse TOML config: {}", path.display()))?,
            ConfigFormat::Yaml => serde_yaml::from_str(&contents)
                .with_context(|| format!("Failed to parse YAML config: {}", path.display()))?,
        };

        Ok(config)
    }

    /// Get the default configuration file paths to check (in order of priority)
    /// Returns paths in order: current directory, user config directory
    pub fn default_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Check current directory first (highest priority)
        for format in &[ConfigFormat::Json, ConfigFormat::Toml, ConfigFormat::Yaml] {
            for ext in format.extensions() {
                paths.push(PathBuf::from(format!("scoutly.{}", ext)));
            }
        }

        // Check user config directory (~/.config/scoutly)
        // Use XDG_CONFIG_HOME if set, otherwise fall back to ~/.config
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .and_then(|p| {
                if p.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(p))
                }
            })
            .or_else(|| dirs::home_dir().map(|home| home.join(".config")));

        if let Some(config_home) = config_home {
            let scoutly_config_dir = config_home.join("scoutly");
            for format in &[ConfigFormat::Json, ConfigFormat::Toml, ConfigFormat::Yaml] {
                for ext in format.extensions() {
                    paths.push(scoutly_config_dir.join(format!("config.{}", ext)));
                }
            }
        }

        paths
    }

    /// Try to load configuration from default paths
    /// Returns the first configuration file found, or None if no config exists
    pub fn from_default_paths() -> Result<Option<Self>> {
        for path in Self::default_paths() {
            if path.exists() {
                return Ok(Some(Self::from_file(&path)?));
            }
        }
        Ok(None)
    }

    /// Merge this configuration with CLI arguments
    /// CLI arguments take precedence over config file values
    pub fn merge_with_cli(&self, cli: &Cli) -> Cli {
        Cli {
            url: cli.url.clone(),
            depth: if cli.depth != 5 {
                cli.depth
            } else {
                self.depth.unwrap_or(cli.depth)
            },
            max_pages: if cli.max_pages != 200 {
                cli.max_pages
            } else {
                self.max_pages.unwrap_or(cli.max_pages)
            },
            output: if cli.output != "text" {
                cli.output.clone()
            } else {
                self.output.clone().unwrap_or_else(|| cli.output.clone())
            },
            save: cli.save.clone().or_else(|| self.save.clone()),
            external: if cli.external {
                cli.external
            } else {
                self.external.unwrap_or(cli.external)
            },
            verbose: if cli.verbose {
                cli.verbose
            } else {
                self.verbose.unwrap_or(cli.verbose)
            },
            ignore_redirects: if cli.ignore_redirects {
                cli.ignore_redirects
            } else {
                self.ignore_redirects.unwrap_or(cli.ignore_redirects)
            },
            keep_fragments: if cli.keep_fragments {
                cli.keep_fragments
            } else {
                self.keep_fragments.unwrap_or(cli.keep_fragments)
            },
            rate_limit: cli.rate_limit.or(self.rate_limit),
            concurrency: if cli.concurrency != 5 {
                cli.concurrency
            } else {
                self.concurrency.unwrap_or(cli.concurrency)
            },
            respect_robots_txt: if !cli.respect_robots_txt {
                cli.respect_robots_txt
            } else {
                self.respect_robots_txt.unwrap_or(cli.respect_robots_txt)
            },
            config: cli.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_format_from_path() {
        assert_eq!(
            ConfigFormat::from_path(Path::new("config.json")),
            Some(ConfigFormat::Json)
        );
        assert_eq!(
            ConfigFormat::from_path(Path::new("config.toml")),
            Some(ConfigFormat::Toml)
        );
        assert_eq!(
            ConfigFormat::from_path(Path::new("config.yaml")),
            Some(ConfigFormat::Yaml)
        );
        assert_eq!(
            ConfigFormat::from_path(Path::new("config.yml")),
            Some(ConfigFormat::Yaml)
        );
        assert_eq!(ConfigFormat::from_path(Path::new("config.txt")), None);
    }

    #[test]
    fn test_load_json_config() {
        let json_content = r#"
{
    "url": "https://example.com",
    "depth": 10,
    "max_pages": 500,
    "output": "json",
    "external": true,
    "verbose": true,
    "concurrency": 10
}
        "#;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().with_extension("json");
        fs::write(&temp_path, json_content).unwrap();

        let config = Config::from_file(&temp_path).unwrap();
        assert_eq!(config.url, Some("https://example.com".to_string()));
        assert_eq!(config.depth, Some(10));
        assert_eq!(config.max_pages, Some(500));
        assert_eq!(config.output, Some("json".to_string()));
        assert_eq!(config.external, Some(true));
        assert_eq!(config.verbose, Some(true));
        assert_eq!(config.concurrency, Some(10));

        fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_load_toml_config() {
        let toml_content = r#"
url = "https://example.com"
depth = 10
max_pages = 500
output = "json"
external = true
verbose = true
concurrency = 10
        "#;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().with_extension("toml");
        fs::write(&temp_path, toml_content).unwrap();

        let config = Config::from_file(&temp_path).unwrap();
        assert_eq!(config.url, Some("https://example.com".to_string()));
        assert_eq!(config.depth, Some(10));
        assert_eq!(config.max_pages, Some(500));
        assert_eq!(config.output, Some("json".to_string()));
        assert_eq!(config.external, Some(true));
        assert_eq!(config.verbose, Some(true));
        assert_eq!(config.concurrency, Some(10));

        fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_load_yaml_config() {
        let yaml_content = r#"
url: "https://example.com"
depth: 10
max_pages: 500
output: "json"
external: true
verbose: true
concurrency: 10
        "#;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().with_extension("yaml");
        fs::write(&temp_path, yaml_content).unwrap();

        let config = Config::from_file(&temp_path).unwrap();
        assert_eq!(config.url, Some("https://example.com".to_string()));
        assert_eq!(config.depth, Some(10));
        assert_eq!(config.max_pages, Some(500));
        assert_eq!(config.output, Some("json".to_string()));
        assert_eq!(config.external, Some(true));
        assert_eq!(config.verbose, Some(true));
        assert_eq!(config.concurrency, Some(10));

        fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_partial_config() {
        let json_content = r#"
{
    "depth": 15,
    "concurrency": 20
}
        "#;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().with_extension("json");
        fs::write(&temp_path, json_content).unwrap();

        let config = Config::from_file(&temp_path).unwrap();
        assert_eq!(config.url, None);
        assert_eq!(config.depth, Some(15));
        assert_eq!(config.max_pages, None);
        assert_eq!(config.concurrency, Some(20));

        fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_invalid_json_config() {
        let invalid_json = r#"{ invalid json }"#;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().with_extension("json");
        fs::write(&temp_path, invalid_json).unwrap();

        let result = Config::from_file(&temp_path);
        assert!(result.is_err());

        fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_invalid_toml_config() {
        let invalid_toml = r#"[[[ invalid toml"#;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().with_extension("toml");
        fs::write(&temp_path, invalid_toml).unwrap();

        let result = Config::from_file(&temp_path);
        assert!(result.is_err());

        fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_invalid_yaml_config() {
        let invalid_yaml = r#"
url: "test
    depth: invalid
        "#;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().with_extension("yaml");
        fs::write(&temp_path, invalid_yaml).unwrap();

        let result = Config::from_file(&temp_path);
        assert!(result.is_err());

        fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_unsupported_format() {
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().with_extension("txt");
        fs::write(&temp_path, "content").unwrap();

        let result = Config::from_file(&temp_path);
        assert!(result.is_err());

        fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_merge_with_cli_defaults() {
        let config = Config {
            depth: Some(15),
            max_pages: Some(300),
            output: Some("json".to_string()),
            concurrency: Some(10),
            ..Default::default()
        };

        let cli = Cli {
            url: "https://example.com".to_string(),
            depth: 5,
            max_pages: 200,
            output: "text".to_string(),
            save: None,
            external: false,
            verbose: false,
            ignore_redirects: false,
            keep_fragments: false,
            rate_limit: None,
            concurrency: 5,
            respect_robots_txt: true,
            config: None,
        };

        let merged = config.merge_with_cli(&cli);
        assert_eq!(merged.url, "https://example.com");
        assert_eq!(merged.depth, 15); // from config
        assert_eq!(merged.max_pages, 300); // from config
        assert_eq!(merged.output, "json"); // from config
        assert_eq!(merged.concurrency, 10); // from config
    }

    #[test]
    fn test_merge_with_cli_overrides() {
        let config = Config {
            depth: Some(15),
            max_pages: Some(300),
            output: Some("json".to_string()),
            concurrency: Some(10),
            external: Some(true),
            ..Default::default()
        };

        let cli = Cli {
            url: "https://example.com".to_string(),
            depth: 20,
            max_pages: 400,
            output: "xml".to_string(),
            save: Some("report.txt".to_string()),
            external: true,
            verbose: true,
            ignore_redirects: true,
            keep_fragments: false,
            rate_limit: Some(2.0),
            concurrency: 15,
            respect_robots_txt: false,
            config: None,
        };

        let merged = config.merge_with_cli(&cli);
        assert_eq!(merged.url, "https://example.com");
        assert_eq!(merged.depth, 20); // CLI override
        assert_eq!(merged.max_pages, 400); // CLI override
        assert_eq!(merged.output, "xml"); // CLI override
        assert_eq!(merged.concurrency, 15); // CLI override
        assert_eq!(merged.save, Some("report.txt".to_string())); // CLI value
        assert!(merged.verbose); // CLI value
        assert!(merged.ignore_redirects); // CLI value
        assert_eq!(merged.rate_limit, Some(2.0)); // CLI value
        assert!(!merged.respect_robots_txt); // CLI override
    }

    #[test]
    fn test_default_paths_exists() {
        let paths = Config::default_paths();
        assert!(!paths.is_empty());

        // Check that current directory paths are included
        assert!(
            paths
                .iter()
                .any(|p| p.to_string_lossy().contains("scoutly.json"))
        );
        assert!(
            paths
                .iter()
                .any(|p| p.to_string_lossy().contains("scoutly.toml"))
        );
        assert!(
            paths
                .iter()
                .any(|p| p.to_string_lossy().contains("scoutly.yaml"))
        );
    }

    #[test]
    fn test_from_default_paths_no_config() {
        // This test assumes no default config exists
        // In a real scenario, we'd need to ensure no config files exist
        let result = Config::from_default_paths();
        assert!(result.is_ok());
    }

    #[test]
    fn test_yaml_with_yml_extension() {
        let yaml_content = r#"
depth: 8
concurrency: 12
        "#;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().with_extension("yml");
        fs::write(&temp_path, yaml_content).unwrap();

        let config = Config::from_file(&temp_path).unwrap();
        assert_eq!(config.depth, Some(8));
        assert_eq!(config.concurrency, Some(12));

        fs::remove_file(temp_path).ok();
    }
}
