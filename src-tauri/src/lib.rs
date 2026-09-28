use std::time::{Duration, Instant};

use reqwest::{header::HeaderName, header::HeaderValue, Client, Method};
use serde::{Deserialize, Serialize};

const RESPONSE_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestInput {
    method: String,
    url: String,
    headers: Vec<HeaderInput>,
    body: Option<String>,
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
}

#[tauri::command]
async fn send_request(mut request: RequestInput) -> Result<ResponseOutput, String> {
    request.url = resolve_environment_references(&request.url)?;
    for header in &mut request.headers {
        header.value = resolve_environment_references(&header.value)?;
    }
    if let Some(body) = &mut request.body {
        *body = resolve_environment_references(body)?;
    }
    let url = reqwest::Url::parse(&request.url)
        .map_err(|_| "Enter an absolute HTTP or HTTPS URL.".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only HTTP and HTTPS URLs are supported.".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Use the Auth tab instead of credentials in the URL.".to_string());
    }
    if !matches!(
        request.method.as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
    ) {
        return Err("Unsupported HTTP method.".to_string());
    }
    let method = Method::from_bytes(request.method.as_bytes())
        .map_err(|_| format!("Unsupported HTTP method: {}", request.method))?;
    let has_body = method != Method::GET && method != Method::HEAD;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| error.without_url().to_string())?;
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
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
    }

    let started_at = Instant::now();
    let mut response = builder.send().await.map_err(network_error)?;
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(key, value)| HeaderInput {
            key: key.to_string(),
            value: value.to_str().unwrap_or("[binary value]").to_string(),
        })
        .collect();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if bytes.len() + chunk.len() > RESPONSE_LIMIT {
            return Err("Response exceeds the 4 MiB inspection limit.".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    let duration_ms = started_at.elapsed().as_millis();
    let size_bytes = bytes.len();
    let body = String::from_utf8_lossy(&bytes).into_owned();

    Ok(ResponseOutput {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("Unknown").to_string(),
        duration_ms,
        headers,
        body,
        size_bytes,
    })
}

fn resolve_environment_references(value: &str) -> Result<String, String> {
    let mut output = String::with_capacity(value.len());
    let mut remainder = value;

    while let Some(start) = remainder.find("<<") {
        output.push_str(&remainder[..start]);
        let after_open = &remainder[start + 2..];
        let Some(end) = after_open.find(">>") else {
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

fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "Request timed out (10 s connection / 30 s total limit).".to_string()
    } else {
        format!("Network request failed: {}", error.without_url())
    }
}

mod app_state;
#[cfg(test)]
mod request_tests;

fn save_window_state(app: &tauri::AppHandle) {
    use tauri_plugin_window_state::{AppHandleExt, StateFlags};
    let _ = app.save_window_state(StateFlags::all());
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::Manager;
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            app.manage(app_state::AppState::new(app.path().app_data_dir()?));
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
            app_state::load_app_state,
            app_state::save_app_state,
            app_state::app_state_ready,
            app_state::finish_app_exit
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
