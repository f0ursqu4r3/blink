//! Shared data types. Field names and optionality match the saved workspace
//! JSON (camelCase, absent optional fields) so snapshots stay compatible.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Token definitions: name to value, in insertion order as a JS object keeps
/// them, so token tables list rows as the user entered them.
pub type Definitions = IndexMap<String, String>;

/// Common methods, offered as suggestions. Any HTTP token is valid.
pub const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// One editable key/value row: query, header, form, or multipart part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pair {
    pub id: u64,
    pub key: String,
    pub value: String,
    pub enabled: bool,
    /// Multipart rows only: `value` is the path of a picked file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BodyMode {
    #[default]
    None,
    Json,
    Text,
    Graphql,
    Form,
    Multipart,
    File,
}

impl BodyMode {
    pub const ALL: [BodyMode; 7] = [
        BodyMode::None,
        BodyMode::Json,
        BodyMode::Text,
        BodyMode::Graphql,
        BodyMode::Form,
        BodyMode::Multipart,
        BodyMode::File,
    ];

    pub fn id(self) -> &'static str {
        match self {
            BodyMode::None => "none",
            BodyMode::Json => "json",
            BodyMode::Text => "text",
            BodyMode::Graphql => "graphql",
            BodyMode::Form => "form",
            BodyMode::Multipart => "multipart",
            BodyMode::File => "file",
        }
    }

    pub fn from_id(id: &str) -> Option<BodyMode> {
        BodyMode::ALL.into_iter().find(|mode| mode.id() == id)
    }
}

/// The legacy flat auth selector kept on every draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    #[default]
    None,
    Bearer,
    Basic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthorizationConfig {
    None,
    Bearer { token: String },
    Basic { username: String, password: String },
}

impl AuthorizationConfig {
    pub fn kind(&self) -> AuthKind {
        match self {
            AuthorizationConfig::None => AuthKind::None,
            AuthorizationConfig::Bearer { .. } => AuthKind::Bearer,
            AuthorizationConfig::Basic { .. } => AuthKind::Basic,
        }
    }
}

/// What a check reads from the response. `Json` runs a jq expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckSource {
    Status,
    Time,
    Size,
    Header,
    Json,
    Body,
}

impl CheckSource {
    pub const ALL: [CheckSource; 6] = [
        CheckSource::Status,
        CheckSource::Time,
        CheckSource::Size,
        CheckSource::Header,
        CheckSource::Json,
        CheckSource::Body,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CheckSource::Status => "Status",
            CheckSource::Time => "Time (ms)",
            CheckSource::Size => "Size (bytes)",
            CheckSource::Header => "Header",
            CheckSource::Json => "JSON (jq)",
            CheckSource::Body => "Body text",
        }
    }

    /// Sources that take a path: a header name or a jq expression.
    pub fn takes_path(self) -> bool {
        matches!(self, CheckSource::Header | CheckSource::Json)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckOperator {
    Equals,
    NotEquals,
    Lt,
    Lte,
    Gt,
    Gte,
    Contains,
    NotContains,
    Matches,
    Exists,
    NotExists,
}

impl CheckOperator {
    pub const ALL: [CheckOperator; 11] = [
        CheckOperator::Equals,
        CheckOperator::NotEquals,
        CheckOperator::Lt,
        CheckOperator::Lte,
        CheckOperator::Gt,
        CheckOperator::Gte,
        CheckOperator::Contains,
        CheckOperator::NotContains,
        CheckOperator::Matches,
        CheckOperator::Exists,
        CheckOperator::NotExists,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CheckOperator::Equals => "equals",
            CheckOperator::NotEquals => "does not equal",
            CheckOperator::Lt => "<",
            CheckOperator::Lte => "≤",
            CheckOperator::Gt => ">",
            CheckOperator::Gte => "≥",
            CheckOperator::Contains => "contains",
            CheckOperator::NotContains => "does not contain",
            CheckOperator::Matches => "matches regex",
            CheckOperator::Exists => "exists",
            CheckOperator::NotExists => "does not exist",
        }
    }

    /// Operators that take no expected value.
    pub fn is_unary(self) -> bool {
        matches!(self, CheckOperator::Exists | CheckOperator::NotExists)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assertion {
    pub id: u64,
    pub enabled: bool,
    pub source: CheckSource,
    /// Header name or jq expression.
    pub path: String,
    pub operator: CheckOperator,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssertionResult {
    pub id: u64,
    pub pass: bool,
    /// The value read, or an error.
    pub actual: String,
    pub description: String,
}

/// Saves a response value as a workspace token after each successful send.
/// `source` is never `Time` or `Size`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub id: u64,
    pub enabled: bool,
    /// Token name, used as {{name}}.
    pub name: String,
    pub source: CheckSource,
    pub path: String,
}

/// A token whose value comes from the last 2xx response of another request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseToken {
    pub id: u64,
    /// Token name, used as {{name}}.
    pub name: String,
    /// The source request (session id).
    pub request_id: u64,
    pub source: CheckSource,
    /// jq expression or header name, as in captures.
    pub path: String,
    /// None: send again only when no usable value exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openapi_contract: Option<crate::openapi_contract::OpenApiContract>,
    pub method: String,
    pub url: String,
    pub query: Vec<Pair>,
    pub headers: Vec<Pair>,
    pub body_mode: BodyMode,
    pub body: String,
    /// GraphQL variables as JSON text. Used only in graphql body mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<String>,
    /// Rows for form and multipart body modes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<Vec<Pair>>,
    /// Path of the picked file for file body mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_file: Option<String>,
    pub auth: AuthKind,
    pub token: String,
    pub username: String,
    pub password: String,
    /// Structured local auth override. None = inherit from group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_auth: Option<AuthorizationConfig>,
    /// Checks run on each response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertions: Option<Vec<Assertion>>,
    /// Response values saved as workspace tokens after each send.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captures: Option<Vec<Capture>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartPart {
    pub key: String,
    pub value: String,
    pub file: bool,
}

/// What the transport sends: the draft after validation and interpolation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RequestInput {
    pub method: String,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Option<String>,
    /// Send this file as the body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_file: Option<String>,
    /// Send a multipart form. `body` is None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multipart: Option<Vec<MultipartPart>>,
}

/// Milliseconds in each request phase. Phases the transport cannot see are absent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResponseTiming {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_ms: Option<f64>,
    /// TCP setup; includes any TLS handshake when `tls_ms` is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_ms: Option<f64>,
    /// From the request start (or the connection) until the response headers.
    pub wait_ms: f64,
    pub download_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse {
    pub status: u16,
    pub status_text: String,
    pub duration_ms: f64,
    pub headers: Vec<Header>,
    /// Preview text: complete unless `truncated`; empty when `binary`.
    pub body: String,
    /// Full body size, not the preview size.
    pub size_bytes: u64,
    /// Stored raw body. Not saved in the workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary: Option<bool>,
    /// Set only when a redirect happened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<ResponseTiming>,
}

impl ApiResponse {
    pub fn is_truncated(&self) -> bool {
        self.truncated.unwrap_or(false)
    }

    pub fn is_binary(&self) -> bool {
        self.binary.unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentColor {
    Destructive,
    Warning,
    Success,
    Info,
    Keyword,
}

impl EnvironmentColor {
    pub const ALL: [EnvironmentColor; 5] = [
        EnvironmentColor::Destructive,
        EnvironmentColor::Warning,
        EnvironmentColor::Success,
        EnvironmentColor::Info,
        EnvironmentColor::Keyword,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EnvironmentColor::Destructive => "Red",
            EnvironmentColor::Warning => "Amber",
            EnvironmentColor::Success => "Green",
            EnvironmentColor::Info => "Blue",
            EnvironmentColor::Keyword => "Purple",
        }
    }
}

/// A named set of token values on a root group. A value here replaces the
/// group's base token of the same name while the environment is active.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub id: u64,
    pub name: String,
    pub color: EnvironmentColor,
    /// Ask before the first send after switching to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected: Option<bool>,
    pub values: Definitions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestGroup {
    pub id: u64,
    pub name: String,
    pub parent_id: Option<u64>,
    pub collapsed: bool,
    /// Local auth override. None = inherit from parent group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_auth: Option<AuthorizationConfig>,
    /// Local token definitions for interpolation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_definitions: Option<Definitions>,
    /// Tokens read from responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_tokens: Option<Vec<ResponseToken>>,
    /// Defaults for new requests in this group. None inherits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_url: Option<String>,
    /// Root groups only: named token sets, switched in the Browser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environments: Option<Vec<Environment>>,
    /// The active environment id. None: base tokens only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_environment_id: Option<u64>,
}

/// One send of a request: its response or its error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: u64,
    /// Epoch milliseconds.
    pub sent_at: f64,
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub duration_ms: f64,
    pub size_bytes: u64,
    pub headers: Vec<Header>,
    /// Empty when `body_omitted`.
    pub body: String,
    /// The body was binary, truncated, or over the history limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_omitted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<ResponseTiming>,
}

/// How the pretty view shows a JSON body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JsonView {
    /// The collapsible tree.
    #[default]
    Tree,
    /// The indented text.
    Formatted,
}

impl JsonView {
    fn is_tree(&self) -> bool {
        *self == JsonView::Tree
    }
}

/// Per-request editor state that survives a restart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestView {
    /// One of query, headers, body, auth, tests.
    pub request_tab: String,
    /// One of body, headers, tests, events.
    pub response_tab: String,
    pub pretty: bool,
    /// The pretty view of JSON bodies. Snapshots leave out the default, so
    /// they stay as the TypeScript app wrote them.
    #[serde(default, skip_serializing_if = "JsonView::is_tree")]
    pub json_view: JsonView,
    pub wrap: bool,
    pub response_scroll: f64,
}

impl Default for RequestView {
    fn default() -> Self {
        RequestView {
            request_tab: "query".into(),
            response_tab: "body".into(),
            pretty: true,
            json_view: JsonView::Tree,
            wrap: false,
            response_scroll: 0.0,
        }
    }
}

/// One server-sent event.
#[derive(Debug, Clone, PartialEq)]
pub struct SseEvent {
    /// "message" when the stream sets no event name.
    pub event: String,
    pub data: String,
    pub id: Option<String>,
    /// Milliseconds from the request start, for live events.
    pub at: Option<f64>,
}

/// An event stream that is arriving now. Not saved.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveStream {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<Header>,
    pub events: Vec<SseEvent>,
    /// The body so far, up to the inspection limit.
    pub text: String,
    pub bytes: u64,
    pub truncated: bool,
}

/// A live stream keeps at most this many events; older ones drop.
pub const STREAM_EVENT_LIMIT: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    Connecting,
    Open,
    Closing,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketDirection {
    In,
    Out,
    System,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SocketMessage {
    pub id: u64,
    pub direction: SocketDirection,
    pub text: String,
    /// Epoch milliseconds.
    pub at: f64,
    pub binary: bool,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SocketSession {
    pub state: SocketState,
    pub messages: Vec<SocketMessage>,
    /// The upgrade response headers.
    pub headers: Vec<Header>,
}

/// The log keeps at most this many messages; older ones drop.
pub const SOCKET_MESSAGE_LIMIT: usize = 5000;

/// One request: the Browser tree owns it; a tab only references it.
#[derive(Debug, Clone, PartialEq)]
pub struct RequestSession {
    pub id: u64,
    pub group_id: Option<u64>,
    pub draft: Draft,
    pub response: Option<ApiResponse>,
    pub busy: bool,
    pub error: String,
    /// Milliseconds since the running send started.
    pub elapsed: f64,
    pub sent_fingerprint: String,
    pub view: RequestView,
    /// Past sends, newest first.
    pub history: Vec<HistoryEntry>,
    /// Assertion results for the shown response. Not saved.
    pub test_results: Option<Vec<AssertionResult>>,
    /// Capture problems from the last send. Not saved.
    pub capture_errors: Option<Vec<String>>,
    /// An event stream that is arriving now. Not saved.
    pub stream: Option<LiveStream>,
    /// The WebSocket connection and its log. Not saved.
    pub socket: Option<SocketSession>,
    /// The request changed since the shown response was sent. Not saved.
    pub stale: bool,
    /// The source request label while a response token dependency sends. Not saved.
    pub waiting_on: Option<String>,
}

impl RequestSession {
    /// Sending, or waiting for a source request before it sends. Either way
    /// Send is Cancel and the request cannot be deleted.
    pub fn running(&self) -> bool {
        self.busy || self.waiting_on.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PaneLayout {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CodeTarget {
    #[default]
    Curl,
    Fetch,
    Python,
    Go,
    Httpie,
    Rust,
}

impl CodeTarget {
    pub const ALL: [CodeTarget; 6] = [
        CodeTarget::Curl,
        CodeTarget::Fetch,
        CodeTarget::Python,
        CodeTarget::Go,
        CodeTarget::Httpie,
        CodeTarget::Rust,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CodeTarget::Curl => "cURL",
            CodeTarget::Fetch => "JavaScript fetch",
            CodeTarget::Python => "Python requests",
            CodeTarget::Go => "Go net/http",
            CodeTarget::Httpie => "HTTPie",
            CodeTarget::Rust => "Rust reqwest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportOptions {
    pub timeout_seconds: u64,
    pub connect_timeout_seconds: u64,
    pub follow_redirects: bool,
    pub max_redirects: u64,
    pub inspection_limit_mi_b: u64,
    /// Off accepts invalid and self-signed certificates.
    #[serde(default = "default_true")]
    pub verify_tls: bool,
    /// Empty uses the system proxy settings.
    #[serde(default)]
    pub proxy_url: String,
    /// Keep response cookies in a jar and send them.
    #[serde(default = "default_true")]
    pub store_cookies: bool,
}

/// Settings saved before these fields existed keep the safe defaults.
fn default_true() -> bool {
    true
}

impl Default for TransportOptions {
    fn default() -> Self {
        TransportOptions {
            timeout_seconds: 30,
            connect_timeout_seconds: 10,
            follow_redirects: false,
            max_redirects: 10,
            inspection_limit_mi_b: 4,
            verify_tls: true,
            proxy_url: String::new(),
            store_cookies: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePreferences {
    pub default_method: String,
    pub default_body_mode: BodyMode,
    pub pretty: bool,
    pub wrap: bool,
    pub confirm_close_drafts: bool,
    pub pane_layout: PaneLayout,
    /// Language in the code panel.
    pub code_target: CodeTarget,
    pub zoom: f64,
    #[serde(flatten)]
    pub transport: TransportOptions,
}

impl Default for WorkspacePreferences {
    fn default() -> Self {
        WorkspacePreferences {
            default_method: "GET".into(),
            default_body_mode: BodyMode::None,
            pretty: true,
            wrap: false,
            confirm_close_drafts: true,
            pane_layout: PaneLayout::Horizontal,
            code_target: CodeTarget::Curl,
            zoom: 1.0,
            transport: TransportOptions::default(),
        }
    }
}

/// An imported collection, before it becomes Browser groups.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportedGroup {
    pub name: String,
    pub definitions: Option<Definitions>,
    pub auth: Option<AuthorizationConfig>,
    pub groups: Vec<ImportedGroup>,
    pub requests: Vec<Draft>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    Openapi,
    Postman,
    Http,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportResult {
    pub format: ImportFormat,
    pub root: ImportedGroup,
    /// Items skipped, with the reason.
    pub skipped: Vec<String>,
}
