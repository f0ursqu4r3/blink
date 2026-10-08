//! Local-only authentication fixtures. This module is absent from release builds.
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::Duration;

pub const CA: &[u8] = include_bytes!("authentication/fixtures/ca.pem");
pub const CLIENT: &[u8] = include_bytes!("authentication/fixtures/client.pem");
const SERVER: &[u8] = include_bytes!("authentication/fixtures/server.pem");
const PREFIX: &str = "blink-auth-test:";
type Accounts = HashMap<String, Vec<u8>>;
static EVENTS: LazyLock<Mutex<HashMap<String, std::sync::mpsc::Sender<&'static str>>>> =
    LazyLock::new(Mutex::default);
static STORES: LazyLock<Mutex<HashMap<String, Accounts>>> = LazyLock::new(Mutex::default);

pub struct Credentials {
    pub scope: String,
    pub events: std::sync::mpsc::Receiver<&'static str>,
}
impl Credentials {
    pub fn new() -> Self {
        let scope = format!("{PREFIX}{}", rand::random::<u128>());
        assert!(
            STORES
                .lock()
                .unwrap()
                .insert(scope.clone(), HashMap::new())
                .is_none()
        );
        let (sender, events) = std::sync::mpsc::channel();
        EVENTS.lock().unwrap().insert(scope.clone(), sender);
        Self { scope, events }
    }
}
impl Drop for Credentials {
    fn drop(&mut self) {
        STORES.lock().unwrap().remove(&self.scope);
        EVENTS.lock().unwrap().remove(&self.scope);
    }
}
pub fn credential_waiting(scope: Option<&str>, operation: &'static str) {
    if let Some(scope) = scope
        && let Some(sender) = EVENTS.lock().unwrap().get(scope)
    {
        let _ = sender.send(operation);
    }
}

/// A retired test scope must fail closed, even if an async task outlives its guard.
pub fn with_store<T>(
    scope: Option<&str>,
    action: impl FnOnce(&mut Accounts) -> T,
) -> Option<Result<T, String>> {
    let scope = scope.filter(|s| s.starts_with(PREFIX))?;
    let mut stores = STORES.lock().unwrap();
    Some(
        stores
            .get_mut(scope)
            .map(action)
            .ok_or_else(|| "Test credential scope has closed.".into()),
    )
}
pub fn trust_fixture(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    builder.add_root_certificate(reqwest::Certificate::from_pem(CA).unwrap())
}

#[derive(Clone, Debug)]
pub struct Request {
    pub target: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub peer_certificate: Option<Vec<u8>>,
}
pub struct Response {
    pub status: &'static str,
    pub headers: Vec<(String, String)>,
    pub body: String,
}
impl Response {
    pub fn json(body: &str) -> Self {
        Self {
            status: "200 OK",
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.into(),
        }
    }
    pub fn redirect(url: &str) -> Self {
        Self {
            status: "302 Found",
            headers: vec![("Location".into(), url.into())],
            body: String::new(),
        }
    }
}
pub struct Server {
    pub url: String,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Server {
    pub fn new(mtls: bool, handler: impl Fn(Request) -> Response + Send + Sync + 'static) -> Self {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let builder = rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap();
        let builder = if mtls {
            let mut roots = rustls::RootCertStore::empty();
            roots
                .add(CertificateDer::from_pem_slice(CA).unwrap())
                .unwrap();
            let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
                Arc::new(roots),
                provider,
            )
            .build()
            .unwrap();
            builder.with_client_cert_verifier(verifier)
        } else {
            builder.with_no_client_auth()
        };
        let config = Arc::new(
            builder
                .with_single_cert(
                    vec![CertificateDer::from_pem_slice(SERVER).unwrap()],
                    PrivateKeyDer::from_pem_slice(SERVER).unwrap(),
                )
                .unwrap(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("https://{}", listener.local_addr().unwrap());
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let handler = Arc::new(handler);
        let thread = std::thread::spawn(move || {
            let mut children = Vec::new();
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((tcp, _)) => {
                        let config = config.clone();
                        let handler = handler.clone();
                        children.push(std::thread::spawn(move || serve(tcp, config, handler)));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("fixture accept failed: {e}"),
                }
            }
            for child in children {
                child.join().unwrap();
            }
        });
        Self {
            url,
            stopped,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
}
fn serve(
    tcp: TcpStream,
    config: Arc<rustls::ServerConfig>,
    handler: Arc<impl Fn(Request) -> Response>,
) {
    // macOS can inherit the listener's nonblocking mode on accepted sockets.
    tcp.set_nonblocking(false).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    let conn = rustls::ServerConnection::new(config).unwrap();
    let mut stream = rustls::StreamOwned::new(conn, tcp);
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let (head_end, length) = loop {
        match stream.read(&mut buffer) {
            Ok(0) => return,
            Err(error) => {
                eprintln!("Authentication fixture TLS/read error: {error}");
                return;
            }
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
        }
        assert!(
            bytes.len() < 2 * 1024 * 1024,
            "fixture request exceeds limit"
        );
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = std::str::from_utf8(&bytes[..end]).unwrap();
            let length = head
                .lines()
                .find_map(|l| {
                    let (key, value) = l.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                break (end, length);
            }
        }
    };
    let head = std::str::from_utf8(&bytes[..head_end]).unwrap();
    let request = Request {
        target: head
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .into(),
        headers: head
            .lines()
            .skip(1)
            .map(|l| {
                let (k, v) = l.split_once(':').unwrap();
                (k.to_ascii_lowercase(), v.trim().into())
            })
            .collect(),
        body: String::from_utf8(bytes[head_end + 4..head_end + 4 + length].to_vec()).unwrap(),
        peer_certificate: stream
            .conn
            .peer_certificates()
            .and_then(|c| c.first())
            .map(|c| c.to_vec()),
    };
    let response = handler(request);
    let mut head = format!(
        "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        response.body.len()
    );
    for (k, v) in response.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    head.push_str(&response.body);
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.flush();
    stream.conn.send_close_notify();
    let _ = stream.flush();
}
