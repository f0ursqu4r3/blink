//! Editor hints for `{{name}}`, `{{_.name}}` and `{{!NAME}}` references.
//! Hints show the value a reference resolves to. Port of `src/lib/token-hints.ts`.
//!
//! Offsets are byte offsets into UTF-8 text.

use std::sync::LazyLock;

use regex::Regex;

use crate::interpolation::{InterpolationContext, interpolate};
use crate::request::js_trim;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenScope {
    Local,
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenOption {
    /// Text between the braces, such as `host` or `_.host`.
    pub name: String,
    pub scope: TokenScope,
    /// The resolved value, or the raw value if it cannot resolve.
    pub value: String,
}

/// Set for a token reference. `Env` values resolve only when sending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenState {
    Resolved,
    Unresolved,
    Env,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSpan {
    pub text: String,
    /// Name inside the braces, for `{{name}}` references.
    pub name: Option<String>,
    pub token: Option<TokenState>,
}

impl TokenSpan {
    fn plain(text: &str) -> Self {
        TokenSpan {
            text: text.to_string(),
            name: None,
            token: None,
        }
    }
}

static REFERENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{!([^{}]*)\}\}|\{\{(_\.)?([^{}]+?)\}\}").unwrap());

/// The value `{{name}}` resolves to, with nested references resolved.
/// Returns None for an unknown name.
pub fn token_value(name: &str, ctx: Option<&InterpolationContext>) -> Option<String> {
    let ctx = ctx?;
    let workspace_only = name.starts_with("_.");
    let key = if workspace_only { &name[2..] } else { name };
    if !is_resolved(key, workspace_only, Some(ctx)) {
        return None;
    }
    match interpolate(&format!("{{{{{name}}}}}"), ctx) {
        Ok(value) => Some(value),
        // A cycle or a missing nested token: show the raw value.
        Err(_) => if workspace_only || !ctx.definitions.contains_key(key) {
            ctx.workspace_definitions.get(key)
        } else {
            ctx.definitions.get(key)
        }
        .cloned(),
    }
}

/// Every name a field can reference, local names first.
pub fn token_options(ctx: Option<&InterpolationContext>) -> Vec<TokenOption> {
    let Some(ctx) = ctx else { return Vec::new() };
    let option = |name: String, scope: TokenScope| TokenOption {
        value: token_value(&name, Some(ctx)).unwrap_or_default(),
        name,
        scope,
    };
    let local = ctx.definitions.keys();
    let global = ctx.workspace_definitions.keys();
    local
        .map(|name| option(name.clone(), TokenScope::Local))
        // A bare global name resolves when no local token has the same name.
        .chain(
            global
                .clone()
                .filter(|name| !ctx.definitions.contains_key(*name))
                .map(|name| option(name.clone(), TokenScope::Global)),
        )
        .chain(global.map(|name| option(format!("_.{name}"), TokenScope::Global)))
        .collect()
}

fn is_resolved(name: &str, workspace_only: bool, ctx: Option<&InterpolationContext>) -> bool {
    let Some(ctx) = ctx else { return false };
    if workspace_only {
        ctx.workspace_definitions.contains_key(name)
    } else {
        ctx.definitions.contains_key(name) || ctx.workspace_definitions.contains_key(name)
    }
}

/// Split `text` into plain runs and token references.
pub fn token_spans(text: &str, ctx: Option<&InterpolationContext>) -> Vec<TokenSpan> {
    let mut spans = Vec::new();
    let mut last = 0;
    for caps in REFERENCE_RE.captures_iter(text) {
        let whole = caps.get(0).unwrap();
        if whole.start() > last {
            spans.push(TokenSpan::plain(&text[last..whole.start()]));
        }
        if caps.get(1).is_some() {
            spans.push(TokenSpan {
                text: whole.as_str().to_string(),
                name: None,
                token: Some(TokenState::Env),
            });
        } else {
            let global = caps.get(2).is_some();
            let name = js_trim(&caps[3]);
            let token = if is_resolved(name, global, ctx) {
                TokenState::Resolved
            } else {
                TokenState::Unresolved
            };
            spans.push(TokenSpan {
                text: whole.as_str().to_string(),
                name: Some(format!("{}{name}", if global { "_." } else { "" })),
                token: Some(token),
            });
        }
        last = whole.end();
    }
    if last < text.len() {
        spans.push(TokenSpan::plain(&text[last..]));
    }
    spans
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRange {
    pub from: usize,
    pub to: usize,
    pub token: TokenState,
    pub name: Option<String>,
}

/// Token ranges in `text`, for editors that decorate by offset.
pub fn token_ranges(text: &str, ctx: Option<&InterpolationContext>) -> Vec<TokenRange> {
    let mut offset = 0;
    token_spans(text, ctx)
        .into_iter()
        .filter_map(|span| {
            let from = offset;
            offset += span.text.len();
            span.token.map(|token| TokenRange {
                from,
                to: offset,
                token,
                name: span.name,
            })
        })
        .collect()
}

/// `text` with each defined `{{name}}` replaced by its value. Undefined
/// references and `{{!NAME}}` stay as typed.
pub fn resolve_for_display(text: &str, ctx: Option<&InterpolationContext>) -> String {
    token_spans(text, ctx)
        .into_iter()
        .map(|span| match &span.name {
            Some(name) => token_value(name, ctx).unwrap_or(span.text),
            None => span.text,
        })
        .collect()
}

/// One-line hint for a reference, such as `endpoint = users`.
pub fn token_hint(span: &TokenSpan, ctx: Option<&InterpolationContext>) -> String {
    if span.token == Some(TokenState::Env) {
        return format!("{} reads the environment when sending", span.text);
    }
    let Some(name) = &span.name else {
        return String::new();
    };
    match token_value(name, ctx) {
        Some(value) => format!("{name} = {value}"),
        None => format!("{name} is not defined"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenQuery {
    /// The offset just after `{{`.
    pub from: usize,
    pub query: String,
}

/// The open `{{` reference before `cursor`, if the user is typing one.
pub fn token_query_at(text: &str, cursor: usize) -> Option<TokenQuery> {
    let before = text.get(..cursor)?;
    let start = before.rfind("{{")? + 2;
    let query = &before[start..];
    if query.contains(['{', '}']) || query.chars().any(crate::request::is_js_whitespace) {
        return None;
    }
    Some(TokenQuery {
        from: cursor - query.len(),
        query: query.to_string(),
    })
}

/// Options that match `query`, prefix matches first.
pub fn match_token_options(options: &[TokenOption], query: &str) -> Vec<TokenOption> {
    let q = query.to_lowercase();
    let matches: Vec<&TokenOption> = options
        .iter()
        .filter(|o| o.name.to_lowercase().contains(&q))
        .collect();
    let (prefix, rest): (Vec<&TokenOption>, Vec<&TokenOption>) = matches
        .into_iter()
        .partition(|o| o.name.to_lowercase().starts_with(&q));
    prefix.into_iter().chain(rest).cloned().collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenEdit {
    pub text: String,
    pub cursor: usize,
}

/// Insert `name` for the reference that starts at `from`. Replaces the typed
/// query and any `}}` already after the cursor.
pub fn apply_token_option(text: &str, from: usize, cursor: usize, name: &str) -> TokenEdit {
    let closing = if text[cursor..].starts_with("}}") {
        2
    } else {
        0
    };
    let insert = format!("{name}}}}}");
    TokenEdit {
        text: format!("{}{insert}{}", &text[..from], &text[cursor + closing..]),
        cursor: from + insert.len(),
    }
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
    fn ctx() -> InterpolationContext {
        InterpolationContext::new(
            defs(&[("endpoint", "users"), ("host", "local")]),
            defs(&[("host", "global"), ("apiKey", "secret")]),
        )
    }
    fn option(name: &str, scope: TokenScope, value: &str) -> TokenOption {
        TokenOption {
            name: name.into(),
            scope,
            value: value.into(),
        }
    }
    fn span(text: &str, token: Option<TokenState>, name: Option<&str>) -> TokenSpan {
        TokenSpan {
            text: text.into(),
            name: name.map(String::from),
            token,
        }
    }

    #[test]
    fn lists_local_names_unshadowed_global_names_then_underscore_names() {
        // Definitions are sorted by name, so globals list apiKey before host.
        assert_eq!(
            token_options(Some(&ctx())),
            [
                option("endpoint", TokenScope::Local, "users"),
                option("host", TokenScope::Local, "local"),
                option("apiKey", TokenScope::Global, "secret"),
                option("_.apiKey", TokenScope::Global, "secret"),
                option("_.host", TokenScope::Global, "global"),
            ]
        );
    }

    #[test]
    fn gives_no_options_without_a_context() {
        assert!(token_options(None).is_empty());
    }

    #[test]
    fn marks_resolved_unresolved_and_environment_references() {
        use TokenState::*;
        assert_eq!(
            token_spans(
                "https://x/{{endpoint}}/{{nope}}?k={{_.apiKey}}&e={{!HOME}}",
                Some(&ctx())
            ),
            [
                span("https://x/", None, None),
                span("{{endpoint}}", Some(Resolved), Some("endpoint")),
                span("/", None, None),
                span("{{nope}}", Some(Unresolved), Some("nope")),
                span("?k=", None, None),
                span("{{_.apiKey}}", Some(Resolved), Some("_.apiKey")),
                span("&e=", None, None),
                span("{{!HOME}}", Some(Env), None),
            ]
        );
    }

    #[test]
    fn treats_a_underscore_reference_to_a_local_only_name_as_unresolved() {
        assert_eq!(
            token_spans("{{_.endpoint}}", Some(&ctx()))[0].token,
            Some(TokenState::Unresolved)
        );
    }

    #[test]
    fn resolves_nested_references() {
        let nested = InterpolationContext::new(
            defs(&[("url", "https://{{_.host}}/{{path}}"), ("path", "v1")]),
            defs(&[("host", "api.test")]),
        );
        assert_eq!(
            token_value("url", Some(&nested)).as_deref(),
            Some("https://api.test/v1")
        );
    }

    #[test]
    fn gives_the_raw_value_when_a_nested_reference_is_missing() {
        let broken = InterpolationContext::local(defs(&[("url", "{{nope}}/x")]));
        assert_eq!(
            token_value("url", Some(&broken)).as_deref(),
            Some("{{nope}}/x")
        );
    }

    #[test]
    fn gives_undefined_for_an_unknown_name() {
        assert_eq!(token_value("nope", Some(&ctx())), None);
    }

    #[test]
    fn shows_the_value_or_says_the_token_is_not_defined() {
        let spans = token_spans("{{host}}/{{nope}}", Some(&ctx()));
        assert_eq!(token_hint(&spans[0], Some(&ctx())), "host = local");
        assert_eq!(token_hint(&spans[2], Some(&ctx())), "nope is not defined");
    }

    #[test]
    fn finds_the_open_reference_before_the_cursor() {
        assert_eq!(
            token_query_at("https://x/{{end", 15),
            Some(TokenQuery {
                from: 12,
                query: "end".into()
            })
        );
    }

    #[test]
    fn ignores_closed_references() {
        assert_eq!(token_query_at("{{endpoint}}/", 13), None);
    }

    #[test]
    fn puts_prefix_matches_first() {
        let names: Vec<String> = match_token_options(&token_options(Some(&ctx())), "host")
            .into_iter()
            .map(|o| o.name)
            .collect();
        assert_eq!(names, ["host", "_.host"]);
    }

    #[test]
    fn completes_the_name_and_closes_the_braces() {
        assert_eq!(
            apply_token_option("a/{{en", 4, 6, "endpoint"),
            TokenEdit {
                text: "a/{{endpoint}}".into(),
                cursor: 14
            }
        );
    }

    #[test]
    fn reuses_braces_already_after_the_cursor() {
        assert_eq!(
            apply_token_option("a/{{en}}/b", 4, 6, "endpoint"),
            TokenEdit {
                text: "a/{{endpoint}}/b".into(),
                cursor: 14
            }
        );
    }

    #[test]
    fn replaces_defined_tokens_and_keeps_the_rest_as_typed() {
        assert_eq!(
            resolve_for_display(
                "https://{{_.host}}/{{endpoint}}/{{nope}}/{{!HOME}}",
                Some(&ctx())
            ),
            "https://global/users/{{nope}}/{{!HOME}}"
        );
    }
}
