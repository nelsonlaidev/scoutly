use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
struct RequestTrace {
    method: String,
    target: String,
    user_agent: String,
    accept_encoding: String,
}

pub(crate) struct TestServer {
    origin: String,
    trace: Arc<Mutex<Vec<RequestTrace>>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl TestServer {
    pub(crate) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("read test server address");
        let origin = format!("http://{address}");
        let trace = Arc::new(Mutex::new(Vec::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_trace = Arc::clone(&trace);
        let worker_stopped = Arc::clone(&stopped);
        let worker_origin = origin.clone();

        let worker = thread::spawn(move || {
            while !worker_stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = handle_connection(stream, &worker_origin, &worker_trace);
                    }
                    Err(_error) if worker_stopped.load(Ordering::Acquire) => break,
                    Err(_) => break,
                }
            }
        });

        Self {
            origin,
            trace,
            stopped,
            worker: Some(worker),
        }
    }

    pub(crate) fn origin(&self) -> &str {
        &self.origin
    }

    pub(crate) fn trace_json(&self) -> String {
        let trace = self.trace.lock().expect("lock request trace");

        format!(
            "{}\n",
            serde_json::to_string_pretty(&*trace).expect("serialize request trace")
        )
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);

        let _ = TcpStream::connect(
            self.origin
                .strip_prefix("http://")
                .expect("test server origin has HTTP scheme"),
        );

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    origin: &str,
    trace: &Arc<Mutex<Vec<RequestTrace>>>,
) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    let mut header_complete = false;

    while !header_complete {
        let previous_length = request.len();
        let read = stream.read(&mut chunk)?;

        if read == 0 {
            return Ok(());
        }

        request.extend_from_slice(&chunk[..read]);

        if request.len() > 32 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "test request headers exceed 32 KiB",
            ));
        }

        header_complete = request[previous_length.saturating_sub(3)..]
            .windows(4)
            .any(|window| window == b"\r\n\r\n");
    }

    let request = String::from_utf8(request)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut lines = request.split("\r\n");
    let request_line = lines.next().expect("test request line");
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_owned();
    let target = request_parts.next().unwrap_or_default().to_owned();
    let mut user_agent = String::new();
    let mut accept_encoding = String::new();

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };

        match name.trim().to_ascii_lowercase().as_str() {
            "user-agent" => user_agent = value.trim().to_owned(),
            "accept-encoding" => accept_encoding = value.trim().to_owned(),
            _ => {}
        }
    }

    trace
        .lock()
        .expect("lock request trace")
        .push(RequestTrace {
            method,
            target: target.clone(),
            user_agent,
            accept_encoding,
        });

    let (status, content_type, body) = fixture_response(origin, &target);
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    stream.write_all(response.as_bytes())?;

    Ok(())
}

fn fixture_response(origin: &str, target: &str) -> (&'static str, &'static str, String) {
    match target {
        "/robots.txt" => (
            "200 OK",
            "text/plain",
            format!("User-agent: *\nDisallow: /private\nSitemap: {origin}/sitemap.xml\n"),
        ),
        "/" => (
            "200 OK",
            "text/html; charset=utf-8",
            format!(
                r#"<!doctype html><html><head><title>Scoutly test fixture home</title><meta name="description" content="A deterministic local page used to verify Scoutly report behavior."><meta property="og:title" content="Scoutly fixture"><meta property="og:description" content="Test fixture"><meta property="og:image" content="{origin}/image.png"><meta property="og:url" content="{origin}/"><meta property="og:type" content="website"></head><body><h1>Test fixture</h1><a href="/about">About</a><a href="/broken#details">Broken</a><a href="/private">Private</a><img src="/image.png" alt="Fixture logo"></body></html>"#
            ),
        ),
        "/about" => (
            "200 OK",
            "text/html",
            "<!doctype html><html><body><p>About the fixture.</p></body></html>".to_owned(),
        ),
        "/broken" => (
            "404 Not Found",
            "text/html",
            "<!doctype html><html><body>Missing</body></html>".to_owned(),
        ),
        "/private" => (
            "200 OK",
            "text/html",
            "<!doctype html><html><head><title>Private</title></head><body><h1>Private</h1></body></html>".to_owned(),
        ),
        "/sitemap.xml" => (
            "200 OK",
            "application/xml",
            format!(
                "<?xml version=\"1.0\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"><url><loc>{origin}/sitemap-only</loc></url></urlset>"
            ),
        ),
        "/sitemap-only" => (
            "200 OK",
            "text/html",
            "<!doctype html><html><head><title>Sitemap fixture page</title><meta name=\"description\" content=\"A deterministic sitemap-only page used by the Scoutly test fixture.\"></head><body><h1>Sitemap page</h1></body></html>".to_owned(),
        ),
        "/image.png" => ("200 OK", "image/png", "not-a-real-png".to_owned()),
        _ => ("404 Not Found", "text/plain", "not found".to_owned()),
    }
}
