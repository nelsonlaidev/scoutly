use actix_web::{App, HttpResponse, HttpServer, web};
use scoutly::models::{Image, Link, OpenGraphTags, PageInfo, SeoIssue};
use scoutly::sitemap::collect_sitemap_entries;
use std::collections::HashMap;

fn page(url: &str, title: Option<&str>) -> PageInfo {
    PageInfo {
        url: url.to_string(),
        status_code: Some(200),
        content_type: Some("text/html".to_string()),
        title: title.map(str::to_string),
        meta_description: None,
        h1_tags: vec![],
        links: Vec::<Link>::new(),
        images: Vec::<Image>::new(),
        open_graph: OpenGraphTags::default(),
        issues: Vec::<SeoIssue>::new(),
        crawl_depth: 0,
    }
}

async fn start_sitemap_test_server(
    include_robots_sitemap: bool,
    use_sitemap_index: bool,
) -> String {
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("Failed to bind sitemap test server");
    let base_url = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("Sitemap test server should have an address")
    );

    let server = HttpServer::new({
        let base_url = base_url.clone();
        move || {
            let base_url = base_url.clone();
            App::new()
                .app_data(web::Data::new(base_url))
                .route(
                    "/robots.txt",
                    web::get().to(move |base_url: web::Data<String>| async move {
                        let body = if include_robots_sitemap {
                            if use_sitemap_index {
                                format!(
                                    "User-agent: *\nSitemap: {}/index.xml\n",
                                    base_url.get_ref()
                                )
                            } else {
                                format!(
                                    "User-agent: *\nSitemap: {}/sitemap.xml\n",
                                    base_url.get_ref()
                                )
                            }
                        } else {
                            "User-agent: *\nDisallow:\n".to_string()
                        };

                        HttpResponse::Ok().content_type("text/plain").body(body)
                    }),
                )
                .route(
                    "/sitemap.xml",
                    web::get().to(|base_url: web::Data<String>| async move {
                        HttpResponse::Ok()
                            .content_type("application/xml")
                            .body(format!(
                                r#"<?xml version="1.0" encoding="UTF-8"?>
                            <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
                              <url>
                                <loc>{}/</loc>
                                <priority>1.0</priority>
                                <changefreq>daily</changefreq>
                              </url>
                              <url>
                                <loc>{}/about</loc>
                                <priority>0.8</priority>
                                <changefreq>weekly</changefreq>
                              </url>
                            </urlset>"#,
                                base_url.get_ref(),
                                base_url.get_ref()
                            ))
                    }),
                )
                .route(
                    "/index.xml",
                    web::get().to(|base_url: web::Data<String>| async move {
                        HttpResponse::Ok()
                            .content_type("application/xml")
                            .body(format!(
                                r#"<?xml version="1.0" encoding="UTF-8"?>
                            <sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
                              <sitemap>
                                <loc>{}/nested.xml</loc>
                              </sitemap>
                            </sitemapindex>"#,
                                base_url.get_ref()
                            ))
                    }),
                )
                .route(
                    "/nested.xml",
                    web::get().to(|base_url: web::Data<String>| async move {
                        HttpResponse::Ok()
                            .content_type("application/xml")
                            .body(format!(
                                r#"<?xml version="1.0" encoding="UTF-8"?>
                            <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
                              <url>
                                <loc>{}/blog</loc>
                                <changefreq>monthly</changefreq>
                              </url>
                            </urlset>"#,
                                base_url.get_ref()
                            ))
                    }),
                )
                .route(
                    "/",
                    web::get().to(|| async {
                        HttpResponse::Ok()
                            .content_type("text/html")
                            .body("<html><head><title>Home</title></head><body>Home</body></html>")
                    }),
                )
                .route(
                    "/about",
                    web::get().to(|| async {
                        HttpResponse::Ok().content_type("text/html").body(
                            "<html><head><title>About</title></head><body>About</body></html>",
                        )
                    }),
                )
                .route(
                    "/blog",
                    web::get().to(|| async {
                        HttpResponse::Ok()
                            .content_type("text/html")
                            .body("<html><head><title>Blog</title></head><body>Blog</body></html>")
                    }),
                )
        }
    })
    .workers(1)
    .listen(listener)
    .expect("Failed to start sitemap test server")
    .run();

    tokio::spawn(async move {
        if let Err(error) = server.await {
            eprintln!("Sitemap test server error: {error}");
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    base_url
}

async fn start_duplicate_sitemap_test_server() -> String {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("Failed to bind duplicate sitemap server");
    let base_url = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("Duplicate sitemap server should have an address")
    );

    let server = HttpServer::new({
        let base_url = base_url.clone();
        move || {
            let base_url = base_url.clone();
            App::new()
                .app_data(web::Data::new(base_url))
                .route(
                    "/robots.txt",
                    web::get().to(|base_url: web::Data<String>| async move {
                        HttpResponse::Ok().content_type("text/plain").body(format!(
                            "User-agent: *\nSitemap: {}/index.xml\n",
                            base_url.get_ref()
                        ))
                    }),
                )
                .route(
                    "/index.xml",
                    web::get().to(|base_url: web::Data<String>| async move {
                        HttpResponse::Ok()
                            .content_type("application/xml")
                            .body(format!(
                                r#"<?xml version="1.0" encoding="UTF-8"?>
                            <sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
                              <sitemap><loc>{}/nested.xml</loc></sitemap>
                              <sitemap><loc>{}/nested.xml</loc></sitemap>
                            </sitemapindex>"#,
                                base_url.get_ref(),
                                base_url.get_ref()
                            ))
                    }),
                )
                .route(
                    "/nested.xml",
                    web::get().to(|base_url: web::Data<String>| async move {
                        HttpResponse::Ok()
                            .content_type("application/xml")
                            .body(format!(
                                r#"<?xml version="1.0" encoding="UTF-8"?>
                            <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
                              <url><loc>{}/about</loc></url>
                              <url><loc>{}/about</loc></url>
                            </urlset>"#,
                                base_url.get_ref(),
                                base_url.get_ref()
                            ))
                    }),
                )
        }
    })
    .workers(1)
    .listen(listener)
    .expect("Failed to start duplicate sitemap server")
    .run();

    tokio::spawn(async move {
        if let Err(error) = server.await {
            eprintln!("Duplicate sitemap server error: {error}");
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    base_url
}

#[tokio::test]
#[serial_test::serial]
async fn collect_sitemap_entries_uses_robots_sitemap_and_joins_titles() {
    let base_url = start_sitemap_test_server(true, false).await;
    let pages = HashMap::from([
        (base_url.clone(), page(&base_url, Some("Home"))),
        (
            format!("{}/about", base_url),
            page(&format!("{}/about", base_url), Some("About")),
        ),
    ]);

    let entries = collect_sitemap_entries(&base_url, &pages, false)
        .await
        .expect("collect sitemap entries should succeed");

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].url, format!("{}/", base_url));
    assert_eq!(entries[0].title, "Home");
    assert_eq!(entries[0].priority.as_deref(), Some("1.0"));
    assert_eq!(entries[0].change_frequency.as_deref(), Some("daily"));
    assert_eq!(entries[1].url, format!("{}/about", base_url));
    assert_eq!(entries[1].title, "About");
}

#[tokio::test]
#[serial_test::serial]
async fn collect_sitemap_entries_follows_sitemap_indexes() {
    let base_url = start_sitemap_test_server(true, true).await;
    let pages = HashMap::from([(
        format!("{}/blog", base_url),
        page(&format!("{}/blog", base_url), Some("Blog")),
    )]);

    let entries = collect_sitemap_entries(&base_url, &pages, false)
        .await
        .expect("collect sitemap entries should succeed");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, format!("{}/blog", base_url));
    assert_eq!(entries[0].title, "Blog");
    assert_eq!(entries[0].change_frequency.as_deref(), Some("monthly"));
}

#[tokio::test]
#[serial_test::serial]
async fn collect_sitemap_entries_falls_back_to_default_sitemap_url() {
    let base_url = start_sitemap_test_server(false, false).await;
    let pages = HashMap::from([(
        format!("{}/about", base_url),
        page(&format!("{}/about", base_url), Some("About")),
    )]);

    let entries = collect_sitemap_entries(&base_url, &pages, false)
        .await
        .expect("collect sitemap entries should succeed");

    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .any(|entry| entry.url == format!("{}/about", base_url))
    );
}

#[tokio::test]
#[serial_test::serial]
async fn collect_sitemap_entries_tolerates_robots_fetch_failures() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind temp listener");
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let base_url = format!("http://{addr}");
    let entries = collect_sitemap_entries(&base_url, &HashMap::new(), false)
        .await
        .expect("robots fetch failures should not abort sitemap discovery");

    assert!(entries.is_empty());
}

#[tokio::test]
#[serial_test::serial]
async fn collect_sitemap_entries_skips_duplicate_sitemaps_and_urls() {
    let base_url = start_duplicate_sitemap_test_server().await;
    let pages = HashMap::from([(
        format!("{}/about", base_url),
        page(&format!("{}/about", base_url), Some("About")),
    )]);

    let entries = collect_sitemap_entries(&base_url, &pages, false)
        .await
        .expect("duplicate sitemap entries should be deduplicated");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, format!("{}/about", base_url));
    assert_eq!(entries[0].title, "About");
}
