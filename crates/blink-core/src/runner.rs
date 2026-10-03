//! The send, stream, check, and staleness rules of
//! `src/composables/useRequestRunner.ts`, and the connection rules of
//! `src/composables/useWebSocket.ts` and `buildWebSocketRequest` in
//! `src/lib/websocket.ts`.
//!
//! The UI drives the async parts with `Engine`:
//! 1. `prepare_send` builds the request from the session and its context.
//! 2. `primary_action` tells whether to ask for a protected environment,
//!    send, connect, or disconnect.
//! 3. For HTTP: `begin_send` returns a `SendTicket`. Release
//!    `ticket.released`, then run `Engine::send_request` with
//!    `ticket.request` and `ticket.options`. Pass each `StreamMessage` to
//!    `apply_stream_message`, call `tick` on a 100 ms timer, and pass the
//!    result to `finish_send`. Apply `SendOutcome::captured` with
//!    `Workspace::capture`. Cancel with `Engine::cancel_request`.
//! 4. For WebSocket: `connect_socket`, then `Engine::ws_connect`; pass each
//!    event to `apply_socket_event`.

use std::time::Instant;

use crate::authorization::ResolvedRequestContext;
use crate::checks::{run_assertions, run_captures};
use crate::engine::{CANCELED, SocketEvent as EngineSocketEvent, StreamMessage};
use crate::history::{HistoryOutcome, add_history, history_entry, next_history_id, now_ms};
use crate::interpolation::interpolate;
use crate::model::{
    ApiResponse, BodyMode, Definitions, Draft, Environment, Header, LiveStream, RequestInput,
    RequestSession, STREAM_EVENT_LIMIT, SocketState, SseEvent, TransportOptions,
};
use crate::preferences::transport_options;
use crate::request::{RequestContext, build_request, js_trim, to_curl};
use crate::response_tokens::TokenSources;
use crate::session::request_fingerprint;
use crate::sse::SseParser;
use crate::transport_options::MIB;
use crate::websocket_log::{self, SocketEvent, is_web_socket_url};
use crate::workspace::auth_type;

pub use crate::engine::CANCELED as REQUEST_CANCELED;

// ── Preparing ───────────────────────────────────────────────────────────────

/// The HTTP request a draft builds, or the message for the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    pub request: Result<RequestInput, String>,
    /// The resolved auth type, when built with a context.
    pub auth_type: Option<&'static str>,
}

impl Prepared {
    /// The validation message; empty when the request builds.
    pub fn error(&self) -> &str {
        self.request.as_ref().err().map_or("", String::as_str)
    }

    pub fn request(&self) -> Option<&RequestInput> {
        self.request.as_ref().ok()
    }

    /// Of the fully resolved request, including the effective auth, so a
    /// group auth or token change marks a shown response stale.
    pub fn fingerprint(&self) -> String {
        request_fingerprint(self.request(), self.auth_type)
    }

    /// The cURL command; empty when the draft does not build.
    pub fn curl(&self, options: &TransportOptions) -> String {
        self.request()
            .map(|request| to_curl(request, options))
            .unwrap_or_default()
    }
}

/// Build a draft. Without a context, the draft's flat auth fields apply.
pub fn prepare(draft: &Draft, ctx: Option<&ResolvedRequestContext>) -> Prepared {
    Prepared {
        request: build_request(draft, ctx.map(RequestContext::Resolved)),
        auth_type: ctx.map(|ctx| auth_type(&ctx.auth)),
    }
}

/// The URL and headers to connect with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketRequest {
    pub url: String,
    pub headers: Vec<Header>,
}

/// Replace a leading `from` (any case) with `to`.
fn replace_scheme(url: &str, from: &str, to: &str) -> String {
    match url.get(..from.len()) {
        Some(head) if head.eq_ignore_ascii_case(from) => format!("{to}{}", &url[from.len()..]),
        _ => url.to_string(),
    }
}

/// The URL and headers to connect with, resolved as a send resolves them:
/// tokens, query rows, headers, and auth. Errors are messages for the user.
pub fn build_web_socket_request(
    draft: &Draft,
    ctx: Option<&ResolvedRequestContext>,
) -> Result<SocketRequest, String> {
    let trimmed = js_trim(&draft.url);
    let raw = match ctx {
        Some(ctx) => interpolate(trimmed, ctx)?,
        None => trimmed.to_string(),
    };
    if !is_web_socket_url(&raw) {
        return Err("Enter a ws:// or wss:// URL.".into());
    }
    // Build as an HTTP GET, which applies every rule, then restore the scheme.
    let http = Draft {
        url: replace_scheme(&raw, "ws", "http"),
        method: "GET".into(),
        body_mode: BodyMode::None,
        ..draft.clone()
    };
    let request = build_request(&http, ctx.map(RequestContext::Resolved))?;
    Ok(SocketRequest {
        url: replace_scheme(&request.url, "http", "ws"),
        headers: request
            .headers
            .into_iter()
            .filter(|header| {
                let key = header.key.to_ascii_lowercase();
                key != "accept" && key != "content-type"
            })
            .collect(),
    })
}

/// Everything a send or connect needs, from the session and its context.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedSend {
    pub ctx: ResolvedRequestContext,
    pub http: Prepared,
    pub socket: Result<SocketRequest, String>,
    /// A ws:// or wss:// URL makes this a WebSocket request.
    pub websocket: bool,
    pub options: TransportOptions,
}

impl PreparedSend {
    /// The validation message for the request kind; empty when it builds.
    pub fn validation_error(&self) -> &str {
        if self.websocket {
            self.socket.as_ref().err().map_or("", String::as_str)
        } else {
            self.http.error()
        }
    }
}

/// The resolved auth and tokens of a request from its group ancestry.
pub fn request_context(session: &RequestSession, sources: &TokenSources) -> ResolvedRequestContext {
    sources.request_context(session)
}

pub fn prepare_send(
    session: &RequestSession,
    sources: &TokenSources,
    preferences: &crate::model::WorkspacePreferences,
) -> PreparedSend {
    let ctx = request_context(session, sources);
    PreparedSend {
        http: prepare(&session.draft, Some(&ctx)),
        socket: build_web_socket_request(&session.draft, Some(&ctx)),
        websocket: is_web_socket_url(&session.draft.url),
        options: transport_options(preferences),
        ctx,
    }
}

/// What the Send (or Connect) button does now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryAction {
    /// Ask before sending to a protected environment; after the user
    /// confirms, call `Workspace::confirm_environment` and run again.
    ConfirmEnvironment,
    Send,
    Connect,
    Disconnect,
}

/// `environment` is the active environment of the request's root group;
/// `confirmed` is `Workspace::environment_confirmed`.
pub fn primary_action(
    session: &RequestSession,
    websocket: bool,
    environment: Option<&Environment>,
    confirmed: bool,
) -> PrimaryAction {
    let active = socket_active(session);
    let connecting = !websocket || !active;
    if connecting && environment.is_some_and(|e| e.protected == Some(true)) && !confirmed {
        return PrimaryAction::ConfirmEnvironment;
    }
    match (websocket, active) {
        (false, _) => PrimaryAction::Send,
        (true, true) => PrimaryAction::Disconnect,
        (true, false) => PrimaryAction::Connect,
    }
}

/// The protected-environment prompt text and its button label.
pub fn confirm_environment_prompt(
    environment: &Environment,
    websocket: bool,
) -> (String, &'static str) {
    let verb = if websocket { "Connect" } else { "Send" };
    (
        format!(
            "{} is protected. {verb} to {}?",
            environment.name, environment.name
        ),
        verb,
    )
}

// ── Staleness ───────────────────────────────────────────────────────────────

/// True when a response exists but the current fully resolved request
/// differs from what was sent.
pub fn is_stale(session: &RequestSession, prepared: &Prepared) -> bool {
    session.response.is_some() && session.sent_fingerprint != prepared.fingerprint()
}

/// Update `session.stale` from the current draft and context. Returns it.
pub fn refresh_stale(session: &mut RequestSession, sources: &TokenSources) -> bool {
    session.stale = session_is_stale(session, sources);
    session.stale
}

/// Whether `session` is stale under `sources`. Pass sources built with
/// `ignoring_max_age`, so an expired value does not mark it edited.
pub fn session_is_stale(session: &RequestSession, sources: &TokenSources) -> bool {
    let ctx = request_context(session, sources);
    is_stale(session, &prepare(&session.draft, Some(&ctx)))
}

// ── Sending ─────────────────────────────────────────────────────────────────

/// A running send. Owns the event stream parser and the start time.
#[derive(Debug)]
pub struct SendTicket {
    pub request: RequestInput,
    pub options: TransportOptions,
    /// Epoch milliseconds.
    pub sent_at: f64,
    /// The URL actually sent, captured at send time so a later edit to the
    /// draft does not change the name suggested for a saved response body.
    pub sent_url: String,
    /// The previous response body, for the engine to release.
    pub released: Option<String>,
    started: Instant,
    parser: SseParser,
    /// The inspection limit in UTF-16 units, as the TS string length counts.
    limit: usize,
    /// `stream.text` length in UTF-16 units.
    text_units: usize,
}

impl SendTicket {
    /// Milliseconds since the send started.
    pub fn elapsed_ms(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }
}

/// Start a send. None when the session is busy or the draft does not build.
pub fn begin_send(
    session: &mut RequestSession,
    prepared: &Prepared,
    options: &TransportOptions,
) -> Option<SendTicket> {
    if session.busy {
        return None;
    }
    let request = prepared.request()?.clone();
    session.busy = true;
    session.error.clear();
    let released = session
        .response
        .take()
        .and_then(|response| response.body_id);
    session.test_results = None;
    session.capture_errors = None;
    session.stream = None;
    session.elapsed = 0.0;
    // Persist the resolved-request fingerprint so stale can compare accurately.
    session.sent_fingerprint = prepared.fingerprint();
    Some(SendTicket {
        sent_url: request.url.clone(),
        request,
        options: options.clone(),
        sent_at: now_ms(),
        released,
        started: Instant::now(),
        parser: SseParser::new(),
        limit: (options.inspection_limit_mi_b * MIB) as usize,
        text_units: 0,
    })
}

/// Update the elapsed time of a running send.
pub fn tick(session: &mut RequestSession, ticket: &SendTicket) {
    session.elapsed = ticket.elapsed_ms();
}

/// The longest prefix of `text` with at most `units` UTF-16 units, and its
/// length in units.
fn utf16_prefix(text: &str, units: usize) -> (&str, usize) {
    let mut count = 0;
    for (index, c) in text.char_indices() {
        let width = c.len_utf16();
        if count + width > units {
            return (&text[..index], count);
        }
        count += width;
    }
    (text, count)
}

/// Show a live event stream as it arrives.
pub fn apply_stream_message(
    session: &mut RequestSession,
    ticket: &mut SendTicket,
    message: StreamMessage,
) {
    match message {
        StreamMessage::Head {
            status,
            status_text,
            headers,
        } => {
            ticket.text_units = 0;
            session.stream = Some(LiveStream {
                status,
                status_text,
                headers: headers
                    .into_iter()
                    .map(|(key, value)| Header { key, value })
                    .collect(),
                events: Vec::new(),
                text: String::new(),
                bytes: 0,
                truncated: false,
            });
        }
        StreamMessage::Chunk { text } => {
            let Some(stream) = session.stream.as_mut() else {
                return;
            };
            stream.bytes += text.len() as u64;
            if ticket.text_units < ticket.limit {
                let (prefix, units) = utf16_prefix(&text, ticket.limit - ticket.text_units);
                stream.text.push_str(prefix);
                ticket.text_units += units;
            } else {
                stream.truncated = true;
            }
            let at = ticket.elapsed_ms().round();
            stream
                .events
                .extend(ticket.parser.push(&text).into_iter().map(|event| SseEvent {
                    at: Some(at),
                    ..event
                }));
            if stream.events.len() > STREAM_EVENT_LIMIT {
                let excess = stream.events.len() - STREAM_EVENT_LIMIT;
                stream.events.drain(..excess);
            }
        }
    }
}

/// What a finished send leaves for the caller.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SendOutcome {
    /// Token values from the captures; apply with `Workspace::capture`.
    pub captured: Definitions,
}

/// Apply the result of a send: the response or the error, history, checks,
/// and captures. Stopping a stream keeps what arrived. A cancel is not
/// recorded in history.
pub fn finish_send(
    session: &mut RequestSession,
    ticket: SendTicket,
    result: Result<ApiResponse, String>,
) -> SendOutcome {
    let mut outcome = SendOutcome::default();
    let entry_id = |session: &RequestSession| next_history_id(&session.history);
    match result {
        Ok(response) => {
            let entry = history_entry(
                entry_id(session),
                ticket.sent_at,
                &ticket.request,
                HistoryOutcome::Response(&response),
            );
            session.history = add_history(&session.history, entry);
            outcome.captured = run_checks(session, &response);
            session.response = Some(response);
        }
        Err(message) => {
            let duration_ms = ticket.elapsed_ms().round();
            match session.stream.take() {
                Some(stream) if message == CANCELED => {
                    let response = ApiResponse {
                        status: stream.status,
                        status_text: stream.status_text,
                        duration_ms,
                        headers: stream.headers,
                        body: stream.text,
                        size_bytes: stream.bytes,
                        body_id: None,
                        truncated: Some(stream.truncated),
                        binary: None,
                        final_url: None,
                        redirect_count: None,
                        timing: None,
                    };
                    let entry = history_entry(
                        entry_id(session),
                        ticket.sent_at,
                        &ticket.request,
                        HistoryOutcome::Response(&response),
                    );
                    session.history = add_history(&session.history, entry);
                    outcome.captured = run_checks(session, &response);
                    session.response = Some(response);
                }
                _ => session.error = message.clone(),
            }
            if message != CANCELED {
                let entry = history_entry(
                    entry_id(session),
                    ticket.sent_at,
                    &ticket.request,
                    HistoryOutcome::Error {
                        error: &message,
                        duration_ms,
                    },
                );
                session.history = add_history(&session.history, entry);
            }
        }
    }
    session.stream = None;
    session.busy = false;
    outcome
}

/// A result whose request no longer exists has no owner: the body id to
/// release.
pub fn orphaned_body(result: &Result<ApiResponse, String>) -> Option<String> {
    result
        .as_ref()
        .ok()
        .and_then(|response| response.body_id.clone())
}

/// Run the assertions and captures of the draft on `response`. Returns the
/// captured token values.
pub fn run_checks(session: &mut RequestSession, response: &ApiResponse) -> Definitions {
    let draft = &session.draft;
    if let Some(assertions) = &draft.assertions
        && assertions.iter().any(|row| row.enabled)
    {
        session.test_results = Some(run_assertions(assertions, response));
    }
    let mut captured = Definitions::new();
    if let Some(captures) = &session.draft.captures
        && captures.iter().any(|row| row.enabled)
    {
        let outcome = run_captures(captures, response);
        session.capture_errors = (!outcome.errors.is_empty()).then_some(outcome.errors);
        captured = outcome.values;
    }
    captured
}

/// Run the checks again on the shown response.
pub fn recheck(session: &mut RequestSession) -> Definitions {
    match (&session.response, session.busy) {
        (Some(response), false) => {
            let response = response.clone();
            run_checks(session, &response)
        }
        _ => Definitions::new(),
    }
}

// ── WebSocket ───────────────────────────────────────────────────────────────

pub fn socket_state(session: &RequestSession) -> SocketState {
    session
        .socket
        .as_ref()
        .map_or(SocketState::Closed, |socket| socket.state)
}

/// Open or connecting.
pub fn socket_active(session: &RequestSession) -> bool {
    websocket_log::is_active(session.socket.as_ref())
}

/// Start a connection. The log keeps the earlier messages. False when
/// already active; otherwise call `Engine::ws_connect` with `request` and
/// `options.connect_timeout_seconds`, and pass a connect error to
/// `apply_socket_event` as an error event.
pub fn connect_socket(session: &mut RequestSession, request: &SocketRequest) -> bool {
    if socket_active(session) {
        return false;
    }
    session.socket = Some(websocket_log::begin_connect(
        session.socket.take(),
        &request.url,
    ));
    true
}

fn log_event(event: EngineSocketEvent) -> SocketEvent {
    match event {
        EngineSocketEvent::Open { status, headers } => SocketEvent::Open { status, headers },
        EngineSocketEvent::Message { text, binary, size } => SocketEvent::Message {
            text,
            binary,
            size: size as u64,
        },
        EngineSocketEvent::Close { code, reason } => SocketEvent::Close { code, reason },
        EngineSocketEvent::Error { message } => SocketEvent::Error { message },
    }
}

/// Apply a connection event. True when the connection ended, so the caller
/// drops its connection id.
pub fn apply_socket_event(session: &mut RequestSession, event: EngineSocketEvent) -> bool {
    match session.socket.as_mut() {
        Some(socket) => websocket_log::apply_event(socket, log_event(event)),
        None => false,
    }
}

/// Start closing. True when the caller should call `Engine::ws_close`;
/// `connected` is whether the caller holds a connection id.
pub fn disconnect_socket(session: &mut RequestSession, connected: bool) -> bool {
    match session.socket.as_mut() {
        Some(socket) if connected => {
            websocket_log::begin_disconnect(socket);
            true
        }
        _ => false,
    }
}

/// Sending needs an open connection and some text.
pub fn can_send_socket(session: &RequestSession, connected: bool, text: &str) -> bool {
    connected && websocket_log::can_send(session.socket.as_ref(), text)
}

/// Log the result of `Engine::ws_send`. A sent message clears the message
/// box (the draft body). True when sent.
pub fn socket_sent(session: &mut RequestSession, text: &str, result: Result<(), String>) -> bool {
    let sent = result.is_ok();
    if let Some(socket) = session.socket.as_mut() {
        websocket_log::log_send(socket, text, result);
    }
    if sent {
        session.draft.body.clear();
    }
    sent
}

pub fn clear_socket(session: &mut RequestSession) {
    if let Some(socket) = session.socket.as_mut() {
        websocket_log::clear(socket);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{create_assertion, create_capture};
    use crate::environments::create_environment;
    use crate::groups::create_group;
    use crate::interpolation::InterpolationContext;
    use crate::model::{
        AuthorizationConfig, CheckOperator, CheckSource, EnvironmentColor, SocketDirection,
    };
    use crate::request::{create_draft, pair};
    use crate::session::{create_session, draft_fingerprint};

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn make_ctx(bearer: bool) -> ResolvedRequestContext {
        ResolvedRequestContext {
            auth: if bearer {
                AuthorizationConfig::Bearer {
                    token: "tok".into(),
                }
            } else {
                AuthorizationConfig::None
            },
            tokens: InterpolationContext::default(),
        }
    }

    fn session(url: &str) -> RequestSession {
        let mut session = create_session(None);
        session.draft.url = url.into();
        session
    }

    fn ok(body: &str) -> ApiResponse {
        ApiResponse {
            status: 200,
            status_text: "OK".into(),
            duration_ms: 5.0,
            headers: vec![],
            body: body.into(),
            size_bytes: body.len() as u64,
            body_id: None,
            truncated: None,
            binary: None,
            final_url: None,
            redirect_count: None,
            timing: None,
        }
    }

    fn has_auth(prepared: &Prepared) -> bool {
        prepared
            .request()
            .is_some_and(|r| r.headers.iter().any(|h| h.key == "Authorization"))
    }

    /// Send `session` and finish with `result`.
    fn send(session: &mut RequestSession, result: Result<ApiResponse, String>) -> SendOutcome {
        let prepared = prepare(&session.draft, None);
        let ticket = begin_send(session, &prepared, &TransportOptions::default()).unwrap();
        finish_send(session, ticket, result)
    }

    // ── context ─────────────────────────────────────────────────────────────

    #[test]
    fn works_without_a_context() {
        let session = session("https://example.test");
        let prepared = prepare(&session.draft, None);
        assert_eq!(prepared.error(), "");
        assert!(prepared.request().is_some());
    }

    #[test]
    fn the_context_decides_the_auth_header_and_curl() {
        let session = session("https://example.test");
        let none = prepare(&session.draft, Some(&make_ctx(false)));
        assert!(!has_auth(&none));
        let options = TransportOptions::default();
        assert!(!none.curl(&options).contains("Authorization"));
        let bearer = prepare(&session.draft, Some(&make_ctx(true)));
        assert!(has_auth(&bearer));
        assert!(bearer.curl(&options).contains("Authorization"));
    }

    #[test]
    fn prepare_send_resolves_group_auth_tokens_and_options() {
        let mut group = create_group("API", None);
        group.local_auth = Some(AuthorizationConfig::Bearer { token: "g".into() });
        group.local_definitions = Some(defs(&[("path", "widgets")]));
        let mut session = session("https://example.test/{{path}}");
        session.group_id = Some(group.id);
        let mut preferences = crate::preferences::default_preferences();
        preferences.transport.timeout_seconds = 7;
        let prepared = prepare_send(
            &session,
            &TokenSources::text(&[group], &Definitions::new()),
            &preferences,
        );
        let request = prepared.http.request().unwrap();
        assert_eq!(request.url, "https://example.test/widgets");
        assert!(has_auth(&prepared.http));
        assert!(!prepared.websocket);
        assert_eq!(prepared.options.timeout_seconds, 7);
        assert_eq!(prepared.validation_error(), "");
    }

    // ── staleness ───────────────────────────────────────────────────────────

    #[test]
    fn a_group_auth_change_marks_the_response_stale() {
        let mut session = session("https://example.test");
        let sent = prepare(&session.draft, Some(&make_ctx(false)));
        session.sent_fingerprint = request_fingerprint(sent.request(), Some("none"));
        session.response = Some(ok("{}"));
        assert!(!is_stale(&session, &sent));
        let now = prepare(&session.draft, Some(&make_ctx(true)));
        assert!(is_stale(&session, &now));
    }

    #[test]
    fn stale_is_false_with_no_response() {
        let session = session("https://example.test");
        assert!(!is_stale(
            &session,
            &prepare(&session.draft, Some(&make_ctx(false)))
        ));
    }

    #[test]
    fn refresh_stale_follows_the_groups() {
        let mut group = create_group("API", None);
        let mut session = session("https://example.test");
        session.group_id = Some(group.id);
        let globals = Definitions::new();
        let prepared = prepare_send(
            &session,
            &TokenSources::text(std::slice::from_ref(&group), &globals),
            &Default::default(),
        );
        let ticket = begin_send(&mut session, &prepared.http, &prepared.options).unwrap();
        finish_send(&mut session, ticket, Ok(ok("{}")));
        assert!(!refresh_stale(
            &mut session,
            &TokenSources::text(std::slice::from_ref(&group), &globals)
        ));
        group.local_auth = Some(AuthorizationConfig::Bearer { token: "g".into() });
        assert!(refresh_stale(
            &mut session,
            &TokenSources::text(&[group], &globals)
        ));
        assert!(session.stale);
    }

    /// A workspace where `dependent` reads `{{t}}` from `login`'s response.
    fn token_workspace() -> (crate::workspace_state::Workspace, u64, u64) {
        use crate::model::{CheckSource, ResponseToken};
        use crate::workspace_state::Workspace;

        let mut workspace = Workspace::new();
        let login = workspace.sessions[0].id;
        let group_id = workspace.add_group("API", None);
        workspace.move_request(login, Some(group_id));
        workspace.sessions[0].draft.url = "https://api.test/login".into();
        let mut dependent = crate::session::create_session(None);
        dependent.group_id = Some(group_id);
        dependent.draft.url = "https://api.test/me?t={{t}}".into();
        let dependent_id = dependent.id;
        workspace.sessions.push(dependent);
        workspace.groups[0].response_tokens = Some(vec![ResponseToken {
            id: 1,
            name: "t".into(),
            request_id: login,
            source: CheckSource::Json,
            path: ".t".into(),
            max_age_secs: None,
        }]);
        (workspace, login, dependent_id)
    }

    fn record_token(workspace: &mut crate::workspace_state::Workspace, login: u64, value: &str) {
        use crate::model::CheckSource;
        use crate::response_token_cache::ValueKey;
        let fingerprint = workspace
            .token_sources(0.0)
            .fingerprint(workspace.session(login).unwrap());
        workspace.response_cache.record(
            login,
            &fingerprint,
            0,
            vec![(
                ValueKey {
                    source: CheckSource::Json,
                    path: ".t".into(),
                },
                value.into(),
            )],
        );
    }

    /// Mark `dependent` as sent with its current resolved request.
    fn mark_sent(workspace: &mut crate::workspace_state::Workspace, dependent_id: u64) {
        let sources = workspace.token_sources(0.0);
        let prepared = prepare_send(
            workspace.session(dependent_id).unwrap(),
            &sources,
            &workspace.preferences,
        );
        assert_eq!(prepared.http.error(), "");
        let fingerprint = prepared.http.fingerprint();
        let session = workspace.session_mut(dependent_id).unwrap();
        session.sent_fingerprint = fingerprint;
        session.response = Some(ok("{}"));
    }

    #[test]
    fn an_expired_value_does_not_mark_the_dependent_stale() {
        let (mut workspace, login, dependent_id) = token_workspace();
        workspace.groups[0].response_tokens.as_mut().unwrap()[0].max_age_secs = Some(1);
        // Fetched at time 0 and sent fresh, then a second passes.
        record_token(&mut workspace, login, "one");
        mark_sent(&mut workspace, dependent_id);
        workspace.refresh_all_stale();
        assert!(!workspace.session(dependent_id).unwrap().stale);
        assert!(!workspace.refresh_stale(dependent_id));
    }

    #[test]
    fn a_new_cached_value_marks_the_dependent_stale() {
        let (mut workspace, login, dependent_id) = token_workspace();
        record_token(&mut workspace, login, "one");
        mark_sent(&mut workspace, dependent_id);
        assert!(!workspace.refresh_stale(dependent_id));
        record_token(&mut workspace, login, "two");
        assert!(workspace.refresh_stale(dependent_id));
    }

    #[test]
    fn is_token_source_finds_group_and_global_sources() {
        let (mut workspace, login, dependent_id) = token_workspace();
        assert!(workspace.is_token_source(login));
        assert!(!workspace.is_token_source(dependent_id));
        workspace.groups[0].response_tokens = None;
        assert!(!workspace.is_token_source(login));
        workspace
            .global_response_tokens
            .push(crate::model::ResponseToken {
                id: 2,
                name: "g".into(),
                request_id: dependent_id,
                source: crate::model::CheckSource::Json,
                path: ".g".into(),
                max_age_secs: None,
            });
        assert!(workspace.is_token_source(dependent_id));
    }

    #[test]
    fn editing_a_source_request_marks_the_dependent_stale() {
        let (mut workspace, login, dependent_id) = token_workspace();
        record_token(&mut workspace, login, "one");
        mark_sent(&mut workspace, dependent_id);
        workspace.refresh_all_stale();
        assert!(!workspace.session(dependent_id).unwrap().stale);
        workspace.session_mut(login).unwrap().draft.url = "https://api.test/login2".into();
        workspace.refresh_all_stale();
        assert!(workspace.session(dependent_id).unwrap().stale);
    }

    #[test]
    fn begin_send_stores_the_resolved_fingerprint_not_the_draft_one() {
        let mut session = session("https://example.test");
        let prepared = prepare(&session.draft, Some(&make_ctx(true)));
        begin_send(&mut session, &prepared, &TransportOptions::default()).unwrap();
        assert!(session.sent_fingerprint.contains("Authorization"));
        assert_ne!(session.sent_fingerprint, draft_fingerprint(&session.draft));
    }

    // ── sending ─────────────────────────────────────────────────────────────

    #[test]
    fn begin_send_captures_the_resolved_url() {
        let mut session = session("https://example.test/{{path}}");
        let ctx = ResolvedRequestContext {
            auth: AuthorizationConfig::None,
            tokens: InterpolationContext::local(defs(&[("path", "widgets")])),
        };
        let prepared = prepare(&session.draft, Some(&ctx));
        let ticket = begin_send(&mut session, &prepared, &TransportOptions::default()).unwrap();
        assert_eq!(ticket.sent_url, "https://example.test/widgets");
        assert_eq!(ticket.sent_url, prepared.request().unwrap().url);
    }

    #[test]
    fn begin_send_uses_the_options_and_releases_the_previous_body() {
        let mut session = session("https://example.test/");
        let mut previous = ok("");
        previous.body_id = Some("old".into());
        session.response = Some(previous);
        session.error = "x".into();
        let options = TransportOptions {
            timeout_seconds: 7,
            ..TransportOptions::default()
        };
        let prepared = prepare(&session.draft, None);
        let ticket = begin_send(&mut session, &prepared, &options).unwrap();
        assert_eq!(ticket.released.as_deref(), Some("old"));
        assert_eq!(ticket.options, options);
        assert!(session.busy);
        assert!(session.response.is_none());
        assert_eq!(session.error, "");
        // A second send waits for the first.
        assert!(begin_send(&mut session, &prepared, &options).is_none());
    }

    #[test]
    fn begin_send_needs_a_request_that_builds() {
        let mut session = session("");
        let prepared = prepare(&session.draft, None);
        assert_ne!(prepared.error(), "");
        assert!(begin_send(&mut session, &prepared, &TransportOptions::default()).is_none());
        assert!(!session.busy);
    }

    #[test]
    fn a_result_without_an_owner_is_released() {
        let mut late = ok("");
        late.body_id = Some("late".into());
        assert_eq!(orphaned_body(&Ok(late)).as_deref(), Some("late"));
        assert_eq!(orphaned_body(&Err("Offline".into())), None);
    }

    #[test]
    fn cancel_reports_the_canceled_send() {
        let mut session = session("https://example.test/");
        send(&mut session, Err(CANCELED.into()));
        assert!(!session.busy);
        assert_eq!(session.error, "Request canceled.");
    }

    #[test]
    fn the_sent_request_is_kept_when_the_draft_changes_during_a_send() {
        let mut session = session("https://example.test/a");
        let prepared = prepare(&session.draft, None);
        let ticket = begin_send(&mut session, &prepared, &TransportOptions::default()).unwrap();
        session.draft.url = "https://example.test/b".into();
        assert_eq!(ticket.request.url, "https://example.test/a");
        finish_send(&mut session, ticket, Ok(ok("")));
        assert!(is_stale(&session, &prepare(&session.draft, None)));
    }

    #[test]
    fn history_records_each_send_newest_first_but_not_a_cancel() {
        let mut session = session("https://example.test/a");
        send(&mut session, Ok(ok("{}")));
        send(&mut session, Ok(ok("{}")));
        let ids: Vec<u64> = session.history.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![2, 1]);
        let newest = &session.history[0];
        assert_eq!(newest.method, "GET");
        assert_eq!(newest.url, "https://example.test/a");
        assert_eq!(newest.status, Some(200));
        send(&mut session, Err(CANCELED.into()));
        send(&mut session, Err("Offline".into()));
        let errors: Vec<Option<&str>> =
            session.history.iter().map(|e| e.error.as_deref()).collect();
        assert_eq!(errors, vec![Some("Offline"), None, None]);
        assert_eq!(session.error, "Offline");
    }

    #[test]
    fn checks_run_assertions_and_return_captures() {
        let mut session = session("https://example.test/a");
        session.draft.assertions = Some(vec![
            create_assertion(CheckSource::Status, CheckOperator::Equals, "200", ""),
            create_assertion(CheckSource::Json, CheckOperator::Equals, "abc", ".token"),
        ]);
        session.draft.captures = Some(vec![create_capture("token", CheckSource::Json, ".token")]);
        let outcome = send(&mut session, Ok(ok(r#"{"token":"abc"}"#)));
        let passes: Vec<bool> = session
            .test_results
            .as_ref()
            .unwrap()
            .iter()
            .map(|r| r.pass)
            .collect();
        assert_eq!(passes, vec![true, true]);
        assert_eq!(outcome.captured, defs(&[("token", "abc")]));
        assert!(session.capture_errors.is_none());
        session.draft.assertions.as_mut().unwrap()[0].expected = "201".into();
        recheck(&mut session);
        assert!(!session.test_results.as_ref().unwrap()[0].pass);
    }

    #[test]
    fn capture_problems_are_kept_on_the_session() {
        let mut session = session("https://example.test/a");
        session.draft.captures = Some(vec![create_capture("token", CheckSource::Json, ".missing")]);
        let outcome = send(&mut session, Ok(ok("{}")));
        assert!(outcome.captured.is_empty());
        assert_eq!(session.capture_errors.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn recheck_waits_for_a_response_and_an_idle_session() {
        let mut session = session("https://example.test/a");
        session.draft.assertions = Some(vec![create_assertion(
            CheckSource::Status,
            CheckOperator::Equals,
            "200",
            "",
        )]);
        recheck(&mut session);
        assert!(session.test_results.is_none());
        session.response = Some(ok(""));
        session.busy = true;
        recheck(&mut session);
        assert!(session.test_results.is_none());
    }

    // ── event streams ───────────────────────────────────────────────────────

    fn head() -> StreamMessage {
        StreamMessage::Head {
            status: 200,
            status_text: "OK".into(),
            headers: vec![("content-type".into(), "text/event-stream".into())],
        }
    }

    fn chunk(text: &str) -> StreamMessage {
        StreamMessage::Chunk { text: text.into() }
    }

    #[test]
    fn live_events_show_and_stay_when_canceled() {
        let mut session = session("https://example.test/sse");
        let prepared = prepare(&session.draft, None);
        let mut ticket = begin_send(&mut session, &prepared, &TransportOptions::default()).unwrap();
        apply_stream_message(&mut session, &mut ticket, head());
        apply_stream_message(&mut session, &mut ticket, chunk("data: 1\n\nda"));
        apply_stream_message(&mut session, &mut ticket, chunk("ta: 2\n\n"));
        let stream = session.stream.as_ref().unwrap();
        let data: Vec<&str> = stream.events.iter().map(|e| e.data.as_str()).collect();
        assert_eq!(data, vec!["1", "2"]);
        assert!(stream.events.iter().all(|e| e.at.is_some()));
        assert_eq!(stream.headers[0].key, "content-type");
        finish_send(&mut session, ticket, Err(CANCELED.into()));
        assert_eq!(session.error, "");
        assert!(session.stream.is_none());
        let response = session.response.as_ref().unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "data: 1\n\ndata: 2\n\n");
        assert_eq!(response.size_bytes, 18);
        assert_eq!(session.history[0].status, Some(200));
    }

    #[test]
    fn chunks_before_the_head_are_ignored() {
        let mut session = session("https://example.test/sse");
        let prepared = prepare(&session.draft, None);
        let mut ticket = begin_send(&mut session, &prepared, &TransportOptions::default()).unwrap();
        apply_stream_message(&mut session, &mut ticket, chunk("data: 1\n\n"));
        assert!(session.stream.is_none());
    }

    #[test]
    fn the_stream_text_stops_at_the_inspection_limit() {
        let mut session = session("https://example.test/sse");
        let prepared = prepare(&session.draft, None);
        let options = TransportOptions {
            inspection_limit_mi_b: 1,
            ..TransportOptions::default()
        };
        let mut ticket = begin_send(&mut session, &prepared, &options).unwrap();
        apply_stream_message(&mut session, &mut ticket, head());
        let big = "x".repeat(MIB as usize - 1);
        apply_stream_message(&mut session, &mut ticket, chunk(&big));
        apply_stream_message(&mut session, &mut ticket, chunk("éé"));
        let stream = session.stream.as_ref().unwrap();
        assert_eq!(stream.text.len(), MIB as usize + 1);
        assert!(stream.text.ends_with('é'));
        assert!(!stream.truncated);
        apply_stream_message(&mut session, &mut ticket, chunk("more"));
        let stream = session.stream.as_ref().unwrap();
        assert!(stream.truncated);
        assert_eq!(stream.bytes, MIB - 1 + 4 + 4);
    }

    #[test]
    fn a_live_stream_keeps_the_newest_events() {
        let mut session = session("https://example.test/sse");
        let prepared = prepare(&session.draft, None);
        let mut ticket = begin_send(&mut session, &prepared, &TransportOptions::default()).unwrap();
        apply_stream_message(&mut session, &mut ticket, head());
        let text: String = (0..STREAM_EVENT_LIMIT + 3)
            .map(|n| format!("data: {n}\n\n"))
            .collect();
        apply_stream_message(&mut session, &mut ticket, chunk(&text));
        let events = &session.stream.as_ref().unwrap().events;
        assert_eq!(events.len(), STREAM_EVENT_LIMIT);
        assert_eq!(events[0].data, "3");
    }

    // ── protected environments ──────────────────────────────────────────────

    #[test]
    fn a_protected_environment_asks_before_a_send_or_connect() {
        let mut prod = create_environment("Prod", EnvironmentColor::Destructive);
        prod.protected = Some(true);
        let mut session = session("https://example.test");
        assert_eq!(
            primary_action(&session, false, Some(&prod), false),
            PrimaryAction::ConfirmEnvironment
        );
        assert_eq!(
            primary_action(&session, false, Some(&prod), true),
            PrimaryAction::Send
        );
        assert_eq!(
            primary_action(&session, false, None, false),
            PrimaryAction::Send
        );
        assert_eq!(
            primary_action(&session, true, Some(&prod), false),
            PrimaryAction::ConfirmEnvironment
        );
        // Disconnecting never asks.
        session.draft.url = "ws://chat.test".into();
        let request = build_web_socket_request(&session.draft, None).unwrap();
        connect_socket(&mut session, &request);
        assert_eq!(
            primary_action(&session, true, Some(&prod), false),
            PrimaryAction::Disconnect
        );
        let (prompt, button) = confirm_environment_prompt(&prod, false);
        assert_eq!(prompt, "Prod is protected. Send to Prod?");
        assert_eq!(button, "Send");
        assert_eq!(confirm_environment_prompt(&prod, true).1, "Connect");
    }

    // ── WebSocket ───────────────────────────────────────────────────────────

    #[test]
    fn web_socket_request_resolves_tokens_query_headers_and_auth() {
        let draft = Draft {
            url: "{{base}}/chat".into(),
            query: vec![pair("room", "1")],
            headers: vec![pair("Accept", "application/json"), pair("X-Id", "{{id}}")],
            local_auth: Some(AuthorizationConfig::Bearer { token: "t".into() }),
            ..create_draft()
        };
        let ctx = ResolvedRequestContext {
            auth: AuthorizationConfig::Bearer { token: "t".into() },
            tokens: InterpolationContext::local(defs(&[("base", "wss://chat.test"), ("id", "7")])),
        };
        let request = build_web_socket_request(&draft, Some(&ctx)).unwrap();
        assert_eq!(request.url, "wss://chat.test/chat?room=1");
        assert_eq!(
            request.headers,
            vec![
                Header {
                    key: "X-Id".into(),
                    value: "7".into()
                },
                Header {
                    key: "Authorization".into(),
                    value: "Bearer t".into()
                },
            ]
        );
    }

    #[test]
    fn web_socket_request_rejects_other_schemes() {
        let draft = Draft {
            url: "https://a.test".into(),
            ..create_draft()
        };
        let error = build_web_socket_request(&draft, None).unwrap_err();
        assert!(error.contains("ws://"));
        let prepared = prepare_send(
            &session("https://a.test"),
            &TokenSources::text(&[], &Definitions::new()),
            &Default::default(),
        );
        assert!(!prepared.websocket);
        assert_eq!(prepared.validation_error(), "");
    }

    #[test]
    fn connects_logs_messages_both_ways_and_closes() {
        let mut session = session("ws://chat.test");
        let prepared = prepare_send(
            &session,
            &TokenSources::text(&[], &Definitions::new()),
            &Default::default(),
        );
        assert!(prepared.websocket);
        let request = prepared.socket.unwrap();
        assert!(connect_socket(&mut session, &request));
        assert_eq!(socket_state(&session), SocketState::Connecting);
        assert!(!connect_socket(&mut session, &request));
        assert!(!can_send_socket(&session, true, "early"));
        assert!(!apply_socket_event(
            &mut session,
            EngineSocketEvent::Open {
                status: 101,
                headers: vec![("x-a".into(), "1".into())],
            },
        ));
        assert_eq!(socket_state(&session), SocketState::Open);
        assert_eq!(
            session.socket.as_ref().unwrap().headers,
            vec![Header {
                key: "x-a".into(),
                value: "1".into()
            }]
        );
        assert!(can_send_socket(&session, true, "hi"));
        assert!(!can_send_socket(&session, true, ""));
        assert!(!can_send_socket(&session, false, "hi"));
        session.draft.body = "hi".into();
        assert!(socket_sent(&mut session, "hi", Ok(())));
        assert_eq!(session.draft.body, "");
        apply_socket_event(
            &mut session,
            EngineSocketEvent::Message {
                text: "yo".into(),
                binary: false,
                size: 2,
            },
        );
        assert!(disconnect_socket(&mut session, true));
        assert_eq!(socket_state(&session), SocketState::Closing);
        assert!(apply_socket_event(
            &mut session,
            EngineSocketEvent::Close {
                code: Some(1000),
                reason: "bye".into(),
            },
        ));
        assert_eq!(socket_state(&session), SocketState::Closed);
        let log: Vec<(SocketDirection, &str)> = session
            .socket
            .as_ref()
            .unwrap()
            .messages
            .iter()
            .map(|m| (m.direction, m.text.as_str()))
            .collect();
        assert_eq!(
            log,
            vec![
                (SocketDirection::System, "Connecting to ws://chat.test"),
                (SocketDirection::System, "Connected · 101"),
                (SocketDirection::Out, "hi"),
                (SocketDirection::In, "yo"),
                (SocketDirection::System, "Closed · 1000 · bye"),
            ]
        );
        clear_socket(&mut session);
        assert!(session.socket.as_ref().unwrap().messages.is_empty());
    }

    #[test]
    fn a_failed_socket_send_is_logged_and_keeps_the_text() {
        let mut session = session("ws://chat.test");
        let request = build_web_socket_request(&session.draft, None).unwrap();
        connect_socket(&mut session, &request);
        session.draft.body = "hi".into();
        assert!(!socket_sent(
            &mut session,
            "hi",
            Err("Not connected.".into())
        ));
        assert_eq!(session.draft.body, "hi");
        let last = session.socket.as_ref().unwrap().messages.last().unwrap();
        assert_eq!(last.direction, SocketDirection::System);
        assert_eq!(last.text, "Not connected.");
    }

    #[test]
    fn a_reconnect_keeps_the_earlier_log() {
        let mut session = session("ws://chat.test");
        let request = build_web_socket_request(&session.draft, None).unwrap();
        connect_socket(&mut session, &request);
        apply_socket_event(
            &mut session,
            EngineSocketEvent::Error {
                message: "Connection failed.".into(),
            },
        );
        assert!(!socket_active(&session));
        assert!(!disconnect_socket(&mut session, false));
        connect_socket(&mut session, &request);
        assert_eq!(session.socket.as_ref().unwrap().messages.len(), 3);
    }
}
