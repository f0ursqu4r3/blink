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
    /// A reference to a response token, usable or not.
    Response,
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
    if ctx.response_info(name).is_some() {
        return None;
    }
    let workspace_only = name.starts_with("_.");
    let key = if workspace_only { &name[2..] } else { name };
    if !is_resolved(key, workspace_only, Some(ctx)) {
        return None;
    }
    match interpolate(&format!("{{{{{name}}}}}"), &display_context(ctx)) {
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

/// A copy of `ctx` where each response token holds its own reference as
/// text, so nested expansion never reveals a response value.
fn display_context(ctx: &InterpolationContext) -> InterpolationContext {
    let mut shown = ctx.clone();
    for name in ctx.response_tokens.keys() {
        shown
            .definitions
            .insert(name.clone(), format!("{{{{{name}}}}}"));
    }
    for name in ctx.workspace_response_tokens.keys() {
        shown
            .workspace_definitions
            .insert(name.clone(), format!("{{{{_.{name}}}}}"));
    }
    shown
}

/// Every name a field can reference, local names first.
pub fn token_options(ctx: Option<&InterpolationContext>) -> Vec<TokenOption> {
    let Some(ctx) = ctx else { return Vec::new() };
    let option = |name: String, scope: TokenScope| TokenOption {
        value: ctx
            .response_info(&name)
            .map(|info| format!("from \"{}\"", info.request_label))
            .or_else(|| token_value(&name, Some(ctx)))
            .unwrap_or_default(),
        name,
        scope,
    };
    // Response tokens without a value are not in the definitions.
    let local = ctx.definitions.keys().chain(
        ctx.response_tokens
            .keys()
            .filter(|name| !ctx.definitions.contains_key(*name)),
    );
    let global: Vec<&String> = ctx
        .workspace_definitions
        .keys()
        .chain(
            ctx.workspace_response_tokens
                .keys()
                .filter(|name| !ctx.workspace_definitions.contains_key(*name)),
        )
        .collect();
    let local_has = |name: &String| {
        ctx.definitions.contains_key(name) || ctx.response_tokens.contains_key(name)
    };
    local
        .map(|name| option(name.clone(), TokenScope::Local))
        // A bare global name resolves when no local token has the same name.
        .chain(
            global
                .iter()
                .filter(|name| !local_has(name))
                .map(|name| option((*name).clone(), TokenScope::Global)),
        )
        .chain(
            global
                .iter()
                .map(|name| option(format!("_.{name}"), TokenScope::Global)),
        )
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
            let full = format!("{}{name}", if global { "_." } else { "" });
            let token = if !global && name.starts_with('@') {
                TokenState::Env
            } else if ctx.and_then(|c| c.response_info(&full)).is_some() {
                TokenState::Response
            } else if is_resolved(name, global, ctx) {
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
pub fn token_hint(span: &TokenSpan, ctx: Option<&InterpolationContext>, now_ms: f64) -> String {
    if span.token == Some(TokenState::Env) {
        return format!("{} reads the environment when sending", span.text);
    }
    let Some(name) = &span.name else {
        return String::new();
    };
    if span.token == Some(TokenState::Response)
        && let Some(info) = ctx.and_then(|ctx| ctx.response_info(name))
    {
        return match (info.fetched_at_ms, &info.problem) {
            (Some(at), _) => {
                let age = age_label((now_ms as u64).saturating_sub(at));
                let source = format!("{} {}", info.source.label(), info.path.trim());
                let mut hint = format!(
                    "From \"{}\" · {} · {age}",
                    info.request_label,
                    source.trim_end()
                );
                if let Some(environment) = &info.environment {
                    hint.push_str(&format!(" · {environment}"));
                }
                hint
            }
            (None, Some(problem)) => format!("No current value · {problem}"),
            (None, None) => format!("No current value · sends \"{}\" first", info.request_label),
        };
    }
    match token_value(name, ctx) {
        Some(value) => format!("{name} = {value}"),
        None => format!("{name} is not defined"),
    }
}

/// How long ago a value arrived: `just now`, `N min ago`, `N h ago`, `N d ago`.
pub fn age_label(ms: u64) -> String {
    let minutes = ms / 60_000;
    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{minutes} min ago")
    } else if minutes < 24 * 60 {
        format!("{} h ago", minutes / 60)
    } else {
        format!("{} d ago", minutes / (24 * 60))
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
    use crate::interpolation::ResponseTokenInfo;
    use crate::model::{CheckSource, Definitions};

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
        assert_eq!(
            token_options(Some(&ctx())),
            [
                option("endpoint", TokenScope::Local, "users"),
                option("host", TokenScope::Local, "local"),
                option("apiKey", TokenScope::Global, "secret"),
                option("_.host", TokenScope::Global, "global"),
                option("_.apiKey", TokenScope::Global, "secret"),
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
        assert_eq!(token_hint(&spans[0], Some(&ctx()), 0.0), "host = local");
        assert_eq!(
            token_hint(&spans[2], Some(&ctx()), 0.0),
            "nope is not defined"
        );
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

    #[test]
    fn response_tokens_hint_their_source_never_their_value() {
        let mut ctx = InterpolationContext::local(defs(&[("access_token", "secret-value")]));
        ctx.response_tokens.insert(
            "access_token".into(),
            ResponseTokenInfo {
                request_label: "Login".into(),
                source: CheckSource::Json,
                path: ".access_token".into(),
                fetched_at_ms: Some(1_000),
                environment: Some("DEV".into()),
                problem: None,
            },
        );
        let span = &token_spans("{{access_token}}", Some(&ctx))[0];
        assert_eq!(span.token, Some(TokenState::Response));
        let hint = token_hint(span, Some(&ctx), 1_000.0 + 3.0 * 60_000.0);
        assert_eq!(
            hint,
            "From \"Login\" · JSON (jq) .access_token · 3 min ago · DEV"
        );
        assert!(!hint.contains("secret-value"));
        let option = token_options(Some(&ctx))
            .into_iter()
            .find(|o| o.name == "access_token")
            .unwrap();
        assert_eq!(option.value, "from \"Login\"");
        assert_eq!(
            resolve_for_display("{{access_token}}", Some(&ctx)),
            "{{access_token}}"
        );
    }

    #[test]
    fn a_response_token_without_a_value_hints_the_send() {
        let mut ctx = InterpolationContext::default();
        ctx.response_tokens.insert(
            "t".into(),
            ResponseTokenInfo {
                request_label: "Login".into(),
                source: CheckSource::Json,
                path: ".t".into(),
                fetched_at_ms: None,
                environment: None,
                problem: None,
            },
        );
        let span = &token_spans("{{t}}", Some(&ctx))[0];
        assert_eq!(span.token, Some(TokenState::Response));
        assert_eq!(
            token_hint(span, Some(&ctx), 0.0),
            "No current value · sends \"Login\" first"
        );
    }

    #[test]
    fn a_response_token_problem_replaces_the_send_hint() {
        let mut ctx = InterpolationContext::default();
        ctx.response_tokens.insert(
            "t".into(),
            ResponseTokenInfo {
                request_label: "Login".into(),
                source: CheckSource::Status,
                path: String::new(),
                fetched_at_ms: None,
                environment: None,
                problem: Some("Login is disabled".into()),
            },
        );
        let span = &token_spans("{{t}}", Some(&ctx))[0];
        assert_eq!(
            token_hint(span, Some(&ctx), 0.0),
            "No current value · Login is disabled"
        );
    }

    #[test]
    fn ages_read_in_the_largest_unit() {
        assert_eq!(age_label(59_000), "just now");
        assert_eq!(age_label(3 * 60_000), "3 min ago");
        assert_eq!(age_label(2 * 3_600_000), "2 h ago");
        assert_eq!(age_label(3 * 86_400_000), "3 d ago");
    }

    fn nested_ctx(fetched: Option<u64>) -> InterpolationContext {
        let mut ctx = InterpolationContext::local(defs(&[
            ("auth", "Bearer {{access_token}}"),
            ("access_token", "secret-value"),
        ]));
        ctx.response_tokens.insert(
            "access_token".into(),
            ResponseTokenInfo {
                request_label: "Login".into(),
                source: CheckSource::Json,
                path: ".access_token".into(),
                fetched_at_ms: fetched,
                environment: None,
                problem: None,
            },
        );
        ctx
    }

    #[test]
    fn a_text_token_never_shows_a_nested_response_value() {
        for fetched in [Some(1), None] {
            let ctx = nested_ctx(fetched);
            let span = &token_spans("{{auth}}", Some(&ctx))[0];
            let hint = token_hint(span, Some(&ctx), 0.0);
            assert_eq!(hint, "auth = Bearer {{access_token}}");
            let option = token_options(Some(&ctx))
                .into_iter()
                .find(|o| o.name == "auth")
                .unwrap();
            assert_eq!(option.value, "Bearer {{access_token}}");
            let shown = crate::token_display::token_display("{{auth}}", Some(&ctx));
            assert_eq!(shown.text, "Bearer {{access_token}}");
            assert!(!shown.text.contains("secret-value"));
        }
    }

    #[test]
    fn a_global_response_token_marks_bare_and_prefixed_references() {
        let mut ctx = InterpolationContext::new(defs(&[]), defs(&[("csrf", "tok")]));
        ctx.workspace_response_tokens.insert(
            "csrf".into(),
            ResponseTokenInfo {
                request_label: "Login".into(),
                source: CheckSource::Json,
                path: ".csrf".into(),
                fetched_at_ms: Some(1),
                environment: None,
                problem: None,
            },
        );
        assert_eq!(interpolate("{{csrf}}", &ctx).unwrap(), "tok");
        assert_eq!(interpolate("{{_.csrf}}", &ctx).unwrap(), "tok");
        for text in ["{{csrf}}", "{{_.csrf}}"] {
            assert_eq!(
                token_spans(text, Some(&ctx))[0].token,
                Some(TokenState::Response)
            );
        }
    }
}
