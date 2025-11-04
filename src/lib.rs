pub mod cli;
pub mod crawler;
pub mod http_client;
pub mod link_checker;
pub mod models;
pub mod reporter;
pub mod seo_analyzer;

use anyhow::Result;
use cli::Cli;
use colored::*;
use crawler::Crawler;
use link_checker::LinkChecker;
use reporter::Reporter;
use seo_analyzer::SeoAnalyzer;

pub async fn run(args: Cli) -> Result<()> {
    println!(
        "{}",
        "Scoutly - Website Crawler & SEO Analyzer"
            .bright_cyan()
            .bold()
    );
    println!("{}", "=".repeat(50).bright_blue());
    println!();

    // Validate URL
    if !args.url.starts_with("http://") && !args.url.starts_with("https://") {
        anyhow::bail!("URL must start with http:// or https://");
    }

    println!("{} {}", "Starting crawl:".bright_white().bold(), args.url);
    println!("{} {}", "Max depth:".bright_white().bold(), args.depth);
    println!("{} {}", "Max pages:".bright_white().bold(), args.max_pages);
    println!();

    // Create crawler and start crawling
    let mut crawler = Crawler::new(
        &args.url,
        args.depth,
        args.max_pages,
        args.external,
        args.keep_fragments,
    )?;

    if args.verbose {
        println!("{}", "Crawling pages...".bright_yellow());
    }

    crawler.crawl().await?;

    let unique_links: std::collections::HashSet<String> = crawler
        .pages
        .values()
        .flat_map(|page| page.links.iter().map(|link| link.url.clone()))
        .collect();

    println!(
        "{} {} unique pages crawled, {} unique links detected",
        "Success:".bright_green().bold(),
        crawler.pages.len(),
        unique_links.len()
    );

    if args.verbose {
        println!(
            "{} {:#?}",
            "Crawled pages:".bright_white().bold(),
            crawler.pages
        );
    }

    println!();

    // Check links
    if args.verbose {
        println!("{}", "Checking links...".bright_yellow());
    }

    let link_checker = LinkChecker::new();
    link_checker
        .check_all_links(&mut crawler.pages, args.ignore_redirects)
        .await?;

    if args.verbose {
        println!("{}", "Links checked".bright_green());
        println!();
    }

    // Analyze SEO
    if args.verbose {
        println!("{}", "Analyzing SEO...".bright_yellow());
    }

    SeoAnalyzer::analyze_pages(&mut crawler.pages);

    if args.verbose {
        println!("{}", "SEO analysis complete".bright_green());
        println!();
    }

    // Generate report
    let report = Reporter::generate_report(&args.url, &crawler.pages);

    // Output report
    match args.output.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&report)?;
            println!("{}", json);
        }
        _ => {
            Reporter::print_text_report(&report);
        }
    }

    // Save to file if requested
    if let Some(filename) = args.save {
        Reporter::save_json_report(&report, &filename)?;
    }

    Ok(())
}
