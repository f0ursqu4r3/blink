//! Drafts and the request the transport sends. Port of `src/lib/request.ts`.

use crate::ids;
use crate::model::{AuthKind, BodyMode, Draft, Pair};

/// An HTTP method is any token (RFC 9110), such as PURGE or PROPFIND.
pub fn is_method(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(is_token_byte)
}

pub(crate) fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+.^_`|~-".contains(&byte)
}

pub fn pair(key: impl Into<String>, value: impl Into<String>) -> Pair {
    Pair {
        id: ids::PAIRS.next(),
        key: key.into(),
        value: value.into(),
        enabled: true,
        file: None,
    }
}

pub fn create_draft() -> Draft {
    Draft {
        method: "GET".into(),
        url: String::new(),
        query: vec![pair("", "")],
        headers: vec![pair("Accept", "application/json")],
        body_mode: BodyMode::None,
        body: String::new(),
        variables: None,
        form: None,
        body_file: None,
        auth: AuthKind::None,
        token: String::new(),
        username: String::new(),
        password: String::new(),
        local_auth: None,
        assertions: None,
        captures: None,
    }
}

pub fn active_pairs(rows: &[Pair]) -> impl Iterator<Item = &Pair> {
    rows.iter().filter(|row| row.enabled && !row.key.trim().is_empty())
}

pub fn supports_body(method: &str) -> bool {
    method != "GET" && method != "HEAD"
}

/// The last segment of a file path, for labels.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}
