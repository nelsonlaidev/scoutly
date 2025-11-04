use crate::models::{CrawlReport, CrawlSummary, IssueSeverity, PageInfo};
use anyhow::Result;
use colored::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

pub struct Reporter;

impl Reporter {
    pub fn generate_report(start_url: &str, pages: &HashMap<String, PageInfo>) -> CrawlReport {
        let summary = Self::calculate_summary(pages);
        let timestamp = chrono::Utc::now().to_rfc3339();

        CrawlReport {
            start_url: start_url.to_string(),
            pages: pages.clone(),
            summary,
            timestamp,
        }
    }

    fn calculate_summary(pages: &HashMap<String, PageInfo>) -> CrawlSummary {
        let mut errors = 0;
        let mut warnings = 0;
        let mut info_count = 0;
        let mut broken_links = 0;
        let mut total_links = 0;

        for page in pages.values() {
            total_links += page.links.len();

            for issue in &page.issues {
                match issue.severity {
                    IssueSeverity::Error => errors += 1,
                    IssueSeverity::Warning => warnings += 1,
                    IssueSeverity::Info => info_count += 1,
                }
            }

            broken_links += page
                .links
                .iter()
                .filter(|link| link.status_code.is_some_and(|code| code >= 400))
                .count();
        }

        CrawlSummary {
            total_pages: pages.len(),
            total_links,
            broken_links,
            errors,
            warnings,
            info_count,
        }
    }

    pub fn print_text_report(report: &CrawlReport) {
        println!("\n{}", "=".repeat(80).bright_blue());
        println!("{}", "Scoutly - Crawl Report".bright_cyan().bold());
        println!("{}", "=".repeat(80).bright_blue());
        println!();

        println!(
            "{}: {}",
            "Start URL".bright_white().bold(),
            report.start_url
        );
        println!(
            "{}: {}",
            "Timestamp".bright_white().bold(),
            report.timestamp
        );
        println!();

        // Summary
        println!("{}", "Summary".bright_yellow().bold().underline());
        println!(
            "  Total Pages Crawled: {}",
            report.summary.total_pages.to_string().bright_green()
        );
        println!(
            "  Total Links Found:   {}",
            report.summary.total_links.to_string().bright_green()
        );
        println!(
            "  Broken Links:        {}",
            if report.summary.broken_links > 0 {
                report.summary.broken_links.to_string().bright_red()
            } else {
                report.summary.broken_links.to_string().bright_green()
            }
        );
        println!(
            "  Errors:              {}",
            if report.summary.errors > 0 {
                report.summary.errors.to_string().bright_red()
            } else {
                report.summary.errors.to_string().bright_green()
            }
        );
        println!(
            "  Warnings:            {}",
            if report.summary.warnings > 0 {
                report.summary.warnings.to_string().yellow()
            } else {
                report.summary.warnings.to_string().bright_green()
            }
        );
        println!(
            "  Info:                {}",
            report.summary.info_count.to_string().bright_cyan()
        );
        println!();

        // Pages with issues
        let mut pages_with_issues: Vec<_> = report
            .pages
            .values()
            .filter(|page| !page.issues.is_empty())
            .collect();
        pages_with_issues.sort_by_key(|page| page.crawl_depth);

        if !pages_with_issues.is_empty() {
            println!("{}", "Pages with Issues".bright_yellow().bold().underline());
            for page in pages_with_issues {
                println!();
                println!("  {} {}", "URL:".bright_white().bold(), page.url);
                println!(
                    "    Status: {}",
                    page.status_code
                        .map(|code| {
                            if code < 300 {
                                code.to_string().bright_green()
                            } else if code < 400 {
                                code.to_string().yellow()
                            } else {
                                code.to_string().bright_red()
                            }
                        })
                        .unwrap_or_else(|| "N/A".dimmed())
                );
                println!("    Depth:  {}", page.crawl_depth);

                if let Some(title) = &page.title {
                    println!("    Title:  {}", title.bright_white());
                }

                println!("    Issues:");
                for issue in &page.issues {
                    let severity_str = match issue.severity {
                        IssueSeverity::Error => "ERROR".bright_red(),
                        IssueSeverity::Warning => "WARN ".yellow(),
                        IssueSeverity::Info => "INFO ".bright_cyan(),
                    };
                    println!("      [{}] {}", severity_str, issue.message);
                }
            }
        }

        println!();
        println!("{}", "=".repeat(80).bright_blue());
    }

    pub fn save_json_report(report: &CrawlReport, filename: &str) -> Result<()> {
        let json = serde_json::to_string_pretty(report)?;
        let mut file = File::create(filename)?;
        file.write_all(json.as_bytes())?;
        println!("Report saved to: {}", filename.bright_green());
        Ok(())
    }
}
