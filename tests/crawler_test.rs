mod server;

use scoutly::crawler::Crawler;
use server::{get_test_server_url, start_link_test_server};

#[tokio::test]
async fn test_crawler() {
    start_link_test_server().await;
    let base_url = get_test_server_url().await;

    // Test case 1: keep_fragments = true
    {
        let mut crawler =
            Crawler::new(&base_url, 2, 50, false, true).expect("Failed to create crawler");

        crawler.crawl().await.expect("Crawl failed");

        let url = format!("{}/crawler-fragments.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("crawler-fragments.html not found");

        // Check that fragment links are extracted with fragments intact
        let fragment_links: Vec<_> = page
            .links
            .iter()
            .filter(|link| link.url.contains("#section"))
            .collect();

        assert!(
            !fragment_links.is_empty(),
            "Should find links with fragments when keep_fragments is true"
        );

        // Verify specific fragment links exist
        let section1_link = page
            .links
            .iter()
            .find(|link| link.url.contains("/crawler-fragments.html#section1"));

        assert!(
            section1_link.is_some(),
            "Should find link to #section1 with fragment preserved"
        );

        let section2_link = page
            .links
            .iter()
            .find(|link| link.url.contains("/crawler-fragments.html#section2"));

        assert!(
            section2_link.is_some(),
            "Should find link to #section2 with fragment preserved"
        );

        let section3_link = page
            .links
            .iter()
            .find(|link| link.url.contains("/crawler-fragments.html#section3"));

        assert!(
            section3_link.is_some(),
            "Should find link to #section3 with fragment preserved"
        );

        // Test that the same URL with different fragments creates different entries
        let intro_link = page
            .links
            .iter()
            .find(|link| link.url.contains("/missing-title.html#intro"));

        assert!(
            intro_link.is_some(),
            "Should find link with fragment to another page"
        );

        // When keep_fragments is true, the crawler should visit and store different URLs with fragments
        // However, since fragments are typically not followed during crawling (they're client-side),
        // we mainly verify that the links are extracted correctly with fragments preserved
        assert!(
            page.links
                .iter()
                .any(|link| link.url.ends_with("#section1")),
            "Fragment URLs should be preserved in extracted links"
        );
    }

    // Test case 2: Extract from <iframe src> tags
    {
        let mut crawler =
            Crawler::new(&base_url, 2, 50, false, false).expect("Failed to create crawler");

        crawler.crawl().await.expect("Crawl failed");

        let url = format!("{}/crawler-iframe.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("crawler-iframe.html not found");

        // Check that iframe links are extracted
        let iframe_links: Vec<_> = page
            .links
            .iter()
            .filter(|link| link.text.starts_with("[iframe]"))
            .collect();

        assert_eq!(
            iframe_links.len(),
            3,
            "Should find 3 iframe links (2 internal, 1 external)"
        );

        // Verify specific iframe links
        let missing_title_iframe = page
            .links
            .iter()
            .find(|link| link.url.contains("/missing-title.html") && link.text.contains("[iframe]"))
            .expect("Should find iframe link to missing-title.html");

        assert_eq!(
            missing_title_iframe.text, "[iframe] Missing Title Page",
            "IFrame link should include title attribute in text"
        );

        let missing_h1_iframe = page
            .links
            .iter()
            .find(|link| link.url.contains("/missing-h1.html") && link.text.contains("[iframe]"))
            .expect("Should find iframe link to missing-h1.html");

        assert_eq!(
            missing_h1_iframe.text, "[iframe] Missing H1 Page",
            "IFrame link should include title attribute in text"
        );

        let port_different_iframe = page
            .links
            .iter()
            .find(|link| link.url.contains("127.0.0.1:3000/ok") && link.text.contains("[iframe]"))
            .expect("Should find iframe link to different port");

        // Note: Since both servers are on 127.0.0.1, this is considered internal
        // (only hostname is compared, not port)
        assert!(
            !port_different_iframe.is_external,
            "Same hostname on different port is considered internal"
        );

        assert_eq!(
            port_different_iframe.text, "[iframe] External OK",
            "IFrame to different port should include title in text"
        );
    }

    // Test case 3: Extract from <video src> and <source src> tags
    {
        let mut crawler =
            Crawler::new(&base_url, 2, 50, false, false).expect("Failed to create crawler");

        crawler.crawl().await.expect("Crawl failed");

        let url = format!("{}/crawler-media.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("crawler-media.html not found");

        // Check video src links
        let video_links: Vec<_> = page
            .links
            .iter()
            .filter(|link| link.text == "[video]")
            .collect();

        assert_eq!(
            video_links.len(),
            1,
            "Should find 1 video src link (from <video src>)"
        );

        let video1_link = page
            .links
            .iter()
            .find(|link| link.url.contains("/media/video1.mp4"))
            .expect("Should find video1.mp4 link");

        assert_eq!(
            video1_link.text, "[video]",
            "Video link should have [video] text"
        );

        // Check source src links
        let source_links: Vec<_> = page
            .links
            .iter()
            .filter(|link| link.text.starts_with("[source"))
            .collect();

        assert_eq!(
            source_links.len(),
            4,
            "Should find 4 source src links (2 video sources, 2 audio sources)"
        );

        // Verify specific source links
        let video2_webm = page
            .links
            .iter()
            .find(|link| link.url.contains("/media/video2.webm"))
            .expect("Should find video2.webm source link");

        assert!(
            video2_webm.text.contains("video/webm"),
            "Source link should include type attribute"
        );

        let video2_mp4 = page
            .links
            .iter()
            .find(|link| link.url.contains("/media/video2.mp4"))
            .expect("Should find video2.mp4 source link");

        assert!(
            video2_mp4.text.contains("video/mp4"),
            "Source link should include type attribute"
        );
    }

    // Test case 4: Extract from <audio src> tags
    {
        let mut crawler =
            Crawler::new(&base_url, 2, 50, false, false).expect("Failed to create crawler");

        crawler.crawl().await.expect("Crawl failed");

        let url = format!("{}/crawler-media.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("crawler-media.html not found");

        // Check audio src links
        let audio_links: Vec<_> = page
            .links
            .iter()
            .filter(|link| link.text == "[audio]")
            .collect();

        assert_eq!(
            audio_links.len(),
            1,
            "Should find 1 audio src link (from <audio src>)"
        );

        let audio1_link = page
            .links
            .iter()
            .find(|link| link.url.contains("/media/audio1.mp3"))
            .expect("Should find audio1.mp3 link");

        assert_eq!(
            audio1_link.text, "[audio]",
            "Audio link should have [audio] text"
        );

        // Check audio source links
        let audio2_ogg = page
            .links
            .iter()
            .find(|link| link.url.contains("/media/audio2.ogg"))
            .expect("Should find audio2.ogg source link");

        assert!(
            audio2_ogg.text.contains("audio/ogg"),
            "Audio source link should include type attribute"
        );

        let audio2_mp3 = page
            .links
            .iter()
            .find(|link| link.url.contains("/media/audio2.mp3"))
            .expect("Should find audio2.mp3 source link");

        assert!(
            audio2_mp3.text.contains("audio/mp3"),
            "Audio source link should include type attribute"
        );
    }

    // Test case 5: Extract from <embed src> tags
    {
        let mut crawler =
            Crawler::new(&base_url, 2, 50, false, false).expect("Failed to create crawler");

        crawler.crawl().await.expect("Crawl failed");

        let url = format!("{}/crawler-media.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("crawler-media.html not found");

        // Check embed src links
        let embed_links: Vec<_> = page
            .links
            .iter()
            .filter(|link| link.text == "[embed]")
            .collect();

        assert_eq!(embed_links.len(), 2, "Should find 2 embed src links");

        let pdf_embed = page
            .links
            .iter()
            .find(|link| link.url.contains("/media/document.pdf"))
            .expect("Should find document.pdf embed link");

        assert_eq!(
            pdf_embed.text, "[embed]",
            "Embed link should have [embed] text"
        );

        let flash_embed = page
            .links
            .iter()
            .find(|link| link.url.contains("/media/flash.swf"))
            .expect("Should find flash.swf embed link");

        assert_eq!(
            flash_embed.text, "[embed]",
            "Embed link should have [embed] text"
        );
    }

    // Test case 6: Extract from <object data> tags
    {
        let mut crawler =
            Crawler::new(&base_url, 2, 50, false, false).expect("Failed to create crawler");

        crawler.crawl().await.expect("Crawl failed");

        let url = format!("{}/crawler-media.html", base_url);
        let page = crawler
            .pages
            .get(&url)
            .expect("crawler-media.html not found");

        // Check object data links
        let object_links: Vec<_> = page
            .links
            .iter()
            .filter(|link| link.text == "[object]")
            .collect();

        assert_eq!(object_links.len(), 2, "Should find 2 object data links");

        let pdf_object = page
            .links
            .iter()
            .find(|link| link.url.contains("/media/object1.pdf"))
            .expect("Should find object1.pdf object link");

        assert_eq!(
            pdf_object.text, "[object]",
            "Object link should have [object] text"
        );

        let svg_object = page
            .links
            .iter()
            .find(|link| link.url.contains("/media/object2.svg"))
            .expect("Should find object2.svg object link");

        assert_eq!(
            svg_object.text, "[object]",
            "Object link should have [object] text"
        );
    }

    // Test case 7: Test max_pages limit
    {
        // Create a crawler with a low max_pages limit
        let mut crawler =
            Crawler::new(&base_url, 5, 3, false, false).expect("Failed to create crawler");

        crawler.crawl().await.expect("Crawl failed");

        // The crawler should stop after visiting exactly max_pages (3) pages
        assert_eq!(
            crawler.pages.len(),
            3,
            "Crawler should stop after visiting max_pages (3) pages"
        );

        // The test site has many more than 3 pages available (20+), so reaching
        // exactly 3 pages confirms the max_pages limit is working correctly
        assert!(
            !crawler.pages.is_empty(),
            "Should have crawled at least some pages"
        );

        // Verify we didn't crawl more than the limit
        assert!(
            crawler.pages.len() <= 3,
            "Should not exceed max_pages limit"
        );
    }

    // Test case 8: Test max_depth limit
    {
        let mut crawler =
            Crawler::new(&base_url, 1, 100, false, false).expect("Failed to create crawler");

        crawler.crawl().await.expect("Crawl failed");

        for (_url, page) in crawler.pages.iter() {
            assert!(
                page.crawl_depth <= 1,
                "Page at depth {} exceeds max_depth of 1",
                page.crawl_depth
            );
        }

        assert!(
            crawler.pages.values().any(|page| page.crawl_depth == 0),
            "Should have at least one page at depth 0 (the starting page)"
        );

        assert!(
            crawler.pages.len() > 1,
            "With max_depth=1, should crawl more than just the starting page"
        );

        let mut crawler_depth_0 =
            Crawler::new(&base_url, 0, 100, false, false).expect("Failed to create crawler");

        crawler_depth_0.crawl().await.expect("Crawl failed");

        assert_eq!(
            crawler_depth_0.pages.len(),
            1,
            "With max_depth=0, should only crawl the starting page"
        );

        assert!(
            crawler_depth_0
                .pages
                .values()
                .all(|page| page.crawl_depth == 0),
            "All pages should be at depth 0 when max_depth=0"
        );
    }

    // Test case 9: Test follow_external parameter
    {
        let mut crawler_no_external =
            Crawler::new(&base_url, 5, 50, false, false).expect("Failed to create crawler");

        crawler_no_external.crawl().await.expect("Crawl failed");

        let mut external_link_urls = std::collections::HashSet::new();
        for page in crawler_no_external.pages.values() {
            for link in &page.links {
                if link.is_external {
                    external_link_urls.insert(link.url.clone());
                }
            }
        }

        if !external_link_urls.is_empty() {
            for external_url in &external_link_urls {
                assert!(
                    !crawler_no_external.pages.contains_key(external_url),
                    "External URL {} should not be crawled when follow_external=false",
                    external_url
                );
            }
        }

        let crawled_external_count = crawler_no_external
            .pages
            .keys()
            .filter(|url| external_link_urls.contains(*url))
            .count();

        assert_eq!(
            crawled_external_count, 0,
            "No external links should be crawled when follow_external=false"
        );

        let mut crawler_with_external =
            Crawler::new(&base_url, 5, 50, true, false).expect("Failed to create crawler");

        crawler_with_external.crawl().await.expect("Crawl failed");

        let mut external_links_with_follow = std::collections::HashSet::new();
        for page in crawler_with_external.pages.values() {
            for link in &page.links {
                if link.is_external {
                    external_links_with_follow.insert(link.url.clone());
                }
            }
        }

        assert_eq!(
            external_link_urls.len(),
            external_links_with_follow.len(),
            "External link extraction should be the same regardless of follow_external setting"
        );
    }
}
