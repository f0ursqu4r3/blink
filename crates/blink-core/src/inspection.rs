//! Ephemeral transport observations. Never persisted in workspace or project files.
use crate::model::Header;
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    #[serde(skip)]
    pub secrets: Vec<String>,
    pub method: String,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Option<String>,
    pub redirects: Vec<RedirectHop>,
    pub http_version: Option<String>,
    pub remote_address: Option<String>,
    pub peer_certificate_sha256: Option<String>,
    pub tls_verified: bool,
    pub client_certificate: bool,
    pub sources: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RedirectHop {
    pub status: u16,
    pub from: String,
    pub to: String,
}

pub fn sensitive(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "authorization",
        "cookie",
        "token",
        "secret",
        "password",
        "api-key",
        "api_key",
        "apikey",
    ]
    .iter()
    .any(|part| name.contains(part))
}
pub fn safe_url(value: &str) -> String {
    let Ok(mut url) = url::Url::parse(value) else {
        return "[invalid URL]".into();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_fragment(None);
    let pairs: Vec<_> = url
        .query_pairs()
        .map(|(k, _)| (k.into_owned(), "[redacted]"))
        .collect();
    if !pairs.is_empty() {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    url.into()
}
impl Inspection {
    /// Explicit reveal is only for the local viewer. Shared copies always call false.
    pub fn text(&self, reveal: bool) -> String {
        let url = if reveal {
            self.url.clone()
        } else {
            safe_url(&self.url)
        };
        let mut lines = vec![format!("{} {}", self.method, url)];
        for header in &self.headers {
            let value = if !reveal && sensitive(&header.key) {
                "[redacted]"
            } else {
                &header.value
            };
            lines.push(format!("{}: {}", header.key, value));
        }
        if let Some(body) = &self.body {
            lines.push(String::new());
            lines.push(if reveal {
                body.clone()
            } else {
                "[body hidden; reveal locally to inspect]".into()
            });
        }
        lines.push(String::new());
        lines.extend(self.sources.iter().cloned());
        for hop in &self.redirects {
            lines.push(format!(
                "Redirect {}: {} → {}",
                hop.status,
                if reveal {
                    hop.from.clone()
                } else {
                    safe_url(&hop.from)
                },
                if reveal {
                    hop.to.clone()
                } else {
                    safe_url(&hop.to)
                }
            ));
        }
        if let Some(version) = &self.http_version {
            lines.push(format!("Protocol: {version}"));
        }
        if let Some(address) = &self.remote_address {
            lines.push(format!("Remote address: {address}"));
        }
        if self.url.starts_with("https:") {
            lines.push(format!(
                "TLS certificate verification: {}",
                if self.tls_verified {
                    "enabled"
                } else {
                    "disabled"
                }
            ));
            if let Some(hash) = &self.peer_certificate_sha256 {
                lines.push(format!("Peer certificate SHA-256: {hash}"));
            }
            lines.push("TLS version and cipher are not exposed by this transport.".into());
        }
        if self.client_certificate {
            lines.push("Client certificate: configured for this origin".into());
        }
        lines.push("Initial prepared request and observed redirects. Protocol framing and per-hop request headers are not captured.".into());
        let mut text = lines.join("\n");
        if !reveal {
            let mut values = self
                .secrets
                .iter()
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>();
            values.sort_by_key(|v| std::cmp::Reverse(v.len()));
            for value in values {
                text = text.replace(value.as_str(), "[redacted]");
                let encoded: String =
                    url::form_urlencoded::byte_serialize(value.as_bytes()).collect();
                text = text.replace(&encoded, "[redacted]");
                text = text.replace(&encoded.replace('+', "%20"), "[redacted]");
            }
        }
        text
    }
}
pub fn body_preview(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[request body preview truncated]", &text[..end])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_named_secrets_in_custom_headers_and_paths() {
        let trace = Inspection {
            url: "https://a.test/secret".into(),
            headers: vec![Header {
                key: "X-Custom".into(),
                value: "secret".into(),
            }],
            secrets: vec!["secret".into()],
            ..Default::default()
        };
        assert!(!trace.text(false).contains("secret"));
        assert!(trace.text(true).contains("secret"));
    }
    #[test]
    fn body_capture_caps_utf8_without_panicking() {
        let preview = body_preview("ééé", 3);
        assert!(preview.starts_with("é\n"));
        assert!(preview.contains("truncated"));
    }
    #[test]
    fn default_view_hides_credentials_queries_and_body() {
        let trace = Inspection {
            url: "https://a.test/path?code=secret".into(),
            headers: vec![Header {
                key: "X-Api-Key".into(),
                value: "secret".into(),
            }],
            body: Some("secret".into()),
            ..Default::default()
        };
        assert!(!trace.text(false).contains("secret"));
        assert!(trace.text(true).contains("secret"));
    }
}
