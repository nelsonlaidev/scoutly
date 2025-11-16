mod server;

use actix_web::{App, HttpResponse, HttpServer, web};
use scoutly::crawler::{Crawler, CrawlerConfig};

/// Create a test server with a robots.txt file
async fn start_robots_test_server() -> String {
    let server = HttpServer::new(|| {
        App::new()
            .route("/robots.txt", web::get().to(|| async {
                HttpResponse::Ok()
                    .content_type("text/plain")
                    .body(r#"User-agent: *
Disallow: /admin
Disallow: /secret
Disallow: /private/
Allow: /private/public
"#)
            }))
            .route("/", web::get().to(|| async {
                HttpResponse::Ok()
                    .content_type("text/html")
                    .body(r#"
                    <html>
                        <head><title>Home</title></head>
                        <body>
                            <h1>Home Page</h1>
                            <a href="/admin">Admin</a>
                            <a href="/secret">Secret</a>
                            <a href="/private/file">Private File</a>
                            <a href="/private/public">Public in Private</a>
                            <a href="/allowed">Allowed</a>
                        </body>
                    </html>
                    "#)
            }))
            .route("/admin", web::get().to(|| async {
                HttpResponse::Ok()
                    .content_type("text/html")
                    .body("<html><head><title>Admin</title></head><body><h1>Admin</h1></body></html>")
            }))
            .route("/secret", web::get().to(|| async {
                HttpResponse::Ok()
                    .content_type("text/html")
                    .body("<html><head><title>Secret</title></head><body><h1>Secret</h1></body></html>")
            }))
            .route("/private/file", web::get().to(|| async {
                HttpResponse::Ok()
                    .content_type("text/html")
                    .body("<html><head><title>Private</title></head><body><h1>Private</h1></body></html>")
            }))
            .route("/private/public", web::get().to(|| async {
                HttpResponse::Ok()
                    .content_type("text/html")
                    .body("<html><head><title>Public</title></head><body><h1>Public</h1></body></html>")
            }))
            .route("/allowed", web::get().to(|| async {
                HttpResponse::Ok()
                    .content_type("text/html")
                    .body("<html><head><title>Allowed</title></head><body><h1>Allowed</h1></body></html>")
            }))
    })
    .bind(("127.0.0.1", 0))
    .expect("Failed to bind robots test server");

    let addr = server.addrs().first().cloned().expect("No address bound");
    let url = format!("http://{}", addr);

    let app_server = server.run();
    tokio::spawn(async move {
        if let Err(e) = app_server.await {
            eprintln!("Robots test server error: {}", e);
        }
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    url
}

#[tokio::test]
async fn test_robots_txt_respected() {
    let base_url = start_robots_test_server().await;

    // Create crawler with robots.txt respect enabled
    let config = CrawlerConfig {
        max_depth: 2,
        max_pages: 50,
        follow_external: false,
        keep_fragments: false,
        requests_per_second: None,
        concurrent_requests: 1,
        respect_robots_txt: true,
    };
    let mut crawler = Crawler::new(&base_url, config).expect("Failed to create crawler");

    crawler.crawl().await.expect("Crawl failed");

    // Should NOT crawl /admin (disallowed by *)
    let admin_url = format!("{}/admin", base_url);
    assert!(
        !crawler.pages.contains_key(&admin_url),
        "Should not crawl /admin (disallowed by robots.txt)"
    );

    // Should NOT crawl /secret (disallowed by wildcard)
    let secret_url = format!("{}/secret", base_url);
    assert!(
        !crawler.pages.contains_key(&secret_url),
        "Should not crawl /secret (disallowed by robots.txt)"
    );

    // Should NOT crawl /private/file (disallowed by /private/)
    let private_url = format!("{}/private/file", base_url);
    assert!(
        !crawler.pages.contains_key(&private_url),
        "Should not crawl /private/file (disallowed by /private/)"
    );

    // Should crawl /private/public (explicitly allowed)
    let public_in_private_url = format!("{}/private/public", base_url);
    assert!(
        crawler.pages.contains_key(&public_in_private_url),
        "Should crawl /private/public (explicitly allowed)"
    );

    // Should crawl /allowed (not restricted)
    let allowed_url = format!("{}/allowed", base_url);
    assert!(
        crawler.pages.contains_key(&allowed_url),
        "Should crawl /allowed (not restricted)"
    );

    // Should crawl root page
    assert!(
        crawler.pages.contains_key(&base_url),
        "Should crawl root page"
    );
}

#[tokio::test]
async fn test_robots_txt_disabled() {
    let base_url = start_robots_test_server().await;

    // Create crawler with robots.txt respect disabled
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

    // Should crawl /admin (robots.txt disabled)
    let admin_url = format!("{}/admin", base_url);
    assert!(
        crawler.pages.contains_key(&admin_url),
        "Should crawl /admin when robots.txt is disabled"
    );

    // Should crawl /secret (robots.txt disabled)
    let secret_url = format!("{}/secret", base_url);
    assert!(
        crawler.pages.contains_key(&secret_url),
        "Should crawl /secret when robots.txt is disabled"
    );
}

#[tokio::test]
async fn test_robots_txt_not_found() {
    // Create a server without robots.txt
    let server = HttpServer::new(|| {
        App::new().route(
            "/",
            web::get().to(|| async {
                HttpResponse::Ok()
                    .content_type("text/html")
                    .body("<html><head><title>Home</title></head><body><h1>Home</h1></body></html>")
            }),
        )
    })
    .bind(("127.0.0.1", 0))
    .expect("Failed to bind test server");

    let addr = server.addrs().first().cloned().expect("No address bound");
    let base_url = format!("http://{}", addr);

    let app_server = server.run();
    tokio::spawn(async move {
        if let Err(e) = app_server.await {
            eprintln!("Test server error: {}", e);
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Create crawler with robots.txt respect enabled
    let config = CrawlerConfig {
        max_depth: 1,
        max_pages: 10,
        follow_external: false,
        keep_fragments: false,
        requests_per_second: None,
        concurrent_requests: 1,
        respect_robots_txt: true,
    };
    let mut crawler = Crawler::new(&base_url, config).expect("Failed to create crawler");

    // Should succeed even though robots.txt doesn't exist
    crawler.crawl().await.expect("Crawl should succeed");

    // Should crawl the root page (404 on robots.txt means allow all)
    assert!(
        crawler.pages.contains_key(&base_url),
        "Should crawl root page when robots.txt returns 404"
    );
}

#[tokio::test]
async fn test_robots_txt_server_error() {
    // Create a server that returns 500 for robots.txt
    let server = HttpServer::new(|| {
        App::new()
            .route(
                "/robots.txt",
                web::get()
                    .to(|| async { HttpResponse::InternalServerError().body("Server Error") }),
            )
            .route(
                "/",
                web::get().to(|| async {
                    HttpResponse::Ok().content_type("text/html").body(
                        "<html><head><title>Home</title></head><body><h1>Home</h1></body></html>",
                    )
                }),
            )
    })
    .bind(("127.0.0.1", 0))
    .expect("Failed to bind test server");

    let addr = server.addrs().first().cloned().expect("No address bound");
    let base_url = format!("http://{}", addr);

    let app_server = server.run();
    tokio::spawn(async move {
        if let Err(e) = app_server.await {
            eprintln!("Test server error: {}", e);
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Create crawler with robots.txt respect enabled
    let config = CrawlerConfig {
        max_depth: 1,
        max_pages: 10,
        follow_external: false,
        keep_fragments: false,
        requests_per_second: None,
        concurrent_requests: 1,
        respect_robots_txt: true,
    };
    let mut crawler = Crawler::new(&base_url, config).expect("Failed to create crawler");

    // Should succeed even though robots.txt returns 500
    crawler.crawl().await.expect("Crawl should succeed");

    // Should crawl the root page (500 on robots.txt means allow all)
    assert!(
        crawler.pages.contains_key(&base_url),
        "Should crawl root page when robots.txt returns 500"
    );
}

#[tokio::test]
async fn test_robots_txt_cache() {
    use scoutly::http_client::build_http_client;
    use scoutly::robots::RobotsTxt;

    let base_url = start_robots_test_server().await;
    let parsed_url = url::Url::parse(&base_url).expect("Failed to parse URL");

    let client = build_http_client(30).expect("Failed to build client");
    let mut robots = RobotsTxt::new();

    // First fetch - should fetch from server
    robots
        .fetch(&client, &parsed_url)
        .await
        .expect("First fetch failed");

    // Verify rules were loaded
    let test_url = url::Url::parse(&format!("{}/admin", base_url)).unwrap();
    assert!(!robots.is_allowed(&test_url, "scoutly"));

    // Second fetch - should use cache
    robots
        .fetch(&client, &parsed_url)
        .await
        .expect("Second fetch failed");

    // Verify rules are still working (cache was used)
    assert!(!robots.is_allowed(&test_url, "scoutly"));
}

#[tokio::test]
async fn test_robots_txt_connection_failure() {
    use scoutly::http_client::build_http_client;
    use scoutly::robots::RobotsTxt;

    // Use a URL that will fail to connect (port unlikely to be in use)
    let bad_url = "http://localhost:65535";
    let parsed_url = url::Url::parse(bad_url).expect("Failed to parse URL");

    let client = build_http_client(1).expect("Failed to build client");
    let mut robots = RobotsTxt::new();

    // Fetch should succeed despite connection failure
    // (it treats connection errors as "no robots.txt, allow all")
    let result = robots.fetch(&client, &parsed_url).await;
    assert!(
        result.is_ok(),
        "Fetch should succeed even when connection fails"
    );

    // Should allow all URLs when robots.txt fetch fails
    let test_url = url::Url::parse(&format!("{}/admin", bad_url)).unwrap();
    assert!(
        robots.is_allowed(&test_url, "scoutly"),
        "Should allow all URLs when robots.txt cannot be fetched"
    );
}
