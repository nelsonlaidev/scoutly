use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "scoutly")]
#[command(about = "A CLI website crawler and SEO analyzer", long_about = None)]
pub struct Cli {
    /// The URL to start crawling from
    #[arg(value_name = "URL")]
    pub url: String,

    /// Maximum crawl depth (default: 5)
    #[arg(short, long, default_value_t = 5)]
    pub depth: usize,

    /// Maximum number of pages to crawl (default: 200)
    #[arg(short, long, default_value_t = 200)]
    pub max_pages: usize,

    /// Output format: text or json
    #[arg(short, long, default_value = "text")]
    pub output: String,

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
    #[arg(short = 'c', long, default_value_t = 5)]
    pub concurrency: usize,

    /// Respect robots.txt rules (default: true)
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub respect_robots_txt: bool,

    /// Path to configuration file (JSON, TOML, or YAML)
    #[arg(long)]
    pub config: Option<String>,
}
