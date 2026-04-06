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
pub mod sitemap;
pub mod tui;
pub mod update;

use anyhow::Result;
use cli::{Cli, OutputFormat};
use colored::*;
use config::{Config, RuntimeOptions};
use crawler::{Crawler, CrawlerConfig};
use link_checker::LinkChecker;
use models::{CrawlReport, PageInfo, SitemapEntry};
use reporter::Reporter;
use runtime::{
    LaunchMode, ProgressSnapshot, RunEvent, RunEventSender, RunStage, TerminalSupport,
    resolve_launch_mode,
};
use seo_analyzer::SeoAnalyzer;
use sitemap::collect_sitemap_entries;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

pub async fn run(args: Cli) -> Result<()> {
    run_with_terminal(args, TerminalSupport::current()).await
}

#[doc(hidden)]
pub async fn run_with_terminal(args: Cli, terminal: TerminalSupport) -> Result<()> {
    run_with_terminal_and_tui_runner(args, terminal, tui::run).await
}

async fn run_with_terminal_and_tui_runner<F, Fut>(
    args: Cli,
    terminal: TerminalSupport,
    run_tui: F,
) -> Result<()>
where
    F: FnOnce(RuntimeOptions) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let loaded_config = load_config(&args)?;
    let runtime = RuntimeOptions::from_cli_and_config(&args, loaded_config.config());

    let launch_mode = resolve_launch_mode(&runtime, terminal)?;

    if matches!(launch_mode, LaunchMode::Tui) {
        if let Some(url) = runtime.url.as_deref() {
            validate_url(url)?;
        }
        return run_tui(runtime).await;
    }

    let (mode_name, output_format) = if matches!(launch_mode, LaunchMode::Json) {
        ("JSON output mode", OutputFormat::Json)
    } else {
        ("CLI mode", OutputFormat::Text)
    };

    validate_required_url(&runtime, mode_name)?;
    run_cli(runtime, loaded_config, output_format).await
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
    let sitemap = collect_sitemap_entries_or_log_error(
        url,
        collect_sitemap_entries(url, &crawler.pages, runtime.keep_fragments),
    )
    .await;
    let pages = std::mem::take(&mut crawler.pages);
    let report = Reporter::generate_report_with_sitemap(url, pages, sitemap);

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

async fn collect_sitemap_entries_or_log_error<Fut>(url: &str, future: Fut) -> Vec<SitemapEntry>
where
    Fut: Future<Output = Result<Vec<SitemapEntry>>>,
{
    future.await.unwrap_or_else(|error| {
        tracing::warn!(error = %error, url = %url, "Failed to collect sitemap data");
        Vec::new()
    })
}

async fn run_cli(
    runtime: RuntimeOptions,
    loaded_config: LoadedConfig,
    output_format: OutputFormat,
) -> Result<()> {
    maybe_emit_update_notice(output_format).await;
    print_config_source(&loaded_config, runtime.verbose, output_format);
    print_run_intro(&runtime, output_format);

    let report = execute_scan(&runtime, None, !output_format.is_json()).await?;
    output_report(&report, output_format)?;
    save_report(&report, &runtime, output_format)?;

    Ok(())
}

async fn maybe_emit_update_notice(output_format: OutputFormat) {
    let notice = tokio::time::timeout(Duration::from_millis(500), update::check_for_update())
        .await
        .ok()
        .flatten();

    if let Some(notice) = notice {
        emit_status_line(output_format, update::format_cli_update_message(&notice));
        emit_blank_line(output_format);
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
        OutputFormat::Text => Reporter::print_text_report(report, std::io::stdout()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{IssueSeverity, IssueType, Link, OpenGraphTags, SeoIssue};
    use actix_web::{App, HttpResponse, HttpServer, web};
    use std::net::TcpListener;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::Duration;
    use tokio::sync::mpsc::unbounded_channel;

    fn page(url: &str) -> PageInfo {
        PageInfo {
            url: url.to_string(),
            status_code: Some(200),
            content_type: Some("text/html".to_string()),
            title: Some("Page".to_string()),
            meta_description: None,
            h1_tags: vec![],
            links: vec![Link {
                url: format!("{url}/child"),
                text: "child".to_string(),
                is_external: false,
                status_code: Some(404),
                redirected_url: None,
                check_error: None,
            }],
            images: vec![],
            open_graph: OpenGraphTags::default(),
            issues: vec![SeoIssue {
                severity: IssueSeverity::Warning,
                issue_type: IssueType::MissingMetaDescription,
                message: "warn".to_string(),
            }],
            crawl_depth: 0,
        }
    }

    fn runtime(url: Option<&str>) -> RuntimeOptions {
        RuntimeOptions {
            url: url.map(str::to_string),
            depth: 1,
            max_pages: 8,
            output: None,
            save: None,
            cli: false,
            external: false,
            verbose: false,
            ignore_redirects: false,
            keep_fragments: false,
            rate_limit: None,
            concurrency: 1,
            respect_robots_txt: false,
            tui: false,
            config: None,
        }
    }

    async fn start_scan_server() -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind scan server");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new({
            let base_url = base_url.clone();
            move || {
                let base_url = base_url.clone();
                App::new()
                    .app_data(web::Data::new(base_url))
                    .route(
                        "/robots.txt",
                        web::get().to(|| async {
                            HttpResponse::Ok()
                                .content_type("text/plain")
                                .body("User-agent: *\nDisallow:\n")
                        }),
                    )
                    .route(
                        "/sitemap.xml",
                        web::get().to(|base_url: web::Data<String>| async move {
                            HttpResponse::Ok()
                                .content_type("application/xml")
                                .body(format!(
                                    r#"<?xml version="1.0" encoding="UTF-8"?>
                                <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
                                  <url><loc>{}/</loc></url>
                                  <url><loc>{}/about</loc></url>
                                </urlset>"#,
                                    base_url.get_ref(),
                                    base_url.get_ref()
                                ))
                        }),
                    )
                    .route(
                        "/",
                        web::get().to(|| async {
                            HttpResponse::Ok().content_type("text/html").body(
                                r#"<html>
                                    <head>
                                      <title>Home</title>
                                      <meta name="description" content="Home page">
                                    </head>
                                    <body>
                                      <h1>Home</h1>
                                      <a href="/about">About</a>
                                    </body>
                                  </html>"#,
                            )
                        }),
                    )
                    .route(
                        "/about",
                        web::get().to(|| async {
                            HttpResponse::Ok().content_type("text/html").body(
                                r#"<html>
                                    <head>
                                      <title>About</title>
                                      <meta name="description" content="About page">
                                    </head>
                                    <body>
                                      <h1>About</h1>
                                      <a href="/">Home</a>
                                    </body>
                                  </html>"#,
                            )
                        }),
                    )
            }
        })
        .workers(1)
        .listen(listener)
        .expect("listen scan server")
        .run();

        tokio::spawn(async move {
            let _ = server.await;
        });

        for _ in 0..20 {
            if reqwest::get(format!("{base_url}/")).await.is_ok() {
                return base_url;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        panic!("scan server failed to start at {base_url}");
    }

    #[test]
    fn snapshot_and_emit_helpers_cover_sender_and_no_sender_paths() {
        let pages = HashMap::from([(
            "https://example.com".to_string(),
            page("https://example.com"),
        )]);
        let snapshot = snapshot_from_pages(
            RunStage::CheckingLinks,
            "checking".to_string(),
            &pages,
            1,
            2,
        );
        assert_eq!(snapshot.pages_crawled, 1);
        assert_eq!(snapshot.links_discovered, 1);
        assert_eq!(snapshot.links_checked, 1);
        assert_eq!(snapshot.total_links, 2);
        assert_eq!(snapshot.summary.broken_links, 1);

        let (sender, mut receiver) = unbounded_channel();
        emit_progress(&Some(sender.clone()), snapshot.clone());
        match receiver.try_recv().expect("progress event") {
            RunEvent::Progress(received) => {
                assert_eq!(received.message, "checking");
                assert_eq!(received.links_checked, 1);
            }
            other => panic!("expected progress event, got {other:?}"),
        }

        emit_event(&Some(sender), RunEvent::Error("boom".to_string()));
        match receiver.try_recv().expect("error event") {
            RunEvent::Error(error) => assert_eq!(error, "boom"),
            other => panic!("expected error event, got {other:?}"),
        }

        emit_event(&None, RunEvent::Error("ignored".to_string()));
        emit_progress(&None, snapshot);
    }

    #[tokio::test]
    async fn collect_sitemap_entries_or_log_error_returns_empty_on_failure() {
        let entries = collect_sitemap_entries_or_log_error("https://example.com", async {
            Err(anyhow::anyhow!("boom"))
        })
        .await;

        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn execute_scan_emits_events_when_sender_is_present() {
        let base_url = start_scan_server().await;
        let (sender, mut receiver) = unbounded_channel();

        let report = execute_scan(&runtime(Some(&base_url)), Some(sender), false)
            .await
            .expect("scan should succeed");

        let mut stages = Vec::new();
        let mut saw_report_ready = false;
        while let Ok(event) = receiver.try_recv() {
            match event {
                RunEvent::Progress(snapshot) => stages.push(snapshot.stage),
                RunEvent::ReportReady(emitted_report) => {
                    saw_report_ready = true;
                    assert_eq!(emitted_report.start_url, base_url);
                }
                RunEvent::UpdateAvailable(_) | RunEvent::Error(_) => {}
            }
        }

        assert!(stages.contains(&RunStage::LoadingConfig));
        assert!(stages.contains(&RunStage::Completed));
        assert!(saw_report_ready);
        assert_eq!(report.start_url, base_url);
        assert_eq!(report.summary.total_pages, 2);
        assert_eq!(report.sitemap.len(), 2);
    }

    #[tokio::test]
    async fn run_with_terminal_uses_injected_tui_runner_for_interactive_sessions() {
        let args = Cli {
            url: Some("https://example.com".to_string()),
            depth: None,
            max_pages: None,
            output: None,
            cli: false,
            tui: false,
            save: None,
            external: false,
            verbose: false,
            ignore_redirects: false,
            keep_fragments: false,
            rate_limit: None,
            concurrency: None,
            respect_robots_txt: Some(false),
            config: None,
        };
        let called = Arc::new(AtomicBool::new(false));
        let called_in_runner = called.clone();

        let result = run_with_terminal_and_tui_runner(
            args,
            TerminalSupport {
                stdin_is_terminal: true,
                stdout_is_terminal: true,
            },
            move |runtime| async move {
                called_in_runner.store(true, Ordering::SeqCst);
                assert_eq!(runtime.url.as_deref(), Some("https://example.com"));
                Ok(())
            },
        )
        .await;

        assert!(result.is_ok());
        assert!(called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn run_with_terminal_validates_tui_urls_before_launching() {
        let args = Cli {
            url: Some("not-a-url".to_string()),
            depth: None,
            max_pages: None,
            output: None,
            cli: false,
            tui: false,
            save: None,
            external: false,
            verbose: false,
            ignore_redirects: false,
            keep_fragments: false,
            rate_limit: None,
            concurrency: None,
            respect_robots_txt: Some(false),
            config: None,
        };
        let called = Arc::new(AtomicBool::new(false));
        let called_in_runner = called.clone();

        let error = run_with_terminal_and_tui_runner(
            args,
            TerminalSupport {
                stdin_is_terminal: true,
                stdout_is_terminal: true,
            },
            move |_| async move {
                called_in_runner.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .expect_err("invalid tui url should fail");

        assert!(
            error
                .to_string()
                .contains("URL must start with http:// or https://")
        );
        assert!(!called.load(Ordering::SeqCst));
    }
}
