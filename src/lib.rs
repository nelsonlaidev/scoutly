pub mod cli;
pub mod config;
pub mod crawler;
pub mod http_client;
pub mod link_checker;
pub mod models;
pub mod reporter;
pub mod robots;
pub mod seo_analyzer;

use anyhow::Result;
use cli::Cli;
use colored::*;
use config::{Config, RuntimeOptions};
use crawler::{Crawler, CrawlerConfig};
use link_checker::LinkChecker;
use reporter::Reporter;
use seo_analyzer::SeoAnalyzer;
use std::path::PathBuf;

pub async fn run(args: Cli) -> Result<()> {
    let loaded_config = load_config(&args)?;
    let runtime = RuntimeOptions::from_cli_and_config(&args, loaded_config.config());

    let output_mode = OutputMode::from(&runtime);
    print_config_source(&loaded_config, runtime.verbose, output_mode);

    validate_url(&runtime.url)?;
    print_run_intro(&runtime, output_mode);

    let mut crawler = build_crawler(&runtime)?;
    crawl_pages(&mut crawler, &runtime, output_mode).await?;

    let unique_links = collect_unique_links(&crawler);
    print_crawl_summary(&runtime, &crawler, unique_links.len(), output_mode);

    check_links(&mut crawler, unique_links.len(), &runtime, output_mode).await?;
    analyze_seo(&mut crawler, &runtime, output_mode);

    let report = Reporter::generate_report(&runtime.url, &crawler.pages);
    output_report(&report, &runtime)?;
    save_report(&report, &runtime, output_mode)?;

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Json,
    Text,
}

impl OutputMode {
    fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

impl From<&RuntimeOptions> for OutputMode {
    fn from(args: &RuntimeOptions) -> Self {
        if args.output == "json" {
            Self::Json
        } else {
            Self::Text
        }
    }
}

enum LoadedConfig {
    Explicit { path: PathBuf, config: Config },
    Default(Config),
    None,
}

impl LoadedConfig {
    fn config(&self) -> Option<&Config> {
        match self {
            Self::Explicit { config, .. } | Self::Default(config) => Some(config),
            Self::None => None,
        }
    }
}

fn load_config(args: &Cli) -> Result<LoadedConfig> {
    if let Some(config_path) = &args.config {
        let path = PathBuf::from(config_path);
        let config = Config::from_file(&path)?;
        return Ok(LoadedConfig::Explicit { path, config });
    }

    Ok(match Config::from_default_paths()? {
        Some(config) => LoadedConfig::Default(config),
        None => LoadedConfig::None,
    })
}

fn print_config_source(config: &LoadedConfig, verbose: bool, output_mode: OutputMode) {
    if !verbose {
        return;
    }

    match config {
        LoadedConfig::Explicit { path, .. } => {
            emit_status_line(
                output_mode,
                format!(
                    "{} {}",
                    "Loading config from:".bright_white().bold(),
                    path.display()
                ),
            );
        }
        LoadedConfig::Default(_) => emit_status_line(
            output_mode,
            "Using default config file"
                .bright_white()
                .bold()
                .to_string(),
        ),
        LoadedConfig::None => {}
    }
}

fn validate_url(url: &str) -> Result<()> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("URL must start with http:// or https://");
    }

    Ok(())
}

fn print_run_intro(args: &RuntimeOptions, output_mode: OutputMode) {
    emit_status_line(
        output_mode,
        "Scoutly - Website Crawler & SEO Analyzer"
            .bright_cyan()
            .bold()
            .to_string(),
    );
    emit_status_line(output_mode, "=".repeat(50).bright_blue().to_string());
    emit_blank_line(output_mode);
    emit_status_line(
        output_mode,
        format!("{} {}", "Starting crawl:".bright_white().bold(), args.url),
    );
    emit_status_line(
        output_mode,
        format!("{} {}", "Max depth:".bright_white().bold(), args.depth),
    );
    emit_status_line(
        output_mode,
        format!("{} {}", "Max pages:".bright_white().bold(), args.max_pages),
    );
    emit_blank_line(output_mode);
}

fn build_crawler(args: &RuntimeOptions) -> Result<Crawler> {
    let config = CrawlerConfig {
        max_depth: args.depth,
        max_pages: args.max_pages,
        follow_external: args.external,
        keep_fragments: args.keep_fragments,
        requests_per_second: args.rate_limit,
        concurrent_requests: args.concurrency,
        respect_robots_txt: args.respect_robots_txt,
    };

    Crawler::new(&args.url, config)
}

async fn crawl_pages(
    crawler: &mut Crawler,
    args: &RuntimeOptions,
    output_mode: OutputMode,
) -> Result<()> {
    if args.verbose {
        emit_status_line(output_mode, "Crawling pages...".bright_yellow().to_string());
    }

    if !output_mode.is_json() {
        crawler.enable_progress_bar();
    }

    crawler.crawl().await
}

fn collect_unique_links(crawler: &Crawler) -> std::collections::HashSet<String> {
    crawler
        .pages
        .values()
        .flat_map(|page| page.links.iter().map(|link| link.url.clone()))
        .collect()
}

fn print_crawl_summary(
    args: &RuntimeOptions,
    crawler: &Crawler,
    unique_link_count: usize,
    output_mode: OutputMode,
) {
    emit_status_line(
        output_mode,
        format!(
            "{} {} unique pages crawled, {} unique links detected",
            "Success:".bright_green().bold(),
            crawler.pages.len(),
            unique_link_count
        ),
    );

    if args.verbose {
        emit_status_line(
            output_mode,
            format!(
                "{} {:#?}",
                "Crawled pages:".bright_white().bold(),
                crawler.pages
            ),
        );
    }

    emit_blank_line(output_mode);
}

async fn check_links(
    crawler: &mut Crawler,
    unique_link_count: usize,
    args: &RuntimeOptions,
    output_mode: OutputMode,
) -> Result<()> {
    if args.verbose {
        emit_status_line(output_mode, "Checking links...".bright_yellow().to_string());
    }

    let mut link_checker = LinkChecker::with_concurrency(args.concurrency);

    if !output_mode.is_json() {
        link_checker.enable_progress_bar(unique_link_count);
    }

    link_checker
        .check_all_links(&mut crawler.pages, args.ignore_redirects)
        .await?;

    if args.verbose {
        emit_status_line(output_mode, "Links checked".bright_green().to_string());
        emit_blank_line(output_mode);
    }

    Ok(())
}

fn analyze_seo(crawler: &mut Crawler, args: &RuntimeOptions, output_mode: OutputMode) {
    if args.verbose {
        emit_status_line(output_mode, "Analyzing SEO...".bright_yellow().to_string());
    }

    SeoAnalyzer::analyze_pages(&mut crawler.pages);

    if args.verbose {
        emit_status_line(
            output_mode,
            "SEO analysis complete".bright_green().to_string(),
        );
        emit_blank_line(output_mode);
    }
}

fn output_report(report: &crate::models::CrawlReport, args: &RuntimeOptions) -> Result<()> {
    match OutputMode::from(args) {
        OutputMode::Json => {
            let json = serde_json::to_string_pretty(report)?;
            println!("{}", json);
        }
        OutputMode::Text => Reporter::print_text_report(report),
    }

    Ok(())
}

fn save_report(
    report: &crate::models::CrawlReport,
    args: &RuntimeOptions,
    output_mode: OutputMode,
) -> Result<()> {
    if let Some(filename) = &args.save {
        Reporter::save_json_report(report, filename)?;
        emit_status_line(
            output_mode,
            format!("Report saved to: {}", filename.bright_green()),
        );
    }

    Ok(())
}

fn emit_status_line(output_mode: OutputMode, message: impl std::fmt::Display) {
    if output_mode.is_json() {
        eprintln!("{message}");
    } else {
        println!("{message}");
    }
}

fn emit_blank_line(output_mode: OutputMode) {
    if output_mode.is_json() {
        eprintln!();
    } else {
        println!();
    }
}
