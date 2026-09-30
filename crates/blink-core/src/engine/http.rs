//! HTTP requests: validation, environment references, the client per
//! request, streaming, the response preview, and cancel by request id.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use reqwest::{
    Client, Method, Proxy, header::HeaderName, header::HeaderValue, multipart, redirect::Policy,
};
use reqwest_cookie_store::CookieStoreMutex;
use serde::Serialize;
use tokio::sync::oneshot;

use super::request_files::FileGrants;
use super::response_store::{ResponseStore, STORE_ERROR};
use super::timing::{Phases, TimedConnectLayer, TimedResolver};
use crate::model::{ApiResponse, Header, RequestInput, TransportOptions};

const MIB: usize = 1024 * 1024;
pub const DOWNLOAD_LIMIT: u64 = 1024 * 1024 * 1024;
pub const UPLOAD_LIMIT: u64 = 1024 * 1024 * 1024;
pub const CANCELED: &str = "Request canceled.";

/// Sent to the UI while an event stream arrives.
#[derive(Debug, Clone, PartialEq, Serialize)]
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

/// Receives stream messages. Tests pass a closure; the engine sends to a channel.
pub type StreamSink<'a> = Option<&'a (dyn Fn(StreamMessage) + Send + Sync)>;

/// UTF-8 decoding across chunk boundaries: holds back a character that a
/// chunk cut in two.
#[derive(Default)]
pub struct StreamDecoder(Vec<u8>);

impl StreamDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> String {
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

/// The UI checks the same ranges. The engine checks again because a saved
/// or imported workspace can hold any value.
pub fn validate(options: &TransportOptions) -> Result<(), String> {
    let valid = (1..=600).contains(&options.timeout_seconds)
        && (1..=options.timeout_seconds).contains(&options.connect_timeout_seconds)
        && (1..=20).contains(&options.max_redirects)
        && (1..=16).contains(&options.inspection_limit_mi_b);
    if valid {
        Ok(())
    } else {
        Err("Invalid transport settings.".to_string())
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
        let max = options.max_redirects as usize;
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

/// Requests that are still running, by the id the UI gave them.
#[derive(Default)]
pub struct InFlight(Mutex<HashMap<String, oneshot::Sender<()>>>);

impl InFlight {
    /// Run `task` until it finishes or `cancel` is called with `id`.
    /// Dropping the task closes the connection and deletes a partial body.
    pub async fn run<T>(
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

    pub fn cancel(&self, id: &str) {
        if let Some(sender) = self.0.lock().unwrap().remove(id) {
            let _ = sender.send(());
        }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }
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

/// Send one request and read its body into the store. Must run on a tokio
/// runtime.
pub async fn execute(
    mut request: RequestInput,
    options: TransportOptions,
    store: &ResponseStore,
    grants: &FileGrants,
    download_limit: u64,
    jar: Option<Arc<CookieStoreMutex>>,
    on_stream: StreamSink<'_>,
) -> Result<ApiResponse, String> {
    validate(&options)?;
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
    let headers: Vec<Header> = response
        .headers()
        .iter()
        .map(|(key, value)| Header {
            key: key.to_string(),
            value: value.to_str().unwrap_or("[binary value]").to_string(),
        })
        .collect();
    let stream = on_stream.filter(|_| {
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .trim()
                    .to_ascii_lowercase()
                    .starts_with("text/event-stream")
            })
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
    let preview_limit = options.inspection_limit_mi_b as usize * MIB;
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
    let duration_ms = started_at.elapsed().as_millis() as f64;
    let timing = phases.report(headers_at - started_at, headers_at.elapsed());
    let truncated = size > preview.len() as u64;
    let (body, binary) = decode_preview(preview);
    let body_id = store.insert(file);

    Ok(ApiResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("Unknown").to_string(),
        duration_ms,
        headers,
        body,
        size_bytes: size,
        final_url,
        redirect_count: (redirect_count > 0).then_some(redirect_count as u64),
        body_id: Some(body_id),
        truncated: Some(truncated),
        binary: Some(binary),
        timing: Some(timing),
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
/// UI resolves `{{name}}` tokens and keeps these as typed, so values from
/// the environment never reach the workspace or the UI.
pub fn resolve_environment_references(value: &str) -> Result<String, String> {
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
