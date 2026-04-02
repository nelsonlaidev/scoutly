mod server;

use actix_web::{App, HttpResponse, HttpServer, web};
use scoutly::update::check_for_update_with_endpoint;
use server::{get_test_server_url, start_link_test_server};
use std::net::TcpListener;
use std::process::Command;
use std::time::Duration;

async fn start_update_server() -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind update test server");
    let base_url = format!("http://{}", listener.local_addr().unwrap());

    let server = HttpServer::new(|| {
        App::new()
            .route(
                "/latest",
                web::get().to(|| async {
                    HttpResponse::Ok().json(serde_json::json!({
                        "tag_name": "v0.4.0",
                        "html_url": "https://github.com/nelsonlaidev/scoutly/releases/tag/v0.4.0"
                    }))
                }),
            )
            .route(
                "/missing-fields",
                web::get().to(|| async {
                    HttpResponse::Ok().json(serde_json::json!({
                        "tag_name": "v0.4.0"
                    }))
                }),
            )
            .route(
                "/malformed-json",
                web::get().to(|| async {
                    HttpResponse::Ok()
                        .content_type("application/json")
                        .body("{invalid-json")
                }),
            )
            .route(
                "/slow",
                web::get().to(|| async {
                    tokio::time::sleep(Duration::from_millis(750)).await;
                    HttpResponse::Ok().json(serde_json::json!({
                        "tag_name": "v0.4.0",
                        "html_url": "https://github.com/nelsonlaidev/scoutly/releases/tag/v0.4.0"
                    }))
                }),
            )
    })
    .listen(listener)
    .expect("listen update test server")
    .run();

    tokio::spawn(async move {
        let _ = server.await;
    });

    for _ in 0..20 {
        match reqwest::get(format!("{base_url}/latest")).await {
            Ok(response) if response.status().is_success() => return base_url,
            _ => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }

    panic!("Update test server at {base_url} failed to start");
}

#[tokio::test]
#[serial_test::serial]
async fn direct_update_check_returns_notice_for_newer_release() {
    let base_url = start_update_server().await;

    let notice = check_for_update_with_endpoint("0.3.0", &format!("{base_url}/latest")).await;

    assert_eq!(
        notice,
        Some(scoutly::update::UpdateNotice {
            latest_version: "0.4.0".to_string(),
            release_url: "https://github.com/nelsonlaidev/scoutly/releases/tag/v0.4.0".to_string(),
        })
    );
}

#[tokio::test]
#[serial_test::serial]
async fn direct_update_check_ignores_malformed_json() {
    let base_url = start_update_server().await;

    let notice =
        check_for_update_with_endpoint("0.3.0", &format!("{base_url}/malformed-json")).await;

    assert!(notice.is_none());
}

#[tokio::test]
#[serial_test::serial]
async fn direct_update_check_ignores_missing_release_fields() {
    let base_url = start_update_server().await;

    let notice =
        check_for_update_with_endpoint("0.3.0", &format!("{base_url}/missing-fields")).await;

    assert!(notice.is_none());
}

#[tokio::test]
#[serial_test::serial]
async fn direct_update_check_ignores_http_failures() {
    let notice = check_for_update_with_endpoint("0.3.0", "http://127.0.0.1:9/latest").await;

    assert!(notice.is_none());
}

#[tokio::test]
#[serial_test::serial]
async fn binary_json_output_stays_valid_when_update_is_available() {
    start_link_test_server().await;
    let crawl_url = get_test_server_url().await;
    let update_url = format!("{}/latest", start_update_server().await);

    let output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_scoutly"))
            .env("SCOUTLY_UPDATE_API_URL", update_url)
            .args([
                crawl_url.as_str(),
                "--depth",
                "1",
                "--max-pages",
                "1",
                "--output",
                "json",
            ])
            .output()
            .expect("run binary with JSON output")
    })
    .await
    .expect("binary task should complete");

    assert!(output.status.success(), "JSON mode should still succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<serde_json::Value>(&stdout).expect("stdout should stay valid JSON");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Update available:"));
    assert!(stderr.contains("v0.4.0"));
}

#[tokio::test]
#[serial_test::serial]
async fn binary_text_mode_surfaces_update_notice() {
    start_link_test_server().await;
    let crawl_url = get_test_server_url().await;
    let update_url = format!("{}/latest", start_update_server().await);

    let output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_scoutly"))
            .env("SCOUTLY_UPDATE_API_URL", update_url)
            .args([
                crawl_url.as_str(),
                "--cli",
                "--depth",
                "1",
                "--max-pages",
                "1",
            ])
            .output()
            .expect("run binary with text output")
    })
    .await
    .expect("binary task should complete");

    assert!(output.status.success(), "text mode should still succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Update available:"));
    assert!(stdout.contains("Scoutly - Crawl Report"));
}

#[tokio::test]
#[serial_test::serial]
async fn binary_startup_continues_when_update_check_times_out() {
    start_link_test_server().await;
    let crawl_url = get_test_server_url().await;
    let update_url = format!("{}/slow", start_update_server().await);

    let output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_scoutly"))
            .env("SCOUTLY_UPDATE_API_URL", update_url)
            .args([
                crawl_url.as_str(),
                "--depth",
                "1",
                "--max-pages",
                "1",
                "--output",
                "json",
            ])
            .output()
            .expect("run binary with slow update server")
    })
    .await
    .expect("binary task should complete");

    assert!(
        output.status.success(),
        "startup should continue on timeout"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<serde_json::Value>(&stdout).expect("stdout should stay valid JSON");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("Update available:"));
}
