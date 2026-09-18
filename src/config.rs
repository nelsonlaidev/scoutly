use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::ValueEnum;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::{FieldError, Options, Rule, RuleIgnore, RuleLevel};

pub const CONFIG_CANDIDATE_NAMES: [&str; 12] = [
    "scoutly.config.json",
    "scoutly.config.yaml",
    "scoutly.config.yml",
    "scoutly.config.toml",
    "scoutly.json",
    "scoutly.yaml",
    "scoutly.yml",
    "scoutly.toml",
    ".scoutly.json",
    ".scoutly.yaml",
    ".scoutly.yml",
    ".scoutly.toml",
];

const CONFIG_MAX_INPUT_BYTES: usize = 1024 * 1024;
const YAML_MAX_EVENTS: usize = 100_000;
const YAML_MAX_ALIASES: usize = 1_000;
const YAML_MAX_ANCHORS: usize = 1_000;
const YAML_MAX_DEPTH: usize = 64;
const YAML_MAX_DOCUMENTS: usize = 1;
const YAML_MAX_NODES: usize = 50_000;
const YAML_MAX_MERGE_KEYS: usize = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ProgressMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub max_depth: u32,
    pub max_pages: usize,
    pub keep_fragments: bool,
    pub ignore_redirects: bool,
    pub rate_limit: f64,
    pub respect_robots: bool,
    pub sitemaps: bool,
    pub images: bool,
    pub max_sitemap_documents: usize,
    pub timeout: u64,
    pub max_redirects: u32,
    pub user_agent: String,
    pub concurrency: usize,
    pub format: OutputFormat,
    pub progress: ProgressMode,
    pub rules: BTreeMap<Rule, RuleLevel>,
    pub include_paths: Vec<String>,
    pub exclude_paths: Vec<String>,
    pub ignore_rules: Vec<RuleIgnore>,
}

impl Config {
    #[must_use]
    pub fn defaults(version: &str) -> Self {
        let version = if version.is_empty() { "dev" } else { version };
        let options = Options::default();

        Self {
            max_depth: options.max_depth,
            max_pages: options.max_pages,
            keep_fragments: options.keep_fragments,
            ignore_redirects: options.ignore_redirects,
            rate_limit: options.rate_limit,
            respect_robots: options.respect_robots,
            sitemaps: options.sitemaps,
            images: options.images,
            max_sitemap_documents: options.max_sitemap_documents,
            timeout: u64::try_from(options.timeout.as_millis())
                .expect("the default timeout fits in milliseconds"),
            max_redirects: options.max_redirects,
            user_agent: format!("scoutly/{version}"),
            concurrency: options.concurrency,
            format: OutputFormat::Text,
            progress: ProgressMode::Auto,
            rules: options.rules,
            include_paths: options.include_paths,
            exclude_paths: options.exclude_paths,
            ignore_rules: options.ignore_rules,
        }
    }

    #[must_use]
    pub fn audit_options(&self) -> Options {
        self.clone().into_audit_options()
    }

    #[must_use]
    pub fn into_audit_options(self) -> Options {
        Options {
            max_depth: self.max_depth,
            max_pages: self.max_pages,
            keep_fragments: self.keep_fragments,
            ignore_redirects: self.ignore_redirects,
            rate_limit: self.rate_limit,
            respect_robots: self.respect_robots,
            sitemaps: self.sitemaps,
            images: self.images,
            max_sitemap_documents: self.max_sitemap_documents,
            timeout: Duration::from_millis(self.timeout),
            max_redirects: self.max_redirects,
            user_agent: self.user_agent,
            concurrency: self.concurrency,
            rules: self.rules,
            include_paths: self.include_paths,
            exclude_paths: self.exclude_paths,
            ignore_rules: self.ignore_rules,
        }
    }

    fn with_overrides(mut self, overrides: ConfigOverrides) -> Self {
        macro_rules! replace {
            ($($field:ident),+ $(,)?) => {
                $(if let Some(value) = overrides.$field {
                    self.$field = value;
                })+
            };
        }

        replace!(
            max_depth,
            max_pages,
            keep_fragments,
            ignore_redirects,
            rate_limit,
            respect_robots,
            sitemaps,
            images,
            max_sitemap_documents,
            timeout,
            max_redirects,
            concurrency,
            format,
            progress,
            user_agent,
            include_paths,
            exclude_paths,
            ignore_rules,
        );

        if let Some(rules) = overrides.rules {
            self.rules.extend(rules);
        }

        self
    }

    fn validate(&self) -> Result<(), ConfigValidationError> {
        let fields = self
            .audit_options()
            .validate()
            .map_or_else(|error| error.fields, |()| Vec::new());

        if fields.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError { fields })
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigOverrides {
    pub max_depth: Option<u32>,
    pub max_pages: Option<usize>,
    pub keep_fragments: Option<bool>,
    pub ignore_redirects: Option<bool>,
    pub rate_limit: Option<f64>,
    pub respect_robots: Option<bool>,
    pub sitemaps: Option<bool>,
    pub images: Option<bool>,
    pub max_sitemap_documents: Option<usize>,
    pub timeout: Option<u64>,
    pub max_redirects: Option<u32>,
    pub user_agent: Option<String>,
    pub concurrency: Option<usize>,
    pub format: Option<OutputFormat>,
    pub progress: Option<ProgressMode>,
    pub rules: Option<BTreeMap<Rule, RuleLevel>>,
    pub include_paths: Option<Vec<String>>,
    pub exclude_paths: Option<Vec<String>>,
    pub ignore_rules: Option<Vec<RuleIgnore>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConfigLoadOptions {
    pub directory: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub disable_discovery: bool,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigLoadResult {
    pub config_path: Option<PathBuf>,
    pub base: Config,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmbiguousConfigError {
    pub paths: Vec<PathBuf>,
}

impl fmt::Display for AmbiguousConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let paths = self
            .paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        write!(formatter, "multiple Scoutly config files found: {paths}")
    }
}

impl Error for AmbiguousConfigError {}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config path and disabled config discovery cannot be used together")]
    InvalidSourceSelection,

    #[error("get working directory: {0}")]
    CurrentDirectory(#[source] std::io::Error),

    #[error("resolve home directory in config path {path:?}: home directory is unavailable")]
    HomeDirectory { path: PathBuf },

    #[error(transparent)]
    Ambiguous(#[from] AmbiguousConfigError),

    #[error("{operation} config {path:?}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("config {path:?} is not a regular file")]
    NotRegular { path: PathBuf },

    #[error("unsupported config format {extension:?}")]
    UnsupportedFormat { extension: String },

    #[error("decode config {path:?}: {source}")]
    Decode {
        path: PathBuf,
        #[source]
        source: ConfigDecodeError,
    },
}

#[derive(Debug)]
pub struct ConfigDecodeError(Box<DecodeErrorKind>);

impl fmt::Display for ConfigDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for ConfigDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.0.as_ref() {
            DecodeErrorKind::Json(error) => Some(error),
            DecodeErrorKind::Yaml(error) => Some(error),
            DecodeErrorKind::Toml(error) => Some(error),
            DecodeErrorKind::Utf8(error) => Some(error),
            DecodeErrorKind::InputTooLarge { .. } | DecodeErrorKind::NullField { .. } => None,
        }
    }
}

#[derive(Debug, Error)]
enum DecodeErrorKind {
    #[error(transparent)]
    Json(serde_json::Error),

    #[error(transparent)]
    Yaml(serde_saphyr::Error),

    #[error(transparent)]
    Toml(toml::de::Error),

    #[error(transparent)]
    Utf8(std::str::Utf8Error),

    #[error("input exceeds {limit}-byte limit")]
    InputTooLarge { limit: usize },

    #[error("field {field:?} cannot be null")]
    NullField { field: &'static str },
}

impl ConfigDecodeError {
    fn json(error: serde_json::Error) -> Self {
        Self(Box::new(DecodeErrorKind::Json(error)))
    }

    fn yaml(error: serde_saphyr::Error) -> Self {
        Self(Box::new(DecodeErrorKind::Yaml(error)))
    }

    fn toml(error: toml::de::Error) -> Self {
        Self(Box::new(DecodeErrorKind::Toml(error)))
    }

    fn utf8(error: std::str::Utf8Error) -> Self {
        Self(Box::new(DecodeErrorKind::Utf8(error)))
    }

    fn input_too_large() -> Self {
        Self(Box::new(DecodeErrorKind::InputTooLarge {
            limit: CONFIG_MAX_INPUT_BYTES,
        }))
    }

    fn null_field(field: &'static str) -> Self {
        Self(Box::new(DecodeErrorKind::NullField { field }))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigValidationError {
    pub fields: Vec<FieldError>,
}

impl fmt::Display for ConfigValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid configuration:")?;
        for field in &self.fields {
            write!(formatter, "\n- {} {}", field.field, field.message)?;
        }
        Ok(())
    }
}

impl Error for ConfigValidationError {}

pub fn load_config(options: ConfigLoadOptions) -> Result<ConfigLoadResult, ConfigError> {
    let base = Config::defaults(&options.version);
    let config_path = options
        .config_path
        .filter(|path| !path.as_os_str().is_empty());
    if config_path.is_some() && options.disable_discovery {
        return Err(ConfigError::InvalidSourceSelection);
    }
    if options.disable_discovery {
        return Ok(ConfigLoadResult {
            config_path: None,
            base,
        });
    }

    let directory = match options
        .directory
        .filter(|path| !path.as_os_str().is_empty())
    {
        Some(directory) => directory,
        None => env::current_dir().map_err(ConfigError::CurrentDirectory)?,
    };
    let config_path = match config_path {
        Some(path) => Some(resolve_explicit_path(&directory, &path)?),
        None => discover_config(&directory)?,
    };
    let Some(config_path) = config_path else {
        return Ok(ConfigLoadResult {
            config_path: None,
            base,
        });
    };

    require_regular_file(&config_path, "stat")?;

    let overrides = decode_file(&config_path)?;

    Ok(ConfigLoadResult {
        config_path: Some(config_path),
        base: base.with_overrides(overrides),
    })
}

pub fn resolve_config(
    base: Config,
    overrides: ConfigOverrides,
) -> Result<Config, ConfigValidationError> {
    let resolved = base.with_overrides(overrides);
    resolved.validate()?;
    Ok(resolved)
}

fn resolve_explicit_path(directory: &Path, path: &Path) -> Result<PathBuf, ConfigError> {
    let require_home = || {
        home_directory().ok_or_else(|| ConfigError::HomeDirectory {
            path: path.to_owned(),
        })
    };
    let expanded = if path == Path::new("~") {
        require_home()?
    } else if let Some(remainder) = path.to_str().and_then(|value| value.strip_prefix("~/")) {
        require_home()?.join(remainder)
    } else {
        path.to_owned()
    };

    Ok(if expanded.is_absolute() {
        expanded
    } else {
        directory.join(expanded)
    })
}

fn home_directory() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .map(PathBuf::from)
}

fn discover_config(directory: &Path) -> Result<Option<PathBuf>, ConfigError> {
    let mut paths = Vec::new();

    for name in CONFIG_CANDIDATE_NAMES {
        let path = directory.join(name);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => paths.push(path),
            Ok(_) => return Err(ConfigError::NotRegular { path }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(ConfigError::Io {
                    operation: "stat candidate",
                    path,
                    source,
                });
            }
        }
    }

    match paths.len() {
        0 => Ok(None),
        1 => Ok(paths.pop()),
        _ => Err(AmbiguousConfigError { paths }.into()),
    }
}

fn require_regular_file(path: &Path, operation: &'static str) -> Result<(), ConfigError> {
    let metadata = fs::metadata(path).map_err(|source| ConfigError::Io {
        operation,
        path: path.to_owned(),
        source,
    })?;

    if !metadata.is_file() {
        return Err(ConfigError::NotRegular {
            path: path.to_owned(),
        });
    }

    Ok(())
}

fn decode_file(path: &Path) -> Result<ConfigOverrides, ConfigError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if !matches!(extension.as_str(), "json" | "yaml" | "yml" | "toml") {
        return Err(ConfigError::UnsupportedFormat {
            extension: path
                .extension()
                .map_or_else(String::new, |value| format!(".{}", value.to_string_lossy())),
        });
    }

    let content = read_config_bytes(path)?;
    let raw = match extension.as_str() {
        "json" => serde_json::from_slice::<RawConfig>(&content).map_err(ConfigDecodeError::json),
        "yaml" | "yml" => decode_yaml(&content).map_err(ConfigDecodeError::yaml),
        "toml" => std::str::from_utf8(&content)
            .map_err(ConfigDecodeError::utf8)
            .and_then(|content| {
                toml::from_str::<RawConfig>(content).map_err(ConfigDecodeError::toml)
            }),
        _ => unreachable!("supported config extensions were checked above"),
    }
    .and_then(RawConfig::into_overrides)
    .map_err(|source| ConfigError::Decode {
        path: path.to_owned(),
        source,
    })?;

    Ok(raw)
}

fn read_config_bytes(path: &Path) -> Result<Vec<u8>, ConfigError> {
    let file = File::open(path).map_err(|source| ConfigError::Io {
        operation: "open",
        path: path.to_owned(),
        source,
    })?;

    let mut content = Vec::new();

    file.take(CONFIG_MAX_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut content)
        .map_err(|source| ConfigError::Io {
            operation: "read",
            path: path.to_owned(),
            source,
        })?;

    if content.len() > CONFIG_MAX_INPUT_BYTES {
        return Err(ConfigError::Decode {
            path: path.to_owned(),
            source: ConfigDecodeError::input_too_large(),
        });
    }

    Ok(content)
}

fn decode_yaml(content: &[u8]) -> Result<RawConfig, serde_saphyr::Error> {
    let options = serde_saphyr::options! {
        budget: serde_saphyr::budget! {
            max_reader_input_bytes: Some(CONFIG_MAX_INPUT_BYTES),
            max_events: YAML_MAX_EVENTS,
            max_aliases: YAML_MAX_ALIASES,
            max_anchors: YAML_MAX_ANCHORS,
            max_depth: YAML_MAX_DEPTH,
            max_documents: YAML_MAX_DOCUMENTS,
            max_nodes: YAML_MAX_NODES,
            max_total_scalar_bytes: CONFIG_MAX_INPUT_BYTES,
            max_merge_keys: YAML_MAX_MERGE_KEYS,
        },
        emit_comments: false,
        duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
        merge_keys: serde_saphyr::MergeKeyPolicy::Error,
        strict_booleans: true,
        no_schema: true,
        reject_unsupported_tags: true,
    };

    serde_saphyr::from_slice_with_options(content, options)
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawConfig {
    max_depth: Presence<u32>,
    max_pages: Presence<usize>,
    keep_fragments: Presence<bool>,
    ignore_redirects: Presence<bool>,
    rate_limit: Presence<f64>,
    respect_robots: Presence<bool>,
    sitemaps: Presence<bool>,
    images: Presence<bool>,
    max_sitemap_documents: Presence<usize>,
    timeout: Presence<u64>,
    max_redirects: Presence<u32>,
    user_agent: Presence<String>,
    concurrency: Presence<usize>,
    format: Presence<OutputFormat>,
    progress: Presence<ProgressMode>,
    rules: Presence<StrictRules>,
    include_paths: Presence<Vec<String>>,
    exclude_paths: Presence<Vec<String>>,
    ignore_rules: Presence<Vec<RuleIgnore>>,
}

impl RawConfig {
    fn into_overrides(self) -> Result<ConfigOverrides, ConfigDecodeError> {
        Ok(ConfigOverrides {
            max_depth: self.max_depth.into_option("max_depth")?,
            max_pages: self.max_pages.into_option("max_pages")?,
            keep_fragments: self.keep_fragments.into_option("keep_fragments")?,
            ignore_redirects: self.ignore_redirects.into_option("ignore_redirects")?,
            rate_limit: self.rate_limit.into_option("rate_limit")?,
            respect_robots: self.respect_robots.into_option("respect_robots")?,
            sitemaps: self.sitemaps.into_option("sitemaps")?,
            images: self.images.into_option("images")?,
            max_sitemap_documents: self
                .max_sitemap_documents
                .into_option("max_sitemap_documents")?,
            timeout: self.timeout.into_option("timeout")?,
            max_redirects: self.max_redirects.into_option("max_redirects")?,
            user_agent: self.user_agent.into_option("user_agent")?,
            concurrency: self.concurrency.into_option("concurrency")?,
            format: self.format.into_option("format")?,
            progress: self.progress.into_option("progress")?,
            rules: self.rules.into_option("rules")?.map(|rules| rules.0),
            include_paths: self.include_paths.into_option("include_paths")?,
            exclude_paths: self.exclude_paths.into_option("exclude_paths")?,
            ignore_rules: self.ignore_rules.into_option("ignore_rules")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
enum Presence<T> {
    #[default]
    Missing,
    Null,
    Value(T),
}

impl<T> Presence<T> {
    fn into_option(self, field: &'static str) -> Result<Option<T>, ConfigDecodeError> {
        match self {
            Self::Missing => Ok(None),
            Self::Null => Err(ConfigDecodeError::null_field(field)),
            Self::Value(value) => Ok(Some(value)),
        }
    }
}

impl<'de, T> Deserialize<'de> for Presence<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(|value| match value {
            Some(value) => Self::Value(value),
            None => Self::Null,
        })
    }
}

#[derive(Clone, Debug)]
struct StrictRules(BTreeMap<Rule, RuleLevel>);

impl<'de> Deserialize<'de> for StrictRules {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RulesVisitor;

        impl<'de> Visitor<'de> for RulesVisitor {
            type Value = StrictRules;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a mapping of rule names to rule levels")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut rules = BTreeMap::new();

                while let Some((rule, level)) = map.next_entry::<Rule, RuleLevel>()? {
                    if rules.insert(rule, level).is_some() {
                        return Err(serde::de::Error::custom(format!("duplicate rule {rule:?}")));
                    }
                }

                Ok(StrictRules(rules))
            }
        }

        deserializer.deserialize_map(RulesVisitor)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{
        CONFIG_CANDIDATE_NAMES, CONFIG_MAX_INPUT_BYTES, Config, ConfigError, ConfigLoadOptions,
        ConfigOverrides, OutputFormat, ProgressMode, home_directory, load_config, resolve_config,
        resolve_explicit_path,
    };
    use crate::{Rule, RuleIgnore, RuleLevel};

    #[test]
    fn defaults_and_presence_aware_overrides_are_preserved() {
        let base = Config::defaults("1.2.3");

        assert_eq!(base.max_depth, 10);
        assert_eq!(base.timeout, 30_000);
        assert_eq!(base.user_agent, "scoutly/1.2.3");
        assert_eq!(base.format, OutputFormat::Text);
        assert_eq!(base.progress, ProgressMode::Auto);
        assert_eq!(
            Config::defaults("test").audit_options(),
            Config::defaults("test").into_audit_options()
        );

        let overrides = ConfigOverrides {
            max_depth: Some(0),
            rate_limit: Some(0.0),
            respect_robots: Some(false),
            include_paths: Some(Vec::new()),
            ..ConfigOverrides::default()
        };
        let resolved = resolve_config(base, overrides).unwrap();

        assert_eq!(resolved.max_depth, 0);
        assert_eq!(resolved.rate_limit, 0.0);
        assert!(!resolved.respect_robots);
        assert!(resolved.include_paths.is_empty());
    }

    #[test]
    fn rules_merge_while_scope_lists_replace() {
        let mut base = Config::defaults("dev");

        base.rules.insert(Rule::MissingTitle, RuleLevel::Warning);
        base.include_paths = vec!["/old".into()];

        let overrides = ConfigOverrides {
            rules: Some([(Rule::ThinContent, RuleLevel::Error)].into()),
            include_paths: Some(vec!["/docs".into()]),
            ignore_rules: Some(vec![RuleIgnore {
                url_prefix: "https://example.com/docs".into(),
                rules: vec![Rule::BrokenLink],
            }]),
            ..ConfigOverrides::default()
        };

        let resolved = resolve_config(base, overrides).unwrap();

        assert_eq!(resolved.rules.len(), 2);
        assert_eq!(resolved.include_paths, ["/docs"]);
        assert_eq!(resolved.ignore_rules.len(), 1);
    }

    #[test]
    fn zero_timeout_is_rejected_and_the_full_u64_range_is_supported() {
        let mut config = Config::defaults("test");
        config.timeout = 0;

        assert!(config.audit_options().timeout.is_zero());

        let error = resolve_config(config, ConfigOverrides::default()).unwrap_err();
        assert_eq!(error.fields.len(), 1);
        assert_eq!(error.fields[0].field, "timeout");

        let mut boundary = Config::defaults("test");
        boundary.timeout = u64::MAX;

        assert_eq!(
            boundary.audit_options().timeout.as_millis(),
            u64::MAX as u128
        );
        assert!(resolve_config(boundary, ConfigOverrides::default()).is_ok());
    }

    #[test]
    fn output_modes_are_decoded_as_closed_enums() {
        let directory = TestDirectory::new();

        for (name, content) in [
            ("format.json", r#"{"format":"xml"}"#),
            ("progress.yaml", "progress: sometimes\n"),
            ("format.toml", "format = \"xml\"\n"),
        ] {
            let path = directory.write(name, content);

            assert!(
                load_config(ConfigLoadOptions {
                    config_path: Some(path),
                    ..ConfigLoadOptions::default()
                })
                .is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn every_format_preserves_zero_and_false_and_rejects_null() {
        let directory = TestDirectory::new();

        for (name, content) in [
            (
                "explicit.json",
                r#"{"max_depth":0,"rate_limit":0,"respect_robots":false}"#,
            ),
            (
                "explicit.yaml",
                "max_depth: 0\nrate_limit: 0\nrespect_robots: false\n",
            ),
            (
                "explicit.toml",
                "max_depth = 0\nrate_limit = 0\nrespect_robots = false\n",
            ),
        ] {
            let path = directory.write(name, content);
            let result = load_config(ConfigLoadOptions {
                directory: Some(directory.path().to_owned()),
                config_path: Some(path),
                version: "test".into(),
                ..ConfigLoadOptions::default()
            })
            .unwrap();

            assert_eq!(result.base.max_depth, 0);
            assert_eq!(result.base.rate_limit, 0.0);
            assert!(!result.base.respect_robots);
        }

        for (name, content) in [
            ("null.json", r#"{"max_depth":null}"#),
            ("null.yaml", "max_depth: null\n"),
        ] {
            let path = directory.write(name, content);
            let error = load_config(ConfigLoadOptions {
                config_path: Some(path),
                ..ConfigLoadOptions::default()
            })
            .unwrap_err();

            assert!(error.to_string().contains("cannot be null"));
        }
    }

    #[test]
    fn strict_decoders_reject_unknown_duplicate_and_multiple_documents() {
        let directory = TestDirectory::new();

        for (name, content) in [
            ("unknown.json", r#"{"max_dept":1}"#),
            ("duplicate.json", r#"{"max_depth":1,"max_depth":2}"#),
            ("multiple.json", r#"{"max_depth":1} {"max_depth":2}"#),
            ("unknown.yaml", "max_dept: 1\n"),
            ("duplicate.yaml", "max_depth: 1\nmax_depth: 2\n"),
            ("multiple.yaml", "max_depth: 1\n---\nmax_depth: 2\n"),
            ("legacy.yaml", "respect_robots: yes\n"),
            ("unknown.toml", "max_dept = 1\n"),
            ("duplicate.toml", "max_depth = 1\nmax_depth = 2\n"),
        ] {
            let path = directory.write(name, content);

            assert!(
                load_config(ConfigLoadOptions {
                    config_path: Some(path),
                    ..ConfigLoadOptions::default()
                })
                .is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn every_format_decodes_typed_rules_and_scope_lists() {
        let directory = TestDirectory::new();

        for (name, content) in [
            (
                "scope.json",
                r#"{"rules":{"broken_link":"off"},"include_paths":["/docs"],"exclude_paths":["/docs/archive"],"ignore_rules":[{"url_prefix":"https://example.com/old","rules":["broken_link"]}]}"#,
            ),
            (
                "scope.yaml",
                "rules:\n  broken_link: off\ninclude_paths: [/docs]\nexclude_paths: [/docs/archive]\nignore_rules:\n  - url_prefix: https://example.com/old\n    rules: [broken_link]\n",
            ),
            (
                "scope.toml",
                "include_paths = [\"/docs\"]\nexclude_paths = [\"/docs/archive\"]\n[rules]\nbroken_link = \"off\"\n[[ignore_rules]]\nurl_prefix = \"https://example.com/old\"\nrules = [\"broken_link\"]\n",
            ),
        ] {
            let path = directory.write(name, content);
            let loaded = load_config(ConfigLoadOptions {
                config_path: Some(path),
                ..ConfigLoadOptions::default()
            })
            .unwrap_or_else(|error| panic!("{name}: {error}"));

            assert_eq!(loaded.base.rules[&Rule::BrokenLink], RuleLevel::Off);
            assert_eq!(loaded.base.include_paths, ["/docs"]);
            assert_eq!(loaded.base.exclude_paths, ["/docs/archive"]);
            assert_eq!(loaded.base.ignore_rules[0].rules, [Rule::BrokenLink]);
            resolve_config(loaded.base, ConfigOverrides::default()).unwrap();
        }
    }

    #[test]
    fn discovery_accepts_each_compatible_candidate_name() {
        for name in CONFIG_CANDIDATE_NAMES {
            let directory = TestDirectory::new();

            let content = if name.ends_with(".json") {
                r#"{"max_depth":3}"#
            } else if name.ends_with(".toml") {
                "max_depth = 3\n"
            } else {
                "max_depth: 3\n"
            };

            let expected_path = directory.write(name, content);

            let loaded = load_config(ConfigLoadOptions {
                directory: Some(directory.path().to_owned()),
                ..ConfigLoadOptions::default()
            })
            .unwrap_or_else(|error| panic!("{name}: {error}"));

            assert_eq!(loaded.config_path.as_deref(), Some(expected_path.as_path()));
            assert_eq!(loaded.base.max_depth, 3);
        }
    }

    #[test]
    fn discovery_is_ordered_and_ambiguous_files_are_rejected() {
        assert_eq!(CONFIG_CANDIDATE_NAMES.len(), 12);

        let directory = TestDirectory::new();
        directory.write("scoutly.json", r#"{"max_depth":1}"#);
        directory.write(".scoutly.toml", "max_depth = 2\n");

        let error = load_config(ConfigLoadOptions {
            directory: Some(directory.path().to_owned()),
            ..ConfigLoadOptions::default()
        })
        .unwrap_err();

        let ConfigError::Ambiguous(ambiguous) = error else {
            panic!("expected ambiguous config error");
        };

        assert_eq!(ambiguous.paths.len(), 2);
        assert!(ambiguous.to_string().contains("scoutly.json"));
    }

    #[test]
    fn every_format_rejects_oversized_input() {
        let directory = TestDirectory::new();

        for (name, prefix) in [
            ("large.json", " "),
            ("large.yaml", "#"),
            ("large.toml", "#"),
        ] {
            let path = directory.write(
                name,
                &format!("{prefix}{}", "x".repeat(CONFIG_MAX_INPUT_BYTES)),
            );

            let error = load_config(ConfigLoadOptions {
                config_path: Some(path),
                ..ConfigLoadOptions::default()
            })
            .unwrap_err();

            let ConfigError::Decode { source, .. } = error else {
                panic!("{name}: expected decode error");
            };

            assert_eq!(
                source.to_string(),
                format!("input exceeds {CONFIG_MAX_INPUT_BYTES}-byte limit"),
                "{name}"
            );
        }
    }

    #[test]
    fn parser_errors_preserve_their_source() {
        let directory = TestDirectory::new();
        let path = directory.write("invalid.json", "{");

        let error = load_config(ConfigLoadOptions {
            config_path: Some(path),
            ..ConfigLoadOptions::default()
        })
        .unwrap_err();

        let ConfigError::Decode { source, .. } = &error else {
            panic!("expected decode error");
        };

        assert!(error.source().is_some());
        assert!(source.source().is_some());
    }

    #[test]
    fn explicit_config_source_errors_and_relative_paths_are_classified() {
        let directory = TestDirectory::new();

        let error = load_config(ConfigLoadOptions {
            directory: Some(directory.path().to_owned()),
            config_path: Some(PathBuf::from("scoutly.toml")),
            disable_discovery: true,
            ..ConfigLoadOptions::default()
        })
        .unwrap_err();

        assert!(matches!(error, ConfigError::InvalidSourceSelection));

        let relative = directory.write("relative.toml", "max_depth = 2\n");
        let loaded = load_config(ConfigLoadOptions {
            directory: Some(directory.path().to_owned()),
            config_path: Some(PathBuf::from("relative.toml")),
            ..ConfigLoadOptions::default()
        })
        .unwrap();

        assert_eq!(loaded.config_path.as_deref(), Some(relative.as_path()));

        let unsupported = directory.write("scoutly.ini", "max_depth = 2\n");
        let error = load_config(ConfigLoadOptions {
            config_path: Some(unsupported),
            ..ConfigLoadOptions::default()
        })
        .unwrap_err();

        assert!(matches!(
            error,
            ConfigError::UnsupportedFormat { extension } if extension == ".ini"
        ));

        let error = load_config(ConfigLoadOptions {
            config_path: Some(directory.path().to_owned()),
            ..ConfigLoadOptions::default()
        })
        .unwrap_err();

        assert!(matches!(error, ConfigError::NotRegular { .. }));
    }

    #[test]
    fn tilde_paths_expand_from_the_home_directory() {
        let home = home_directory().expect("the test environment provides a home directory");

        assert_eq!(
            resolve_explicit_path(Path::new("unused"), Path::new("~")).unwrap(),
            home
        );
        assert_eq!(
            resolve_explicit_path(Path::new("unused"), Path::new("~/scoutly.toml")).unwrap(),
            home.join("scoutly.toml")
        );
    }

    struct TestDirectory(tempfile::TempDir);

    impl TestDirectory {
        fn new() -> Self {
            Self(tempfile::tempdir().unwrap())
        }

        fn path(&self) -> &std::path::Path {
            self.0.path()
        }

        fn write(&self, name: &str, content: &str) -> PathBuf {
            let path = self.path().join(name);
            fs::write(&path, content).unwrap();
            path
        }
    }
}
