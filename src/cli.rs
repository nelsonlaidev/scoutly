use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

pub const DEFAULT_DEPTH: usize = 5;
pub const DEFAULT_MAX_PAGES: usize = 200;
pub const DEFAULT_CONCURRENCY: usize = 5;
pub const DEFAULT_RESPECT_ROBOTS_TXT: bool = true;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Text,
    Json,
}

impl OutputFormat {
    pub const fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

#[derive(Parser, Debug, Clone)]
#[command(name = "scoutly")]
#[command(about = "A CLI website crawler and SEO analyzer", long_about = None)]
pub struct Cli {
    /// The URL to start crawling from (optional in TUI mode)
    #[arg(value_name = "URL")]
    pub url: Option<String>,

    /// Maximum crawl depth (default: 5)
    #[arg(short, long)]
    pub depth: Option<usize>,

    /// Maximum number of pages to crawl (default: 200)
    #[arg(short, long)]
    pub max_pages: Option<usize>,

    /// CLI output format: text or json
    #[arg(short, long, value_enum, conflicts_with = "tui")]
    pub output: Option<OutputFormat>,

    /// Force CLI mode instead of launching the TUI
    #[arg(long, conflicts_with = "tui")]
    pub cli: bool,

    /// Force the interactive TUI (errors if no interactive terminal is available)
    #[arg(long, conflicts_with = "cli")]
    pub tui: bool,

    /// Save report to file
    #[arg(short, long)]
    pub save: Option<String>,

    /// Follow external links
    #[arg(short, long)]
    pub external: bool,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Ignore redirect issues in the report
    #[arg(long)]
    pub ignore_redirects: bool,

    /// Treat URLs with fragment identifiers (#) as unique links
    #[arg(long)]
    pub keep_fragments: bool,

    /// Rate limit for requests per second (optional, e.g., 1.0 for 1 req/s)
    #[arg(short = 'r', long)]
    pub rate_limit: Option<f64>,

    /// Number of concurrent requests (default: 5)
    #[arg(short = 'c', long)]
    pub concurrency: Option<usize>,

    /// Respect robots.txt rules (default: true)
    #[arg(long, action = clap::ArgAction::Set)]
    pub respect_robots_txt: Option<bool>,

    /// Path to configuration file (JSON, TOML, or YAML)
    #[arg(long)]
    pub config: Option<String>,
}
