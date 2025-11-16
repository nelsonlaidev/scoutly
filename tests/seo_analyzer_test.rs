mod server;

use scoutly::crawler::{Crawler, CrawlerConfig};
use scoutly::models::{IssueSeverity, IssueType};
use scoutly::seo_analyzer::SeoAnalyzer;
use server::get_test_server_url;

#[tokio::test]
async fn test_seo_analyzer() {
    let base_url = get_test_server_url().await;

    let config = CrawlerConfig {
        max_depth: 2,
        max_pages: 50,
        follow_external: false,
        keep_fragments: false,
        requests_per_second: None,
        concurrent_requests: 1,
        respect_robots_txt: false,
    };
    let mut crawler = Crawler::new(&base_url, config).expect("Failed to create crawler");

    crawler.crawl().await.expect("Crawl failed");

    SeoAnalyzer::analyze_pages(&mut crawler.pages);

    let test_cases = [
        TestCase {
            file: "missing-title.html",
            issue_type: IssueType::MissingTitle,
            severity: IssueSeverity::Error,
            description: "missing title",
        },
        TestCase {
            file: "title-too-short.html",
            issue_type: IssueType::TitleTooShort,
            severity: IssueSeverity::Warning,
            description: "short title",
        },
        TestCase {
            file: "title-too-long.html",
            issue_type: IssueType::TitleTooLong,
            severity: IssueSeverity::Warning,
            description: "long title",
        },
        TestCase {
            file: "missing-meta-desc.html",
            issue_type: IssueType::MissingMetaDescription,
            severity: IssueSeverity::Error,
            description: "missing meta description",
        },
        TestCase {
            file: "meta-desc-too-short.html",
            issue_type: IssueType::MetaDescriptionTooShort,
            severity: IssueSeverity::Warning,
            description: "short meta description",
        },
        TestCase {
            file: "meta-desc-too-long.html",
            issue_type: IssueType::MetaDescriptionTooLong,
            severity: IssueSeverity::Warning,
            description: "long meta description",
        },
        TestCase {
            file: "missing-image-alt.html",
            issue_type: IssueType::MissingImageAlt,
            severity: IssueSeverity::Warning,
            description: "missing image alt text",
        },
        TestCase {
            file: "missing-h1.html",
            issue_type: IssueType::MissingH1,
            severity: IssueSeverity::Warning,
            description: "missing H1 tag",
        },
        TestCase {
            file: "multiple-h1.html",
            issue_type: IssueType::MultipleH1,
            severity: IssueSeverity::Warning,
            description: "multiple H1 tags",
        },
        TestCase {
            file: "thin-content.html",
            issue_type: IssueType::ThinContent,
            severity: IssueSeverity::Warning,
            description: "thin content",
        },
        TestCase {
            file: "og-missing.html",
            issue_type: IssueType::MissingOgTitle,
            severity: IssueSeverity::Info,
            description: "missing og:title",
        },
        TestCase {
            file: "og-missing.html",
            issue_type: IssueType::MissingOgDescription,
            severity: IssueSeverity::Info,
            description: "missing og:description",
        },
        TestCase {
            file: "og-missing.html",
            issue_type: IssueType::MissingOgImage,
            severity: IssueSeverity::Info,
            description: "missing og:image",
        },
        TestCase {
            file: "og-missing.html",
            issue_type: IssueType::MissingOgUrl,
            severity: IssueSeverity::Info,
            description: "missing og:url",
        },
        TestCase {
            file: "og-missing.html",
            issue_type: IssueType::MissingOgType,
            severity: IssueSeverity::Info,
            description: "missing og:type",
        },
    ];

    for case in test_cases {
        let url = format!("{}/{}", base_url, case.file);
        let page = crawler
            .pages
            .get(&url)
            .unwrap_or_else(|| panic!("Page not found: {}", url));

        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| issue.issue_type == case.issue_type)
            .collect();

        assert!(
            !issues.is_empty(),
            "Should detect {} (URL: {})",
            case.description,
            url
        );

        assert_eq!(
            issues[0].severity, case.severity,
            "Incorrect severity for {} (URL: {})",
            case.description, url
        );
    }
}

struct TestCase {
    file: &'static str,
    issue_type: IssueType,
    severity: IssueSeverity,
    description: &'static str,
}

#[tokio::test]
async fn test_open_graph_extraction() {
    let base_url = get_test_server_url().await;

    let config = CrawlerConfig {
        max_depth: 2,
        max_pages: 50,
        follow_external: false,
        keep_fragments: false,
        requests_per_second: None,
        concurrent_requests: 1,
        respect_robots_txt: false,
    };
    let mut crawler = Crawler::new(&base_url, config).expect("Failed to create crawler");

    crawler.crawl().await.expect("Crawl failed");

    // Test complete OG tags page
    let url_complete = format!("{}/og-complete.html", base_url);
    let page_complete = crawler
        .pages
        .get(&url_complete)
        .expect("og-complete.html not found");

    // Verify all OG tags are extracted
    assert_eq!(
        page_complete.open_graph.og_title.as_ref().unwrap(),
        "Complete OG Test Page",
        "og:title should be extracted"
    );
    assert_eq!(
        page_complete.open_graph.og_description.as_ref().unwrap(),
        "This page has all the essential Open Graph meta tags for social media sharing.",
        "og:description should be extracted"
    );
    assert_eq!(
        page_complete.open_graph.og_image.as_ref().unwrap(),
        "https://example.com/images/og-image.jpg",
        "og:image should be extracted"
    );
    assert_eq!(
        page_complete.open_graph.og_url.as_ref().unwrap(),
        "https://example.com/og-complete.html",
        "og:url should be extracted"
    );
    assert_eq!(
        page_complete.open_graph.og_type.as_ref().unwrap(),
        "website",
        "og:type should be extracted"
    );
    assert_eq!(
        page_complete.open_graph.og_site_name.as_ref().unwrap(),
        "Scoutly Test Site",
        "og:site_name should be extracted"
    );
    assert_eq!(
        page_complete.open_graph.og_locale.as_ref().unwrap(),
        "en_US",
        "og:locale should be extracted"
    );

    // Test missing OG tags page
    let url_missing = format!("{}/og-missing.html", base_url);
    let page_missing = crawler
        .pages
        .get(&url_missing)
        .expect("og-missing.html not found");

    // Verify all OG tags are None
    assert!(
        page_missing.open_graph.og_title.is_none(),
        "og:title should be None when not present"
    );
    assert!(
        page_missing.open_graph.og_description.is_none(),
        "og:description should be None when not present"
    );
    assert!(
        page_missing.open_graph.og_image.is_none(),
        "og:image should be None when not present"
    );
    assert!(
        page_missing.open_graph.og_url.is_none(),
        "og:url should be None when not present"
    );
    assert!(
        page_missing.open_graph.og_type.is_none(),
        "og:type should be None when not present"
    );
    assert!(
        page_missing.open_graph.og_site_name.is_none(),
        "og:site_name should be None when not present"
    );
    assert!(
        page_missing.open_graph.og_locale.is_none(),
        "og:locale should be None when not present"
    );
}
