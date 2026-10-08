//! OpenAPI, Postman, and `.http` import. Port of `src/lib/import.ts`.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::{Map, Value};

use crate::curl_import::{contains_ignore_ascii_case, is_js_space, js_trim};
use crate::model::{
    AuthorizationConfig, BodyMode, Definitions, Draft, ImportFormat, ImportResult, ImportedGroup,
    Pair,
};
use crate::request::{create_draft, is_method, pair};

const MAX_NAME: usize = 80;

/// The first `units` UTF-16 code units of `text`, as `String.slice` counts.
pub(crate) fn slice_utf16(text: &str, units: usize) -> &str {
    let mut count = 0;
    for (at, char) in text.char_indices() {
        count += char.len_utf16();
        if count > units {
            return &text[..at];
        }
    }
    text
}

fn group_name(name: Option<&Value>, fallback: &str) -> String {
    let name = match name {
        Some(Value::String(text)) if !js_trim(text).is_empty() => js_trim(text),
        _ => fallback,
    };
    slice_utf16(name, MAX_NAME).to_string()
}

fn empty() -> &'static Map<String, Value> {
    static EMPTY: OnceLock<Map<String, Value>> = OnceLock::new();
    EMPTY.get_or_init(Map::new)
}

fn object(value: Option<&Value>) -> &Map<String, Value> {
    match value {
        Some(Value::Object(map)) => map,
        _ => empty(),
    }
}

fn list(value: Option<&Value>) -> &[Value] {
    match value {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

fn str(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => js_number(number.as_f64().unwrap_or(0.0)),
        Some(Value::Bool(flag)) => flag.to_string(),
        _ => String::new(),
    }
}

/// JavaScript truthiness of a JSON value; `None` is `undefined`.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

/// JavaScript `??`: `None` for undefined or null.
fn present(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| !value.is_null())
}

/// JavaScript `String(number)`.
pub(crate) fn js_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if value == 0.0 {
        return "0".into();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    // Shortest round-trip digits, then JavaScript's Number::toString layout.
    let exp = format!("{:e}", value.abs());
    let (mantissa, exponent) = exp.split_once('e').unwrap_or((&exp, "0"));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exponent.parse::<i32>().unwrap_or(0) + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let e = if e < 0 {
            format!("-{}", -e)
        } else {
            format!("+{e}")
        };
        if k == 1 {
            format!("{digits}e{e}")
        } else {
            format!("{}.{}e{e}", &digits[..1], &digits[1..])
        }
    };
    format!("{sign}{body}")
}

/// A key JavaScript treats as an array index, which object iteration puts first.
fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key.starts_with('0')) {
        return None;
    }
    if !key.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    key.parse::<u32>().ok().filter(|index| *index < u32::MAX)
}

/// `Object.entries` order: array-index keys ascending, then insertion order.
pub(crate) fn js_entries(map: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut indexed: Vec<(u32, (&String, &Value))> = Vec::new();
    let mut named = Vec::new();
    for entry in map {
        match array_index(entry.0) {
            Some(index) => indexed.push((index, entry)),
            None => named.push(entry),
        }
    }
    indexed.sort_by_key(|(index, _)| *index);
    indexed
        .into_iter()
        .map(|(_, entry)| entry)
        .chain(named)
        .collect()
}

/// `JSON.stringify(value, null, 2)`.
pub(crate) fn js_stringify_pretty(value: &Value) -> String {
    fn write(out: &mut String, value: &Value, depth: usize) {
        match value {
            Value::Null => out.push_str("null"),
            Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
            Value::Number(number) => {
                let n = number.as_f64().unwrap_or(0.0);
                out.push_str(&if n.is_finite() {
                    js_number(n)
                } else {
                    "null".into()
                });
            }
            Value::String(text) => out.push_str(&serde_json::to_string(text).unwrap_or_default()),
            Value::Array(items) if items.is_empty() => out.push_str("[]"),
            Value::Array(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    newline(out, depth + 1);
                    write(out, item, depth + 1);
                }
                newline(out, depth);
                out.push(']');
            }
            Value::Object(map) if map.is_empty() => out.push_str("{}"),
            Value::Object(map) => {
                out.push('{');
                for (index, (key, item)) in js_entries(map).into_iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    newline(out, depth + 1);
                    out.push_str(&serde_json::to_string(key).unwrap_or_default());
                    out.push_str(": ");
                    write(out, item, depth + 1);
                }
                newline(out, depth);
                out.push('}');
            }
        }
    }
    fn newline(out: &mut String, depth: usize) {
        out.push('\n');
        out.push_str(&"  ".repeat(depth));
    }
    let mut out = String::new();
    write(&mut out, value, 0);
    out
}

pub fn count_requests(group: &ImportedGroup) -> usize {
    group.requests.len() + group.groups.iter().map(count_requests).sum::<usize>()
}

struct Body {
    mode: BodyMode,
    text: Option<String>,
    form: Option<Vec<(String, String)>>,
}

fn draft(
    method: &str,
    url: &str,
    headers: Vec<(String, String, bool)>,
    body: Option<Body>,
) -> Draft {
    let mut result = create_draft();
    let upper = method.to_uppercase();
    result.method = if is_method(&upper) {
        upper
    } else {
        "GET".into()
    };
    result.url = url.to_string();
    result.headers = if headers.is_empty() {
        vec![pair("", "")]
    } else {
        headers
            .into_iter()
            .map(|(key, value, enabled)| Pair {
                enabled,
                ..pair(key, value)
            })
            .collect()
    };
    if let Some(body) = body {
        result.body_mode = body.mode;
        result.body = body.text.unwrap_or_default();
        if let Some(form) = body.form {
            result.form = Some(
                form.into_iter()
                    .map(|(key, value)| pair(key, value))
                    .collect(),
            );
        }
    }
    result
}

// ── OpenAPI 3 and Swagger 2 ─────────────────────────────────────────────────

const OPERATIONS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

fn example_of(schema: &Map<String, Value>, depth: usize) -> Value {
    if depth > 6 {
        return Value::Null;
    }
    if let Some(example) = schema.get("example") {
        return example.clone();
    }
    if let Some(default) = schema.get("default") {
        return default.clone();
    }
    let kind = schema.get("type").and_then(Value::as_str);
    if let Some(first) = list(schema.get("enum")).first() {
        return first.clone();
    }
    if kind == Some("object") || truthy(schema.get("properties")) {
        let mut result = Map::new();
        for (key, value) in js_entries(object(schema.get("properties"))) {
            result.insert(key.clone(), example_of(object(Some(value)), depth + 1));
        }
        return Value::Object(result);
    }
    match kind {
        Some("array") => Value::Array(vec![example_of(object(schema.get("items")), depth + 1)]),
        Some("integer" | "number") => Value::from(0),
        Some("boolean") => Value::Bool(false),
        Some("string") => Value::String(String::new()),
        _ => Value::Null,
    }
}

/// Resolve a local `$ref` such as `#/components/schemas/User`.
fn resolve<'a>(spec: &'a Value, value: Option<&'a Value>) -> &'a Map<String, Value> {
    resolve_map(spec, object(value))
}

fn resolve_map<'a>(spec: &'a Value, mut item: &'a Map<String, Value>) -> &'a Map<String, Value> {
    let mut seen: Vec<&str> = Vec::new();
    loop {
        let Some(Value::String(reference)) = item.get("$ref") else {
            return item;
        };
        let Some(path) = reference.strip_prefix("#/") else {
            return item;
        };
        if seen.contains(&reference.as_str()) {
            return item;
        }
        seen.push(reference);
        let mut target = Some(spec);
        for part in path.split('/') {
            target = object(target).get(&part.replace("~1", "/").replace("~0", "~"));
        }
        item = object(target);
    }
}

/// `/[^\w.-]/g` to `_`, one per UTF-16 unit.
fn token_name(name: &str) -> String {
    let mut out = String::new();
    for char in name.chars() {
        if char.is_ascii_alphanumeric() || matches!(char, '_' | '.' | '-') {
            out.push(char);
        } else {
            out.push_str(&"_".repeat(char.len_utf16()));
        }
    }
    out
}

fn import_openapi_source(spec: &Value, source: &str) -> ImportResult {
    let root_map = object(Some(spec));
    let info = object(root_map.get("info"));
    let mut skipped = Vec::new();
    let mut base_url;
    if truthy(root_map.get("swagger")) {
        let scheme = str(list(root_map.get("schemes")).first());
        let scheme = if scheme.is_empty() {
            "https".into()
        } else {
            scheme
        };
        base_url = if truthy(root_map.get("host")) {
            format!(
                "{scheme}://{}{}",
                str(root_map.get("host")),
                str(root_map.get("basePath"))
            )
        } else {
            String::new()
        };
    } else {
        let server = object(list(root_map.get("servers")).first());
        base_url = str(server.get("url"));
        for (name, variable) in js_entries(object(server.get("variables"))) {
            base_url = base_url.replace(
                &format!("{{{name}}}"),
                &str(object(Some(variable)).get("default")),
            );
        }
    }
    if base_url.ends_with('/') {
        base_url.pop();
    }
    let mut definitions = Definitions::new();
    definitions.insert("baseUrl".into(), base_url.clone());
    let mut root = ImportedGroup {
        name: group_name(info.get("title"), "OpenAPI import"),
        ..Default::default()
    };
    // Tag groups in first-seen order, by the raw tag.
    let mut tags: Vec<String> = Vec::new();
    let components = object(root_map.get("components"));
    let bearer = js_entries(object(components.get("securitySchemes")))
        .into_iter()
        .any(|(_, scheme)| {
            let value = object(Some(scheme));
            value.get("type").and_then(Value::as_str) == Some("http")
                && str(value.get("scheme")).to_lowercase() == "bearer"
        });
    // An empty token would fail every send, so auth stays for the user to set.
    if bearer {
        skipped.push("Bearer authentication: set a token in the group settings.".to_string());
    }

    for (path, path_item) in js_entries(object(root_map.get("paths"))) {
        let path_map = resolve(spec, Some(path_item));
        let shared = list(path_map.get("parameters"));
        for method in OPERATIONS {
            if !path_map.contains_key(method) {
                continue;
            }
            let operation = object(path_map.get(method));
            let parameters: Vec<&Map<String, Value>> = shared
                .iter()
                .chain(list(operation.get("parameters")))
                .map(|parameter| resolve(spec, Some(parameter)))
                .collect();
            let mut query: Vec<(String, String, bool)> = Vec::new();
            let mut headers: Vec<(String, String, bool)> = Vec::new();
            let mut url = format!("{{{{baseUrl}}}}{path}");
            for parameter in &parameters {
                let name = str(parameter.get("name"));
                if name.is_empty() {
                    continue;
                }
                let example = match present(parameter.get("example")) {
                    Some(example) => str(Some(example)),
                    None => {
                        let target = match present(parameter.get("schema")) {
                            Some(schema) => resolve(spec, Some(schema)),
                            None => resolve_map(spec, parameter),
                        };
                        str(Some(&example_of(target, 0)))
                    }
                };
                let required = parameter.get("required") == Some(&Value::Bool(true));
                match parameter.get("in").and_then(Value::as_str) {
                    Some("path") => {
                        // {id} becomes a token defined on the group.
                        let token = token_name(&name);
                        url = url.replace(&format!("{{{name}}}"), &format!("{{{{{token}}}}}"));
                        definitions.entry(token).or_insert(example);
                    }
                    Some("query") => query.push((name, example, required)),
                    Some("header") => headers.push((name, example, required)),
                    _ => {}
                }
            }
            let request_body = resolve(spec, operation.get("requestBody"));
            let content = object(request_body.get("content"));
            let json = js_entries(content)
                .into_iter()
                .find(|(kind, _)| contains_ignore_ascii_case(kind, "json"));
            let form = content.get("application/x-www-form-urlencoded");
            let swagger_body = parameters
                .iter()
                .find(|p| p.get("in").and_then(Value::as_str) == Some("body"));
            let mut body = None;
            if json.is_some() || swagger_body.is_some() {
                let media = object(json.map(|(_, media)| media));
                let schema_value = present(media.get("schema"))
                    .or_else(|| swagger_body.and_then(|b| b.get("schema")));
                let schema = resolve(spec, schema_value);
                let first_example = js_entries(object(media.get("examples")))
                    .into_iter()
                    .next()
                    .map(|(_, example)| example);
                let example = present(media.get("example"))
                    .cloned()
                    .or_else(|| present(object(first_example).get("value")).cloned())
                    .unwrap_or_else(|| example_of(schema, 0));
                let example = if example.is_null() {
                    Value::Object(Map::new())
                } else {
                    example
                };
                body = Some(Body {
                    mode: BodyMode::Json,
                    text: Some(js_stringify_pretty(&example)),
                    form: None,
                });
            } else if truthy(form) {
                let schema = resolve(spec, object(form).get("schema"));
                body = Some(Body {
                    mode: BodyMode::Form,
                    text: None,
                    form: Some(
                        js_entries(object(schema.get("properties")))
                            .into_iter()
                            .map(|(key, _)| (key.clone(), String::new()))
                            .collect(),
                    ),
                });
            }
            let mut all_headers =
                vec![("Accept".to_string(), "application/json".to_string(), true)];
            all_headers.extend(headers);
            let mut request = draft(method, &url, all_headers, body);
            request.query = if query.is_empty() {
                vec![pair("", "")]
            } else {
                query
                    .into_iter()
                    .map(|(key, value, enabled)| Pair {
                        enabled,
                        ..pair(key, value)
                    })
                    .collect()
            };
            if spec.get("openapi").is_some() {
                request.openapi_contract = Some(crate::openapi_contract::OpenApiContract::new(
                    source, spec, path, method,
                ));
            }
            let tag = str(list(operation.get("tags")).first());
            if tag.is_empty() {
                root.requests.push(request);
            } else {
                let index = match tags.iter().position(|seen| *seen == tag) {
                    Some(index) => index,
                    None => {
                        root.groups.push(ImportedGroup {
                            name: group_name(Some(&Value::String(tag.clone())), "Untagged"),
                            ..Default::default()
                        });
                        tags.push(tag);
                        root.groups.len() - 1
                    }
                };
                root.groups[index].requests.push(request);
            }
        }
    }
    if base_url.is_empty() {
        skipped.push("No server URL. Set baseUrl in the group settings.".to_string());
    }
    root.definitions = Some(definitions);
    ImportResult {
        format: ImportFormat::Openapi,
        root,
        skipped,
    }
}

// ── Postman collection v2.0 / v2.1 ──────────────────────────────────────────

fn postman_auth(value: Option<&Value>) -> Option<AuthorizationConfig> {
    let auth = object(value);
    let entries = |kind: &str| -> HashMap<String, String> {
        list(auth.get(kind))
            .iter()
            .map(|entry| {
                let entry = object(Some(entry));
                (str(entry.get("key")), str(entry.get("value")))
            })
            .collect()
    };
    match auth.get("type").and_then(Value::as_str) {
        Some("noauth") => Some(AuthorizationConfig::None),
        Some("bearer") => Some(AuthorizationConfig::Bearer {
            token: entries("bearer").remove("token").unwrap_or_default(),
        }),
        Some("basic") => {
            let mut basic = entries("basic");
            Some(AuthorizationConfig::Basic {
                username: basic.remove("username").unwrap_or_default(),
                password: basic.remove("password").unwrap_or_default(),
            })
        }
        _ => None,
    }
}

fn postman_url(value: Option<&Value>) -> String {
    if let Some(Value::String(url)) = value {
        return url.clone();
    }
    let url = object(value);
    if truthy(url.get("raw")) {
        return str(url.get("raw"));
    }
    let join = |key: &str, separator: &str| {
        list(url.get(key))
            .iter()
            .map(|part| str(Some(part)))
            .collect::<Vec<_>>()
            .join(separator)
    };
    let host = join("host", ".");
    let path = join("path", "/");
    let protocol = str(url.get("protocol"));
    let protocol = if protocol.is_empty() {
        "https".into()
    } else {
        protocol
    };
    let path = if path.is_empty() {
        String::new()
    } else {
        format!("/{path}")
    };
    format!("{protocol}://{host}{path}")
}

fn postman_folder(
    name: Option<&Value>,
    items: &[Value],
    auth: Option<&Value>,
    variables: Option<Definitions>,
    skipped: &mut Vec<String>,
) -> ImportedGroup {
    let mut group = ImportedGroup {
        name: group_name(name, "Postman import"),
        ..Default::default()
    };
    if let Some(variables) = variables.filter(|variables| !variables.is_empty()) {
        group.definitions = Some(variables);
    }
    group.auth = postman_auth(auth);
    for raw in items {
        let item = object(Some(raw));
        if let Some(Value::Array(children)) = item.get("item") {
            group.groups.push(postman_folder(
                item.get("name"),
                children,
                item.get("auth"),
                None,
                skipped,
            ));
            continue;
        }
        let owned;
        let request = match item.get("request") {
            Some(Value::String(url)) => {
                let mut map = Map::new();
                map.insert("url".into(), Value::String(url.clone()));
                map.insert("method".into(), Value::String("GET".into()));
                owned = map;
                &owned
            }
            other => object(other),
        };
        let item_name = str(item.get("name"));
        let url = postman_url(request.get("url"));
        if url.is_empty() {
            let label = if item_name.is_empty() {
                "Request"
            } else {
                &item_name
            };
            skipped.push(format!("{label}: no URL"));
            continue;
        }
        let headers = list(request.get("header"))
            .iter()
            .map(|entry| {
                let header = object(Some(entry));
                (
                    str(header.get("key")),
                    str(header.get("value")),
                    header.get("disabled") != Some(&Value::Bool(true)),
                )
            })
            .collect();
        let source = object(request.get("body"));
        let mode = source.get("mode").and_then(Value::as_str);
        let mut body = None;
        match mode {
            Some("raw") => {
                let options = object(source.get("options"));
                let language = str(object(options.get("raw")).get("language"));
                let text = str(source.get("raw"));
                let json = if language.is_empty() {
                    serde_json::from_str::<Value>(&text).is_ok()
                } else {
                    language == "json"
                };
                body = Some(Body {
                    mode: if json { BodyMode::Json } else { BodyMode::Text },
                    text: Some(text),
                    form: None,
                });
            }
            Some(kind @ ("urlencoded" | "formdata")) => {
                let rows: Vec<&Map<String, Value>> = list(source.get(kind))
                    .iter()
                    .map(|entry| object(Some(entry)))
                    .collect();
                let is_file = |row: &Map<String, Value>| {
                    row.get("type").and_then(Value::as_str) == Some("file")
                };
                if rows.iter().any(|row| is_file(row)) {
                    skipped.push(format!("{item_name}: file fields need a file"));
                }
                body = Some(Body {
                    mode: if kind == "urlencoded" {
                        BodyMode::Form
                    } else {
                        BodyMode::Multipart
                    },
                    text: None,
                    form: Some(
                        rows.iter()
                            .filter(|row| row.get("disabled") != Some(&Value::Bool(true)))
                            .map(|row| {
                                let value = if is_file(row) {
                                    String::new()
                                } else {
                                    str(row.get("value"))
                                };
                                (str(row.get("key")), value)
                            })
                            .collect(),
                    ),
                });
            }
            Some("graphql") => {
                let graphql = object(source.get("graphql"));
                body = Some(Body {
                    mode: BodyMode::Graphql,
                    text: Some(str(graphql.get("query"))),
                    form: None,
                });
            }
            _ => {}
        }
        let method = str(request.get("method"));
        let method = if method.is_empty() {
            "GET".into()
        } else {
            method
        };
        let mut result = draft(&method, &url, headers, body);
        if mode == Some("graphql") {
            result.variables = Some(str(object(source.get("graphql")).get("variables")));
        }
        if let Some(auth) = postman_auth(request.get("auth")) {
            result.local_auth = Some(auth);
        }
        group.requests.push(result);
    }
    group
}

fn import_postman(collection: &Value) -> ImportResult {
    let mut skipped = Vec::new();
    let map = object(Some(collection));
    let info = object(map.get("info"));
    let definitions: Definitions = list(map.get("variable"))
        .iter()
        .map(|entry| {
            let entry = object(Some(entry));
            (str(entry.get("key")), str(entry.get("value")))
        })
        .filter(|(key, _)| !key.is_empty() && !key.starts_with('_'))
        .collect();
    let root = postman_folder(
        info.get("name"),
        list(map.get("item")),
        map.get("auth"),
        Some(definitions),
        &mut skipped,
    );
    ImportResult {
        format: ImportFormat::Postman,
        root,
        skipped,
    }
}

// ── .http / .rest files (JetBrains HTTP Client, VS Code REST Client) ───────

fn is_line_break(char: char) -> bool {
    matches!(char, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `text.split(/^###.*$/m)`.
fn split_blocks(text: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut start = 0;
    let mut line_start = 0;
    loop {
        let line_end = text[line_start..]
            .find(is_line_break)
            .map_or(text.len(), |at| line_start + at);
        if text[line_start..line_end].starts_with("###") {
            blocks.push(&text[start..line_start]);
            start = line_end;
        }
        if line_end >= text.len() {
            break;
        }
        let next = text[line_end..].chars().next().map_or(1, char::len_utf8);
        line_start = line_end + next;
    }
    blocks.push(&text[start..]);
    blocks
}

/// Whether `.*$` matches all of `text`: no line terminators.
fn single_line(text: &str) -> bool {
    !text.contains(is_line_break)
}

/// `^@([\w.-]+)\s*=\s*(.*)$`
fn file_variable(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('@')?;
    let name_end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')))
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }
    let after = rest[name_end..].trim_start_matches(is_js_space);
    let value = after.strip_prefix('=')?.trim_start_matches(is_js_space);
    single_line(value).then_some((&rest[..name_end], value))
}

/// `^([^:\s]+)\s*:\s*(.*)$`
fn header_line(line: &str) -> Option<(&str, &str)> {
    let key_end = line
        .find(|c: char| c == ':' || is_js_space(c))
        .unwrap_or(line.len());
    if key_end == 0 {
        return None;
    }
    let after = line[key_end..].trim_start_matches(is_js_space);
    let value = after.strip_prefix(':')?.trim_start_matches(is_js_space);
    single_line(value).then_some((&line[..key_end], value))
}

/// `^(?:([A-Z]+)\s+)?((?:https?:\/\/|\{\{)\S*)(?:\s+HTTP\/[\d.]+)?\s*$`
///
/// "GET https://…", "POST {{host}}/…", or a bare URL. HTTP/1.1 is optional.
fn request_line(line: &str) -> Option<(Option<&str>, &str)> {
    fn target(rest: &str) -> Option<&str> {
        if !(rest.starts_with("http://") || rest.starts_with("https://") || rest.starts_with("{{"))
        {
            return None;
        }
        let end = rest.find(is_js_space).unwrap_or(rest.len());
        let tail = &rest[end..];
        let version = tail.trim_start_matches(is_js_space);
        let valid = version.chars().all(is_js_space)
            || version.strip_prefix("HTTP/").is_some_and(|number| {
                let digits = number.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
                digits.len() < number.len() && digits.chars().all(is_js_space)
            }) && version.len() < tail.len();
        valid.then_some(&rest[..end])
    }
    let method_end = line
        .find(|c: char| !c.is_ascii_uppercase())
        .unwrap_or(line.len());
    if method_end > 0 {
        let after = &line[method_end..];
        let rest = after.trim_start_matches(is_js_space);
        if rest.len() < after.len()
            && let Some(url) = target(rest)
        {
            return Some((Some(&line[..method_end]), url));
        }
    }
    target(line).map(|url| (None, url))
}

/// `/^\s+[?&]/`: a query continuation line, such as "    ?page=2".
fn is_query_continuation(line: &str) -> bool {
    let rest = line.trim_start_matches(is_js_space);
    rest.len() < line.len() && rest.starts_with(['?', '&'])
}

pub fn import_http(text: &str, name: &str) -> ImportResult {
    let mut root = ImportedGroup {
        name: group_name(Some(&Value::String(name.into())), "HTTP file"),
        ..Default::default()
    };
    let mut definitions = Definitions::new();
    let mut skipped = Vec::new();
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    for block in split_blocks(&normalized) {
        let lines: Vec<&str> = block.split('\n').collect();
        let mut index = 0;
        // File variables and comments before the request line.
        while index < lines.len() {
            let line = js_trim(lines[index]);
            if let Some((key, value)) = file_variable(line) {
                definitions.insert(key.to_string(), js_trim(value).to_string());
            } else if !line.is_empty() && !line.starts_with('#') && !line.starts_with("//") {
                break;
            }
            index += 1;
        }
        if index >= lines.len() {
            continue;
        }
        let first = js_trim(lines[index]);
        let Some((method, url)) = request_line(first) else {
            skipped.push(format!("Unknown request line: {}", slice_utf16(first, 60)));
            continue;
        };
        let mut url = url.to_string();
        let method = method.unwrap_or("GET");
        index += 1;
        // Query continuation lines, such as "    ?page=2" or "    &size=10".
        while index < lines.len() && is_query_continuation(lines[index]) {
            url += js_trim(lines[index]);
            index += 1;
        }
        let mut headers: Vec<(String, String, bool)> = Vec::new();
        while index < lines.len() && !js_trim(lines[index]).is_empty() {
            if let Some((key, value)) = header_line(js_trim(lines[index])) {
                headers.push((key.to_string(), value.to_string(), true));
            }
            index += 1;
        }
        let body = js_trim(&lines.get(index + 1..).unwrap_or_default().join("\n")).to_string();
        let content_type = headers
            .iter()
            .find(|(key, _, _)| key.to_lowercase() == "content-type")
            .map(|(_, value, _)| value.as_str())
            .unwrap_or("");
        let mode = if contains_ignore_ascii_case(content_type, "json") {
            BodyMode::Json
        } else {
            BodyMode::Text
        };
        let body = (!body.is_empty()).then_some(Body {
            mode,
            text: Some(body),
            form: None,
        });
        root.requests.push(draft(method, &url, headers, body));
    }
    if !definitions.is_empty() {
        root.definitions = Some(definitions);
    }
    ImportResult {
        format: ImportFormat::Http,
        root,
        skipped,
    }
}

/// A YAML value as the `yaml` package turns it into JavaScript.
fn yaml_to_json(value: serde_yaml_ng::Value) -> Value {
    use serde_yaml_ng::Value as Yaml;
    match value {
        Yaml::Null => Value::Null,
        Yaml::Bool(flag) => Value::Bool(flag),
        Yaml::Number(number) => {
            if let Some(n) = number.as_i64() {
                Value::from(n)
            } else if let Some(n) = number.as_u64() {
                Value::from(n)
            } else {
                // Infinity and NaN have no JSON form.
                number
                    .as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map_or(Value::Null, Value::Number)
            }
        }
        Yaml::String(text) => Value::String(text),
        Yaml::Sequence(items) => Value::Array(items.into_iter().map(yaml_to_json).collect()),
        Yaml::Mapping(mapping) => {
            let mut map = Map::new();
            for (key, item) in mapping {
                let key = match yaml_to_json(key) {
                    Value::String(text) => text,
                    Value::Null => String::new(),
                    Value::Bool(flag) => flag.to_string(),
                    Value::Number(number) => js_number(number.as_f64().unwrap_or(0.0)),
                    other => other.to_string(),
                };
                map.insert(key, yaml_to_json(item));
            }
            Value::Object(map)
        }
        Yaml::Tagged(tagged) => yaml_to_json(tagged.value),
    }
}

/// `/^(openapi|swagger)\s*:/m`
fn looks_like_yaml_spec(text: &str) -> bool {
    text.split(is_line_break).any(|line| {
        ["openapi", "swagger"].iter().any(|key| {
            line.strip_prefix(key)
                .is_some_and(|rest| rest.trim_start_matches(is_js_space).starts_with(':'))
        })
    })
}

/// Parse a file as OpenAPI (JSON or YAML), Postman, or an .http file.
pub fn parse_import(text: &str, file_name: &str) -> Result<ImportResult, String> {
    let trimmed = js_trim(text);
    if trimmed.is_empty() {
        return Err("The file is empty.".into());
    }
    let data: Option<Value> = if trimmed.starts_with('{') {
        Some(serde_json::from_str(trimmed).map_err(|_| "The file is not valid JSON.")?)
    } else if looks_like_yaml_spec(trimmed) {
        let yaml: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(trimmed).map_err(|_| "The file is not valid YAML.")?;
        Some(yaml_to_json(yaml))
    } else {
        None
    };
    if let Some(data) = &data {
        let spec = object(Some(data));
        if truthy(spec.get("openapi")) || truthy(spec.get("swagger")) {
            return Ok(import_openapi_source(data, file_name));
        }
        let info = object(spec.get("info"));
        if truthy(info.get("schema")) || (truthy(info.get("name")) && truthy(spec.get("item"))) {
            return Ok(import_postman(data));
        }
        return Err("Blink can import OpenAPI, Postman collections, and .http files.".into());
    }
    let display_name = std::path::Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(file_name);
    let result = import_http(text, &strip_http_extension(display_name));
    if result.root.requests.is_empty() {
        return Err(
            "No requests found. Blink can import OpenAPI, Postman collections, and .http files."
                .into(),
        );
    }
    Ok(result)
}

/// `fileName.replace(/\.(http|rest)$/i, '')`
fn strip_http_extension(file_name: &str) -> String {
    for extension in [".http", ".rest"] {
        if file_name.len() >= extension.len() {
            let at = file_name.len() - extension.len();
            if file_name.is_char_boundary(at) && file_name[at..].eq_ignore_ascii_case(extension) {
                return file_name[..at].to_string();
            }
        }
    }
    file_name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENAPI: &str = r##"openapi: 3.0.0
info:
  title: Pet Store
servers:
  - url: https://{env}.pets.test/v1/
    variables:
      env:
        default: api
components:
  securitySchemes:
    auth:
      type: http
      scheme: bearer
  schemas:
    Pet:
      type: object
      properties:
        name:
          type: string
          example: Rex
        age:
          type: integer
paths:
  /pets/{petId}:
    parameters:
      - name: petId
        in: path
        required: true
        schema:
          type: integer
          example: 7
    get:
      tags: [pets]
      parameters:
        - name: expand
          in: query
          schema:
            type: string
        - name: X-Trace
          in: header
          required: true
          schema:
            type: string
    put:
      tags: [pets]
      requestBody:
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/Pet"
  /health:
    get: {}
"##;

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn builds_tag_groups_tokens_params_and_bodies() {
        let ImportResult {
            format,
            root,
            skipped,
        } = parse_import(OPENAPI, "").unwrap();
        assert_eq!(format, ImportFormat::Openapi);
        assert_eq!(
            skipped,
            vec!["Bearer authentication: set a token in the group settings."]
        );
        assert_eq!(root.name, "Pet Store");
        assert_eq!(
            root.definitions,
            Some(defs(&[
                ("baseUrl", "https://api.pets.test/v1"),
                ("petId", "7")
            ]))
        );
        assert_eq!(root.auth, None);
        let urls: Vec<&str> = root.requests.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(urls, vec!["{{baseUrl}}/health"]);
        let group = &root.groups[0];
        let (get, put) = (&group.requests[0], &group.requests[1]);
        assert_eq!(group.name, "pets");
        assert_eq!(get.url, "{{baseUrl}}/pets/{{petId}}");
        assert_eq!(get.query.len(), 1);
        assert_eq!(get.query[0].key, "expand");
        assert!(!get.query[0].enabled);
        let headers: Vec<(&str, bool)> = get
            .headers
            .iter()
            .map(|h| (h.key.as_str(), h.enabled))
            .collect();
        assert_eq!(headers, vec![("Accept", true), ("X-Trace", true)]);
        assert_eq!(put.method, "PUT");
        assert_eq!(put.body_mode, BodyMode::Json);
        assert_eq!(
            serde_json::from_str::<Value>(&put.body).unwrap(),
            serde_json::json!({ "name": "Rex", "age": 0 })
        );
        assert_eq!(put.body, "{\n  \"name\": \"Rex\",\n  \"age\": 0\n}");
        assert_eq!(count_requests(&root), 3);
    }

    #[test]
    fn reads_swagger_2_json() {
        let text = serde_json::json!({
            "swagger": "2.0",
            "info": { "title": "Old" },
            "host": "old.test",
            "basePath": "/api",
            "schemes": ["http"],
            "paths": { "/a": { "post": {} } },
        })
        .to_string();
        let root = parse_import(&text, "").unwrap().root;
        assert_eq!(
            root.definitions.unwrap().get("baseUrl").map(String::as_str),
            Some("http://old.test/api")
        );
        assert_eq!(root.requests[0].method, "POST");
    }

    #[test]
    fn builds_folders_variables_auth_and_bodies() {
        let text = serde_json::json!({
            "info": {
                "name": "Shop",
                "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json",
            },
            "variable": [{ "key": "host", "value": "https://shop.test" }],
            "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "{{tok}}" }] },
            "item": [
                {
                    "name": "Orders",
                    "item": [
                        {
                            "name": "Create",
                            "request": {
                                "method": "POST",
                                "url": { "raw": "{{host}}/orders" },
                                "header": [
                                    { "key": "X-A", "value": "1" },
                                    { "key": "X-B", "value": "2", "disabled": true },
                                ],
                                "body": {
                                    "mode": "raw",
                                    "raw": "{\"a\":1}",
                                    "options": { "raw": { "language": "json" } },
                                },
                            },
                        },
                        {
                            "name": "Upload",
                            "request": {
                                "method": "POST",
                                "url": "{{host}}/upload",
                                "body": {
                                    "mode": "formdata",
                                    "formdata": [
                                        { "key": "note", "value": "x", "type": "text" },
                                        { "key": "file", "type": "file", "src": "/a" },
                                    ],
                                },
                                "auth": { "type": "noauth" },
                            },
                        },
                    ],
                },
                { "name": "Ping", "request": "https://shop.test/ping" },
            ],
        })
        .to_string();
        let ImportResult {
            format,
            root,
            skipped,
        } = parse_import(&text, "").unwrap();
        assert_eq!(format, ImportFormat::Postman);
        assert_eq!(root.name, "Shop");
        assert_eq!(
            root.definitions,
            Some(defs(&[("host", "https://shop.test")]))
        );
        assert_eq!(
            root.auth,
            Some(AuthorizationConfig::Bearer {
                token: "{{tok}}".into()
            })
        );
        assert_eq!(root.requests[0].url, "https://shop.test/ping");
        let (create, upload) = (&root.groups[0].requests[0], &root.groups[0].requests[1]);
        assert_eq!(create.body_mode, BodyMode::Json);
        let enabled: Vec<bool> = create.headers.iter().map(|h| h.enabled).collect();
        assert_eq!(enabled, vec![true, false]);
        assert_eq!(upload.body_mode, BodyMode::Multipart);
        assert_eq!(upload.local_auth, Some(AuthorizationConfig::None));
        assert_eq!(skipped, vec!["Upload: file fields need a file"]);
    }

    #[test]
    fn reads_requests_headers_bodies_and_file_variables() {
        let root = import_http(
            "@host = https://api.test
# comment
GET {{host}}/users
    ?page=2
Accept: application/json

### Create
POST {{host}}/users HTTP/1.1
Content-Type: application/json

{
  \"name\": \"Ada\"
}

###
https://api.test/plain
",
            "users",
        )
        .root;
        assert_eq!(root.name, "users");
        assert_eq!(
            root.definitions,
            Some(defs(&[("host", "https://api.test")]))
        );
        let requests: Vec<(&str, &str)> = root
            .requests
            .iter()
            .map(|r| (r.method.as_str(), r.url.as_str()))
            .collect();
        assert_eq!(
            requests,
            vec![
                ("GET", "{{host}}/users?page=2"),
                ("POST", "{{host}}/users"),
                ("GET", "https://api.test/plain"),
            ]
        );
        assert_eq!(root.requests[1].body_mode, BodyMode::Json);
        assert_eq!(
            serde_json::from_str::<Value>(&root.requests[1].body).unwrap(),
            serde_json::json!({ "name": "Ada" })
        );
    }

    #[test]
    fn rejects_files_without_requests() {
        assert!(
            parse_import("hello world", "")
                .unwrap_err()
                .contains("No requests")
        );
        assert!(
            parse_import("{\"a\":1}", "")
                .unwrap_err()
                .contains("can import")
        );
    }

    #[test]
    fn formats_numbers_as_javascript() {
        let cases = [
            (0.0, "0"),
            (7.0, "7"),
            (1.5, "1.5"),
            (-0.25, "-0.25"),
            (1e21, "1e+21"),
            (123456789012345680000.0, "123456789012345680000"),
            (1e-7, "1e-7"),
            (0.000001, "0.000001"),
            (1.2345e-9, "1.2345e-9"),
        ];
        for (value, expected) in cases {
            assert_eq!(js_number(value), expected);
        }
    }

    #[test]
    fn reads_yaml_as_the_yaml_package_does() {
        let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(
            "openapi: 3.0.0\nv: 0o14\nw: 0x1f\nt: True\nn: yes\nd: 2020-01-01\ne: 1e3\n200: a\n~: c",
        )
        .unwrap();
        assert_eq!(
            yaml_to_json(yaml),
            serde_json::json!({
                "openapi": "3.0.0", "v": 12, "w": 31, "t": true, "n": "yes",
                "d": "2020-01-01", "e": 1000.0, "200": "a", "": "c",
            })
        );
        assert!(serde_yaml_ng::from_str::<serde_yaml_ng::Value>("openapi: 3\na: 1\na: 2").is_err());
        assert!(serde_yaml_ng::from_str::<serde_yaml_ng::Value>("openapi: 3\n---\nb: 1").is_err());
    }

    #[test]
    fn orders_integer_keys_first_like_javascript() {
        let value: Value = serde_json::from_str(r#"{"b":1,"2":2,"1":3,"01":4}"#).unwrap();
        assert_eq!(
            js_stringify_pretty(&value),
            "{\n  \"1\": 3,\n  \"2\": 2,\n  \"b\": 1,\n  \"01\": 4\n}"
        );
    }
}
