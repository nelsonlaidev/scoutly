pub mod cli;
pub mod config;
pub mod crawler;
pub mod http_client;
pub mod link_checker;
pub mod models;
pub mod reporter;
pub mod robots;
pub mod runtime;
pub mod seo_analyzer;
pub mod tui;

use anyhow::Result;
use cli::{Cli, OutputFormat};
use colored::*;
use config::{Config, RuntimeOptions};
use crawler::{Crawler, CrawlerConfig};
use link_checker::LinkChecker;
use models::{CrawlReport, PageInfo};
use reporter::Reporter;
use runtime::{
    LaunchMode, ProgressSnapshot, RunEvent, RunEventSender, RunStage, TerminalSupport,
    resolve_launch_mode,
};
use seo_analyzer::SeoAnalyzer;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub async fn run(args: Cli) -> Result<()> {
    run_with_terminal(args, TerminalSupport::current()).await
}

#[doc(hidden)]
pub async fn run_with_terminal(args: Cli, terminal: TerminalSupport) -> Result<()> {
    let loaded_config = load_config(&args)?;
    let runtime = RuntimeOptions::from_cli_and_config(&args, loaded_config.config());

    let launch_mode = resolve_launch_mode(&runtime, terminal)?;

    match launch_mode {
        LaunchMode::Tui => {
            if let Some(url) = runtime.url.as_deref() {
                validate_url(url)?;
            }
            tui::run(runtime).await
        }
        LaunchMode::ClassicText => {
            validate_required_url(&runtime, "classic CLI mode")?;
            run_classic(runtime, loaded_config, OutputFormat::Text).await
        }
        LaunchMode::ClassicJson => {
            validate_required_url(&runtime, "JSON output mode")?;
            run_classic(runtime, loaded_config, OutputFormat::Json).await
        }
    }
}

pub(crate) async fn execute_scan(
    runtime: &RuntimeOptions,
    event_sender: Option<RunEventSender>,
    show_progress_bars: bool,
) -> Result<CrawlReport> {
    let url = runtime
        .url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("A URL is required to start a scan"))?;
    validate_url(url)?;

    emit_progress(
        &event_sender,
        ProgressSnapshot::new(RunStage::LoadingConfig, format!("Preparing scan for {url}")),
    );

    let mut crawler = build_crawler(runtime)?;
    if let Some(sender) = &event_sender {
        crawler.set_progress_sender(sender.clone());
    }
    if show_progress_bars {
        crawler.enable_progress_bar();
    }

    emit_progress(
        &event_sender,
        ProgressSnapshot::new(RunStage::Crawling, format!("Crawling {url}")),
    );
    crawler.crawl().await?;

    let unique_links = collect_unique_links(&crawler);
    emit_progress(
        &event_sender,
        snapshot_from_pages(
            RunStage::CheckingLinks,
            format!(
                "Discovered {} page(s) and {} unique link(s)",
                crawler.pages.len(),
                unique_links.len()
            ),
            &crawler.pages,
            0,
            unique_links.len(),
        ),
    );

    let mut link_checker = LinkChecker::with_concurrency(runtime.concurrency);
    if let Some(sender) = &event_sender {
        link_checker.set_progress_sender(sender.clone());
    }
    if show_progress_bars {
        link_checker.enable_progress_bar(unique_links.len());
    }
    link_checker
        .check_all_links(&mut crawler.pages, runtime.ignore_redirects)
        .await?;

    emit_progress(
        &event_sender,
        snapshot_from_pages(
            RunStage::AnalyzingSeo,
            "Analyzing SEO issues".to_string(),
            &crawler.pages,
            unique_links.len(),
            unique_links.len(),
        ),
    );
    SeoAnalyzer::analyze_pages(&mut crawler.pages);

    emit_progress(
        &event_sender,
        snapshot_from_pages(
            RunStage::GeneratingReport,
            "Generating crawl report".to_string(),
            &crawler.pages,
            unique_links.len(),
            unique_links.len(),
        ),
    );
    let report = Reporter::generate_report(url, &crawler.pages);

    let mut complete = ProgressSnapshot::new(RunStage::Completed, "Report ready");
    complete.pages_crawled = report.summary.total_pages;
    complete.links_discovered = report.summary.total_links;
    complete.links_checked = unique_links.len();
    complete.total_links = unique_links.len();
    complete.summary = report.summary.clone();
    emit_progress(&event_sender, complete);
    emit_event(&event_sender, RunEvent::ReportReady(report.clone()));

    Ok(report)
}

async fn run_classic(
    runtime: RuntimeOptions,
    loaded_config: LoadedConfig,
    output_format: OutputFormat,
) -> Result<()> {
    print_config_source(&loaded_config, runtime.verbose, output_format);
    print_run_intro(&runtime, output_format);

    let report = execute_scan(&runtime, None, !output_format.is_json()).await?;
    output_report(&report, output_format)?;
    save_report(&report, &runtime, output_format)?;

    Ok(())
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

fn print_config_source(config: &LoadedConfig, verbose: bool, output_format: OutputFormat) {
    if !verbose {
        return;
    }

    match config {
        LoadedConfig::Explicit { path, .. } => emit_status_line(
            output_format,
            format!(
                "{} {}",
                "Loading config from:".bright_white().bold(),
                path.display()
            ),
        ),
        LoadedConfig::Default(_) => emit_status_line(
            output_format,
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

fn validate_required_url(runtime: &RuntimeOptions, mode_name: &str) -> Result<()> {
    let Some(url) = runtime.url.as_deref() else {
        anyhow::bail!(
            "A URL is required for {mode_name}. Provide a URL argument or launch the TUI and enter it there."
        );
    };

    validate_url(url)
}

fn print_run_intro(args: &RuntimeOptions, output_format: OutputFormat) {
    emit_status_line(
        output_format,
        "Scoutly - Website Crawler & SEO Analyzer"
            .bright_cyan()
            .bold()
            .to_string(),
    );
    emit_status_line(output_format, "=".repeat(50).bright_blue().to_string());
    emit_blank_line(output_format);
    emit_status_line(
        output_format,
        format!(
            "{} {}",
            "Starting crawl:".bright_white().bold(),
            args.url.as_deref().unwrap_or("(enter in TUI)")
        ),
    );
    emit_status_line(
        output_format,
        format!("{} {}", "Max depth:".bright_white().bold(), args.depth),
    );
    emit_status_line(
        output_format,
        format!("{} {}", "Max pages:".bright_white().bold(), args.max_pages),
    );
    emit_blank_line(output_format);
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

    Crawler::new(
        args.url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("A URL is required to build the crawler"))?,
        config,
    )
}

fn collect_unique_links(crawler: &Crawler) -> HashSet<String> {
    crawler
        .pages
        .values()
        .flat_map(|page| page.links.iter().map(|link| link.url.clone()))
        .collect()
}

fn snapshot_from_pages(
    stage: RunStage,
    message: String,
    pages: &HashMap<String, PageInfo>,
    links_checked: usize,
    total_links: usize,
) -> ProgressSnapshot {
    let summary = Reporter::summarize_pages(pages);
    let mut snapshot = ProgressSnapshot::new(stage, message);
    snapshot.pages_crawled = pages.len();
    snapshot.links_discovered = summary.total_links;
    snapshot.links_checked = links_checked;
    snapshot.total_links = total_links;
    snapshot.summary = summary;
    snapshot
}

fn emit_progress(sender: &Option<RunEventSender>, snapshot: ProgressSnapshot) {
    emit_event(sender, RunEvent::Progress(snapshot));
}

fn emit_event(sender: &Option<RunEventSender>, event: RunEvent) {
    if let Some(sender) = sender {
        let _ = sender.send(event);
    }
}

fn output_report(report: &CrawlReport, output_format: OutputFormat) -> Result<()> {
    match output_format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(report)?;
            println!("{}", json);
        }
        OutputFormat::Text => Reporter::print_text_report(report),
    }

    Ok(())
}

fn save_report(
    report: &CrawlReport,
    args: &RuntimeOptions,
    output_format: OutputFormat,
) -> Result<()> {
    if let Some(filename) = &args.save {
        Reporter::save_json_report(report, filename)?;
        emit_status_line(
            output_format,
            format!("Report saved to: {}", filename.bright_green()),
        );
    }

    Ok(())
}

fn emit_status_line(output_format: OutputFormat, message: impl std::fmt::Display) {
    if output_format.is_json() {
        eprintln!("{message}");
    } else {
        println!("{message}");
    }
}

fn emit_blank_line(output_format: OutputFormat) {
    if output_format.is_json() {
        eprintln!();
    } else {
        println!();
    }
}
