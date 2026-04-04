mod server;

use scoutly::crawler::{Crawler, CrawlerConfig};
use scoutly::link_checker::LinkChecker;
use scoutly::models::{IssueSeverity, IssueType};
use scoutly::runtime::RunEvent;
use server::{get_test_server_url, link_test_server_url, start_link_test_server};
use tokio::sync::mpsc::unbounded_channel;

#[tokio::test]
#[serial_test::serial]
async fn test_link_checker() {
    let link_server_url = start_link_test_server().await;

    let base_url = get_test_server_url().await;

    let mut crawler = Crawler::new(
        &base_url,
        CrawlerConfig {
            max_depth: 2,
            max_pages: 50,
            follow_external: false,
            keep_fragments: false,
            requests_per_second: None,
            concurrent_requests: 1,
            respect_robots_txt: false,
        },
    )
    .expect("Failed to create crawler");

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
            .find(|link| link.url == format!("{}/ok", link_server_url))
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
                    && issue.message.contains(&format!("{}/ok", link_server_url))
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
            .find(|link| link.url == format!("{}/not-found", link_server_url))
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
                    && issue
                        .message
                        .contains(&format!("{}/not-found", link_server_url))
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
            .find(|link| link.url == format!("{}/server-error", link_server_url))
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
                    && issue
                        .message
                        .contains(&format!("{}/server-error", link_server_url))
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
            .find(|link| link.url == format!("{}/redirect", link_server_url))
            .expect("Redirect link not found");

        assert_eq!(
            redirect_link.status_code,
            Some(200),
            "Redirect should follow to final destination with 200"
        );

        assert_eq!(
            redirect_link.redirected_url,
            Some(format!("{}/ok", link_server_url)),
            "Redirect link should capture the final URL"
        );

        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| {
                issue.issue_type == IssueType::Redirect
                    && issue
                        .message
                        .contains(&format!("{}/redirect", link_server_url))
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
            .find(|link| link.url == format!("{}/ok#section1", link_server_url))
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
                    && issue
                        .message
                        .contains(&format!("{}/ok#section1", link_server_url))
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
            .find(|link| link.url == format!("{}/ok", link_server_url))
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
            .find(|link| link.url == format!("{}/ok", link_server_url))
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
            .find(|link| link.url == format!("{}/ok", link_server_url))
            .expect("Working link not found");

        assert_eq!(working_link.status_code, Some(200));

        // Should have one broken link
        let broken_link = page
            .links
            .iter()
            .find(|link| link.url == format!("{}/not-found", link_server_url))
            .expect("Broken link not found");

        assert_eq!(broken_link.status_code, Some(404));

        // Should have one redirect link
        let redirect_link = page
            .links
            .iter()
            .find(|link| link.url == format!("{}/redirect", link_server_url))
            .expect("Redirect link not found");

        assert_eq!(redirect_link.status_code, Some(200));
        assert_eq!(
            redirect_link.redirected_url,
            Some(format!("{}/ok", link_server_url))
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

        assert_eq!(
            unreachable_link.check_error.as_deref(),
            Some("connection failed"),
            "Unreachable link should record a stable transport failure"
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
            !issues.is_empty(),
            "Connection failure should generate an explicit broken link issue"
        );
        assert!(
            issues[0].message.contains("connection failed"),
            "Broken link issue should describe the transport failure"
        );
    }

    // Test case 8: Redirects ignored
    {
        // Create a new crawler and check with ignore_redirects = true
        let mut crawler = Crawler::new(
            &base_url,
            CrawlerConfig {
                max_depth: 2,
                max_pages: 50,
                follow_external: false,
                keep_fragments: false,
                requests_per_second: None,
                concurrent_requests: 1,
                respect_robots_txt: false,
            },
        )
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
            .find(|link| link.url == format!("{}/redirect", link_server_url))
            .expect("Redirect link not found");

        assert_eq!(
            redirect_link.redirected_url,
            Some(format!("{}/ok", link_server_url)),
            "Redirect link should still capture the final URL even when ignored"
        );

        let issues: Vec<_> = page
            .issues
            .iter()
            .filter(|issue| {
                issue.issue_type == IssueType::Redirect
                    && issue
                        .message
                        .contains(&format!("{}/redirect", link_server_url))
            })
            .collect();

        assert!(
            issues.is_empty(),
            "Redirect link should NOT generate an issue when ignored"
        );
    }
}

#[tokio::test]
#[serial_test::serial]
async fn test_link_checker_skips_special_scheme_links() {
    let start_url = format!("{}/links-special-schemes.html", get_test_server_url().await);

    let mut crawler = Crawler::new(
        &start_url,
        CrawlerConfig {
            max_depth: 0,
            max_pages: 10,
            follow_external: true,
            keep_fragments: false,
            requests_per_second: None,
            concurrent_requests: 1,
            respect_robots_txt: false,
        },
    )
    .expect("Failed to create crawler");

    crawler.crawl().await.expect("Crawl failed");

    LinkChecker::new()
        .check_all_links(&mut crawler.pages, false)
        .await
        .expect("Link checking failed");

    let page = crawler
        .pages
        .get(&start_url)
        .expect("links-special-schemes.html not found");

    for url in [
        "mailto:team@example.com",
        "tel:+15551234567",
        "javascript:void(0)",
        "ftp://example.com/files/report.csv",
    ] {
        let link = page
            .links
            .iter()
            .find(|link| link.url == url)
            .unwrap_or_else(|| panic!("Expected special-scheme link {url} to be present"));

        assert_eq!(
            link.status_code, None,
            "Special-scheme links should be skipped instead of fetched"
        );
        assert_eq!(
            link.redirected_url, None,
            "Skipped special-scheme links should not report redirects"
        );
        assert_eq!(
            link.check_error, None,
            "Skipped special-scheme links should not report transport failures"
        );
    }

    assert!(
        page.issues.iter().all(|issue| !matches!(
            issue.issue_type,
            IssueType::BrokenLink | IssueType::Redirect
        )),
        "Special-scheme links should not generate broken-link or redirect issues"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_link_checker_emits_live_progress_with_current_url() {
    start_link_test_server().await;

    let base_url = get_test_server_url().await;

    let mut crawler = Crawler::new(
        &base_url,
        CrawlerConfig {
            max_depth: 2,
            max_pages: 50,
            follow_external: false,
            keep_fragments: false,
            requests_per_second: None,
            concurrent_requests: 1,
            respect_robots_txt: false,
        },
    )
    .expect("Failed to create crawler");

    crawler.crawl().await.expect("Crawl failed");

    let expected_unique_links = crawler
        .pages
        .values()
        .flat_map(|page| page.links.iter().map(|link| link.url.clone()))
        .collect::<std::collections::HashSet<_>>()
        .len();

    let (sender, mut receiver) = unbounded_channel();
    let mut checker = LinkChecker::with_concurrency(2);
    checker.set_progress_sender(sender);

    checker
        .check_all_links(&mut crawler.pages, false)
        .await
        .expect("Link checking failed");

    drop(checker);

    let mut snapshots = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        if let RunEvent::Progress(snapshot) = event {
            snapshots.push(snapshot);
        }
    }

    assert!(
        !snapshots.is_empty(),
        "Link checker should emit progress snapshots while checking links"
    );

    assert!(
        snapshots
            .iter()
            .any(|snapshot| snapshot.message.contains(link_test_server_url())),
        "At least one progress message should include the current link URL"
    );

    let last_snapshot = snapshots.last().expect("Missing final progress snapshot");
    assert_eq!(last_snapshot.links_checked, expected_unique_links);
    assert_eq!(last_snapshot.total_links, expected_unique_links);
    assert!(
        last_snapshot.message.starts_with(&format!(
            "Checking link {}/{}:",
            expected_unique_links, expected_unique_links
        )),
        "Final progress message should report the completed unique-link count"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_link_checker_default() {
    start_link_test_server().await;

    let base_url = get_test_server_url().await;

    let mut crawler = Crawler::new(
        &base_url,
        CrawlerConfig {
            max_depth: 2,
            max_pages: 50,
            follow_external: false,
            keep_fragments: false,
            requests_per_second: None,
            concurrent_requests: 1,
            respect_robots_txt: false,
        },
    )
    .expect("Failed to create crawler");

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
        .find(|link| link.url == format!("{}/ok", link_test_server_url()))
        .expect("Working link not found");

    assert_eq!(
        working_link.status_code,
        Some(200),
        "Default checker should work the same as new()"
    );
}
