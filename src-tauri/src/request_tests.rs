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
        ..RequestInput::default()
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

fn store() -> (tempfile::TempDir, ResponseStore) {
    let root = tempfile::tempdir().unwrap();
    let store = ResponseStore::new(root.path().join("responses"));
    (root, store)
}

fn grants(root: &tempfile::TempDir) -> FileGrants {
    FileGrants::load(root.path().join("file-grants.json"))
}

async fn run(request: RequestInput, options: TransportOptions) -> Result<ResponseOutput, String> {
    let (root, store) = store();
    execute(request, options, &store, &grants(&root), DOWNLOAD_LIMIT, None).await
}

#[test]
fn resolves_environment_references_without_exposing_values_in_errors() {
    std::env::set_var("BLINK_TOKEN_ENV_TEST", "from-environment");
    assert_eq!(
        resolve_environment_references("Bearer {{!BLINK_TOKEN_ENV_TEST}}").unwrap(),
        "Bearer from-environment"
    );
    let error = resolve_environment_references("{{!BLINK_TOKEN_MISSING}}").unwrap_err();
    assert!(error.contains("BLINK_TOKEN_MISSING"));
    assert!(!error.contains("from-environment"));
    std::env::remove_var("BLINK_TOKEN_ENV_TEST");
}

#[test]
fn keeps_shift_operators_and_other_braces_as_text() {
    let text = "x << 2 >> 1; <<EOF; {\"a\": {\"b\": 1}}";
    assert_eq!(resolve_environment_references(text).unwrap(), text);
}

#[test]
fn rejects_invalid_environment_references() {
    for text in ["{{!}}", "{{!1ABC}}", "{{!A-B}}", "{{!OPEN"] {
        assert_eq!(
            resolve_environment_references(text).unwrap_err(),
            "Invalid environment variable reference."
        );
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
    let result = run(request, options()).await.unwrap();
    assert_eq!(result.status, 200);
    assert_eq!(result.body, "{\"ok\":true}");
    assert_eq!(result.size_bytes, result.body.len());
    assert!(result.duration_ms >= 70);
    let timing = result.timing;
    let phases = timing.dns_ms + timing.connect_ms + timing.wait_ms + timing.download_ms;
    assert!(timing.dns_ms >= 0.0 && timing.connect_ms > 0.0);
    // The phases cover the whole request, within rounding.
    assert!((phases - result.duration_ms as f64).abs() < 2.0, "{phases} vs {}", result.duration_ms);
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
    assert_eq!(run(request, options()).await.unwrap().status, 201);
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
    assert_eq!(run(input(url), options()).await.unwrap().status, 302);
}

#[tokio::test]
async fn error_status_retains_body() {
    let (url, _received) = fixture("401 Unauthorized", "", "denied".into(), Duration::ZERO);
    let result = run(input(url), options()).await.unwrap();
    assert_eq!(result.status, 401);
    assert_eq!(result.body, "denied");
}

#[tokio::test]
async fn validates_urls_and_does_not_leak_query_secrets_on_error() {
    assert!(run(input("file:///tmp/example".into()), options())
        .await
        .unwrap_err()
        .contains("HTTP"));
    assert!(run(
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
    let error = run(input(url), options()).await.unwrap_err();
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
    let result = run(input(format!("{base}/a")), following(5)).await.unwrap();
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
        run(input(format!("{base}/a")), following(1))
            .await
            .unwrap_err(),
        "Stopped after 1 redirects."
    );
}

#[tokio::test]
async fn omits_redirect_fields_without_a_redirect() {
    let base = serve(vec![("200 OK", String::new(), b"ok".to_vec())]);
    let result = run(input(base), following(5)).await.unwrap();
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
            run(input("http://127.0.0.1:9/".into()), invalid)
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
    let error = run(
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

#[tokio::test]
async fn truncates_the_preview_and_stores_the_full_body() {
    let body = "x".repeat(1024 * 1024 + 10).into_bytes();
    let base = serve(vec![("200 OK", String::new(), body.clone())]);
    let (root, store) = store();
    let result = execute(
        input(base),
        TransportOptions {
            inspection_limit_mib: 1,
            ..options()
        },
        &store,
        &grants(&root),
        DOWNLOAD_LIMIT,
        None,
    )
    .await
    .unwrap();
    assert!(result.truncated);
    assert!(!result.binary);
    assert_eq!(result.size_bytes, body.len());
    assert_eq!(result.body.len(), 1024 * 1024);
    let path = store.path(result.body_id.as_deref().unwrap()).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), body);
}

#[tokio::test]
async fn detects_binary_bodies() {
    for body in [vec![b'a', 0, b'b'], vec![b'a', 0xff, b'b']] {
        let base = serve(vec![("200 OK", String::new(), body)]);
        let result = run(input(base), options()).await.unwrap();
        assert!(result.binary);
        assert_eq!(result.body, "");
    }
}

#[tokio::test]
async fn a_character_cut_at_the_preview_limit_stays_text() {
    // "é" is two bytes. Put its first byte at the last preview position.
    let mut body = "a".repeat(1024 * 1024 - 1).into_bytes();
    body.extend_from_slice("é tail".as_bytes());
    let base = serve(vec![("200 OK", String::new(), body)]);
    let result = run(
        input(base),
        TransportOptions {
            inspection_limit_mib: 1,
            ..options()
        },
    )
    .await
    .unwrap();
    assert!(!result.binary);
    assert!(result.truncated);
    assert_eq!(result.body.len(), 1024 * 1024 - 1);
}

#[tokio::test]
async fn rejects_a_body_over_the_download_limit_and_keeps_no_file() {
    let base = serve(vec![("200 OK", String::new(), vec![b'x'; 64])]);
    let (root, store) = store();
    let error = execute(input(base), options(), &store, &grants(&root), 32, None)
        .await
        .unwrap_err();
    assert_eq!(error, "Response exceeds the 1 GiB download limit.");
    assert_eq!(
        std::fs::read_dir(root.path().join("responses"))
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn sends_a_custom_method() {
    let (url, received) = fixture("200 OK", "", String::new(), Duration::ZERO);
    let request = RequestInput {
        method: "PURGE".into(),
        ..input(url)
    };
    assert_eq!(run(request, options()).await.unwrap().status, 200);
    assert!(received
        .recv()
        .unwrap()
        .starts_with("PURGE /check HTTP/1.1"));
}

#[tokio::test]
async fn rejects_an_invalid_method() {
    let request = RequestInput {
        method: "BAD METHOD".into(),
        ..input("http://127.0.0.1:9/".into())
    };
    assert_eq!(
        run(request, options()).await.unwrap_err(),
        "Invalid HTTP method."
    );
}

#[tokio::test]
async fn cancel_stops_a_running_request() {
    let (url, _received) = fixture("200 OK", "", "late".into(), Duration::from_secs(5));
    let (root, store) = store();
    let grants = grants(&root);
    let in_flight = InFlight::default();
    let started = Instant::now();
    let request = in_flight.run(
        "r1".into(),
        execute(input(url), options(), &store, &grants, DOWNLOAD_LIMIT, None),
    );
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        in_flight.cancel("r1");
    };
    let (result, ()) = tokio::join!(request, cancel);
    assert_eq!(result.unwrap_err(), CANCELED);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(in_flight.0.lock().unwrap().is_empty());
    assert_eq!(
        std::fs::read_dir(root.path().join("responses"))
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn sends_a_granted_file_body() {
    let (url, received) = fixture("200 OK", "", String::new(), Duration::ZERO);
    let (root, store) = store();
    let grants = grants(&root);
    let file = root.path().join("payload.bin");
    std::fs::write(&file, b"file-bytes").unwrap();
    grants.grant(&file).unwrap();
    let request = RequestInput {
        method: "PUT".into(),
        body_file: Some(file.to_string_lossy().into_owned()),
        ..input(url)
    };
    execute(request, options(), &store, &grants, DOWNLOAD_LIMIT, None)
        .await
        .unwrap();
    assert!(received.recv().unwrap().ends_with("\r\n\r\nfile-bytes"));
}

#[tokio::test]
async fn refuses_a_file_that_was_not_picked() {
    let (root, store) = store();
    let file = root.path().join("secret.txt");
    std::fs::write(&file, b"secret").unwrap();
    let request = RequestInput {
        method: "POST".into(),
        body_file: Some(file.to_string_lossy().into_owned()),
        ..input("http://127.0.0.1:9/".into())
    };
    let error = execute(request, options(), &store, &grants(&root), DOWNLOAD_LIMIT, None)
        .await
        .unwrap_err();
    assert_eq!(error, "Choose secret.txt again to allow Blink to read it.");
}

#[tokio::test]
async fn sends_multipart_text_and_file_parts() {
    let (url, received) = fixture("200 OK", "", String::new(), Duration::ZERO);
    let (root, store) = store();
    let grants = grants(&root);
    let file = root.path().join("photo.png");
    std::fs::write(&file, b"PNGDATA").unwrap();
    grants.grant(&file).unwrap();
    let request = RequestInput {
        method: "POST".into(),
        multipart: Some(vec![
            MultipartInput {
                key: "title".into(),
                value: "Hello".into(),
                file: false,
            },
            MultipartInput {
                key: "upload".into(),
                value: file.to_string_lossy().into_owned(),
                file: true,
            },
        ]),
        ..input(url)
    };
    execute(request, options(), &store, &grants, DOWNLOAD_LIMIT, None)
        .await
        .unwrap();
    let raw = received.recv().unwrap();
    assert!(raw.contains("content-type: multipart/form-data; boundary="));
    assert!(raw.contains("name=\"title\"\r\n\r\nHello"));
    assert!(raw.contains("name=\"upload\"; filename=\"photo.png\""));
    assert!(raw.contains("PNGDATA"));
}

#[tokio::test]
async fn rejects_an_invalid_proxy_url() {
    let error = run(
        input("http://127.0.0.1:9/".into()),
        TransportOptions {
            proxy_url: "ftp://proxy".into(),
            ..options()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error, "Invalid proxy URL.");
}

#[tokio::test]
async fn sends_through_a_configured_proxy() {
    // The fixture acts as the proxy: it receives the absolute target URL.
    let (proxy, received) = fixture("200 OK", "", String::new(), Duration::ZERO);
    let proxy = proxy.trim_end_matches("/check").to_string();
    let result = run(
        input("http://example.invalid/through".into()),
        TransportOptions {
            proxy_url: proxy,
            ..options()
        },
    )
    .await
    .unwrap();
    assert_eq!(result.status, 200);
    assert!(received
        .recv()
        .unwrap()
        .starts_with("GET http://example.invalid/through HTTP/1.1"));
}

#[test]
fn transport_options_default_to_verified_tls_and_system_proxy() {
    let parsed: TransportOptions = serde_json::from_str(
        r#"{"timeoutSeconds":5,"connectTimeoutSeconds":2,"followRedirects":false,"maxRedirects":3,"inspectionLimitMiB":8}"#,
    )
    .unwrap();
    assert!(parsed.verify_tls);
    assert!(parsed.proxy_url.is_empty());
}

#[tokio::test]
async fn cookie_jar_sends_stored_cookies_and_saves_them() {
    let first = fixture(
        "200 OK",
        "Set-Cookie: session=abc; Path=/\r\n",
        String::new(),
        Duration::ZERO,
    );
    let (root, store) = store();
    let cookies = crate::cookies::Cookies::load(root.path().join("cookies.json"));
    execute(
        input(first.0.clone()),
        options(),
        &store,
        &grants(&root),
        DOWNLOAD_LIMIT,
        Some(cookies.jar()),
    )
    .await
    .unwrap();
    let listed = serde_json::to_value(cookies.list()).unwrap();
    assert_eq!(listed[0]["name"], "session");
    assert_eq!(listed[0]["value"], "abc");
    assert_eq!(listed[0]["domain"], "127.0.0.1");
    cookies.save().unwrap();
    // A reloaded jar still holds the session cookie.
    let reloaded = crate::cookies::Cookies::load(root.path().join("cookies.json"));
    assert_eq!(reloaded.list().len(), 1);

    let second = fixture("200 OK", "", String::new(), Duration::ZERO);
    let port = second.0.rsplit(':').next().unwrap().to_string();
    // Cookies are per host, not per port, so the second server gets it.
    execute(
        input(format!("http://127.0.0.1:{port}")),
        options(),
        &store,
        &grants(&root),
        DOWNLOAD_LIMIT,
        Some(reloaded.jar()),
    )
    .await
    .unwrap();
    assert!(second.1.recv().unwrap().to_lowercase().contains("cookie: session=abc"));
    // Without a jar no cookie goes out.
    let third = fixture("200 OK", "", String::new(), Duration::ZERO);
    run(input(third.0), options()).await.unwrap();
    assert!(!third.1.recv().unwrap().to_lowercase().contains("cookie:"));
}
