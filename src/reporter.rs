use crate::models::{CrawlReport, CrawlSummary, IssueSeverity, PageInfo, SitemapEntry};
use anyhow::Result;
use colored::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

pub struct Reporter;

impl Reporter {
    pub fn generate_report(start_url: &str, pages: HashMap<String, PageInfo>) -> CrawlReport {
        Self::generate_report_with_sitemap(start_url, pages, Vec::new())
    }

    pub fn generate_report_with_sitemap(
        start_url: &str,
        pages: HashMap<String, PageInfo>,
        sitemap: Vec<SitemapEntry>,
    ) -> CrawlReport {
        let summary = Self::summarize_pages(&pages);
        let timestamp = chrono::Utc::now().to_rfc3339();

        CrawlReport {
            start_url: start_url.to_string(),
            pages,
            sitemap,
            summary,
            timestamp,
        }
    }

    pub fn summarize_pages(pages: &HashMap<String, PageInfo>) -> CrawlSummary {
        let mut errors = 0;
        let mut warnings = 0;
        let mut infos = 0;
        let mut broken_links = 0;
        let mut total_links = 0;

        for page in pages.values() {
            total_links += page.links.len();

            for issue in &page.issues {
                match issue.severity {
                    IssueSeverity::Error => errors += 1,
                    IssueSeverity::Warning => warnings += 1,
                    IssueSeverity::Info => infos += 1,
                }
            }

            broken_links += page
                .links
                .iter()
                .filter(|link| {
                    link.status_code.is_some_and(|code| code >= 400) || link.check_error.is_some()
                })
                .count();
        }

        CrawlSummary {
            total_pages: pages.len(),
            total_links,
            broken_links,
            errors,
            warnings,
            infos,
        }
    }

    pub fn print_text_report(report: &CrawlReport, mut out: impl std::io::Write) {
        writeln!(out, "\n{}", "=".repeat(80).bright_blue()).unwrap();
        writeln!(out, "{}", "Scoutly - Crawl Report".bright_cyan().bold()).unwrap();
        writeln!(out, "{}", "=".repeat(80).bright_blue()).unwrap();
        writeln!(out).unwrap();

        let start_url = report.start_url.as_str();
        let start_url_line = format!("{}: {}", "Start URL".bright_white().bold(), start_url);
        writeln!(out, "{start_url_line}").unwrap();

        let timestamp = report.timestamp.as_str();
        let timestamp_line = format!("{}: {}", "Timestamp".bright_white().bold(), timestamp);
        writeln!(out, "{timestamp_line}").unwrap();
        writeln!(out).unwrap();

        // Summary
        writeln!(out, "{}", "Summary".bright_yellow().bold().underline()).unwrap();
        writeln!(
            out,
            "  Total Pages Crawled: {}",
            report.summary.total_pages.to_string().bright_green()
        )
        .unwrap();
        writeln!(
            out,
            "  Total Links Found:   {}",
            report.summary.total_links.to_string().bright_green()
        )
        .unwrap();
        writeln!(
            out,
            "  Broken Links:        {}",
            if report.summary.broken_links > 0 {
                report.summary.broken_links.to_string().bright_red()
            } else {
                report.summary.broken_links.to_string().bright_green()
            }
        )
        .unwrap();
        writeln!(
            out,
            "  Errors:              {}",
            if report.summary.errors > 0 {
                report.summary.errors.to_string().bright_red()
            } else {
                report.summary.errors.to_string().bright_green()
            }
        )
        .unwrap();
        writeln!(
            out,
            "  Warnings:            {}",
            if report.summary.warnings > 0 {
                report.summary.warnings.to_string().yellow()
            } else {
                report.summary.warnings.to_string().bright_green()
            }
        )
        .unwrap();
        writeln!(
            out,
            "  Info:                {}",
            report.summary.infos.to_string().bright_cyan()
        )
        .unwrap();
        writeln!(
            out,
            "  Sitemap Entries:     {}",
            report.sitemap.len().to_string().bright_green()
        )
        .unwrap();
        writeln!(out).unwrap();

        // Pages with issues
        let mut pages_with_issues: Vec<_> = report
            .pages
            .values()
            .filter(|page| !page.issues.is_empty())
            .collect();
        pages_with_issues.sort_by_key(|page| page.crawl_depth);

        if !pages_with_issues.is_empty() {
            writeln!(
                out,
                "{}",
                "Pages with Issues".bright_yellow().bold().underline()
            )
            .unwrap();
            for page in pages_with_issues {
                writeln!(out).unwrap();
                writeln!(out, "  {} {}", "URL:".bright_white().bold(), page.url).unwrap();
                writeln!(
                    out,
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
                )
                .unwrap();
                writeln!(out, "    Depth:  {}", page.crawl_depth).unwrap();

                if let Some(title) = &page.title {
                    writeln!(out, "    Title:  {}", title.bright_white()).unwrap();
                }

                // Display Open Graph information if present
                if page.open_graph.og_title.is_some()
                    || page.open_graph.og_description.is_some()
                    || page.open_graph.og_image.is_some()
                    || page.open_graph.og_url.is_some()
                    || page.open_graph.og_type.is_some()
                {
                    writeln!(out, "    Open Graph:").unwrap();
                    if let Some(og_title) = &page.open_graph.og_title {
                        writeln!(out, "      og:title:       {}", og_title.bright_white()).unwrap();
                    }
                    if let Some(og_desc) = &page.open_graph.og_description {
                        writeln!(out, "      og:description: {}", og_desc.bright_white()).unwrap();
                    }
                    if let Some(og_image) = &page.open_graph.og_image {
                        writeln!(out, "      og:image:       {}", og_image.bright_white()).unwrap();
                    }
                    if let Some(og_url) = &page.open_graph.og_url {
                        writeln!(out, "      og:url:         {}", og_url.bright_white()).unwrap();
                    }
                    if let Some(og_type) = &page.open_graph.og_type {
                        writeln!(out, "      og:type:        {}", og_type.bright_white()).unwrap();
                    }
                    if let Some(og_site_name) = &page.open_graph.og_site_name {
                        writeln!(out, "      og:site_name:   {}", og_site_name.bright_white())
                            .unwrap();
                    }
                    if let Some(og_locale) = &page.open_graph.og_locale {
                        writeln!(out, "      og:locale:      {}", og_locale.bright_white())
                            .unwrap();
                    }
                }

                writeln!(out, "    Issues:").unwrap();
                for issue in &page.issues {
                    let severity_str = match issue.severity {
                        IssueSeverity::Error => "ERROR".bright_red(),
                        IssueSeverity::Warning => "WARN ".yellow(),
                        IssueSeverity::Info => "INFO ".bright_cyan(),
                    };
                    writeln!(out, "      [{}] {}", severity_str, issue.message).unwrap();
                }
            }
        }

        if !report.sitemap.is_empty() {
            writeln!(out).unwrap();
            writeln!(out, "{}", "Sitemap".bright_yellow().bold().underline()).unwrap();
            for entry in &report.sitemap {
                writeln!(out).unwrap();
                writeln!(out, "  {} {}", "URL:".bright_white().bold(), entry.url).unwrap();
                writeln!(out, "    Title:             {}", entry.title.bright_white()).unwrap();
                writeln!(out, "    Priority:          {}", entry.display_priority()).unwrap();
                writeln!(
                    out,
                    "    Change frequency:  {}",
                    entry.display_change_frequency()
                )
                .unwrap();
            }
        }

        writeln!(out).unwrap();
        writeln!(out, "{}", "=".repeat(80).bright_blue()).unwrap();
    }

    pub fn save_json_report(report: &CrawlReport, filename: &str) -> Result<()> {
        let json = serde_json::to_string_pretty(report)?;
        let mut file = File::create(filename)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{IssueType, Link, OpenGraphTags, SeoIssue};

    fn create_test_page(
        url: &str,
        status_code: Option<u16>,
        title: Option<&str>,
        issues: Vec<SeoIssue>,
        links: Vec<Link>,
        crawl_depth: usize,
    ) -> PageInfo {
        PageInfo {
            url: url.to_string(),
            status_code,
            content_type: Some("text/html".to_string()),
            title: title.map(|t| t.to_string()),
            meta_description: None,
            h1_tags: vec![],
            links,
            images: vec![],
            open_graph: OpenGraphTags::default(),
            issues,
            crawl_depth,
        }
    }

    fn create_test_issue(severity: IssueSeverity, message: &str) -> SeoIssue {
        let issue_type = match severity {
            IssueSeverity::Error => IssueType::MissingTitle,
            IssueSeverity::Warning => IssueType::MissingImageAlt,
            IssueSeverity::Info => IssueType::Redirect,
        };

        SeoIssue {
            severity,
            issue_type,
            message: message.to_string(),
        }
    }

    fn create_test_link(url: &str, status_code: Option<u16>) -> Link {
        Link {
            url: url.to_string(),
            text: "Link Text".to_string(),
            is_external: false,
            status_code,
            redirected_url: None,
            check_error: None,
        }
    }

    fn create_transport_error_link(url: &str, error: &str) -> Link {
        Link {
            url: url.to_string(),
            text: "Link Text".to_string(),
            is_external: false,
            status_code: None,
            redirected_url: None,
            check_error: Some(error.to_string()),
        }
    }

    #[test]
    fn test_generate_report_empty_pages() {
        let pages = HashMap::new();
        let report = Reporter::generate_report("https://example.com", pages);

        assert_eq!(report.start_url, "https://example.com");
        assert_eq!(report.summary.total_pages, 0);
        assert_eq!(report.summary.total_links, 0);
        assert_eq!(report.summary.broken_links, 0);
        assert_eq!(report.summary.errors, 0);
        assert_eq!(report.summary.warnings, 0);
        assert_eq!(report.summary.infos, 0);
        assert!(report.sitemap.is_empty());
        assert!(!report.timestamp.is_empty());
    }

    #[test]
    fn test_generate_report_with_all_severity_types() {
        let mut pages = HashMap::new();

        let issues = vec![
            create_test_issue(IssueSeverity::Error, "Error issue"),
            create_test_issue(IssueSeverity::Warning, "Warning issue"),
            create_test_issue(IssueSeverity::Info, "Info issue"),
        ];

        let links = vec![
            create_test_link("https://example.com/page1", Some(200)),
            create_test_link("https://example.com/page2", Some(404)),
            create_test_link("https://example.com/page3", Some(500)),
        ];

        let page = create_test_page(
            "https://example.com",
            Some(200),
            Some("Test Page"),
            issues,
            links,
            0,
        );

        pages.insert("https://example.com".to_string(), page);

        let report = Reporter::generate_report("https://example.com", pages);

        assert_eq!(report.summary.total_pages, 1);
        assert_eq!(report.summary.total_links, 3);
        assert_eq!(report.summary.broken_links, 2);
        assert_eq!(report.summary.errors, 1);
        assert_eq!(report.summary.warnings, 1);
        assert_eq!(report.summary.infos, 1);
    }

    #[test]
    fn test_generate_report_multiple_pages() {
        let mut pages = HashMap::new();

        let page1_issues = vec![
            create_test_issue(IssueSeverity::Error, "Error 1"),
            create_test_issue(IssueSeverity::Error, "Error 2"),
        ];
        let page1_links = vec![
            create_test_link("https://example.com/link1", Some(404)),
            create_test_link("https://example.com/link2", Some(200)),
        ];
        let page1 = create_test_page(
            "https://example.com/page1",
            Some(200),
            Some("Page 1"),
            page1_issues,
            page1_links,
            1,
        );

        let page2_issues = vec![
            create_test_issue(IssueSeverity::Warning, "Warning 1"),
            create_test_issue(IssueSeverity::Warning, "Warning 2"),
            create_test_issue(IssueSeverity::Warning, "Warning 3"),
        ];
        let page2_links = vec![create_test_link("https://example.com/link3", Some(200))];
        let page2 = create_test_page(
            "https://example.com/page2",
            Some(200),
            Some("Page 2"),
            page2_issues,
            page2_links,
            2,
        );

        let page3_issues = vec![
            create_test_issue(IssueSeverity::Info, "Info 1"),
            create_test_issue(IssueSeverity::Info, "Info 2"),
        ];
        let page3 = create_test_page(
            "https://example.com/page3",
            Some(200),
            None,
            page3_issues,
            vec![],
            0,
        );

        pages.insert("https://example.com/page1".to_string(), page1);
        pages.insert("https://example.com/page2".to_string(), page2);
        pages.insert("https://example.com/page3".to_string(), page3);

        let report = Reporter::generate_report("https://example.com", pages);

        assert_eq!(report.summary.total_pages, 3);
        assert_eq!(report.summary.total_links, 3);
        assert_eq!(report.summary.broken_links, 1);
        assert_eq!(report.summary.errors, 2);
        assert_eq!(report.summary.warnings, 3);
        assert_eq!(report.summary.infos, 2);
    }

    #[test]
    fn test_generate_report_broken_links_boundary() {
        let mut pages = HashMap::new();

        let links = vec![
            create_test_link("https://example.com/ok", Some(200)),
            create_test_link("https://example.com/redirect", Some(301)),
            create_test_link("https://example.com/redirect2", Some(399)),
            create_test_link("https://example.com/bad", Some(400)),
            create_test_link("https://example.com/notfound", Some(404)),
            create_test_link("https://example.com/error", Some(500)),
        ];

        let page = create_test_page("https://example.com", Some(200), None, vec![], links, 0);
        pages.insert("https://example.com".to_string(), page);

        let report = Reporter::generate_report("https://example.com", pages);

        assert_eq!(report.summary.total_links, 6);
        assert_eq!(report.summary.broken_links, 3);
    }

    #[test]
    fn test_generate_report_links_without_status_code() {
        let mut pages = HashMap::new();

        let links = vec![
            create_test_link("https://example.com/page1", None),
            create_test_link("https://example.com/page2", None),
            create_test_link("https://example.com/page3", Some(404)),
        ];

        let page = create_test_page("https://example.com", Some(200), None, vec![], links, 0);
        pages.insert("https://example.com".to_string(), page);

        let report = Reporter::generate_report("https://example.com", pages);

        assert_eq!(report.summary.total_links, 3);
        assert_eq!(report.summary.broken_links, 1);
    }

    #[test]
    fn test_generate_report_transport_failures_count_as_broken_links() {
        let mut pages = HashMap::new();

        let links = vec![
            create_transport_error_link("https://example.com/down", "connection failed"),
            create_test_link("https://example.com/ok", Some(200)),
        ];

        let page = create_test_page("https://example.com", Some(200), None, vec![], links, 0);
        pages.insert("https://example.com".to_string(), page);

        let report = Reporter::generate_report("https://example.com", pages);

        assert_eq!(report.summary.total_links, 2);
        assert_eq!(report.summary.broken_links, 1);
    }

    #[test]
    fn test_print_text_report_with_issues() {
        let mut pages = HashMap::new();

        let issues = vec![
            create_test_issue(IssueSeverity::Error, "Error message"),
            create_test_issue(IssueSeverity::Warning, "Warning message"),
            create_test_issue(IssueSeverity::Info, "Info message"),
        ];

        let page1 = create_test_page(
            "https://example.com/page1",
            Some(200),
            Some("Test Page"),
            issues.clone(),
            vec![],
            1,
        );

        let page2 = create_test_page(
            "https://example.com/page2",
            Some(301),
            Some("Redirect Page"),
            vec![create_test_issue(IssueSeverity::Error, "Error on redirect")],
            vec![],
            2,
        );

        let page3 = create_test_page(
            "https://example.com/page3",
            Some(404),
            Some("Not Found"),
            vec![create_test_issue(IssueSeverity::Warning, "Warning on 404")],
            vec![],
            0,
        );

        let page4 = create_test_page(
            "https://example.com/page4",
            None,
            None,
            vec![create_test_issue(
                IssueSeverity::Info,
                "Info without status",
            )],
            vec![],
            3,
        );

        pages.insert("https://example.com/page1".to_string(), page1);
        pages.insert("https://example.com/page2".to_string(), page2);
        pages.insert("https://example.com/page3".to_string(), page3);
        pages.insert("https://example.com/page4".to_string(), page4);

        let report = Reporter::generate_report("https://example.com", pages);

        Reporter::print_text_report(&report, std::io::stdout());
    }

    #[test]
    fn test_print_text_report_no_issues() {
        let mut pages = HashMap::new();

        let page = create_test_page(
            "https://example.com",
            Some(200),
            Some("Clean Page"),
            vec![],
            vec![],
            0,
        );

        pages.insert("https://example.com".to_string(), page);

        let report = Reporter::generate_report("https://example.com", pages);

        Reporter::print_text_report(&report, std::io::stdout());
    }

    #[test]
    fn test_print_text_report_with_broken_links() {
        let mut pages = HashMap::new();

        let links = vec![create_test_link("https://example.com/broken", Some(404))];

        let page = create_test_page(
            "https://example.com",
            Some(200),
            Some("Page with Broken Link"),
            vec![],
            links,
            0,
        );

        pages.insert("https://example.com".to_string(), page);

        let report = Reporter::generate_report("https://example.com", pages);

        assert_eq!(report.summary.broken_links, 1);

        Reporter::print_text_report(&report, std::io::stdout());
    }

    #[test]
    fn test_print_text_report_summary_colors() {
        let mut pages = HashMap::new();

        let issues = vec![
            create_test_issue(IssueSeverity::Error, "Error"),
            create_test_issue(IssueSeverity::Warning, "Warning"),
        ];

        let links = vec![create_test_link("https://example.com/broken", Some(404))];

        let page = create_test_page(
            "https://example.com",
            Some(200),
            Some("Test"),
            issues,
            links,
            0,
        );

        pages.insert("https://example.com".to_string(), page);

        let report = Reporter::generate_report("https://example.com", pages);

        Reporter::print_text_report(&report, std::io::stdout());
    }

    #[test]
    fn test_save_json_report() {
        let mut pages = HashMap::new();

        let issues = vec![create_test_issue(IssueSeverity::Error, "Test error")];

        let page = create_test_page(
            "https://example.com",
            Some(200),
            Some("Test Page"),
            issues,
            vec![],
            0,
        );

        pages.insert("https://example.com".to_string(), page);

        let report = Reporter::generate_report("https://example.com", pages);

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_report.json");
        let filename = file_path.to_str().unwrap();

        let result = Reporter::save_json_report(&report, filename);
        assert!(result.is_ok());

        let json_content = std::fs::read_to_string(filename).expect("Failed to read file");
        assert!(!json_content.is_empty());

        let deserialized: CrawlReport =
            serde_json::from_str(&json_content).expect("Failed to deserialize");
        assert_eq!(deserialized.start_url, "https://example.com");
        assert_eq!(deserialized.summary.total_pages, 1);
        assert_eq!(deserialized.summary.errors, 1);
        assert!(deserialized.sitemap.is_empty());
    }

    #[test]
    fn test_generate_report_with_sitemap_entries() {
        let report = Reporter::generate_report_with_sitemap(
            "https://example.com",
            HashMap::new(),
            vec![SitemapEntry {
                url: "https://example.com/about".to_string(),
                title: "About".to_string(),
                priority: Some("0.8".to_string()),
                change_frequency: Some("weekly".to_string()),
            }],
        );

        assert_eq!(report.sitemap.len(), 1);
        assert_eq!(report.sitemap[0].title, "About");
        assert_eq!(report.sitemap[0].display_priority(), "0.8");
    }

    #[test]
    fn test_pages_cloned_in_report() {
        let mut pages = HashMap::new();

        let page = create_test_page(
            "https://example.com",
            Some(200),
            Some("Test"),
            vec![],
            vec![],
            0,
        );

        pages.insert("https://example.com".to_string(), page);

        let report = Reporter::generate_report("https://example.com", pages);

        assert_eq!(report.pages.len(), 1);
        assert!(report.pages.contains_key("https://example.com"));
    }

    #[test]
    fn test_print_text_report_with_open_graph_tags() {
        let mut pages = HashMap::new();

        let page = PageInfo {
            url: "https://example.com/og-page".to_string(),
            status_code: Some(200),
            content_type: Some("text/html".to_string()),
            title: Some("Page with OG Tags".to_string()),
            meta_description: None,
            h1_tags: vec![],
            links: vec![],
            images: vec![],
            open_graph: OpenGraphTags {
                og_title: Some("OG Title".to_string()),
                og_description: Some("OG Description".to_string()),
                og_image: Some("https://example.com/og-image.jpg".to_string()),
                og_url: Some("https://example.com/og-page".to_string()),
                og_type: Some("website".to_string()),
                og_site_name: Some("Example Site".to_string()),
                og_locale: Some("en_US".to_string()),
            },
            issues: vec![create_test_issue(
                IssueSeverity::Info,
                "Test issue to trigger display",
            )],
            crawl_depth: 0,
        };

        pages.insert("https://example.com/og-page".to_string(), page);

        let report = Reporter::generate_report("https://example.com", pages);

        Reporter::print_text_report(&report, std::io::stdout());
    }

    #[test]
    fn test_print_text_report_with_partial_open_graph_tags() {
        let mut pages = HashMap::new();

        let page = PageInfo {
            url: "https://example.com/partial-og".to_string(),
            status_code: Some(200),
            content_type: Some("text/html".to_string()),
            title: Some("Page with Partial OG Tags".to_string()),
            meta_description: None,
            h1_tags: vec![],
            links: vec![],
            images: vec![],
            open_graph: OpenGraphTags {
                og_title: Some("Partial OG Title".to_string()),
                og_description: None,
                og_image: Some("https://example.com/image.jpg".to_string()),
                og_url: None,
                og_type: Some("article".to_string()),
                og_site_name: None,
                og_locale: None,
            },
            issues: vec![create_test_issue(
                IssueSeverity::Warning,
                "Test warning issue",
            )],
            crawl_depth: 1,
        };

        pages.insert("https://example.com/partial-og".to_string(), page);

        let report = Reporter::generate_report("https://example.com", pages);

        Reporter::print_text_report(&report, std::io::stdout());
    }

    #[test]
    fn text_report_includes_header_timestamp_and_sitemap_section() {
        let report = Reporter::generate_report_with_sitemap(
            "https://example.com",
            HashMap::from([(
                "https://example.com/page".to_string(),
                create_test_page(
                    "https://example.com/page",
                    Some(200),
                    Some("Page"),
                    vec![create_test_issue(IssueSeverity::Info, "redirected")],
                    vec![create_test_link("https://example.com/next", Some(200))],
                    1,
                ),
            )]),
            vec![SitemapEntry {
                url: "https://example.com/page".to_string(),
                title: "Page".to_string(),
                priority: Some("0.8".to_string()),
                change_frequency: Some("weekly".to_string()),
            }],
        );

        let mut out = Vec::new();
        Reporter::print_text_report(&report, &mut out);
        let output = String::from_utf8(out).expect("report output should be utf8");

        assert!(output.contains("Scoutly - Crawl Report"));
        assert!(output.contains("Start URL"));
        assert!(output.contains("https://example.com"));
        assert!(output.contains("Timestamp"));
        assert!(output.contains(&report.timestamp));
        assert!(output.contains("Sitemap"));
        assert!(output.contains("Priority"));
        assert!(output.contains("Change frequency"));
        assert!(output.contains("weekly"));
    }
}
