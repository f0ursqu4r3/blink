//! Port of `src/lib/response-body.ts`: the save rules and file names.
//! Storing, releasing, and writing raw bodies belongs to the engine.

use std::sync::LazyLock;

use fancy_regex::Regex;

use crate::model::ApiResponse;

pub const BODY_UNAVAILABLE: &str = "The response body is no longer available";

/// After a restart only complete text previews can be saved.
pub fn can_save_response(response: &ApiResponse) -> bool {
    response.body_id.as_deref().is_some_and(|id| !id.is_empty())
        || (!response.is_truncated() && !response.is_binary())
}

const EXTENSIONS: [(&str, &str); 9] = [
    ("application/json", ".json"),
    ("application/xml", ".xml"),
    ("text/xml", ".xml"),
    ("text/html", ".html"),
    ("text/plain", ".txt"),
    ("image/png", ".png"),
    ("image/jpeg", ".jpg"),
    ("application/pdf", ".pdf"),
    ("application/zip", ".zip"),
];

fn header<'a>(response: &'a ApiResponse, name: &str) -> Option<&'a str> {
    response
        .headers
        .iter()
        .find(|header| header.key.to_lowercase() == name)
        .map(|header| header.value.as_str())
}

/// `decodeURIComponent`: None for a bad escape or invalid UTF-8.
fn decode_uri_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn decode(value: &str) -> String {
    decode_uri_component(value).unwrap_or_else(|| value.to_string())
}

fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

// No path separators, control characters, or names that mean a directory.
fn clean(name: &str) -> String {
    let safe: String = name
        .chars()
        .filter(|&c| !matches!(c, '/' | '\\' | '\u{0}'..='\u{1f}' | '\u{7f}'))
        .collect();
    let safe = js_trim(&safe);
    if safe == "." || safe == ".." {
        String::new()
    } else {
        safe.to_string()
    }
}

static ENCODED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)filename\*\s*=\s*UTF-8''([^;]+)").unwrap());
static PLAIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)filename\s*=\s*"?([^";]+)"?"#).unwrap());

fn first_group(regex: &Regex, text: &str) -> Option<String> {
    let captures = regex.captures(text).ok()??;
    Some(captures.get(1)?.as_str().to_string())
}

pub fn suggested_file_name(response: &ApiResponse, request_url: &str) -> String {
    let disposition = header(response, "content-disposition").unwrap_or("");
    let from_header = match first_group(&ENCODED, disposition) {
        Some(encoded) => clean(&decode(js_trim(&encoded))),
        None => clean(js_trim(
            &first_group(&PLAIN, disposition).unwrap_or_default(),
        )),
    };
    if !from_header.is_empty() {
        return from_header;
    }
    let url = response.final_url.as_deref().unwrap_or(request_url);
    // Not an absolute URL: fall through to the content type.
    if let Ok(url) = url::Url::parse(url) {
        let segment = url.path().split('/').rfind(|part| !part.is_empty());
        let from_url = clean(&decode(segment.unwrap_or("")));
        if !from_url.is_empty() {
            return from_url;
        }
    }
    let kind = header(response, "content-type")
        .map(|value| js_trim(value.split(';').next().unwrap_or("")).to_lowercase())
        .unwrap_or_default();
    let extension = EXTENSIONS
        .iter()
        .find(|(name, _)| *name == kind)
        .map_or(".bin", |(_, extension)| extension);
    format!("response{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Header;

    fn base() -> ApiResponse {
        ApiResponse {
            status: 200,
            status_text: "OK".into(),
            duration_ms: 1.0,
            size_bytes: 2,
            headers: vec![],
            body: "{}".into(),
            body_id: None,
            truncated: None,
            binary: None,
            final_url: None,
            redirect_count: None,
            timing: None,
        }
    }

    fn with_header(key: &str, value: &str) -> ApiResponse {
        ApiResponse {
            headers: vec![Header {
                key: key.into(),
                value: value.into(),
            }],
            ..base()
        }
    }

    #[test]
    fn prefers_content_disposition() {
        assert_eq!(
            suggested_file_name(
                &with_header("Content-Disposition", "attachment; filename=\"report.csv\""),
                "https://x.test/a/b"
            ),
            "report.csv"
        );
        assert_eq!(
            suggested_file_name(
                &with_header(
                    "content-disposition",
                    "attachment; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf"
                ),
                "https://x.test/"
            ),
            "résumé.pdf"
        );
    }

    #[test]
    fn removes_path_separators_and_control_characters() {
        assert_eq!(
            suggested_file_name(
                &with_header(
                    "Content-Disposition",
                    "attachment; filename=\"../../etc/pass\u{7}wd\""
                ),
                "https://x.test/"
            ),
            "....etcpasswd"
        );
    }

    #[test]
    fn uses_the_last_url_segment_then_the_content_type() {
        assert_eq!(
            suggested_file_name(&base(), "https://x.test/files/logo.png?x=1"),
            "logo.png"
        );
        assert_eq!(
            suggested_file_name(
                &ApiResponse {
                    final_url: Some("https://cdn.test/real.zip".into()),
                    ..base()
                },
                "https://x.test/download"
            ),
            "real.zip"
        );
        assert_eq!(
            suggested_file_name(
                &with_header("Content-Type", "application/json; charset=utf-8"),
                "https://x.test/"
            ),
            "response.json"
        );
        assert_eq!(suggested_file_name(&base(), "not a url"), "response.bin");
    }

    #[test]
    fn needs_a_stored_body_unless_the_preview_is_complete_text() {
        assert!(can_save_response(&base()));
        assert!(!can_save_response(&ApiResponse {
            truncated: Some(true),
            ..base()
        }));
        assert!(!can_save_response(&ApiResponse {
            binary: Some(true),
            ..base()
        }));
        assert!(can_save_response(&ApiResponse {
            binary: Some(true),
            body_id: Some("b".into()),
            ..base()
        }));
    }
}
