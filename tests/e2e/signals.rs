#[cfg(unix)]
mod unix {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use crate::common::repository_root;
    use crate::{assert_command_succeeded, binary};

    struct BlockingServer {
        origin: String,
        started: Receiver<()>,
        stopped: Arc<AtomicBool>,
        worker: Option<JoinHandle<()>>,
    }

    impl BlockingServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind blocking server");
            let address = listener.local_addr().expect("read blocking server address");
            let origin = format!("http://{address}");
            let (started_sender, started) = mpsc::sync_channel(1);
            let stopped = Arc::new(AtomicBool::new(false));
            let worker_stopped = Arc::clone(&stopped);

            let worker = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept blocking request");

                stream
                    .set_read_timeout(Some(Duration::from_millis(50)))
                    .expect("set blocking request timeout");

                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];

                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) => return,
                        Ok(read) => request.extend_from_slice(&buffer[..read]),
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) && !worker_stopped.load(Ordering::Acquire) => {}
                        Err(_) => return,
                    }
                }

                started_sender.send(()).expect("notify blocking request");

                while !worker_stopped.load(Ordering::Acquire) {
                    match stream.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => break,
                    }
                }
            });

            Self {
                origin,
                started,
                stopped,
                worker: Some(worker),
            }
        }
    }

    impl Drop for BlockingServer {
        fn drop(&mut self) {
            self.stopped.store(true, Ordering::Release);

            let _ = TcpStream::connect(
                self.origin
                    .strip_prefix("http://")
                    .expect("blocking origin has HTTP scheme"),
            );

            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    #[test]
    fn interrupt_exits_130_without_a_partial_report() {
        let server = BlockingServer::start();
        let mut child = Command::new(binary())
            .args([
                "--no-config",
                "--max-depth=0",
                "--max-pages=1",
                "--no-respect-robots",
                "--no-sitemaps",
                "--no-images",
                "--progress=never",
                &server.origin,
            ])
            .current_dir(repository_root())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("start audit for interrupt test");

        server
            .started
            .recv_timeout(Duration::from_secs(5))
            .expect("audit request starts");

        let signal = Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .output()
            .expect("send SIGINT to Rust audit");

        assert_command_succeeded("kill -INT", &signal);

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll interrupted Rust audit") {
                break status;
            }

            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                panic!("audit did not stop after SIGINT");
            }

            thread::sleep(Duration::from_millis(10));
        };

        let mut stdout = Vec::new();

        child
            .stdout
            .take()
            .expect("capture Rust stdout")
            .read_to_end(&mut stdout)
            .unwrap();

        let mut stderr = Vec::new();

        child
            .stderr
            .take()
            .expect("capture Rust stderr")
            .read_to_end(&mut stderr)
            .unwrap();

        assert_eq!(status.code(), Some(130));
        assert!(stdout.is_empty(), "partial report: {stdout:?}");
        assert_eq!(
            String::from_utf8(stderr).unwrap(),
            "scoutly: error: audit canceled\n"
        );
    }
}
