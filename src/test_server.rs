use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use url::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TestRequest {
    pub(crate) target: String,
    headers: Vec<(String, String)>,
}

impl TestRequest {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TestResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    header_delay: Duration,
    body_delay: Duration,
    body_chunk: Option<(usize, Duration)>,
}

impl TestResponse {
    pub(crate) fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
            header_delay: Duration::ZERO,
            body_delay: Duration::ZERO,
            body_chunk: None,
        }
    }

    pub(crate) fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));

        self
    }

    pub(crate) fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();

        self
    }

    pub(crate) fn header_delay(mut self, delay: Duration) -> Self {
        self.header_delay = delay;

        self
    }

    pub(crate) fn body_delay(mut self, delay: Duration) -> Self {
        self.body_delay = delay;

        self
    }

    pub(crate) fn body_chunks(mut self, size: usize, delay: Duration) -> Self {
        assert!(size > 0, "test response chunk size must be positive");

        self.body_chunk = Some((size, delay));

        self
    }
}

pub(crate) struct TestServer {
    address: SocketAddr,
    origin: String,
    requests: Arc<Mutex<Vec<TestRequest>>>,
    active_connections: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl TestServer {
    pub(crate) fn start(
        handler: impl Fn(TestRequest) -> TestResponse + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let origin = format!("http://{address}");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let active_connections = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_requests = Arc::clone(&requests);
        let worker_active_connections = Arc::clone(&active_connections);
        let worker_stopped = Arc::clone(&stopped);
        let handler = Arc::new(handler);

        let worker = thread::spawn(move || {
            while !worker_stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if worker_stopped.load(Ordering::Acquire) {
                            break;
                        }

                        let connection_requests = Arc::clone(&worker_requests);
                        let connection_active_connections = Arc::clone(&worker_active_connections);
                        let connection_handler = Arc::clone(&handler);
                        connection_active_connections.fetch_add(1, Ordering::SeqCst);

                        thread::spawn(move || {
                            let _active = ActiveConnection(connection_active_connections);
                            serve_connection(stream, connection_requests, connection_handler);
                        });
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::Interrupted
                                | std::io::ErrorKind::ConnectionAborted
                                | std::io::ErrorKind::ConnectionReset
                        ) => {}
                    Err(error) => {
                        eprintln!(
                            "test server listener stopped after accept error {:?}: {error}",
                            error.kind()
                        );
                        break;
                    }
                }
            }
        });

        Self {
            address,
            origin,
            requests,
            active_connections,
            stopped,
            worker: Some(worker),
        }
    }

    pub(crate) fn url(&self, path: &str) -> Url {
        Url::parse(&format!("{}{path}", self.origin)).unwrap()
    }

    pub(crate) fn requests(&self) -> Vec<TestRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub(crate) fn wait_for_requests(&self, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);

        while self.requests.lock().unwrap().len() < count {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for request {count}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub(crate) fn wait_for_idle(&self) {
        let deadline = Instant::now() + Duration::from_secs(2);

        while self.active_connections.load(Ordering::SeqCst) > 0 {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for test server connections to close"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub(crate) fn unused_url() -> Url {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);

        Url::parse(&format!("http://{address}/")).unwrap()
    }
}

struct ActiveConnection(Arc<AtomicUsize>);

impl Drop for ActiveConnection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve_connection(
    mut stream: TcpStream,
    requests: Arc<Mutex<Vec<TestRequest>>>,
    handler: Arc<impl Fn(TestRequest) -> TestResponse>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let Some(request) = read_request(&mut stream) else {
        return;
    };

    requests.lock().unwrap().push(request.clone());
    let response = handler(request);

    thread::sleep(response.header_delay);
    let reason = reqwest::StatusCode::from_u16(response.status)
        .ok()
        .and_then(|status| status.canonical_reason())
        .unwrap_or("Unknown");
    let has_content_length = response
        .headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-length"));

    let mut head = format!("HTTP/1.1 {} {reason}\r\n", response.status);

    for (name, value) in response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }

    if !has_content_length {
        head.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
    }

    head.push_str("Connection: close\r\n\r\n");

    if stream.write_all(head.as_bytes()).is_err() || stream.flush().is_err() {
        return;
    }

    thread::sleep(response.body_delay);
    if let Some((size, delay)) = response.body_chunk {
        for chunk in response.body.chunks(size) {
            if stream.write_all(chunk).is_err() || stream.flush().is_err() {
                return;
            }
            thread::sleep(delay);
        }
    } else {
        let _ = stream.write_all(&response.body);
    }
}

fn read_request(stream: &mut TcpStream) -> Option<TestRequest> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 1024];
    let mut header_complete = false;

    while bytes.len() < 32 * 1024 && !header_complete {
        let previous_length = bytes.len();
        let count = match stream.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        };

        if count == 0 {
            return None;
        }

        bytes.extend_from_slice(&buffer[..count]);
        let search_from = previous_length.saturating_sub(3);
        header_complete = bytes[search_from..]
            .windows(4)
            .any(|window| window == b"\r\n\r\n");
    }

    if !header_complete {
        return None;
    }

    let source = String::from_utf8_lossy(&bytes);
    let mut lines = source.split("\r\n");
    let target = lines.next()?.split_whitespace().nth(1)?.to_owned();
    let headers = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.to_owned(), value.trim().to_owned()))
        })
        .collect();

    Some(TestRequest { target, headers })
}
