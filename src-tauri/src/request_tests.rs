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

/// Serve each response on its own connection, in order. `{base}` in a
/// response's headers is replaced with the server's base URL.
fn serve(responses: Vec<(&'static str, String, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let fixture_base = base.clone();
    thread::spawn(move || {
        for (status, headers, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0; 4096];
            while !String::from_utf8_lossy(&bytes).contains("\r\n\r\n") {
                let read = stream.read(&mut chunk).unwrap_or(0);
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..read]);
            }
            let headers = headers.replace("{base}", &fixture_base);
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    base
}

fn options() -> TransportOptions {
    TransportOptions::default()
}

fn following(max_redirects: usize) -> TransportOptions {
    TransportOptions {
        follow_redirects: true,
        max_redirects,
        ..TransportOptions::default()
    }
}

#[test]
fn resolves_environment_references_without_exposing_values_in_errors() {
    std::env::set_var("BLINK_TOKEN_ENV_TEST", "from-environment");
    assert_eq!(
        resolve_environment_references("Bearer <<BLINK_TOKEN_ENV_TEST>>").unwrap(),
        "Bearer from-environment"
    );
    let error = resolve_environment_references("<<BLINK_TOKEN_MISSING>>").unwrap_err();
    assert!(error.contains("BLINK_TOKEN_MISSING"));
    assert!(!error.contains("from-environment"));
    std::env::remove_var("BLINK_TOKEN_ENV_TEST");
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
    let result = execute(request, options()).await.unwrap();
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
    assert_eq!(execute(request, options()).await.unwrap().status, 201);
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
    assert_eq!(execute(input(url), options()).await.unwrap().status, 302);
}

#[tokio::test]
async fn error_status_retains_body() {
    let (url, _received) = fixture("401 Unauthorized", "", "denied".into(), Duration::ZERO);
    let result = execute(input(url), options()).await.unwrap();
    assert_eq!(result.status, 401);
    assert_eq!(result.body, "denied");
}

#[tokio::test]
async fn rejects_oversized_response() {
    let (url, _received) = fixture("200 OK", "", "x".repeat(RESPONSE_LIMIT + 1), Duration::ZERO);
    assert!(execute(input(url), options())
        .await
        .unwrap_err()
        .contains("4 MiB"));
}

#[tokio::test]
async fn validates_urls_and_does_not_leak_query_secrets_on_error() {
    assert!(execute(input("file:///tmp/example".into()), options())
        .await
        .unwrap_err()
        .contains("HTTP"));
    assert!(execute(
        input("https://user:password@example.test".into()),
        options()
    )
    .await
    .unwrap_err()
    .contains("Auth tab"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/?token=private-value",
        listener.local_addr().unwrap()
    );
    drop(listener);
    let error = execute(input(url), options()).await.unwrap_err();
    assert!(!error.contains("private-value"));
}

#[tokio::test]
async fn follows_redirects_and_reports_the_final_url() {
    let base = serve(vec![
        ("302 Found", "Location: {base}/b\r\n".into(), vec![]),
        (
            "301 Moved Permanently",
            "Location: {base}/c\r\n".into(),
            vec![],
        ),
        ("200 OK", String::new(), b"done".to_vec()),
    ]);
    let result = execute(input(format!("{base}/a")), following(5))
        .await
        .unwrap();
    assert_eq!(result.status, 200);
    assert_eq!(result.body, "done");
    assert_eq!(
        result.final_url.as_deref(),
        Some(format!("{base}/c").as_str())
    );
    assert_eq!(result.redirect_count, Some(2));
}

#[tokio::test]
async fn stops_at_the_redirect_limit() {
    let base = serve(vec![
        ("302 Found", "Location: {base}/b\r\n".into(), vec![]),
        ("302 Found", "Location: {base}/c\r\n".into(), vec![]),
        ("200 OK", String::new(), b"done".to_vec()),
    ]);
    assert_eq!(
        execute(input(format!("{base}/a")), following(1))
            .await
            .unwrap_err(),
        "Stopped after 1 redirects."
    );
}

#[tokio::test]
async fn omits_redirect_fields_without_a_redirect() {
    let base = serve(vec![("200 OK", String::new(), b"ok".to_vec())]);
    let result = execute(input(base), following(5)).await.unwrap();
    assert_eq!(result.final_url, None);
    assert_eq!(result.redirect_count, None);
}

#[tokio::test]
async fn rejects_out_of_range_transport_options() {
    for invalid in [
        TransportOptions {
            timeout_seconds: 0,
            ..options()
        },
        TransportOptions {
            timeout_seconds: 601,
            ..options()
        },
        TransportOptions {
            connect_timeout_seconds: 31,
            ..options()
        },
        TransportOptions {
            max_redirects: 21,
            ..options()
        },
        TransportOptions {
            inspection_limit_mib: 17,
            ..options()
        },
    ] {
        assert_eq!(
            execute(input("http://127.0.0.1:9/".into()), invalid)
                .await
                .unwrap_err(),
            "Invalid transport settings."
        );
    }
}

#[test]
fn transport_options_use_the_frontend_field_names() {
    let parsed: TransportOptions = serde_json::from_str(
        r#"{"timeoutSeconds":5,"connectTimeoutSeconds":2,"followRedirects":true,"maxRedirects":3,"inspectionLimitMiB":8}"#,
    )
    .unwrap();
    assert_eq!(parsed.timeout_seconds, 5);
    assert_eq!(parsed.inspection_limit_mib, 8);
}

#[tokio::test]
async fn timeout_message_uses_the_configured_limits() {
    let (url, _received) = fixture("200 OK", "", "late".into(), Duration::from_secs(3));
    let error = execute(
        input(url),
        TransportOptions {
            timeout_seconds: 1,
            connect_timeout_seconds: 1,
            ..options()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        error,
        "Request timed out (1 s connection / 1 s total limit)."
    );
}
