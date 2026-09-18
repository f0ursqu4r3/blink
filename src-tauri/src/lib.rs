use std::time::Instant;

use reqwest::{header::HeaderName, header::HeaderValue, Client, Method};
use serde::{Deserialize, Serialize};

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
}

#[tauri::command]
async fn send_request(request: RequestInput) -> Result<ResponseOutput, String> {
    let method = Method::from_bytes(request.method.as_bytes())
        .map_err(|_| format!("Unsupported HTTP method: {}", request.method))?;
    let client = Client::new();
    let mut builder = client.request(method, &request.url);

    for header in request.headers {
        if header.key.trim().is_empty() {
            continue;
        }

        let name = HeaderName::from_bytes(header.key.trim().as_bytes())
            .map_err(|_| format!("Invalid header name: {}", header.key))?;
        let value = HeaderValue::from_str(header.value.trim())
            .map_err(|_| format!("Invalid value for header: {}", header.key))?;
        builder = builder.header(name, value);
    }

    if let Some(body) = request.body.filter(|body| !body.is_empty()) {
        builder = builder.body(body);
    }

    let started_at = Instant::now();
    let response = builder
        .send()
        .await
        .map_err(|error| format!("Network request failed: {error}"))?;
    let duration_ms = started_at.elapsed().as_millis();
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(key, value)| HeaderInput {
            key: key.to_string(),
            value: value.to_str().unwrap_or("[binary value]").to_string(),
        })
        .collect();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Could not read response body: {error}"))?;

    Ok(ResponseOutput {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("Unknown").to_string(),
        duration_ms,
        headers,
        body,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![send_request])
        .run(tauri::generate_context!())
        .expect("error while running Blink");
}
