mod server;

use scoutly::crawler::Crawler;
use scoutly::link_checker::LinkChecker;
use scoutly::models::{IssueSeverity, IssueType};
use server::{get_test_server_url, start_link_test_server};

#[tokio::test]
async fn test_link_checker() {
    start_link_test_server().await;

    let base_url = get_test_server_url().await;

    let mut crawler =
        Crawler::new(&base_url, 2, 50, false, false, None, 1).expect("Failed to create crawler");

    crawler.crawl().await.expect("Crawl failed");

    let checker = LinkChecker::new();

    checker
        .check_all_links(&mut crawler.pages, false)
        .await
        .expect("Link checking failed");

    // Test case 1: Working links
    {
        let url = format!("{}/links-working.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("links-working.html not found");

        let working_link = page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:3000/ok")
            .expect("Working link not found");

        assert_eq!(
            working_link.status_code,
            Some(200),
            "Working link should have status code 200"
        );

        assert_eq!(
            working_link.redirected_url, None,
            "Working link should not redirect"
        );

        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| {
                issue.issue_type == IssueType::BrokenLink
                    && issue.message.contains("http://127.0.0.1:3000/ok")
            })
            .collect();

        assert!(
            issues.is_empty(),
            "Working link should not have broken link issues"
        );
    }

    // Test case 2: Broken links (404 and 500)
    {
        let url = format!("{}/links-broken.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("links-broken.html not found");

        // Check 404 link
        let not_found_link = page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:3000/not-found")
            .expect("404 link not found");

        assert_eq!(
            not_found_link.status_code,
            Some(404),
            "404 link should have status code 404"
        );

        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| {
                issue.issue_type == IssueType::BrokenLink
                    && issue.message.contains("http://127.0.0.1:3000/not-found")
            })
            .collect();

        assert!(
            !issues.is_empty(),
            "404 link should generate a broken link issue"
        );

        assert_eq!(
            issues[0].severity,
            IssueSeverity::Error,
            "Broken link issue should have Error severity"
        );

        // Check 500 link
        let server_error_link = page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:3000/server-error")
            .expect("500 link not found");

        assert_eq!(
            server_error_link.status_code,
            Some(500),
            "500 link should have status code 500"
        );

        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| {
                issue.issue_type == IssueType::BrokenLink
                    && issue.message.contains("http://127.0.0.1:3000/server-error")
            })
            .collect();

        assert!(
            !issues.is_empty(),
            "500 link should generate a broken link issue"
        );
    }

    // Test case 3: Redirects not ignored
    {
        let url = format!("{}/links-redirect.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("links-redirect.html not found");

        let redirect_link = page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:3000/redirect")
            .expect("Redirect link not found");

        assert_eq!(
            redirect_link.status_code,
            Some(200),
            "Redirect should follow to final destination with 200"
        );

        assert_eq!(
            redirect_link.redirected_url,
            Some("http://127.0.0.1:3000/ok".to_string()),
            "Redirect link should capture the final URL"
        );

        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| {
                issue.issue_type == IssueType::Redirect
                    && issue.message.contains("http://127.0.0.1:3000/redirect")
            })
            .collect();

        assert!(
            !issues.is_empty(),
            "Redirect link should generate a redirect issue when not ignored"
        );

        assert_eq!(
            issues[0].severity,
            IssueSeverity::Info,
            "Redirect issue should have Info severity"
        );
    }

    // Test case 4: Fragment handling
    {
        let url = format!("{}/links-fragments.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("links-fragments.html not found");

        let fragment_link = page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:3000/ok#section1")
            .expect("Fragment link not found");

        assert_eq!(
            fragment_link.status_code,
            Some(200),
            "Fragment link should have status code 200"
        );

        assert_eq!(
            fragment_link.redirected_url, None,
            "Fragment-only difference should not be marked as redirect"
        );

        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| {
                issue.issue_type == IssueType::Redirect
                    && issue.message.contains("http://127.0.0.1:3000/ok#section1")
            })
            .collect();

        assert!(
            issues.is_empty(),
            "Fragment-only differences should not generate redirect issues"
        );
    }

    // Test case 5: Link deduplication
    {
        let working_url = format!("{}/links-working.html", base_url);
        let working_page = crawler
            .pages
            .get(&working_url)
            .expect("links-working.html not found");

        let working_link = working_page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:3000/ok")
            .expect("Working link not found on working page");

        assert_eq!(
            working_link.status_code,
            Some(200),
            "Working link should have status code 200 on first page"
        );

        let duplicate_url = format!("{}/links-duplicate.html", base_url);
        let duplicate_page = crawler
            .pages
            .get(&duplicate_url)
            .expect("links-duplicate.html not found");

        let duplicate_link = duplicate_page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:3000/ok")
            .expect("Duplicate link not found on duplicate page");

        assert_eq!(
            duplicate_link.status_code,
            Some(200),
            "Same link should have status code 200 on second page (deduplication)"
        );
    }

    // Test case 6: Mixed links
    {
        let url = format!("{}/links-mixed.html", base_url);
        let page = crawler.pages.get(&url).expect("links-mixed.html not found");

        // Should have one working link
        let working_link = page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:3000/ok")
            .expect("Working link not found");

        assert_eq!(working_link.status_code, Some(200));

        // Should have one broken link
        let broken_link = page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:3000/not-found")
            .expect("Broken link not found");

        assert_eq!(broken_link.status_code, Some(404));

        // Should have one redirect link
        let redirect_link = page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:3000/redirect")
            .expect("Redirect link not found");

        assert_eq!(redirect_link.status_code, Some(200));
        assert_eq!(
            redirect_link.redirected_url,
            Some("http://127.0.0.1:3000/ok".to_string())
        );

        // Should have issues for both broken link and redirect
        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| issue.issue_type == IssueType::BrokenLink)
            .collect();

        assert_eq!(issues.len(), 1, "Should have exactly one broken link issue");

        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| issue.issue_type == IssueType::Redirect)
            .collect();

        assert_eq!(issues.len(), 1, "Should have exactly one redirect issue");
    }

    // Test case 7: Unreachable links
    {
        let url = format!("{}/links-unreachable.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("links-unreachable.html not found");

        let unreachable_link = page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:9999/unreachable")
            .expect("Unreachable link not found");

        assert_eq!(
            unreachable_link.status_code, None,
            "Unreachable link should have None as status code"
        );

        assert_eq!(
            unreachable_link.redirected_url, None,
            "Unreachable link should have None as redirected URL"
        );

        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| {
                issue.issue_type == IssueType::BrokenLink
                    && issue.message.contains("http://127.0.0.1:9999/unreachable")
            })
            .collect();

        assert!(
            issues.is_empty(),
            "Connection failure should not generate broken link issue (only HTTP errors do)"
        );
    }

    // Test case 8: Redirects ignored
    {
        // Create a new crawler and check with ignore_redirects = true
        let mut crawler = Crawler::new(&base_url, 2, 50, false, false, None, 1)
            .expect("Failed to create crawler");

        crawler.crawl().await.expect("Crawl failed");

        let checker = LinkChecker::new();

        checker
            .check_all_links(&mut crawler.pages, true)
            .await
            .expect("Link checking failed");

        let url = format!("{}/links-redirect.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("links-redirect.html not found");

        let redirect_link = page
            .links
            .iter()
            .find(|link| link.url == "http://127.0.0.1:3000/redirect")
            .expect("Redirect link not found");

        assert_eq!(
            redirect_link.redirected_url,
            Some("http://127.0.0.1:3000/ok".to_string()),
            "Redirect link should still capture the final URL even when ignored"
        );

        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| {
                issue.issue_type == IssueType::Redirect
                    && issue.message.contains("http://127.0.0.1:3000/redirect")
            })
            .collect();

        assert!(
            issues.is_empty(),
            "Redirect link should NOT generate an issue when ignored"
        );
    }
}

#[tokio::test]
async fn test_link_checker_default() {
    start_link_test_server().await;

    let base_url = get_test_server_url().await;

    let mut crawler =
        Crawler::new(&base_url, 2, 50, false, false, None, 1).expect("Failed to create crawler");

    crawler.crawl().await.expect("Crawl failed");

    let checker = LinkChecker::default();

    checker
        .check_all_links(&mut crawler.pages, false)
        .await
        .expect("Link checking failed with default checker");

    let url = format!("{}/links-working.html", base_url);
    let page = crawler
        .pages
        .get(&url)
        .expect("links-working.html not found");

    let working_link = page
        .links
        .iter()
        .find(|link| link.url == "http://127.0.0.1:3000/ok")
        .expect("Working link not found");

    assert_eq!(
        working_link.status_code,
        Some(200),
        "Default checker should work the same as new()"
    );
}
