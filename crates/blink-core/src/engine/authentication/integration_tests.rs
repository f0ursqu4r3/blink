//! Exercises real HTTPS and Engine calls; only Keychain storage is substituted.
use super::*;
use crate::engine::auth_test_support::{CLIENT, Credentials, Request, Response, Server};
use crate::model::{Header, RequestInput, TransportOptions};
use futures::executor::block_on;
use rustls::pki_types::{CertificateDer, pem::PemObject};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::Instant;

struct Harness {
    engine: Engine,
    credentials: Credentials,
    directory: tempfile::TempDir,
}
impl Harness {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let engine = Engine::new(crate::engine::Paths::with_base(
            directory.path().join("app"),
        ))
        .unwrap();
        Self {
            engine,
            credentials: Credentials::new(),
            directory,
        }
    }
    fn seed(&self, token_url: &str) {
        let credential = OAuthCredential {
            config: config(token_url),
            access_token: "expired-access".into(),
            refresh_token: Some("refresh-old".into()),
            expires_at: Some(0),
        };
        credentials::set(
            Some(&self.credentials.scope),
            "oauth:api",
            &serde_json::to_vec(&credential).unwrap(),
        )
        .unwrap();
    }
    fn stored(&self) -> OAuthCredential {
        serde_json::from_slice(
            &credentials::get(Some(&self.credentials.scope), "oauth:api")
                .unwrap()
                .unwrap(),
        )
        .unwrap()
    }
    fn send(
        &self,
        url: &str,
        id: &str,
        bearer: bool,
    ) -> impl Future<Output = Result<crate::model::ApiResponse, String>> + Send + 'static {
        self.engine.send_request_scoped(
            input(url, bearer),
            TransportOptions {
                follow_redirects: true,
                ..Default::default()
            },
            id.into(),
            None,
            Some(self.credentials.scope.clone()),
        )
    }
}
fn config(token_url: &str) -> OAuthConfig {
    OAuthConfig {
        authorization_url: "https://provider.example/authorize".into(),
        token_url: token_url.into(),
        client_id: "public-test-client".into(),
        scopes: "read write".into(),
    }
}
fn input(url: &str, bearer: bool) -> RequestInput {
    RequestInput {
        method: "GET".into(),
        url: url.into(),
        headers: if bearer {
            vec![Header {
                key: "Authorization".into(),
                value: "Bearer {{@api}}".into(),
            }]
        } else {
            Vec::new()
        },
        ..Default::default()
    }
}
fn fields(request: &Request) -> HashMap<String, String> {
    url::form_urlencoded::parse(request.body.as_bytes())
        .into_owned()
        .collect()
}
fn callback_request(url: &str) -> String {
    let url = url::Url::parse(url).unwrap();
    let address = format!("127.0.0.1:{}", url.port().unwrap());
    let mut socket = std::net::TcpStream::connect(&address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    write!(
        socket,
        "GET {}?{} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n",
        url.path(),
        url.query().unwrap()
    )
    .unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    response
}
fn wait_until(mut test: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !test() {
        assert!(
            Instant::now() < deadline,
            "authentication operation did not finish"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
const ROTATED: &str = r#"{"token_type":"Bearer","access_token":"access-new","refresh_token":"refresh-new","expires_in":3600}"#;

#[test]
fn browser_callback_exchanges_pkce_code_then_sends_and_refreshes() {
    let (tx, rx) = mpsc::channel();
    let server = Server::new(false, move |request| {
        let target = request.target.clone();
        let form = fields(&request);
        tx.send(request).unwrap();
        if target == "/token"
            && form.get("grant_type").map(String::as_str) == Some("authorization_code")
        {
            Response::json(
                r#"{"token_type":"Bearer","access_token":"first-access","refresh_token":"refresh-old","expires_in":3600}"#,
            )
        } else if target == "/token" {
            Response::json(ROTATED)
        } else {
            Response::json(r#"{"ok":true}"#)
        }
    });
    let h = Harness::new();
    let login = block_on(
        h.engine
            .begin_oauth(config(&format!("{}/token", server.url))),
    )
    .unwrap();
    let auth = url::Url::parse(&login.authorization_url).unwrap();
    let params: HashMap<_, _> = auth.query_pairs().into_owned().collect();
    assert_eq!(params["code_challenge_method"], "S256");
    assert_eq!(params["code_challenge"], challenge(&login.verifier));
    let redirect = login.redirect.clone();
    let state = login.state.clone();
    let finish = h
        .engine
        .finish_oauth(login, Some(h.credentials.scope.clone()), "api".into());
    assert!(
        callback_request(&format!("{redirect}?code=foreign&state=wrong"))
            .contains("400 Bad Request")
    );
    assert!(rx.try_recv().is_err());
    assert!(
        callback_request(&format!("{redirect}?code=auth-code&state={state}")).contains("200 OK")
    );
    block_on(finish).unwrap();
    let exchange = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let form = fields(&exchange);
    assert_eq!(form["code"], "auth-code");
    assert_eq!(form["redirect_uri"], redirect);
    assert_eq!(challenge(&form["code_verifier"]), params["code_challenge"]);
    block_on(h.send(&format!("{}/api", server.url), "first", true)).unwrap();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(3)).unwrap().headers["authorization"],
        "Bearer first-access"
    );
    let mut old = h.stored();
    old.expires_at = Some(0);
    credentials::set(
        Some(&h.credentials.scope),
        "oauth:api",
        &serde_json::to_vec(&old).unwrap(),
    )
    .unwrap();
    block_on(h.send(&format!("{}/api", server.url), "refresh", true)).unwrap();
    let refresh = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(fields(&refresh)["refresh_token"], "refresh-old");
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(3)).unwrap().headers["authorization"],
        "Bearer access-new"
    );
    assert_eq!(h.stored().refresh_token.as_deref(), Some("refresh-new"));
    let inspection = h.engine.take_inspection("refresh").unwrap();
    assert!(!inspection.text(false).contains("access-new"));
    assert!(inspection.tls_verified);
}

#[test]
fn simultaneous_expired_sends_rotate_once_including_collection_engine() {
    let (entered_tx, entered) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let release = Mutex::new(release);
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let server = Server::new(false, move |request| {
        if request.target == "/token" {
            assert_eq!(fields(&request)["refresh_token"], "refresh-old");
            count.fetch_add(1, Ordering::SeqCst);
            entered_tx.send(()).unwrap();
            release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Response::json(ROTATED)
        } else {
            assert_eq!(request.headers["authorization"], "Bearer access-new");
            Response::json("{}")
        }
    });
    let h = Harness::new();
    h.seed(&format!("{}/token", server.url));
    let (row, _row_files) = h.engine.for_collection_row().unwrap();
    assert!(Arc::ptr_eq(
        &h.engine.0.credential_lock,
        &row.0.credential_lock
    ));
    let first = h.send(&format!("{}/api", server.url), "one", true);
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(
        h.credentials
            .events
            .recv_timeout(Duration::from_secs(3))
            .unwrap(),
        "resolve"
    );
    let second = row.send_request_scoped(
        input(&format!("{}/api", server.url), true),
        TransportOptions::default(),
        "two".into(),
        None,
        Some(h.credentials.scope.clone()),
    );
    assert_eq!(
        h.credentials
            .events
            .recv_timeout(Duration::from_secs(3))
            .unwrap(),
        "resolve"
    );
    assert!(h.engine.0.credential_lock.try_lock().is_err());
    release_tx.send(()).unwrap();
    let (a, b) = block_on(futures::future::join(first, second));
    a.unwrap();
    b.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(h.stored().refresh_token.as_deref(), Some("refresh-new"));
}

#[test]
fn canceled_api_send_preserves_rotated_refresh_without_sending_api_request() {
    let (entered_tx, entered) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let release = Mutex::new(release);
    let api_calls = Arc::new(AtomicUsize::new(0));
    let calls = api_calls.clone();
    let server = Server::new(false, move |request| {
        if request.target == "/token" {
            entered_tx.send(()).unwrap();
            release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Response::json(ROTATED)
        } else {
            calls.fetch_add(1, Ordering::SeqCst);
            Response::json("{}")
        }
    });
    let h = Harness::new();
    h.seed(&format!("{}/token", server.url));
    let send = h.send(&format!("{}/api", server.url), "cancel", true);
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    h.engine.cancel_request("cancel");
    assert_eq!(block_on(send).unwrap_err(), crate::engine::http::CANCELED);
    release_tx.send(()).unwrap();
    wait_until(|| h.stored().refresh_token.as_deref() == Some("refresh-new"));
    // Wait behind the credential task, then issue a fresh API send using its result.
    block_on(h.send(&format!("{}/api", server.url), "after", true)).unwrap();
    assert_eq!(api_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn removing_a_credential_during_refresh_cannot_restore_it_afterward() {
    let (entered_tx, entered) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let release = Mutex::new(release);
    let server = Server::new(false, move |request| {
        if request.target == "/token" {
            entered_tx.send(()).unwrap();
            release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Response::json(ROTATED)
        } else {
            Response::json("{}")
        }
    });
    let h = Harness::new();
    h.seed(&format!("{}/token", server.url));
    let send = h.send(&format!("{}/api", server.url), "send", true);
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(
        h.credentials
            .events
            .recv_timeout(Duration::from_secs(3))
            .unwrap(),
        "resolve"
    );
    let remove = h
        .engine
        .remove_secret(Some(h.credentials.scope.clone()), "api".into());
    assert_eq!(
        h.credentials
            .events
            .recv_timeout(Duration::from_secs(3))
            .unwrap(),
        "remove"
    );
    assert!(h.engine.0.credential_lock.try_lock().is_err());
    release_tx.send(()).unwrap();
    block_on(send).unwrap();
    block_on(remove).unwrap();
    assert!(
        credentials::get(Some(&h.credentials.scope), "oauth:api")
            .unwrap()
            .is_none()
    );
    assert!(
        block_on(h.send(&format!("{}/api", server.url), "removed", true))
            .unwrap_err()
            .contains("not stored")
    );
}

#[test]
fn token_redirect_and_error_responses_never_expose_or_forward_credentials() {
    let target_calls = Arc::new(AtomicUsize::new(0));
    let calls = target_calls.clone();
    let target = Server::new(false, move |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        Response::json(ROTATED)
    });
    let target_url = target.url.clone();
    let source = Server::new(false, move |request| {
        if request.target == "/redirect" {
            Response::redirect(&target_url)
        } else {
            Response {
                status: "400 Bad Request",
                headers: Vec::new(),
                body: "sensitive-token-response".into(),
            }
        }
    });
    for path in ["/redirect", "/error"] {
        let h = Harness::new();
        h.seed(&format!("{}{path}", source.url));
        let error = block_on(h.send(&format!("{}/api", source.url), "failure", true)).unwrap_err();
        assert!(error.contains("HTTP"));
        assert!(!error.contains("sensitive-token-response"));
        assert_eq!(h.stored().refresh_token.as_deref(), Some("refresh-old"));
    }
    assert_eq!(target_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn client_certificate_is_required_origin_scoped_and_never_redirected_elsewhere() {
    let (tx, rx) = mpsc::channel();
    let other_calls = Arc::new(AtomicUsize::new(0));
    let calls = other_calls.clone();
    let other = Server::new(true, move |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        Response::json("{}")
    });
    let other_url = other.url.clone();
    let server = Server::new(true, move |request| {
        assert_eq!(
            request.peer_certificate.as_deref(),
            Some(CertificateDer::from_pem_slice(CLIENT).unwrap().as_ref())
        );
        let target = request.target.clone();
        tx.send(request).unwrap();
        match target.as_str() {
            "/same" => Response::redirect("/ok"),
            "/other" => Response::redirect(&other_url),
            _ => Response::json("{}"),
        }
    });
    let h = Harness::new();
    assert!(block_on(h.send(&server.url, "no-identity", false)).is_err());
    assert!(rx.try_recv().is_err());
    let pem = h.directory.path().join("client.pem");
    std::fs::write(&pem, CLIENT).unwrap();
    block_on(h.engine.save_client_identity(
        Some(h.credentials.scope.clone()),
        server.url.clone(),
        pem,
    ))
    .unwrap();
    let response = block_on(h.send(&format!("{}/same", server.url), "same-origin", false)).unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.redirect_count, Some(1));
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(3)).unwrap().target,
        "/same"
    );
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(3)).unwrap().target,
        "/ok"
    );
    let trace = h.engine.take_inspection("same-origin").unwrap();
    assert!(trace.client_certificate && trace.tls_verified);
    assert_eq!(trace.peer_certificate_sha256.unwrap().len(), 64);
    let error =
        block_on(h.send(&format!("{}/other", server.url), "cross-origin", false)).unwrap_err();
    assert!(error.contains("configured origin"), "{error}");
    assert!(block_on(h.send(&other.url, "other-origin", false)).is_err());
    assert_eq!(other_calls.load(Ordering::SeqCst), 0);
    let other_scope = Credentials::new();
    assert!(
        block_on(h.engine.send_request_scoped(
            input(&server.url, false),
            TransportOptions::default(),
            "other-project".into(),
            None,
            Some(other_scope.scope.clone())
        ))
        .is_err()
    );
    block_on(
        h.engine
            .remove_client_identity(Some(h.credentials.scope.clone()), server.url.clone()),
    )
    .unwrap();
    assert!(block_on(h.send(&server.url, "removed-identity", false)).is_err());
}
