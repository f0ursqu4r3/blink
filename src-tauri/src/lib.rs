use std::collections::HashMap;
use std::io::Write;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use reqwest::{
    header::HeaderName, header::HeaderValue, multipart, redirect::Policy, Client, Method, Proxy,
};
use serde::{Deserialize, Serialize};

use request_files::FileGrants;
use response_store::{ResponseStore, STORE_ERROR};
use timing::{Phases, TimedConnectLayer, TimedResolver, Timing};
use reqwest_cookie_store::CookieStoreMutex;
use tokio::sync::oneshot;

const MIB: usize = 1024 * 1024;
const DOWNLOAD_LIMIT: u64 = 1024 * 1024 * 1024;
const UPLOAD_LIMIT: u64 = 1024 * 1024 * 1024;
const CANCELED: &str = "Request canceled.";

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestInput {
    method: String,
    url: String,
    headers: Vec<HeaderInput>,
    body: Option<String>,
    /// Send this file as the body. Replaces `body`.
    #[serde(default)]
    body_file: Option<String>,
    /// Send a multipart form. Replaces `body`.
    #[serde(default)]
    multipart: Option<Vec<MultipartInput>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MultipartInput {
    key: String,
    /// Text, or a file path when `file` is set.
    value: String,
    #[serde(default)]
    file: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct HeaderInput {
    key: String,
    value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResponseOutput {
    status: u16,
    status_text: String,
    duration_ms: u128,
    headers: Vec<HeaderInput>,
    body: String,
    size_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    body_id: Option<String>,
    truncated: bool,
    binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    final_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_count: Option<usize>,
    timing: Timing,
}

/// Sent to the webview while an event stream arrives.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StreamMessage {
    Head {
        status: u16,
        #[serde(rename = "statusText")]
        status_text: String,
        headers: Vec<(String, String)>,
    },
    Chunk {
        text: String,
    },
}

/// Receives stream messages. Tests pass a closure; the app sends to a channel.
type StreamSink<'a> = Option<&'a (dyn Fn(StreamMessage) + Send + Sync)>;

/// UTF-8 decoding across chunk boundaries: holds back a character that a
/// chunk cut in two.
#[derive(Default)]
struct StreamDecoder(Vec<u8>);

impl StreamDecoder {
    fn push(&mut self, bytes: &[u8]) -> String {
        self.0.extend_from_slice(bytes);
        let valid = match std::str::from_utf8(&self.0) {
            Ok(_) => self.0.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            // Invalid bytes: pass them on as replacement characters.
            Err(_) => self.0.len(),
        };
        let rest = self.0.split_off(valid);
        let text = String::from_utf8_lossy(&self.0).into_owned();
        self.0 = rest;
        text
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransportOptions {
    timeout_seconds: u64,
    connect_timeout_seconds: u64,
    follow_redirects: bool,
    max_redirects: usize,
    #[serde(rename = "inspectionLimitMiB")]
    inspection_limit_mib: usize,
    #[serde(default = "default_verify_tls")]
    verify_tls: bool,
    /// Empty uses the system proxy settings.
    #[serde(default)]
    proxy_url: String,
    /// Send and keep cookies with the shared jar.
    #[serde(default = "default_store_cookies")]
    store_cookies: bool,
}

fn default_store_cookies() -> bool {
    true
}

fn default_verify_tls() -> bool {
    true
}

impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            timeout_seconds: 30,
            connect_timeout_seconds: 10,
            follow_redirects: false,
            max_redirects: 10,
            inspection_limit_mib: 4,
            verify_tls: true,
            proxy_url: String::new(),
            store_cookies: true,
        }
    }
}

impl TransportOptions {
    /// The frontend checks the same ranges. Rust checks again because the
    /// webview is not the trust boundary for limits.
    fn validate(&self) -> Result<(), String> {
        let valid = (1..=600).contains(&self.timeout_seconds)
            && (1..=self.timeout_seconds).contains(&self.connect_timeout_seconds)
            && (1..=20).contains(&self.max_redirects)
            && (1..=16).contains(&self.inspection_limit_mib);
        if valid {
            Ok(())
        } else {
            Err("Invalid transport settings.".to_string())
        }
    }
}

/// Build a client for one request. `redirects` receives the hop count and
/// `phases` the phase times, so the client must not be shared between requests.
fn build_client(
    options: &TransportOptions,
    redirects: Arc<AtomicUsize>,
    phases: Arc<Phases>,
    jar: Option<Arc<CookieStoreMutex>>,
) -> Result<Client, String> {
    let policy = if options.follow_redirects {
        let max = options.max_redirects;
        Policy::custom(move |attempt| {
            // `previous` holds every URL already requested, so its length is
            // the number of this hop.
            let hops = attempt.previous().len();
            if hops > max {
                attempt.error(format!("Stopped after {max} redirects."))
            } else {
                redirects.store(hops, Ordering::Relaxed);
                attempt.follow()
            }
        })
    } else {
        Policy::none()
    };
    // The total timeout is applied around the send and the body read, so an
    // event stream can stay open after its headers arrive.
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(options.connect_timeout_seconds))
        .redirect(policy)
        .dns_resolver(Arc::new(TimedResolver(phases.clone())))
        .connector_layer(TimedConnectLayer(phases))
        .danger_accept_invalid_certs(!options.verify_tls);
    if let Some(jar) = jar {
        builder = builder.cookie_provider(jar);
    }
    let proxy_url = options.proxy_url.trim();
    if !proxy_url.is_empty() {
        let valid = reqwest::Url::parse(proxy_url)
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h"));
        if !valid {
            return Err("Invalid proxy URL.".to_string());
        }
        builder = builder.proxy(Proxy::all(proxy_url).map_err(|_| "Invalid proxy URL.")?);
    }
    builder
        .build()
        .map_err(|error| error.without_url().to_string())
}

/// Requests that are still running, by the id the frontend gave them.
#[derive(Default)]
struct InFlight(Mutex<HashMap<String, oneshot::Sender<()>>>);

impl InFlight {
    /// Run `task` until it finishes or `cancel` is called with `id`.
    /// Dropping the task closes the connection and deletes a partial body.
    async fn run<T>(
        &self,
        id: String,
        task: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        let (sender, receiver) = oneshot::channel();
        self.0.lock().unwrap().insert(id.clone(), sender);
        let result = tokio::select! {
            result = task => result,
            _ = receiver => Err(CANCELED.to_string()),
        };
        self.0.lock().unwrap().remove(&id);
        result
    }

    fn cancel(&self, id: &str) {
        if let Some(sender) = self.0.lock().unwrap().remove(id) {
            let _ = sender.send(());
        }
    }
}

#[tauri::command]
async fn send_request(
    request: RequestInput,
    options: TransportOptions,
    request_id: String,
    store: tauri::State<'_, ResponseStore>,
    grants: tauri::State<'_, FileGrants>,
    in_flight: tauri::State<'_, InFlight>,
    cookies: tauri::State<'_, cookies::Cookies>,
    on_stream: tauri::ipc::Channel<StreamMessage>,
) -> Result<ResponseOutput, String> {
    let jar = options.store_cookies.then(|| cookies.jar());
    let sink = move |message: StreamMessage| {
        let _ = on_stream.send(message);
    };
    let result = in_flight
        .run(
            request_id,
            execute(
                request,
                options,
                &store,
                &grants,
                DOWNLOAD_LIMIT,
                jar.clone(),
                Some(&sink),
            ),
        )
        .await;
    if jar.is_some() {
        // A failed cookie save does not fail the request.
        let _ = cookies.save();
    }
    result
}

#[tauri::command]
fn cancel_request(request_id: String, in_flight: tauri::State<'_, InFlight>) {
    in_flight.cancel(&request_id);
}

/// Read a granted file for upload.
async fn read_upload(grants: &FileGrants, path: &str) -> Result<(String, Vec<u8>), String> {
    let file = grants.check(path)?;
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size = tokio::fs::metadata(&file)
        .await
        .map_err(|_| format!("Cannot read {name}."))?
        .len();
    if size > UPLOAD_LIMIT {
        return Err(format!("{name} exceeds the 1 GiB upload limit."));
    }
    let bytes = tokio::fs::read(&file)
        .await
        .map_err(|_| format!("Cannot read {name}."))?;
    Ok((name, bytes))
}

async fn execute(
    mut request: RequestInput,
    options: TransportOptions,
    store: &ResponseStore,
    grants: &FileGrants,
    download_limit: u64,
    jar: Option<Arc<CookieStoreMutex>>,
    on_stream: StreamSink<'_>,
) -> Result<ResponseOutput, String> {
    options.validate()?;
    request.url = resolve_environment_references(&request.url)?;
    for header in &mut request.headers {
        header.value = resolve_environment_references(&header.value)?;
    }
    if let Some(body) = &mut request.body {
        *body = resolve_environment_references(body)?;
    }
    for part in request.multipart.iter_mut().flatten() {
        part.key = resolve_environment_references(&part.key)?;
        if !part.file {
            part.value = resolve_environment_references(&part.value)?;
        }
    }
    let url = reqwest::Url::parse(&request.url)
        .map_err(|_| "Enter an absolute HTTP or HTTPS URL.".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only HTTP and HTTPS URLs are supported.".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Use the Auth tab instead of credentials in the URL.".to_string());
    }
    let method = Method::from_bytes(request.method.as_bytes())
        .map_err(|_| "Invalid HTTP method.".to_string())?;
    let has_body = method != Method::GET && method != Method::HEAD;
    let redirects = Arc::new(AtomicUsize::new(0));
    let phases = Arc::new(Phases::default());
    let client = build_client(&options, redirects.clone(), phases.clone(), jar)?;
    let mut builder = client.request(method, url);

    for header in request.headers {
        if header.key.trim().is_empty() {
            continue;
        }

        let name = HeaderName::from_bytes(header.key.trim().as_bytes())
            .map_err(|_| format!("Invalid header name: {}", header.key))?;
        let value = HeaderValue::from_str(&header.value)
            .map_err(|_| format!("Invalid value for header: {}", header.key))?;
        builder = builder.header(name, value);
    }

    if has_body {
        if let Some(parts) = request.multipart {
            let mut form = multipart::Form::new();
            for part in parts {
                form = if part.file {
                    let (name, bytes) = read_upload(grants, &part.value).await?;
                    form.part(part.key, multipart::Part::bytes(bytes).file_name(name))
                } else {
                    form.text(part.key, part.value)
                };
            }
            builder = builder.multipart(form);
        } else if let Some(path) = request.body_file {
            builder = builder.body(read_upload(grants, &path).await?.1);
        } else if let Some(body) = request.body {
            builder = builder.body(body);
        }
    }

    let started_at = Instant::now();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(options.timeout_seconds);
    let timed_out = || {
        format!(
            "Request timed out ({} s connection / {} s total limit).",
            options.connect_timeout_seconds, options.timeout_seconds
        )
    };
    let mut response = tokio::time::timeout_at(deadline, builder.send())
        .await
        .map_err(|_| timed_out())?
        .map_err(|error| network_error(error, &options))?;
    let headers_at = Instant::now();
    // The policy has run for every hop once `send` returns.
    let redirect_count = redirects.load(Ordering::Relaxed);
    let final_url = (redirect_count > 0).then(|| response.url().to_string());
    let status = response.status();
    let headers: Vec<HeaderInput> = response
        .headers()
        .iter()
        .map(|(key, value)| HeaderInput {
            key: key.to_string(),
            value: value.to_str().unwrap_or("[binary value]").to_string(),
        })
        .collect();
    let stream = on_stream.filter(|_| {
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.trim().to_ascii_lowercase().starts_with("text/event-stream"))
    });
    if let Some(sink) = stream {
        sink(StreamMessage::Head {
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or("Unknown").to_string(),
            headers: headers
                .iter()
                .map(|header| (header.key.clone(), header.value.clone()))
                .collect(),
        });
    }
    let mut decoder = StreamDecoder::default();
    let preview_limit = options.inspection_limit_mib * MIB;
    let mut file = store.create()?;
    let mut size: u64 = 0;
    let mut preview = Vec::new();
    loop {
        let next = if stream.is_some() {
            response.chunk().await
        } else {
            tokio::time::timeout_at(deadline, response.chunk())
                .await
                .map_err(|_| timed_out())?
        };
        let Some(chunk) = next.map_err(|error| network_error(error, &options))? else {
            break;
        };
        if let Some(sink) = stream {
            sink(StreamMessage::Chunk {
                text: decoder.push(&chunk),
            });
        }
        size += chunk.len() as u64;
        if size > download_limit {
            // Dropping `file` deletes it.
            return Err("Response exceeds the 1 GiB download limit.".to_string());
        }
        file.write_all(&chunk)
            .map_err(|_| STORE_ERROR.to_string())?;
        if preview.len() < preview_limit {
            let take = (preview_limit - preview.len()).min(chunk.len());
            preview.extend_from_slice(&chunk[..take]);
        }
    }
    let duration_ms = started_at.elapsed().as_millis();
    let timing = phases.report(headers_at - started_at, headers_at.elapsed());
    let truncated = size > preview.len() as u64;
    let (body, binary) = decode_preview(preview);
    let body_id = store.insert(file);

    Ok(ResponseOutput {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("Unknown").to_string(),
        duration_ms,
        headers,
        body,
        size_bytes: size as usize,
        final_url,
        redirect_count: (redirect_count > 0).then_some(redirect_count),
        body_id: Some(body_id),
        truncated,
        binary,
        timing,
    })
}

/// Decode the preview as UTF-8 text. Returns `(text, binary)`. A NUL byte or
/// an invalid sequence makes the body binary. An incomplete character at the
/// end is the preview cut, not binary data, so it is dropped.
fn decode_preview(mut preview: Vec<u8>) -> (String, bool) {
    if preview.contains(&0) {
        return (String::new(), true);
    }
    match std::str::from_utf8(&preview) {
        Ok(_) => (String::from_utf8(preview).unwrap_or_default(), false),
        Err(error) if error.error_len().is_none() => {
            preview.truncate(error.valid_up_to());
            (String::from_utf8(preview).unwrap_or_default(), false)
        }
        Err(_) => (String::new(), true),
    }
}

/// Replace each `{{!NAME}}` with the NAME process environment variable. The
/// frontend resolves `{{name}}` tokens and keeps these as typed, so values
/// from the environment never reach the webview.
fn resolve_environment_references(value: &str) -> Result<String, String> {
    let mut output = String::with_capacity(value.len());
    let mut remainder = value;

    while let Some(start) = remainder.find("{{!") {
        output.push_str(&remainder[..start]);
        let after_open = &remainder[start + 3..];
        let Some(end) = after_open.find("}}") else {
            return Err("Invalid environment variable reference.".to_string());
        };
        let name = &after_open[..end];
        let valid_name = !name.is_empty()
            && name.bytes().enumerate().all(|(index, byte)| {
                byte == b'_' || byte.is_ascii_alphabetic() || index > 0 && byte.is_ascii_digit()
            });
        if !valid_name {
            return Err("Invalid environment variable reference.".to_string());
        }
        let variable = std::env::var(name)
            .map_err(|_| format!("Undefined environment variable: \"{name}\"."))?;
        output.push_str(&variable);
        remainder = &after_open[end + 2..];
    }

    output.push_str(remainder);
    Ok(output)
}

fn network_error(error: reqwest::Error, options: &TransportOptions) -> String {
    if error.is_timeout() {
        format!(
            "Request timed out ({} s connection / {} s total limit).",
            options.connect_timeout_seconds, options.timeout_seconds
        )
    } else if error.is_redirect() {
        // The policy's own message, such as "Stopped after N redirects."
        std::error::Error::source(&error)
            .map(|source| source.to_string())
            .unwrap_or_else(|| format!("Network request failed: {}", error.without_url()))
    } else {
        format!("Network request failed: {}", error.without_url())
    }
}

mod app_state;
mod cookies;
mod ghostty_themes;
mod request_files;
#[cfg(test)]
mod request_tests;
mod response_store;
mod timing;
mod websocket;

fn save_window_state(app: &tauri::AppHandle) {
    use tauri_plugin_window_state::{AppHandleExt, StateFlags};
    let _ = app.save_window_state(StateFlags::all());
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::Manager;
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            app.manage(app_state::AppState::new(app.path().app_data_dir()?));
            app.manage(FileGrants::load(
                app.path().app_data_dir()?.join("file-grants.json"),
            ));
            app.manage(InFlight::default());
            app.manage(websocket::Sockets::default());
            app.manage(cookies::Cookies::load(
                app.path().app_data_dir()?.join("cookies.json"),
            ));
            app.manage(ResponseStore::new(
                app.path().app_cache_dir()?.join("responses"),
            ));
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                save_window_state(window.app_handle());
                if app_state::request_exit(window.app_handle()) {
                    api.prevent_close();
                }
            }
            // The plugin only writes on a clean exit. Also write on blur so the
            // layout survives killed processes and `tauri dev` restarts.
            tauri::WindowEvent::Focused(false) => save_window_state(window.app_handle()),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            send_request,
            cancel_request,
            request_files::pick_request_file,
            app_state::load_app_state,
            app_state::save_app_state,
            app_state::app_state_ready,
            app_state::finish_app_exit,
            ghostty_themes::list_ghostty_themes,
            ghostty_themes::read_ghostty_theme,
            response_store::release_response,
            response_store::save_response,
            response_store::save_response_text,
            cookies::list_cookies,
            cookies::delete_cookie,
            cookies::clear_cookies,
            websocket::ws_connect,
            websocket::ws_send,
            websocket::ws_close
        ])
        .build(tauri::generate_context!())
        .expect("error while building Blink")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if app_state::request_exit(app) {
                    api.prevent_exit();
                }
            }
        });
}
