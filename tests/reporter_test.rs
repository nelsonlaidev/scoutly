use scoutly::models::{CrawlReport, IssueSeverity, IssueType, Link, PageInfo, SeoIssue};
use scoutly::reporter::Reporter;
use std::collections::HashMap;
use std::fs;

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
    }
}

#[test]
fn test_generate_report_empty_pages() {
    let pages = HashMap::new();
    let report = Reporter::generate_report("https://example.com", &pages);

    assert_eq!(report.start_url, "https://example.com");
    assert_eq!(report.summary.total_pages, 0);
    assert_eq!(report.summary.total_links, 0);
    assert_eq!(report.summary.broken_links, 0);
    assert_eq!(report.summary.errors, 0);
    assert_eq!(report.summary.warnings, 0);
    assert_eq!(report.summary.infos, 0);
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

    let report = Reporter::generate_report("https://example.com", &pages);

    assert_eq!(report.summary.total_pages, 1);
    assert_eq!(report.summary.total_links, 3);
    assert_eq!(report.summary.broken_links, 2); // 404 and 500
    assert_eq!(report.summary.errors, 1);
    assert_eq!(report.summary.warnings, 1);
    assert_eq!(report.summary.infos, 1);
}

#[test]
fn test_generate_report_multiple_pages() {
    let mut pages = HashMap::new();

    // Page 1: with errors and broken links
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

    // Page 2: with warnings
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

    // Page 3: with info issues
    let page3_issues = vec![
        create_test_issue(IssueSeverity::Info, "Info 1"),
        create_test_issue(IssueSeverity::Info, "Info 2"),
    ];
    let page3 = create_test_page(
        "https://example.com/page3",
        Some(200),
        None, // No title
        page3_issues,
        vec![],
        0,
    );

    pages.insert("https://example.com/page1".to_string(), page1);
    pages.insert("https://example.com/page2".to_string(), page2);
    pages.insert("https://example.com/page3".to_string(), page3);

    let report = Reporter::generate_report("https://example.com", &pages);

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

    // Test status codes at boundaries
    let links = vec![
        create_test_link("https://example.com/ok", Some(200)),
        create_test_link("https://example.com/redirect", Some(301)),
        create_test_link("https://example.com/redirect2", Some(399)),
        create_test_link("https://example.com/bad", Some(400)), // Broken
        create_test_link("https://example.com/notfound", Some(404)), // Broken
        create_test_link("https://example.com/error", Some(500)), // Broken
    ];

    let page = create_test_page("https://example.com", Some(200), None, vec![], links, 0);
    pages.insert("https://example.com".to_string(), page);

    let report = Reporter::generate_report("https://example.com", &pages);

    assert_eq!(report.summary.total_links, 6);
    assert_eq!(report.summary.broken_links, 3); // 400, 404, 500
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

    let report = Reporter::generate_report("https://example.com", &pages);

    assert_eq!(report.summary.total_links, 3);
    assert_eq!(report.summary.broken_links, 1); // Only the 404
}

#[test]
fn test_print_text_report_with_issues() {
    let mut pages = HashMap::new();

    let issues = vec![
        create_test_issue(IssueSeverity::Error, "Error message"),
        create_test_issue(IssueSeverity::Warning, "Warning message"),
        create_test_issue(IssueSeverity::Info, "Info message"),
    ];

    // Page with issues and status code 200 (< 300)
    let page1 = create_test_page(
        "https://example.com/page1",
        Some(200),
        Some("Test Page"),
        issues.clone(),
        vec![],
        1,
    );

    // Page with issues and status code 301 (< 400)
    let page2 = create_test_page(
        "https://example.com/page2",
        Some(301),
        Some("Redirect Page"),
        vec![create_test_issue(IssueSeverity::Error, "Error on redirect")],
        vec![],
        2,
    );

    // Page with issues and status code 404 (>= 400)
    let page3 = create_test_page(
        "https://example.com/page3",
        Some(404),
        Some("Not Found"),
        vec![create_test_issue(IssueSeverity::Warning, "Warning on 404")],
        vec![],
        0,
    );

    // Page with issues but no status code
    let page4 = create_test_page(
        "https://example.com/page4",
        None,
        None, // No title
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

    let report = Reporter::generate_report("https://example.com", &pages);

    // This test just ensures the function runs without panic
    Reporter::print_text_report(&report);
}

#[test]
fn test_print_text_report_no_issues() {
    let mut pages = HashMap::new();

    let page = create_test_page(
        "https://example.com",
        Some(200),
        Some("Clean Page"),
        vec![], // No issues
        vec![],
        0,
    );

    pages.insert("https://example.com".to_string(), page);

    let report = Reporter::generate_report("https://example.com", &pages);

    // This test ensures the function runs without panic when there are no issues
    Reporter::print_text_report(&report);
}

#[test]
fn test_print_text_report_with_broken_links() {
    let mut pages = HashMap::new();

    let links = vec![create_test_link("https://example.com/broken", Some(404))];

    let page = create_test_page(
        "https://example.com",
        Some(200),
        Some("Page with Broken Link"),
        vec![], // No issues on the page itself
        links,
        0,
    );

    pages.insert("https://example.com".to_string(), page);

    let report = Reporter::generate_report("https://example.com", &pages);

    // Test that broken links are counted correctly
    assert_eq!(report.summary.broken_links, 1);

    Reporter::print_text_report(&report);
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

    let report = Reporter::generate_report("https://example.com", &pages);

    // This tests the color branching in print_text_report
    Reporter::print_text_report(&report);
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

    let report = Reporter::generate_report("https://example.com", &pages);

    let filename = "test_report.json";

    // Save the report
    let result = Reporter::save_json_report(&report, filename);
    assert!(result.is_ok());

    // Verify the file exists and can be read
    let json_content = fs::read_to_string(filename).expect("Failed to read file");
    assert!(!json_content.is_empty());

    // Verify it can be deserialized back to a CrawlReport
    let deserialized: CrawlReport =
        serde_json::from_str(&json_content).expect("Failed to deserialize");
    assert_eq!(deserialized.start_url, "https://example.com");
    assert_eq!(deserialized.summary.total_pages, 1);
    assert_eq!(deserialized.summary.errors, 1);

    // Clean up
    fs::remove_file(filename).expect("Failed to remove test file");
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

    let report = Reporter::generate_report("https://example.com", &pages);

    assert_eq!(report.pages.len(), 1);
    assert!(report.pages.contains_key("https://example.com"));
}
