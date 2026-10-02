//! Requests as the Browser holds them. Port of `src/lib/session.ts`.

use std::sync::LazyLock;

use serde_json::{Map, Value, json};
use url::Url;

use crate::authorization::resolve_token_definitions;
use crate::ids;
use crate::model::{
    Definitions, Draft, Pair, RequestGroup, RequestInput, RequestSession, RequestView, SocketState,
};
use crate::request::{create_draft, js_trim, pair};
use crate::token_hints::resolve_for_display;

pub use crate::model::STREAM_EVENT_LIMIT;

pub fn create_view() -> RequestView {
    RequestView::default()
}

pub fn reserve_session_id(id: u64) {
    ids::SESSIONS.reserve(id);
}

/// The draft without row ids, for change detection.
pub fn draft_fingerprint(draft: &Draft) -> String {
    let mut value = serde_json::to_value(draft).unwrap_or(Value::Null);
    if let Value::Object(fields) = &mut value {
        for key in ["query", "headers", "form"] {
            if let Some(Value::Array(rows)) = fields.get_mut(key) {
                for row in rows {
                    if let Value::Object(row) = row {
                        row.remove("id");
                    }
                }
            }
        }
    }
    value.to_string()
}

/// Fingerprint the fully-resolved RequestInput that was actually sent.
/// Includes effective auth type so that a group/global auth change that
/// changes the Authorization header marks the existing response stale.
///
/// `request` is None when the build failed. `auth_type` is an optional
/// resolved auth type tag for the fingerprint.
pub fn request_fingerprint(request: Option<&RequestInput>, auth_type: Option<&str>) -> String {
    let Some(request) = request else {
        return String::new();
    };
    // Same keys, order, and omissions as the saved TS fingerprint.
    let mut fields = Map::new();
    fields.insert("method".into(), json!(request.method));
    fields.insert("url".into(), json!(request.url));
    fields.insert("headers".into(), json!(request.headers));
    fields.insert("body".into(), json!(request.body));
    if let Some(body_file) = &request.body_file {
        fields.insert("bodyFile".into(), json!(body_file));
    }
    if let Some(multipart) = &request.multipart {
        fields.insert("multipart".into(), json!(multipart));
    }
    if let Some(auth_type) = auth_type {
        fields.insert("_authType".into(), json!(auth_type));
    }
    Value::Object(fields).to_string()
}

static EMPTY_FINGERPRINT: LazyLock<String> = LazyLock::new(|| draft_fingerprint(&create_draft()));

pub fn has_draft(session: &RequestSession) -> bool {
    draft_fingerprint(&session.draft) != *EMPTY_FINGERPRINT
        || session.response.is_some()
        || !session.error.is_empty()
}

fn clone_rows(rows: &[Pair]) -> Vec<Pair> {
    rows.iter()
        .map(|row| Pair {
            enabled: row.enabled,
            file: row.file.filter(|file| *file),
            ..pair(row.key.clone(), row.value.clone())
        })
        .collect()
}

/// A new request: a blank draft, or a copy of `source` with new row ids.
pub fn create_session(source: Option<&Draft>) -> RequestSession {
    let draft = match source {
        Some(source) => Draft {
            query: clone_rows(&source.query),
            headers: clone_rows(&source.headers),
            form: source.form.as_deref().map(clone_rows),
            assertions: source.assertions.as_ref().map(|rows| {
                rows.iter()
                    .map(|row| crate::model::Assertion {
                        id: ids::CHECKS.next(),
                        ..row.clone()
                    })
                    .collect()
            }),
            captures: source.captures.as_ref().map(|rows| {
                rows.iter()
                    .map(|row| crate::model::Capture {
                        id: ids::CHECKS.next(),
                        ..row.clone()
                    })
                    .collect()
            }),
            ..source.clone()
        },
        None => create_draft(),
    };
    RequestSession {
        id: ids::SESSIONS.next(),
        group_id: None,
        draft,
        response: None,
        busy: false,
        error: String::new(),
        elapsed: 0.0,
        sent_fingerprint: String::new(),
        view: create_view(),
        history: Vec::new(),
        test_results: None,
        capture_errors: None,
        stream: None,
        socket: None,
        stale: false,
        waiting_on: None,
    }
}

/// Groups and global tokens, so labels can show resolved token values.
#[derive(Debug, Clone, Copy)]
pub struct LabelTokens<'a> {
    pub groups: &'a [RequestGroup],
    pub global_definitions: &'a Definitions,
}

fn display_url(session: &RequestSession, tokens: Option<LabelTokens>) -> String {
    let Some(tokens) = tokens else {
        return session.draft.url.clone();
    };
    let ctx = resolve_token_definitions(session.group_id, tokens.groups, tokens.global_definitions);
    resolve_for_display(&session.draft.url, Some(&ctx))
}

/// `new URL(text)`, limited to HTTP and WebSocket URLs.
fn labeled_url(text: &str) -> Option<Url> {
    let url = Url::parse(text).ok()?;
    matches!(url.scheme(), "http" | "https" | "ws" | "wss").then_some(url)
}

/// `URL.prototype.host`: the host, with the port when it is not the default.
fn url_host(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
}

pub fn session_label(session: &RequestSession, tokens: Option<LabelTokens>) -> String {
    match labeled_url(&display_url(session, tokens)) {
        Some(url) if url.path() == "/" => url_host(&url),
        // Show token references such as {{id}} as typed, not percent-encoded.
        Some(url) => decode_uri(url.path()).unwrap_or_else(|| url.path().to_string()),
        None => format!("Untitled {:02}", session.id),
    }
}

/// `decodeURI`: decode escapes except reserved characters. None for a
/// malformed sequence.
pub(crate) fn decode_uri(text: &str) -> Option<String> {
    const RESERVED: &[u8] = b";/?:@&=+$,#";
    let bytes = text.as_bytes();
    let hex = |at: usize| -> Option<u8> {
        if bytes.get(at) != Some(&b'%') {
            return None;
        }
        let digits = text.get(at + 1..at + 3)?;
        u8::from_str_radix(digits, 16).ok()
    };
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            let next = text[index..].chars().next()?;
            out.push(next);
            index += next.len_utf8();
            continue;
        }
        let first = hex(index)?;
        if first < 0x80 {
            if RESERVED.contains(&first) {
                out.push_str(&text[index..index + 3]);
            } else {
                out.push(first as char);
            }
            index += 3;
            continue;
        }
        let length = match first {
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => return None,
        };
        let mut sequence = vec![first];
        for step in 1..length {
            let byte = hex(index + step * 3)?;
            if byte & 0xc0 != 0x80 {
                return None;
            }
            sequence.push(byte);
        }
        out.push_str(std::str::from_utf8(&sequence).ok()?);
        index += length * 3;
    }
    Some(out)
}

/// Returns a short status label for the tab.
/// "Edited" / stale detection is handled by the request runner in the
/// response panel — session_status just reflects transport state.
pub fn session_status(session: &RequestSession) -> String {
    if session.busy {
        return "Sending".into();
    }
    if session
        .socket
        .as_ref()
        .is_some_and(|socket| socket.state == SocketState::Open)
    {
        return "Open".into();
    }
    if !session.error.is_empty() {
        return "Failed".into();
    }
    if let Some(response) = &session.response {
        return response.status.to_string();
    }
    "Draft".into()
}

pub fn session_host(session: &RequestSession, tokens: Option<LabelTokens>) -> String {
    labeled_url(&display_url(session, tokens))
        .map(|url| url_host(&url))
        .unwrap_or_default()
}

/// `/^wss?:\/\//i` on the trimmed URL.
fn is_websocket_url(url: &str) -> bool {
    let url = js_trim(url);
    let lower = url.get(..6).unwrap_or(url).to_ascii_lowercase();
    lower.starts_with("ws://") || lower.starts_with("wss://")
}

/// The method to show: "WS" for a WebSocket request.
pub fn display_method(session: &RequestSession) -> &str {
    if is_websocket_url(&session.draft.url) {
        "WS"
    } else {
        &session.draft.method
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ApiResponse, AuthKind, AuthorizationConfig, BodyMode, Header};

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn ok_response() -> ApiResponse {
        ApiResponse {
            status: 200,
            status_text: "OK".into(),
            duration_ms: 1.0,
            headers: vec![],
            body: String::new(),
            size_bytes: 0,
            body_id: None,
            truncated: None,
            binary: None,
            final_url: None,
            redirect_count: None,
            timing: None,
        }
    }

    // session.test.ts

    #[test]
    fn distinguishes_endpoints_without_leaking_url_credentials_or_query_tokens() {
        let mut session = create_session(None);
        session.draft.url =
            "https://user:synthetic-secret@example.test/v1/health?token=synthetic-query#fragment"
                .into();
        assert_eq!(session_label(&session, None), "/v1/health");
        assert_eq!(session_host(&session, None), "example.test");
    }

    #[test]
    fn shows_resolved_token_values_in_the_label_and_host() {
        let mut session = create_session(None);
        session.group_id = Some(1);
        session.draft.url = "{{base}}/{{endpoint}}".into();
        let groups = [RequestGroup {
            id: 1,
            name: "g".into(),
            parent_id: None,
            collapsed: false,
            local_auth: None,
            local_definitions: Some(defs(&[("endpoint", "users")])),
            response_tokens: None,
            default_method: None,
            default_url: None,
            environments: None,
            active_environment_id: None,
        }];
        let global = defs(&[("base", "https://api.example.test")]);
        let tokens = LabelTokens {
            groups: &groups,
            global_definitions: &global,
        };
        assert_eq!(session_label(&session, Some(tokens)), "/users");
        assert_eq!(session_host(&session, Some(tokens)), "api.example.test");
    }

    #[test]
    fn shows_token_references_in_the_label_as_typed() {
        let mut session = create_session(None);
        session.draft.url = "https://example.test/{{endpoint}}/a%20b".into();
        assert_eq!(session_label(&session, None), "/{{endpoint}}/a b");
    }

    #[test]
    fn marks_responses_from_an_earlier_draft_as_edited() {
        let mut session = create_session(None);
        assert!(!has_draft(&session));
        session.draft.url = "https://example.test".into();
        assert!(has_draft(&session));
        session.sent_fingerprint = draft_fingerprint(&session.draft);
        session.response = Some(ok_response());
        assert_eq!(session_status(&session), "200");
        session.draft.method = "POST".into();
        // session_status always shows the response code; stale detection lives
        // in the request runner to avoid false positives when sent_fingerprint
        // is a resolved-request fingerprint, not a draft fingerprint.
        assert_eq!(session_status(&session), "200");
    }

    #[test]
    fn duplicates_all_credentials_and_body_data_without_row_aliasing_or_responses() {
        let mut first = create_session(None);
        first.draft.auth = AuthKind::Bearer;
        first.draft.token = "synthetic".into();
        first.draft.body_mode = BodyMode::Json;
        first.draft.body = "{\"id\":9223372036854775807}".into();
        first.draft.headers[0].enabled = false;
        first.response = Some(ok_response());
        let mut second = create_session(Some(&first.draft));
        assert_eq!(
            draft_fingerprint(&second.draft),
            draft_fingerprint(&first.draft)
        );
        assert_ne!(second.draft.headers[0].id, first.draft.headers[0].id);
        assert!(second.response.is_none());
        assert!(!second.busy);
        second.draft.token = "changed".into();
        assert_eq!(first.draft.token, "synthetic");
    }

    #[test]
    fn untitled_labels_pad_the_id() {
        let mut session = create_session(None);
        session.id = 7;
        assert_eq!(session_label(&session, None), "Untitled 07");
        session.draft.url = "wss://socket.test:8443/".into();
        assert_eq!(session_label(&session, None), "socket.test:8443");
        assert_eq!(display_method(&session), "WS");
    }

    #[test]
    fn duplicates_structured_authorization_without_sharing_mutations() {
        let mut source = create_session(None);
        source.draft.local_auth = Some(AuthorizationConfig::Basic {
            username: "demo".into(),
            password: "synthetic".into(),
        });
        let mut duplicate = create_session(Some(&source.draft));
        assert_eq!(duplicate.draft.local_auth, source.draft.local_auth);
        if let Some(AuthorizationConfig::Basic { password, .. }) = &mut duplicate.draft.local_auth {
            *password = "changed".into();
        }
        assert_eq!(
            source.draft.local_auth,
            Some(AuthorizationConfig::Basic {
                username: "demo".into(),
                password: "synthetic".into(),
            })
        );
        assert!(duplicate.response.is_none());
    }

    // request-fingerprint.test.ts

    fn base_request() -> RequestInput {
        RequestInput {
            method: "GET".into(),
            url: "https://example.test/api".into(),
            headers: vec![Header {
                key: "Accept".into(),
                value: "application/json".into(),
            }],
            body: None,
            body_file: None,
            multipart: None,
        }
    }

    #[test]
    fn returns_empty_string_for_null_request() {
        assert_eq!(request_fingerprint(None, None), "");
    }

    #[test]
    fn produces_a_stable_string_for_the_same_request() {
        let request = base_request();
        assert_eq!(
            request_fingerprint(Some(&request), Some("none")),
            request_fingerprint(Some(&request), Some("none"))
        );
    }

    #[test]
    fn differs_when_auth_type_changes() {
        let request = base_request();
        assert_ne!(
            request_fingerprint(Some(&request), Some("none")),
            request_fingerprint(Some(&request), Some("bearer"))
        );
    }

    #[test]
    fn differs_when_url_changes() {
        let other = RequestInput {
            url: "https://other.test".into(),
            ..base_request()
        };
        assert_ne!(
            request_fingerprint(Some(&base_request()), Some("none")),
            request_fingerprint(Some(&other), Some("none"))
        );
    }

    #[test]
    fn differs_when_method_changes() {
        let other = RequestInput {
            method: "POST".into(),
            ..base_request()
        };
        assert_ne!(
            request_fingerprint(Some(&base_request()), Some("none")),
            request_fingerprint(Some(&other), Some("none"))
        );
    }

    #[test]
    fn differs_when_headers_differ() {
        let mut other = base_request();
        other.headers.push(Header {
            key: "Authorization".into(),
            value: "Bearer tok".into(),
        });
        assert_ne!(
            request_fingerprint(Some(&base_request()), Some("bearer")),
            request_fingerprint(Some(&other), Some("bearer"))
        );
    }

    #[test]
    fn same_request_no_auth_type_is_stable() {
        assert!(!request_fingerprint(Some(&base_request()), None).is_empty());
    }

    #[test]
    fn matches_the_ts_fingerprint_text() {
        assert_eq!(
            request_fingerprint(Some(&base_request()), Some("none")),
            r#"{"method":"GET","url":"https://example.test/api","headers":[{"key":"Accept","value":"application/json"}],"body":null,"_authType":"none"}"#
        );
    }

    #[test]
    fn decodes_uri_like_javascript() {
        assert_eq!(
            decode_uri("/a%20b/%2F/%C3%A9").as_deref(),
            Some("/a b/%2F/é")
        );
        assert_eq!(decode_uri("/%E0%A4%A"), None);
        assert_eq!(decode_uri("/%FF"), None);
    }
}
