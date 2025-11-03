mod server;

use scoutly::crawler::Crawler;
use scoutly::models::{IssueSeverity, IssueType};
use scoutly::seo_analyzer::SeoAnalyzer;
use server::get_test_server_url;

#[tokio::test]
async fn test_seo_analyzer() {
    let base_url = get_test_server_url().await;

    let mut crawler =
        Crawler::new(&base_url, 2, 50, false, false).expect("Failed to create crawler");

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
