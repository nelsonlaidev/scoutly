mod server;

use scoutly::crawler::Crawler;
use actix_web::{App, HttpServer, web, HttpResponse};

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
    let mut crawler = Crawler::new(&base_url, 2, 50, false, false, None, 1, true)
        .expect("Failed to create crawler");

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
    let mut crawler = Crawler::new(&base_url, 2, 50, false, false, None, 1, false)
        .expect("Failed to create crawler");

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
