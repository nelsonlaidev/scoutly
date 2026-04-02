use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use std::fs;
use std::path::PathBuf;
use std::sync::{Once, OnceLock};

#[allow(dead_code)]
static INIT: Once = Once::new();
static FIXTURE_INIT: Once = Once::new();
static LINK_TEST_SERVER_BASE_URL: OnceLock<String> = OnceLock::new();
static FIXTURE_TEST_SERVER_BASE_URL: OnceLock<String> = OnceLock::new();

const LINK_TEST_SERVER_HOST: &str = "127.0.0.1";

#[allow(dead_code)]
pub fn link_test_server_url() -> &'static str {
    LINK_TEST_SERVER_BASE_URL
        .get()
        .expect("Link test server should be started before use")
}

async fn serve_static_fixture(
    request: HttpRequest,
    link_server_url: web::Data<String>,
) -> HttpResponse {
    let relative_path = match request.path() {
        "/" => "index.html",
        path => path.trim_start_matches('/'),
    };

    let file_path = PathBuf::from("tests/static").join(relative_path);

    match fs::read_to_string(&file_path) {
        Ok(contents) => {
            let html = contents.replace("http://127.0.0.1:3000", link_server_url.get_ref());
            HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .body(html)
        }
        Err(_) => HttpResponse::NotFound().body("Not Found"),
    }
}

#[allow(dead_code)]
pub async fn get_test_server_url() -> String {
    let link_server_url = start_link_test_server().await;

    FIXTURE_INIT.call_once(|| {
        let listener = std::net::TcpListener::bind((LINK_TEST_SERVER_HOST, 0))
            .expect("Failed to bind test server");
        let base_url = format!(
            "http://{}",
            listener
                .local_addr()
                .expect("Fixture test server should have a bound address")
        );
        FIXTURE_TEST_SERVER_BASE_URL
            .set(base_url.clone())
            .expect("Fixture test server URL should only be initialized once");

        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(link_server_url.clone()))
                .default_service(web::to(serve_static_fixture))
        })
        .listen(listener)
        .expect("Failed to start test server")
        .run();

        tokio::spawn(async move {
            if let Err(error) = server.await {
                eprintln!("Test server error: {error}");
            }
        });
    });

    let base_url = FIXTURE_TEST_SERVER_BASE_URL
        .get()
        .expect("Fixture test server should be started before use")
        .to_string();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    for _ in 0..20 {
        match reqwest::get(&base_url).await {
            Ok(response) if response.status().is_success() => return base_url,
            _ => tokio::time::sleep(tokio::time::Duration::from_millis(50)).await,
        }
    }

    panic!(
        "Fixture test server at {} failed to start after 1 second",
        base_url
    );
}

#[allow(dead_code)]
pub async fn start_link_test_server() -> String {
    INIT.call_once(|| {
        let listener = std::net::TcpListener::bind((LINK_TEST_SERVER_HOST, 0))
            .expect("Failed to bind link test server");
        let base_url = format!(
            "http://{}",
            listener
                .local_addr()
                .expect("Link test server should have a bound address")
        );
        LINK_TEST_SERVER_BASE_URL
            .set(base_url.clone())
            .expect("Link test server URL should only be initialized once");

        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(base_url.clone()))
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
                    web::get().to(|base_url: web::Data<String>| async move {
                        HttpResponse::MovedPermanently()
                            .append_header(("Location", format!("{}/ok", base_url.get_ref())))
                            .finish()
                    }),
                )
                .route(
                    "/redirect-temp",
                    web::get().to(|base_url: web::Data<String>| async move {
                        HttpResponse::Found()
                            .append_header(("Location", format!("{}/ok", base_url.get_ref())))
                            .finish()
                    }),
                )
                .route(
                    "/server-error",
                    web::get().to(|| async { HttpResponse::InternalServerError().body("Error") }),
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
                            .body(vec![0u8; 100])
                    }),
                )
                .route(
                    "/pdf-response",
                    web::get().to(|| async {
                        HttpResponse::Ok()
                            .content_type("application/pdf")
                            .body(vec![0u8; 100])
                    }),
                )
        })
        .listen(listener)
        .expect("Failed to start link test server")
        .run();

        tokio::spawn(async move {
            if let Err(error) = server.await {
                eprintln!("Link test server error: {error}");
            }
        });
    });

    let base_url = link_test_server_url().to_string();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    for _ in 0..20 {
        match reqwest::get(format!("{}/ok", base_url)).await {
            Ok(response) if response.status().is_success() => {
                return base_url;
            }
            _ => {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        }
    }

    panic!(
        "Link test server at {} failed to start after 1 second",
        base_url
    );
}
