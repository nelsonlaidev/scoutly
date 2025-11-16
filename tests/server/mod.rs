use actix_files::Files;
use actix_web::{App, HttpResponse, HttpServer, web};
use std::sync::Once;

#[allow(dead_code)]
static INIT: Once = Once::new();

#[allow(dead_code)]
pub async fn get_test_server_url() -> String {
    let http_server = HttpServer::new(|| {
        App::new().service(
            Files::new("/", "tests/static/")
                .index_file("index.html")
                .show_files_listing(),
        )
    })
    .bind(("127.0.0.1", 0))
    .expect("Failed to bind test server");

    let addr = http_server
        .addrs()
        .first()
        .cloned()
        .expect("No address bound");
    let url = format!("http://{}", addr);

    let app_server = http_server.run();

    tokio::spawn(async move {
        if let Err(e) = app_server.await {
            eprintln!("Test server error: {}", e);
        }
    });

    url
}

#[allow(dead_code)]
pub async fn start_link_test_server() {
    INIT.call_once(|| {
        tokio::spawn(async {
            let server = HttpServer::new(|| {
                App::new()
                    .route(
                        "/ok",
                        web::get().to(|| async { HttpResponse::Ok().body("OK") }),
                    )
                    .route(
                        "/not-found",
                        web::get().to(|| async { HttpResponse::NotFound().body("Not Found") }),
                    )
                    .route(
                        "/redirect",
                        web::get().to(|| async {
                            HttpResponse::MovedPermanently()
                                .append_header(("Location", "http://127.0.0.1:3000/ok"))
                                .finish()
                        }),
                    )
                    .route(
                        "/redirect-temp",
                        web::get().to(|| async {
                            HttpResponse::Found()
                                .append_header(("Location", "http://127.0.0.1:3000/ok"))
                                .finish()
                        }),
                    )
                    .route(
                        "/server-error",
                        web::get()
                            .to(|| async { HttpResponse::InternalServerError().body("Error") }),
                    )
                    .route(
                        "/json-response",
                        web::get().to(|| async {
                            HttpResponse::Ok()
                                .content_type("application/json")
                                .body(r#"{"message": "This is JSON"}"#)
                        }),
                    )
                    .route(
                        "/image-response",
                        web::get().to(|| async {
                            HttpResponse::Ok()
                                .content_type("image/png")
                                .body(vec![0u8; 100]) // Fake image data
                        }),
                    )
                    .route(
                        "/pdf-response",
                        web::get().to(|| async {
                            HttpResponse::Ok()
                                .content_type("application/pdf")
                                .body(vec![0u8; 100]) // Fake PDF data
                        }),
                    )
            })
            .bind(("127.0.0.1", 3000))
            .expect("Failed to bind link test server on port 3000");

            if let Err(e) = server.run().await {
                eprintln!("Link test server error: {}", e);
            }
        });

        // Give the server time to start
        std::thread::sleep(std::time::Duration::from_millis(100));
    });

    // Additional wait to ensure server is ready (especially for slower machines)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify server is responding by attempting a connection
    for _ in 0..10 {
        if reqwest::get("http://127.0.0.1:3000/ok").await.is_ok() {
            return;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
}
