//! Drafts and the request the transport sends. Port of `src/lib/request.ts`.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use url::Url;

use crate::authorization::ResolvedRequestContext;
use crate::ids;
use crate::interpolation::{InterpolationContext, interpolate};
use crate::model::{
    AuthKind, AuthorizationConfig, BodyMode, Draft, Header, MultipartPart, Pair, RequestInput,
    TransportOptions,
};

pub use crate::model::METHODS;

/// Every body mode, in menu order.
pub const BODY_MODES: [BodyMode; 7] = BodyMode::ALL;

/// An HTTP method is any token (RFC 9110), such as PURGE or PROPFIND.
pub fn is_method(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(is_token_byte)
}

pub(crate) fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+.^_`|~-".contains(&byte)
}

/// Whitespace as JavaScript's `\s` and `trim` define it.
pub(crate) fn is_js_whitespace(c: char) -> bool {
    (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}'
}

/// `String.prototype.trim`.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(is_js_whitespace)
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
    rows.iter()
        .filter(|row| row.enabled && !js_trim(&row.key).is_empty())
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

/// Tokens for interpolation, and the resolved auth when known.
#[derive(Debug, Clone, Copy)]
pub enum RequestContext<'a> {
    Tokens(&'a InterpolationContext),
    Resolved(&'a ResolvedRequestContext),
}

impl<'a> From<&'a InterpolationContext> for RequestContext<'a> {
    fn from(ctx: &'a InterpolationContext) -> Self {
        RequestContext::Tokens(ctx)
    }
}

impl<'a> From<&'a ResolvedRequestContext> for RequestContext<'a> {
    fn from(ctx: &'a ResolvedRequestContext) -> Self {
        RequestContext::Resolved(ctx)
    }
}

impl RequestContext<'_> {
    fn tokens(&self) -> &InterpolationContext {
        match self {
            RequestContext::Tokens(ctx) => ctx,
            RequestContext::Resolved(ctx) => &ctx.tokens,
        }
    }

    fn auth(&self) -> Option<&AuthorizationConfig> {
        match self {
            RequestContext::Tokens(_) => None,
            RequestContext::Resolved(ctx) => Some(&ctx.auth),
        }
    }
}

/// `URLSearchParams` serialization.
fn search_params<'a>(rows: impl Iterator<Item = (String, String)> + 'a) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in rows {
        serializer.append_pair(&key, &value);
    }
    serializer.finish()
}

pub fn build_request(draft: &Draft, ctx: Option<RequestContext>) -> Result<RequestInput, String> {
    let interp = |s: &str| -> Result<String, String> {
        match ctx {
            Some(ctx) => interpolate(s, ctx.tokens()),
            None => Ok(s.to_string()),
        }
    };
    if !is_method(&draft.method) {
        return Err("Enter a method name without spaces, such as GET.".into());
    }

    let raw_url = interp(js_trim(&draft.url))?;

    let target = Url::parse(&raw_url).map_err(|_| "Enter an absolute HTTP or HTTPS URL.")?;
    if !["http", "https"].contains(&target.scheme()) {
        return Err("Only HTTP and HTTPS URLs are supported.".into());
    }
    if !has_http_authority(&raw_url) {
        return Err("Enter an absolute URL starting with http:// or https://.".into());
    }
    if !target.username().is_empty() || target.password().is_some_and(|p| !p.is_empty()) {
        return Err("Use the Auth tab instead of credentials in the URL.".into());
    }
    let mut url = raw_url.split('#').next().unwrap_or_default().to_string();
    let query = active_pairs(&draft.query)
        .map(|row| Ok((interp(&row.key)?, interp(&row.value)?)))
        .collect::<Result<Vec<_>, String>>()?;
    if !query.is_empty() {
        let additions = search_params(query.into_iter());
        let separator = if url.contains('?') {
            if url.ends_with(['?', '&']) { "" } else { "&" }
        } else {
            "?"
        };
        url.push_str(separator);
        url.push_str(&additions);
    }
    let mut headers = active_pairs(&draft.headers)
        .map(|row| {
            Ok(Header {
                key: js_trim(&row.key).to_string(),
                value: interp(&row.value)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    for header in &headers {
        if header.key.is_empty() || !header.key.bytes().all(is_token_byte) {
            return Err(format!("Invalid header name: {}", header.key));
        }
        if header.value.contains(['\r', '\n']) {
            return Err(format!("Line breaks are not allowed in {}.", header.key));
        }
    }

    let flat_auth = || match draft.auth {
        AuthKind::Bearer => AuthorizationConfig::Bearer {
            token: draft.token.clone(),
        },
        AuthKind::Basic => AuthorizationConfig::Basic {
            username: draft.username.clone(),
            password: draft.password.clone(),
        },
        AuthKind::None => AuthorizationConfig::None,
    };
    let resolved_auth = ctx
        .and_then(|ctx| ctx.auth().cloned())
        .or_else(|| draft.local_auth.clone())
        .unwrap_or_else(flat_auth);

    if resolved_auth != AuthorizationConfig::None {
        if headers
            .iter()
            .any(|h| h.key.eq_ignore_ascii_case("authorization"))
        {
            return Err("Remove the Authorization header or select No auth.".into());
        }
        match &resolved_auth {
            AuthorizationConfig::Bearer { token } => {
                let token = interp(token)?;
                let token = js_trim(&token);
                if token.is_empty() || token.chars().any(is_js_whitespace) {
                    return Err("Enter a bearer token without spaces or line breaks.".into());
                }
                headers.push(Header {
                    key: "Authorization".into(),
                    value: format!("Bearer {token}"),
                });
            }
            AuthorizationConfig::Basic { username, password } => {
                let username = interp(username)?;
                let password = interp(password)?;
                if username.contains(':') {
                    return Err("Basic auth usernames cannot contain a colon.".into());
                }
                headers.push(Header {
                    key: "Authorization".into(),
                    value: format!("Basic {}", BASE64.encode(format!("{username}:{password}"))),
                });
            }
            AuthorizationConfig::None => {}
        }
    }

    let sends_body = supports_body(&draft.method) && draft.body_mode != BodyMode::None;
    let content_type = headers
        .iter()
        .any(|h| h.key.eq_ignore_ascii_case("content-type"));
    let form = draft.form.as_deref().unwrap_or_default();
    if sends_body && draft.body_mode == BodyMode::Multipart {
        if content_type {
            return Err(
                "Remove the Content-Type header. Multipart bodies set their own boundary.".into(),
            );
        }
        let multipart = active_pairs(form)
            .map(|row| {
                let file = row.file.unwrap_or(false);
                if file && row.value.is_empty() {
                    return Err(format!("Choose a file for the {} part.", row.key));
                }
                Ok(MultipartPart {
                    key: interp(&row.key)?,
                    value: if file {
                        row.value.clone()
                    } else {
                        interp(&row.value)?
                    },
                    file,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        return Ok(RequestInput {
            method: draft.method.clone(),
            url,
            headers,
            body: None,
            body_file: None,
            multipart: Some(multipart),
        });
    }
    if sends_body && draft.body_mode == BodyMode::File {
        let Some(body_file) = draft.body_file.clone().filter(|path| !path.is_empty()) else {
            return Err("Choose a file to send as the body.".into());
        };
        if !content_type {
            headers.push(Header {
                key: "Content-Type".into(),
                value: "application/octet-stream".into(),
            });
        }
        return Ok(RequestInput {
            method: draft.method.clone(),
            url,
            headers,
            body: None,
            body_file: Some(body_file),
            multipart: None,
        });
    }
    let mut body = sends_body.then(|| draft.body.clone());
    if let Some(text) = body.as_mut() {
        if draft.body_mode == BodyMode::Form {
            let rows = active_pairs(form)
                .map(|row| Ok((interp(&row.key)?, interp(&row.value)?)))
                .collect::<Result<Vec<_>, String>>()?;
            *text = search_params(rows.into_iter());
        } else if ctx.is_some() {
            *text = interp(text)?;
        }
        if draft.body_mode == BodyMode::Json
            && serde_json::from_str::<serde::de::IgnoredAny>(text).is_err()
        {
            return Err("Invalid JSON body. Fix the JSON or select Text.".into());
        }
        if draft.body_mode == BodyMode::Graphql {
            if js_trim(text).is_empty() {
                return Err("Enter a GraphQL query.".into());
            }
            let raw_variables = interp(draft.variables.as_deref().unwrap_or_default())?;
            let mut payload = serde_json::Map::new();
            payload.insert("query".into(), serde_json::Value::String(text.clone()));
            if !js_trim(&raw_variables).is_empty() {
                match serde_json::from_str::<serde_json::Value>(&raw_variables) {
                    Ok(variables @ serde_json::Value::Object(_)) => {
                        payload.insert("variables".into(), variables);
                    }
                    _ => return Err("GraphQL variables must be a JSON object.".into()),
                }
            }
            *text = serde_json::Value::Object(payload).to_string();
        }
    }
    if body.is_some() && !content_type {
        headers.push(Header {
            key: "Content-Type".into(),
            value: match draft.body_mode {
                BodyMode::Json | BodyMode::Graphql => "application/json",
                BodyMode::Form => "application/x-www-form-urlencoded",
                _ => "text/plain; charset=utf-8",
            }
            .into(),
        });
    }
    Ok(RequestInput {
        method: draft.method.clone(),
        url,
        headers,
        body,
        body_file: None,
        multipart: None,
    })
}

/// `/^https?:\/\/[^/\\\s]/i`
fn has_http_authority(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &url[8..]
    } else if lower.starts_with("http://") {
        &url[7..]
    } else {
        return false;
    };
    rest.chars()
        .next()
        .is_some_and(|c| c != '/' && c != '\\' && !is_js_whitespace(c))
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// A cURL command that sends what Blink sends with these transport options.
pub fn to_curl(request: &RequestInput, options: &TransportOptions) -> String {
    let mut lines = vec![format!(
        "curl --disable --globoff --max-time {} --connect-timeout {}",
        options.timeout_seconds, options.connect_timeout_seconds
    )];
    if options.follow_redirects {
        lines.push(format!("--location --max-redirs {}", options.max_redirects));
    }
    if !options.verify_tls {
        lines.push("--insecure".into());
    }
    if !options.proxy_url.is_empty() {
        lines.push(format!("--proxy {}", quote(&options.proxy_url)));
    }
    lines.push(if request.method == "HEAD" {
        "--head".into()
    } else {
        format!("--request {}", request.method)
    });
    lines.push(format!("--url {}", quote(&request.url)));
    for Header { key, value } in &request.headers {
        lines.push(format!("--header {}", quote(&format!("{key}: {value}"))));
    }
    if let Some(multipart) = &request.multipart {
        for MultipartPart { key, value, file } in multipart {
            // --form-string keeps a leading @ or < in a text value literal.
            lines.push(if *file {
                format!("--form {}", quote(&format!("{key}=@{value}")))
            } else {
                format!("--form-string {}", quote(&format!("{key}={value}")))
            });
        }
    } else if let Some(body_file) = request.body_file.as_deref().filter(|f| !f.is_empty()) {
        lines.push(format!("--data-binary {}", quote(&format!("@{body_file}"))));
    } else if let Some(body) = &request.body {
        lines.push(format!("--data-raw {}", quote(body)));
    }
    lines.join(" \\\n  ")
}

/// `value / unit` with `places` decimals, rounding ties up as `toFixed` does.
fn fixed(value: u64, unit: u64, places: u32) -> String {
    let scale = 10u128.pow(places);
    let scaled = (value as u128 * scale * 2 + unit as u128) / (unit as u128 * 2);
    format!(
        "{}.{:0width$}",
        scaled / scale,
        scaled % scale,
        width = places as usize
    )
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        return format!("{} KiB", fixed(bytes, 1024, 1));
    }
    format!("{} MiB", fixed(bytes, 1024 * 1024, 2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Definitions;

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    fn tokens(entries: &[(&str, &str)]) -> InterpolationContext {
        InterpolationContext::local(defs(entries))
    }
    fn build(draft: &Draft) -> Result<RequestInput, String> {
        build_request(draft, None)
    }
    fn build_with(draft: &Draft, ctx: &InterpolationContext) -> Result<RequestInput, String> {
        build_request(draft, Some(ctx.into()))
    }
    fn header(key: &str, value: &str) -> Header {
        Header {
            key: key.into(),
            value: value.into(),
        }
    }
    fn off(mut row: Pair) -> Pair {
        row.enabled = false;
        row
    }
    fn file_row(key: &str, value: &str) -> Pair {
        Pair {
            file: Some(true),
            ..pair(key, value)
        }
    }
    /// `URLSearchParams.getAll` on a built URL.
    fn search_all(url: &str, name: &str) -> Vec<String> {
        Url::parse(url)
            .unwrap()
            .query_pairs()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .collect()
    }
    fn authorization(request: &RequestInput) -> String {
        request
            .headers
            .iter()
            .find(|h| h.key == "Authorization")
            .unwrap()
            .value
            .clone()
    }
    fn basic(value: &str) -> String {
        format!("Basic {}", BASE64.encode(value))
    }

    // request construction

    fn draft() -> Draft {
        Draft {
            url: "https://example.test/resource?a=1%20two".into(),
            ..create_draft()
        }
    }

    #[test]
    fn preserves_a_signed_query_string_without_additional_rows() {
        assert_eq!(
            build(&draft()).unwrap().url,
            "https://example.test/resource?a=1%20two"
        );
    }

    #[test]
    fn appends_query_rows_without_rewriting_the_original_encoding() {
        let mut request = draft();
        request.url = "https://example.test/%2f?token=a%20b&path=%2f#fragment".into();
        request.query = vec![pair("filter", "two words")];
        assert_eq!(
            build(&request).unwrap().url,
            "https://example.test/%2f?token=a%20b&path=%2f&filter=two+words"
        );
        for url in [
            "https://example.test",
            "https://example.test?",
            "https://example.test?existing=1&",
        ] {
            request.url = url.into();
            let result = build(&request).unwrap().url;
            assert_eq!(search_all(&result, "filter"), ["two words"]);
            let search = Url::parse(&result).unwrap().query().unwrap().to_string();
            assert!(!search.contains("??"));
            assert!(!search.contains("&&"));
        }
    }

    #[test]
    fn appends_enabled_duplicate_query_parameters() {
        let mut request = draft();
        request.query = vec![
            pair("a", "three"),
            pair("a", "four"),
            off(pair("ignored", "x")),
        ];
        assert_eq!(
            search_all(&build(&request).unwrap().url, "a"),
            ["1 two", "three", "four"]
        );
    }

    #[test]
    fn preserves_body_whitespace_and_adds_json_content_type() {
        let mut request = draft();
        request.method = "POST".into();
        request.body_mode = BodyMode::Json;
        request.body = "  {\"ok\":true}\n".into();
        let built = build(&request).unwrap();
        assert_eq!(built.body.as_deref(), Some(request.body.as_str()));
        assert!(
            built
                .headers
                .contains(&header("Content-Type", "application/json"))
        );
    }

    #[test]
    fn gives_text_bodies_the_same_content_type_in_native_preview_and_curl() {
        let mut request = draft();
        request.method = "POST".into();
        request.body_mode = BodyMode::Text;
        request.body = "plain text".into();
        assert!(
            build(&request)
                .unwrap()
                .headers
                .contains(&header("Content-Type", "text/plain; charset=utf-8"))
        );
        request
            .headers
            .push(pair("Content-Type", "application/xml"));
        let content_types: Vec<Header> = build(&request)
            .unwrap()
            .headers
            .into_iter()
            .filter(|h| h.key.eq_ignore_ascii_case("content-type"))
            .collect();
        assert_eq!(content_types, [header("Content-Type", "application/xml")]);
    }

    #[test]
    fn wraps_graphql_query_and_variables_in_a_json_payload() {
        let mut request = draft();
        request.method = "POST".into();
        request.body_mode = BodyMode::Graphql;
        request.body = "query Q($id: ID!) { node(id: $id) { id } }".into();
        request.variables = Some("{\"id\":\"{{id}}\"}".into());
        let built = build_with(&request, &tokens(&[("id", "42")])).unwrap();
        let body: serde_json::Value = serde_json::from_str(built.body.as_deref().unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({ "query": request.body, "variables": { "id": "42" } })
        );
        assert!(
            built
                .headers
                .contains(&header("Content-Type", "application/json"))
        );
        request.variables = Some("  ".into());
        let body: serde_json::Value =
            serde_json::from_str(build(&request).unwrap().body.as_deref().unwrap()).unwrap();
        assert_eq!(body, serde_json::json!({ "query": request.body }));
    }

    #[test]
    fn rejects_empty_graphql_queries_and_invalid_variables() {
        let mut request = draft();
        request.method = "POST".into();
        request.body_mode = BodyMode::Graphql;
        request.body = "  ".into();
        assert!(build(&request).unwrap_err().contains("GraphQL query"));
        request.body = "{ ok }".into();
        request.variables = Some("{".into());
        assert!(build(&request).unwrap_err().contains("GraphQL variables"));
        request.variables = Some("[1]".into());
        assert!(build(&request).unwrap_err().contains("GraphQL variables"));
    }

    #[test]
    fn rejects_relative_or_malformed_http_urls() {
        for url in [
            "https:example.test",
            "http:/example.test",
            "http:///example.test",
            "/resource",
        ] {
            let request = Draft {
                url: url.into(),
                ..draft()
            };
            assert!(build(&request).is_err(), "{url}");
        }
    }

    #[test]
    fn omits_get_and_head_bodies_without_destroying_the_draft() {
        let mut request = draft();
        request.body_mode = BodyMode::Json;
        request.body = "invalid retained draft".into();
        assert_eq!(build(&request).unwrap().body, None);
        request.method = "HEAD".into();
        assert_eq!(build(&request).unwrap().body, None);
        assert_eq!(request.body, "invalid retained draft");
    }

    #[test]
    fn rejects_invalid_json_unsupported_schemes_and_header_injection() {
        let mut request = draft();
        request.method = "POST".into();
        request.body_mode = BodyMode::Json;
        request.body = "{".into();
        assert!(build(&request).unwrap_err().contains("Invalid JSON"));
        request.url = "file:///etc/hosts".into();
        assert!(build(&request).unwrap_err().contains("HTTP"));
        request.url = "https://example.test".into();
        request.headers = vec![pair("X-Test", "a\r\nInjected: b")];
        assert!(build(&request).unwrap_err().contains("Line breaks"));
    }

    #[test]
    fn detects_conflicting_authorization_headers() {
        let mut request = draft();
        request.auth = AuthKind::Bearer;
        request.token = "synthetic".into();
        request.headers = vec![pair("authorization", "Basic abc")];
        assert!(build(&request).unwrap_err().contains("Authorization"));
    }

    #[test]
    fn encodes_unicode_basic_authentication() {
        let mut request = draft();
        request.auth = AuthKind::Basic;
        request.username = "tést".into();
        request.password = "value".into();
        assert_eq!(
            authorization(&build(&request).unwrap()),
            basic("tést:value")
        );
        assert_eq!(
            authorization(&build(&request).unwrap()),
            "Basic dMOpc3Q6dmFsdWU="
        );
    }

    #[test]
    fn quotes_shell_text_and_uses_head_correctly() {
        let mut request = build(&draft()).unwrap();
        request.method = "HEAD".into();
        request.headers = vec![header("X-Test", "a'b $(echo bad)")];
        let command = to_curl(&request, &TransportOptions::default());
        assert!(command.contains("--head"));
        assert!(!command.contains("--request HEAD"));
        assert!(command.contains("'X-Test: a'\"'\"'b $(echo bad)'"));
        assert!(command.contains(" \\\n  "));
    }

    // methods and body modes

    fn bare(changes: impl FnOnce(&mut Draft)) -> Draft {
        let mut draft = Draft {
            url: "https://api.test/items".into(),
            headers: vec![],
            ..create_draft()
        };
        changes(&mut draft);
        draft
    }

    #[test]
    fn sends_a_custom_method_with_a_body() {
        let request = build(&bare(|d| {
            d.method = "PURGE".into();
            d.body_mode = BodyMode::Text;
            d.body = "x".into();
        }))
        .unwrap();
        assert_eq!(request.method, "PURGE");
        assert_eq!(request.body.as_deref(), Some("x"));
    }

    #[test]
    fn rejects_a_method_that_is_not_an_http_token() {
        assert!(
            build(&bare(|d| d.method = "GET ME".into()))
                .unwrap_err()
                .contains("method name")
        );
    }

    #[test]
    fn encodes_enabled_form_rows_as_application_x_www_form_urlencoded() {
        let request = build_with(
            &bare(|d| {
                d.method = "POST".into();
                d.body_mode = BodyMode::Form;
                d.form = Some(vec![
                    pair("name", "{{who}}"),
                    pair("q", "a b&c"),
                    off(pair("skip", "1")),
                ]);
            }),
            &tokens(&[("who", "Ada")]),
        )
        .unwrap();
        assert_eq!(request.body.as_deref(), Some("name=Ada&q=a+b%26c"));
        assert!(
            request
                .headers
                .contains(&header("Content-Type", "application/x-www-form-urlencoded"))
        );
    }

    #[test]
    fn builds_multipart_parts_and_leaves_the_boundary_to_the_transport() {
        let request = build_with(
            &bare(|d| {
                d.method = "POST".into();
                d.body_mode = BodyMode::Multipart;
                d.form = Some(vec![
                    pair("title", "{{who}}"),
                    file_row("upload", "/tmp/photo.png"),
                ]);
            }),
            &tokens(&[("who", "Ada")]),
        )
        .unwrap();
        assert_eq!(request.body, None);
        assert_eq!(
            request.multipart.as_deref().unwrap(),
            [
                MultipartPart {
                    key: "title".into(),
                    value: "Ada".into(),
                    file: false
                },
                MultipartPart {
                    key: "upload".into(),
                    value: "/tmp/photo.png".into(),
                    file: true
                },
            ]
        );
        assert!(request.headers.is_empty());
        let curl = to_curl(&request, &TransportOptions::default());
        assert!(curl.contains("--form-string 'title=Ada'"));
        assert!(curl.contains("--form 'upload=@/tmp/photo.png'"));
    }

    #[test]
    fn rejects_a_content_type_header_on_a_multipart_body() {
        let error = build(&bare(|d| {
            d.method = "POST".into();
            d.body_mode = BodyMode::Multipart;
            d.headers = vec![pair("Content-Type", "multipart/form-data")];
        }))
        .unwrap_err();
        assert!(error.contains("Remove the Content-Type header"));
    }

    #[test]
    fn requires_a_picked_file_for_a_file_part_and_for_a_file_body() {
        let error = build(&bare(|d| {
            d.method = "POST".into();
            d.body_mode = BodyMode::Multipart;
            d.form = Some(vec![file_row("upload", "")]);
        }))
        .unwrap_err();
        assert!(error.contains("Choose a file for the upload part."));
        let error = build(&bare(|d| {
            d.method = "POST".into();
            d.body_mode = BodyMode::File;
        }))
        .unwrap_err();
        assert!(error.contains("Choose a file"));
    }

    #[test]
    fn sends_a_file_body_as_octet_stream_unless_a_content_type_is_set() {
        let request = build(&bare(|d| {
            d.method = "PUT".into();
            d.body_mode = BodyMode::File;
            d.body_file = Some("/tmp/a.bin".into());
        }))
        .unwrap();
        assert_eq!(request.body_file.as_deref(), Some("/tmp/a.bin"));
        assert!(
            request
                .headers
                .contains(&header("Content-Type", "application/octet-stream"))
        );
        assert!(
            to_curl(&request, &TransportOptions::default()).contains("--data-binary '@/tmp/a.bin'")
        );
    }

    // cURL export options

    fn plain_request() -> RequestInput {
        build(&Draft {
            url: "https://api.test/".into(),
            ..create_draft()
        })
        .unwrap()
    }

    #[test]
    fn uses_the_configured_timeouts_and_redirect_policy() {
        let curl = to_curl(
            &plain_request(),
            &TransportOptions {
                timeout_seconds: 90,
                connect_timeout_seconds: 5,
                follow_redirects: true,
                max_redirects: 3,
                ..TransportOptions::default()
            },
        );
        assert!(curl.contains("--max-time 90 --connect-timeout 5"));
        assert!(curl.contains("--location --max-redirs 3"));
        assert!(!curl.contains("--insecure"));
    }

    #[test]
    fn adds_insecure_and_proxy_when_set() {
        let curl = to_curl(
            &plain_request(),
            &TransportOptions {
                verify_tls: false,
                proxy_url: "http://127.0.0.1:8080".into(),
                ..TransportOptions::default()
            },
        );
        assert!(curl.contains("--insecure"));
        assert!(curl.contains("--proxy 'http://127.0.0.1:8080'"));
    }

    #[test]
    fn formats_bytes_like_to_fixed() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1280), "1.3 KiB");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.00 MiB");
    }

    // buildRequest with ResolvedRequestContext (request-resolved-auth.test.ts)

    fn resolved(auth: AuthorizationConfig, entries: &[(&str, &str)]) -> ResolvedRequestContext {
        ResolvedRequestContext {
            auth,
            tokens: tokens(entries),
        }
    }
    fn base() -> Draft {
        Draft {
            url: "https://example.test/api".into(),
            ..create_draft()
        }
    }

    #[test]
    fn uses_resolved_bearer_auth_from_context_ignoring_draft_flat_auth() {
        let ctx = resolved(
            AuthorizationConfig::Bearer {
                token: "resolved-tok".into(),
            },
            &[],
        );
        let request = build_request(&base(), Some((&ctx).into())).unwrap();
        assert!(
            request
                .headers
                .contains(&header("Authorization", "Bearer resolved-tok"))
        );
    }

    #[test]
    fn uses_resolved_basic_auth_from_context() {
        let ctx = resolved(
            AuthorizationConfig::Basic {
                username: "alice".into(),
                password: "pw".into(),
            },
            &[],
        );
        let value = authorization(&build_request(&base(), Some((&ctx).into())).unwrap());
        let decoded = BASE64
            .decode(value.strip_prefix("Basic ").unwrap())
            .unwrap();
        assert_eq!(decoded, b"alice:pw");
    }

    #[test]
    fn resolved_auth_none_skips_authorization_header() {
        let ctx = resolved(AuthorizationConfig::None, &[]);
        let request = build_request(&base(), Some((&ctx).into())).unwrap();
        assert!(!request.headers.iter().any(|h| h.key == "Authorization"));
    }

    #[test]
    fn interpolates_tokens_from_context_definitions() {
        let draft = Draft {
            url: "https://{{host}}/api".into(),
            ..base()
        };
        let ctx = resolved(AuthorizationConfig::None, &[("host", "api.example.com")]);
        assert!(
            build_request(&draft, Some((&ctx).into()))
                .unwrap()
                .url
                .contains("api.example.com")
        );
    }

    #[test]
    fn bearer_token_from_context_is_interpolated_using_definitions() {
        let ctx = resolved(
            AuthorizationConfig::Bearer {
                token: "{{myTok}}".into(),
            },
            &[("myTok", "tok-value")],
        );
        let request = build_request(&base(), Some((&ctx).into())).unwrap();
        assert!(
            request
                .headers
                .contains(&header("Authorization", "Bearer tok-value"))
        );
    }

    #[test]
    fn backward_compat_no_context_uses_draft_flat_auth_fields_as_before() {
        let draft = Draft {
            auth: AuthKind::Bearer,
            token: "flat-tok".into(),
            ..base()
        };
        assert!(
            build(&draft)
                .unwrap()
                .headers
                .contains(&header("Authorization", "Bearer flat-tok"))
        );
    }

    #[test]
    fn uses_the_request_local_auth_override_when_no_context_is_supplied() {
        let draft = Draft {
            local_auth: Some(AuthorizationConfig::Bearer {
                token: "local-tok".into(),
            }),
            ..base()
        };
        assert!(
            build(&draft)
                .unwrap()
                .headers
                .contains(&header("Authorization", "Bearer local-tok"))
        );
    }

    // buildRequest with interpolation context (interpolation.test.ts)

    fn base_draft() -> Draft {
        Draft {
            url: "https://example.test/resource".into(),
            ..create_draft()
        }
    }

    #[test]
    fn interpolates_tokens_in_the_base_url() {
        let d = Draft {
            url: "https://{{host}}/path".into(),
            ..base_draft()
        };
        let request = build_with(&d, &tokens(&[("host", "api.example.com")])).unwrap();
        assert!(request.url.contains("https://api.example.com/path"));
    }

    #[test]
    fn errors_on_a_missing_url_token_naming_the_reference() {
        let d = Draft {
            url: "https://{{host}}/path".into(),
            ..base_draft()
        };
        assert!(build_with(&d, &tokens(&[])).unwrap_err().contains("host"));
    }

    #[test]
    fn interpolates_enabled_query_row_values() {
        let d = Draft {
            query: vec![pair("filter", "{{val}}")],
            ..base_draft()
        };
        assert!(
            build_with(&d, &tokens(&[("val", "active")]))
                .unwrap()
                .url
                .contains("filter=active")
        );
    }

    #[test]
    fn interpolates_enabled_query_row_keys() {
        let d = Draft {
            query: vec![pair("{{paramName}}", "1")],
            ..base_draft()
        };
        assert!(
            build_with(&d, &tokens(&[("paramName", "page")]))
                .unwrap()
                .url
                .contains("page=1")
        );
    }

    #[test]
    fn interpolates_header_values_but_leaves_header_names_untouched() {
        let d = Draft {
            headers: vec![pair("X-Api-Key", "{{apiKey}}")],
            ..base_draft()
        };
        let request = build_with(&d, &tokens(&[("apiKey", "secret123")])).unwrap();
        assert!(request.headers.contains(&header("X-Api-Key", "secret123")));
    }

    #[test]
    fn errors_on_a_missing_header_value_token() {
        let d = Draft {
            headers: vec![pair("X-Api-Key", "{{apiKey}}")],
            ..base_draft()
        };
        assert!(build_with(&d, &tokens(&[])).unwrap_err().contains("apiKey"));
    }

    fn post(mode: BodyMode, body: &str) -> Draft {
        Draft {
            method: "POST".into(),
            body_mode: mode,
            body: body.into(),
            ..base_draft()
        }
    }

    #[test]
    fn interpolates_tokens_in_a_text_body() {
        let d = post(BodyMode::Text, "Hello {{name}}");
        let request = build_with(&d, &tokens(&[("name", "world")])).unwrap();
        assert_eq!(request.body.as_deref(), Some("Hello world"));
    }

    #[test]
    fn interpolates_tokens_in_a_json_body_then_validates_the_result() {
        let d = post(BodyMode::Json, "{\"key\":\"{{val}}\"}");
        let request = build_with(&d, &tokens(&[("val", "hello")])).unwrap();
        assert_eq!(request.body.as_deref(), Some("{\"key\":\"hello\"}"));
    }

    #[test]
    fn validates_json_body_after_interpolation_not_before() {
        let d = post(BodyMode::Json, "{{jsonFragment}}");
        let request = build_with(&d, &tokens(&[("jsonFragment", "{\"ok\":true}")])).unwrap();
        assert_eq!(request.body.as_deref(), Some("{\"ok\":true}"));
    }

    #[test]
    fn rejects_an_invalid_json_body_after_interpolation() {
        let d = post(BodyMode::Json, "{{fragment}}");
        assert!(
            build_with(&d, &tokens(&[("fragment", "not-json")]))
                .unwrap_err()
                .contains("Invalid JSON")
        );
    }

    #[test]
    fn interpolates_bearer_token_before_building_the_authorization_header() {
        let d = Draft {
            auth: AuthKind::Bearer,
            token: "{{tok}}".into(),
            ..base_draft()
        };
        let request = build_with(&d, &tokens(&[("tok", "mytoken")])).unwrap();
        assert!(
            request
                .headers
                .contains(&header("Authorization", "Bearer mytoken"))
        );
    }

    #[test]
    fn interpolates_basic_auth_username_and_password() {
        let d = Draft {
            auth: AuthKind::Basic,
            username: "{{user}}".into(),
            password: "{{pass}}".into(),
            ..base_draft()
        };
        let request = build_with(&d, &tokens(&[("user", "alice"), ("pass", "s3cr3t")])).unwrap();
        assert_eq!(authorization(&request), basic("alice:s3cr3t"));
    }

    #[test]
    fn existing_manual_authorization_header_conflict_still_triggers_error_after_interpolation_resolves_auth()
     {
        let d = Draft {
            auth: AuthKind::Bearer,
            token: "{{tok}}".into(),
            headers: vec![pair("authorization", "Basic abc")],
            ..base_draft()
        };
        assert!(
            build_with(&d, &tokens(&[("tok", "xyz")]))
                .unwrap_err()
                .contains("Authorization")
        );
    }

    #[test]
    fn resolves_global_name_from_workspace_definitions() {
        let d = Draft {
            url: "https://example.test/{{_.version}}/path".into(),
            ..base_draft()
        };
        let ctx = InterpolationContext::new(defs(&[]), defs(&[("version", "v2")]));
        assert!(build_with(&d, &ctx).unwrap().url.contains("/v2/path"));
    }

    #[test]
    fn backward_compatible_no_context_argument_still_works() {
        let d = Draft {
            auth: AuthKind::Bearer,
            token: "plain-token".into(),
            ..base_draft()
        };
        assert!(
            build(&d)
                .unwrap()
                .headers
                .contains(&header("Authorization", "Bearer plain-token"))
        );
    }

    #[test]
    fn backward_compatible_undefined_context_does_not_throw_on_literal_values() {
        assert!(build(&base_draft()).is_ok());
    }
}
