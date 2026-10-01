//! Headless flow: TLS verification and proxy settings reach the engine, and
//! the status bar warns while verification is off.

use std::io::{Read as _, Write as _};
use std::sync::Arc;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

use crate::test_support::{self, Harness};

/// A self-signed certificate for 127.0.0.1 and localhost (test data only).
const CERT: &str = "-----BEGIN CERTIFICATE-----
MIIBmzCCAUGgAwIBAgIUHyLWTDEiUuhL5apcx28PZFiAx5wwCgYIKoZIzj0EAwIw
FDESMBAGA1UEAwwJbG9jYWxob3N0MCAXDTI2MDkzMDIzNDg0MVoYDzIxMjYwOTA2
MjM0ODQxWjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjO
PQMBBwNCAASKwC6DfBTFPPZxo5Xsn+falBLiKHbHtdJMeAsW51uEsgz3JP/jKpoL
ywcHvbDxFs5Ep7xqGj6wdPdpaP4Y12P/o28wbTAdBgNVHQ4EFgQUX8oMJTdK4v3d
jDAqthur7vfV+AEwHwYDVR0jBBgwFoAUX8oMJTdK4v3djDAqthur7vfV+AEwDwYD
VR0TAQH/BAUwAwEB/zAaBgNVHREEEzARhwR/AAABgglsb2NhbGhvc3QwCgYIKoZI
zj0EAwIDSAAwRQIgBJ13+Tl3ECnzULLvfiOrjpohoIJCcxqD4LhmDX/IGSQCIQDd
5OvJo65IHTLo705Ye13PG3K+vq96+sGJC8VaAueNDQ==
-----END CERTIFICATE-----
";
const KEY: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgrFlX7mcaHyPdSqhy
zvzGwNlBP2XKMcfYDcF6Jb9CgX2hRANCAASKwC6DfBTFPPZxo5Xsn+falBLiKHbH
tdJMeAsW51uEsgz3JP/jKpoLywcHvbDxFs5Ep7xqGj6wdPdpaP4Y12P/
-----END PRIVATE KEY-----
";

/// An HTTPS server with the self-signed certificate. Answers "secure".
fn tls_server() -> String {
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![CertificateDer::from_pem_slice(CERT.as_bytes()).unwrap()],
        PrivateKeyDer::from_pem_slice(KEY.as_bytes()).unwrap(),
    )
    .unwrap();
    let config = Arc::new(config);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for tcp in listener.incoming() {
            let Ok(tcp) = tcp else { break };
            let config = config.clone();
            std::thread::spawn(move || {
                let _ = tcp.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let connection = rustls::ServerConnection::new(config).unwrap();
                let mut stream = rustls::StreamOwned::new(connection, tcp);
                let mut request = Vec::new();
                let mut chunk = [0; 4096];
                while test_support::find(&request, b"\r\n\r\n").is_none() {
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => return,
                        Ok(read) => request.extend_from_slice(&chunk[..read]),
                    }
                }
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 6\r\n\
                      Connection: close\r\n\r\nsecure",
                );
                let _ = stream.flush();
                stream.conn.send_close_notify();
                let _ = stream.flush();
            });
        }
    });
    format!("https://127.0.0.1:{}/", address.port())
}

fn set_transport(
    harness: &Harness,
    cx: &mut TestAppContext,
    change: impl FnOnce(&mut blink_core::model::TransportOptions),
) {
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| change(&mut workspace.preferences.transport));
    });
    harness.draw(cx);
}

fn tls_label_shown(harness: &Harness, cx: &mut TestAppContext) -> bool {
    harness.draw(cx);
    harness.update(cx, |window, _| {
        window
            .try_find("status-tls")
            .is_some_and(|label| label.label() == Some("TLS VERIFY OFF"))
    })
}

#[gpui_kit::test]
fn verifies_tls_by_default_and_accepts_self_signed_when_off(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let url = tls_server();
    let harness = test_support::open(cx, &engine);

    // Verify on: the self-signed certificate is refused.
    assert!(!tls_label_shown(&harness, cx));
    harness.send(cx, &url);
    let (error, response) = harness.session(cx, |s| (s.error.clone(), s.response.is_some()));
    assert!(error.starts_with("Network request failed:"), "{error}");
    assert!(!response);

    // Verify off: the same server answers, and the status bar warns.
    set_transport(&harness, cx, |transport| transport.verify_tls = false);
    assert!(tls_label_shown(&harness, cx));
    harness.send(cx, &url);
    let (error, body) = harness.session(cx, |s| {
        (s.error.clone(), s.response.as_ref().map(|r| r.body.clone()))
    });
    assert_eq!(error, "");
    assert_eq!(body.as_deref(), Some("secure"));

    set_transport(&harness, cx, |transport| transport.verify_tls = true);
    assert!(!tls_label_shown(&harness, cx));
}

#[gpui_kit::test]
fn a_bad_proxy_url_fails_with_the_vue_error(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    for proxy in ["ftp://proxy.local", "not a url"] {
        set_transport(&harness, cx, |transport| transport.proxy_url = proxy.into());
        harness.send(cx, "http://127.0.0.1:9/");
        assert_eq!(
            harness.session(cx, |s| s.error.clone()),
            "Invalid proxy URL."
        );
    }
}
