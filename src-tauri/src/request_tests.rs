use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

fn fixture(
    status: &str,
    headers: &str,
    body: String,
    delay: Duration,
) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let status = status.to_string();
    let headers = headers.to_string();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let read = stream.read(&mut chunk).unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&bytes);
            if let Some(end) = text.find("\r\n\r\n") {
                let length = text[..end]
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        tx.send(String::from_utf8_lossy(&bytes).to_string())
            .unwrap();
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).unwrap();
        thread::sleep(delay);
        let _ = stream.write_all(body.as_bytes());
    });
    (format!("http://{address}/check"), rx)
}
fn input(url: String) -> RequestInput {
    RequestInput {
        method: "GET".into(),
        url,
        headers: vec![],
        body: None,
    }
}

#[tokio::test]
async fn get_omits_body_and_measures_full_response() {
    let (url, received) = fixture(
        "200 OK",
        "Content-Type: application/json\r\nX-Test: native\r\n",
        "{\"ok\":true}".into(),
        Duration::from_millis(80),
    );
    let mut request = input(url);
    request.body = Some("not sent".into());
    let result = send_request(request).await.unwrap();
    assert_eq!(result.status, 200);
    assert_eq!(result.body, "{\"ok\":true}");
    assert_eq!(result.size_bytes, result.body.len());
    assert!(result.duration_ms >= 70);
    assert!(result
        .headers
        .iter()
        .any(|h| h.key == "x-test" && h.value == "native"));
    assert!(!received.recv().unwrap().contains("not sent"));
}

#[tokio::test]
async fn post_preserves_whitespace_and_duplicate_headers() {
    let (url, received) = fixture("201 Created", "", "created".into(), Duration::ZERO);
    let mut request = input(url);
    request.method = "POST".into();
    request.body = Some("  exact body\n".into());
    request.headers = vec![
        HeaderInput {
            key: "X-Tag".into(),
            value: "a".into(),
        },
        HeaderInput {
            key: "X-Tag".into(),
            value: "b".into(),
        },
    ];
    assert_eq!(send_request(request).await.unwrap().status, 201);
    let wire = received.recv().unwrap();
    assert!(wire.ends_with("  exact body\n"));
    assert!(wire.contains("x-tag: a\r\n"));
    assert!(wire.contains("x-tag: b\r\n"));
}

#[tokio::test]
async fn returns_redirect_without_following_it() {
    let (url, _received) = fixture(
        "302 Found",
        "Location: http://127.0.0.1:1/private\r\n",
        String::new(),
        Duration::ZERO,
    );
    assert_eq!(send_request(input(url)).await.unwrap().status, 302);
}

#[tokio::test]
async fn error_status_retains_body() {
    let (url, _received) = fixture("401 Unauthorized", "", "denied".into(), Duration::ZERO);
    let result = send_request(input(url)).await.unwrap();
    assert_eq!(result.status, 401);
    assert_eq!(result.body, "denied");
}

#[tokio::test]
async fn rejects_oversized_response() {
    let (url, _received) = fixture("200 OK", "", "x".repeat(RESPONSE_LIMIT + 1), Duration::ZERO);
    assert!(send_request(input(url))
        .await
        .unwrap_err()
        .contains("4 MiB"));
}

#[tokio::test]
async fn validates_urls_and_does_not_leak_query_secrets_on_error() {
    assert!(send_request(input("file:///tmp/example".into()))
        .await
        .unwrap_err()
        .contains("HTTP"));
    assert!(
        send_request(input("https://user:password@example.test".into()))
            .await
            .unwrap_err()
            .contains("Auth tab")
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/?token=private-value",
        listener.local_addr().unwrap()
    );
    drop(listener);
    let error = send_request(input(url)).await.unwrap_err();
    assert!(!error.contains("private-value"));
}
