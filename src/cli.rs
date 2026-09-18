use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use clap::Parser;
use crossterm::terminal;
use scoutly::{
    AuditError, Config, ConfigError, ConfigLoadOptions, ConfigOverrides, ConfigValidationError,
    OutputFormat, ProgressMode, audit, load_config, resolve_config,
};
use thiserror::Error;
use tokio::sync::mpsc;

use crate::cli_progress::ProgressDisplay;
use crate::output::{OutputError, format_report};
use crate::tui::{TuiError, run as run_tui};

const PROGRESS_CHANNEL_CAPACITY: usize = 64;
const DEFAULT_TERMINAL_WIDTH: usize = 80;

#[derive(Debug, Parser)]
#[command(
    name = "scoutly",
    about = "A fast website auditing tool.",
    version = env!("CARGO_PKG_VERSION")
)]
pub(crate) struct Cli {
    /// The website target URL to audit.
    target: Option<String>,

    /// Path to a Scoutly config file.
    #[arg(long, conflicts_with = "no_config")]
    config: Option<PathBuf>,

    /// Disable automatic config discovery.
    #[arg(long)]
    no_config: bool,

    /// Maximum depth to crawl.
    #[arg(long)]
    max_depth: Option<u32>,

    /// Maximum number of pages to crawl.
    #[arg(long)]
    max_pages: Option<usize>,

    /// Keep URL fragments when crawling.
    #[arg(long, overrides_with = "no_keep_fragments")]
    keep_fragments: bool,

    /// Do not keep URL fragments when crawling.
    #[arg(long, overrides_with = "keep_fragments")]
    no_keep_fragments: bool,

    /// Do not report redirects as issues.
    #[arg(long, overrides_with = "no_ignore_redirects")]
    ignore_redirects: bool,

    /// Report redirects as issues.
    #[arg(long, overrides_with = "ignore_redirects")]
    no_ignore_redirects: bool,

    /// Maximum HTTP requests per second.
    #[arg(long)]
    rate_limit: Option<f64>,

    /// Respect robots.txt crawl rules.
    #[arg(long, overrides_with = "no_respect_robots")]
    respect_robots: bool,

    /// Ignore robots.txt crawl rules.
    #[arg(long, overrides_with = "respect_robots")]
    no_respect_robots: bool,

    /// Discover pages from XML sitemaps.
    #[arg(long, overrides_with = "no_sitemaps")]
    sitemaps: bool,

    /// Do not discover pages from XML sitemaps.
    #[arg(long, overrides_with = "sitemaps")]
    no_sitemaps: bool,

    /// Check discovered images.
    #[arg(long, overrides_with = "no_images")]
    images: bool,

    /// Do not check discovered images.
    #[arg(long, overrides_with = "images")]
    no_images: bool,

    /// Maximum sitemap documents to fetch.
    #[arg(long)]
    max_sitemap_documents: Option<usize>,

    /// Request timeout in milliseconds.
    #[arg(long)]
    timeout: Option<u64>,

    /// Maximum number of redirects per request.
    #[arg(long)]
    max_redirects: Option<u32>,

    /// User-Agent header sent with requests.
    #[arg(long)]
    user_agent: Option<String>,

    /// Maximum number of concurrent page crawls, link checks, and image checks.
    #[arg(long)]
    concurrency: Option<usize>,

    /// Crawl only this path prefix (repeatable); referenced resources are still checked.
    #[arg(long = "include-path", value_name = "INCLUDE_PATH")]
    include_paths: Vec<String>,

    /// Do not crawl this path prefix (repeatable); referenced resources are still checked.
    #[arg(long = "exclude-path", value_name = "EXCLUDE_PATH")]
    exclude_paths: Vec<String>,

    /// Output format.
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,

    /// Progress display mode.
    #[arg(long, value_enum)]
    progress: Option<ProgressMode>,
}

impl Cli {
    fn resolve(self) -> Result<(Option<String>, Config), CliError> {
        let loaded = load_config(ConfigLoadOptions {
            directory: None,
            config_path: self.config,
            disable_discovery: self.no_config,
            version: env!("CARGO_PKG_VERSION").to_owned(),
        })?;

        let resolved = resolve_config(
            loaded.base,
            ConfigOverrides {
                max_depth: self.max_depth,
                max_pages: self.max_pages,
                keep_fragments: bool_override(self.keep_fragments, self.no_keep_fragments),
                ignore_redirects: bool_override(self.ignore_redirects, self.no_ignore_redirects),
                rate_limit: self.rate_limit,
                respect_robots: bool_override(self.respect_robots, self.no_respect_robots),
                sitemaps: bool_override(self.sitemaps, self.no_sitemaps),
                images: bool_override(self.images, self.no_images),
                max_sitemap_documents: self.max_sitemap_documents,
                timeout: self.timeout,
                max_redirects: self.max_redirects,
                user_agent: self.user_agent,
                concurrency: self.concurrency,
                format: self.format,
                progress: self.progress,
                include_paths: (!self.include_paths.is_empty()).then_some(self.include_paths),
                exclude_paths: (!self.exclude_paths.is_empty()).then_some(self.exclude_paths),
                ..ConfigOverrides::default()
            },
        )?;

        Ok((self.target, resolved))
    }
}

fn bool_override(enabled: bool, disabled: bool) -> Option<bool> {
    if enabled {
        Some(true)
    } else if disabled {
        Some(false)
    } else {
        None
    }
}

#[derive(Debug, Error)]
pub(crate) enum CliError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    InvalidConfiguration(#[from] ConfigValidationError),

    #[error(transparent)]
    Audit(#[from] AuditError),

    #[error(transparent)]
    Output(#[from] OutputError),

    #[error("listen for interrupt: {0}")]
    Interrupt(#[source] io::Error),

    #[error("write progress display: {0}")]
    Progress(#[source] io::Error),

    #[error("write audit report: {0}")]
    Report(#[source] io::Error),

    #[error("TUI requires a terminal; pass a target URL for non-interactive mode")]
    TuiRequiresTerminal,

    #[error(transparent)]
    Tui(TuiError),

    #[error("audit canceled")]
    Cancelled,
}

impl CliError {
    pub(crate) const fn exit_code(&self) -> u8 {
        if matches!(self, Self::Cancelled) {
            130
        } else {
            1
        }
    }
}

pub(crate) async fn run(cli: Cli) -> Result<(), CliError> {
    let (target, config) = cli.resolve()?;
    let Some(target) = target else {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(CliError::TuiRequiresTerminal);
        }

        return run_tui(config.into_audit_options()).await.map_err(|error| {
            if matches!(error, TuiError::Cancelled) {
                CliError::Cancelled
            } else {
                CliError::Tui(error)
            }
        });
    };

    let stdout = io::stdout();
    let stderr = io::stderr();
    let stderr_is_terminal = stderr.is_terminal();
    let width = if stderr_is_terminal {
        terminal::size()
            .ok()
            .map_or(DEFAULT_TERMINAL_WIDTH, |(width, _)| {
                usize::from(width).max(1)
            })
    } else {
        DEFAULT_TERMINAL_WIDTH
    };
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();

    run_audit(
        &target,
        config,
        &mut stdout,
        &mut stderr,
        stderr_is_terminal,
        width,
    )
    .await
}

async fn run_audit(
    target: &str,
    config: Config,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    stderr_is_terminal: bool,
    terminal_width: usize,
) -> Result<(), CliError> {
    let progress_mode = config.progress;
    let format = config.format;
    let options = config.into_audit_options();
    let mut progress =
        ProgressDisplay::new(progress_mode, stderr, stderr_is_terminal, terminal_width);
    let (sender, mut receiver) = mpsc::channel(PROGRESS_CHANNEL_CAPACITY);
    let sender = progress.enabled().then_some(sender);
    let audit = audit(target, options, sender);
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(audit);
    tokio::pin!(interrupt);

    let audit_result = loop {
        tokio::select! {
            result = &mut audit => break result,
            signal = &mut interrupt => {
                let result = signal.map_or_else(
                    |error| Err(CliError::Interrupt(error)),
                    |()| Err(CliError::Cancelled),
                );
                let _ = progress.close();
                return result;
            }
            Some(snapshot) = receiver.recv(), if progress.enabled() => {
                if let Err(error) = progress.update(snapshot) {
                    let _ = progress.close();
                    return Err(CliError::Progress(error));
                }
            }
        }
    };

    while let Ok(snapshot) = receiver.try_recv() {
        if let Err(error) = progress.update(snapshot) {
            let _ = progress.close();
            return Err(CliError::Progress(error));
        }
    }

    let report = match audit_result {
        Ok(report) => report,
        Err(error) => {
            let _ = progress.close();
            return Err(CliError::Audit(error));
        }
    };

    progress.close().map_err(CliError::Progress)?;

    let output = format_report(&report, format)?;

    writeln!(stdout, "{output}").map_err(CliError::Report)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Cli;

    #[test]
    fn unset_fields_preserve_file_values_and_repeatable_paths_replace_lists() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("scoutly.toml");

        std::fs::write(
            &config_path,
            "include_paths = [\"/old\"]\nexclude_paths = [\"/private\"]\n",
        )
        .unwrap();

        let cli = Cli::try_parse_from([
            "scoutly",
            "example.com/docs",
            "--config",
            config_path.to_str().unwrap(),
            "--include-path",
            "/docs",
            "--include-path",
            "/news,updates",
        ])
        .unwrap();

        let (target, config) = cli.resolve().unwrap();

        assert_eq!(target.as_deref(), Some("example.com/docs"));
        assert_eq!(config.include_paths, ["/docs", "/news,updates"]);
        assert_eq!(config.exclude_paths, ["/private"]);
    }

    #[test]
    fn paired_boolean_flags_preserve_presence_and_last_value() {
        let default = Cli::try_parse_from(["scoutly", "--no-config"]).unwrap();
        assert!(default.resolve().unwrap().1.respect_robots);

        let disabled = Cli::try_parse_from([
            "scoutly",
            "--no-config",
            "--respect-robots",
            "--no-respect-robots",
        ])
        .unwrap();
        assert!(!disabled.resolve().unwrap().1.respect_robots);

        let enabled = Cli::try_parse_from([
            "scoutly",
            "--no-config",
            "--no-respect-robots",
            "--respect-robots",
        ])
        .unwrap();
        assert!(enabled.resolve().unwrap().1.respect_robots);
    }

    #[test]
    fn config_selection_and_empty_values_are_rejected() {
        assert!(Cli::try_parse_from(["scoutly", "--config=scoutly.toml", "--no-config"]).is_err());
        assert!(Cli::try_parse_from(["scoutly", "--config="]).is_err());

        let empty_path = Cli::try_parse_from(["scoutly", "--include-path="]).unwrap();
        assert!(empty_path.resolve().is_err());
    }

    #[test]
    fn unsigned_flags_reject_negative_values_during_argument_parsing() {
        for flag in [
            "--max-depth",
            "--max-pages",
            "--max-sitemap-documents",
            "--timeout",
            "--max-redirects",
            "--concurrency",
        ] {
            assert!(
                Cli::try_parse_from(["scoutly", "--no-config", flag, "-1"]).is_err(),
                "{flag}"
            );
        }
    }
}
