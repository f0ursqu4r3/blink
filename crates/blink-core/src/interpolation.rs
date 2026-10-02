//! Token interpolation for Blink request values. Port of `src/lib/interpolation.ts`.
//!
//! Syntax
//!   {{name}}   — resolves from local definitions, falls back to workspace
//!   {{_.name}} — resolves from workspace-global definitions only
//!   {{!NAME}}  — reads NAME from the process environment. The native
//!                transport resolves it when sending, so it stays as typed here.
//!
//! Token values can reference other tokens. Resolution detects cycles and never
//! includes values in errors.

use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::{Captures, Regex};

use crate::model::{CheckSource, Definitions};

/// What a context knows about a response token, for errors and hints.
/// Never holds the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseTokenInfo {
    pub request_label: String,
    pub source: CheckSource,
    pub path: String,
    /// Set when a usable value is in the definitions.
    pub fetched_at_ms: Option<u64>,
    pub environment: Option<String>,
    /// Why no value can be read, when the source request cannot supply one.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InterpolationContext {
    /// Local definitions (group or request level).
    pub definitions: Definitions,
    /// Workspace-global definitions.
    pub workspace_definitions: Definitions,
    /// Response tokens of the group scopes, by name.
    pub response_tokens: IndexMap<String, ResponseTokenInfo>,
    /// Workspace-global response tokens, by name.
    pub workspace_response_tokens: IndexMap<String, ResponseTokenInfo>,
}

impl InterpolationContext {
    pub fn new(definitions: Definitions, workspace_definitions: Definitions) -> Self {
        InterpolationContext {
            definitions,
            workspace_definitions,
            response_tokens: IndexMap::new(),
            workspace_response_tokens: IndexMap::new(),
        }
    }

    /// Local definitions only.
    pub fn local(definitions: Definitions) -> Self {
        InterpolationContext {
            definitions,
            workspace_definitions: Definitions::new(),
            response_tokens: IndexMap::new(),
            workspace_response_tokens: IndexMap::new(),
        }
    }

    /// The response token `name` refers to, in the order `interpolate` looks
    /// names up. `name` may start with `_.`.
    pub fn response_info(&self, name: &str) -> Option<&ResponseTokenInfo> {
        if let Some(name) = name.strip_prefix("_.") {
            return self.workspace_response_tokens.get(name);
        }
        if self.definitions.contains_key(name) && !self.response_tokens.contains_key(name) {
            return None;
        }
        self.response_tokens.get(name).or_else(|| {
            if self.definitions.contains_key(name) {
                None
            } else {
                self.workspace_response_tokens.get(name)
            }
        })
    }
}

pub(crate) static TOKEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{(_\.)?([^{}]+?)\}\}").unwrap());
const ENV_PREFIX: char = '!';

/// Resolve all `{{name}}` / `{{_.name}}` tokens in `template`.
///
/// Errors name the first unresolvable reference (never reveal values).
pub fn interpolate(template: &str, ctx: &InterpolationContext) -> Result<String, String> {
    resolve(template, ctx, &mut Vec::new())
}

fn resolve(
    source: &str,
    ctx: &InterpolationContext,
    stack: &mut Vec<String>,
) -> Result<String, String> {
    let mut out = String::with_capacity(source.len());
    let mut last = 0;
    for caps in TOKEN_RE.captures_iter(source) {
        let whole = caps.get(0).unwrap();
        out.push_str(&source[last..whole.start()]);
        last = whole.end();
        out.push_str(&replace(&caps, ctx, stack)?);
    }
    out.push_str(&source[last..]);
    Ok(out)
}

fn replace(
    caps: &Captures,
    ctx: &InterpolationContext,
    stack: &mut Vec<String>,
) -> Result<String, String> {
    let token = &caps[0];
    let global_prefix = caps.get(1).is_some();
    let name = &caps[2];
    if !global_prefix && name.starts_with(ENV_PREFIX) {
        return Ok(token.to_string());
    }
    let workspace_only = global_prefix;
    let key = format!("{}{name}", if workspace_only { "_." } else { "" });
    if stack.contains(&key) {
        return Err(format!("Circular token reference: \"{name}\"."));
    }
    let lookup = if workspace_only {
        format!("_.{name}")
    } else {
        name.to_string()
    };
    let value = if workspace_only {
        ctx.workspace_definitions.get(name)
    } else {
        ctx.definitions.get(name).or_else(|| {
            if ctx.response_tokens.contains_key(name) {
                None
            } else {
                ctx.workspace_definitions.get(name)
            }
        })
    };
    let Some(value) = value else {
        if let Some(info) = ctx.response_info(&lookup) {
            return Err(format!(
                "\"{name}\" has no current value. Send \"{}\" or the request that uses it.",
                info.request_label
            ));
        }
        return Err(format!(
            "Undefined token reference: \"{name}\". Define it in your {} variables.",
            if workspace_only {
                "workspace"
            } else {
                "group or workspace"
            }
        ));
    };
    // A response value is literal text, never a template.
    if ctx.response_info(&lookup).is_some() {
        return Ok(value.clone());
    }
    stack.push(key);
    let resolved = resolve(value, ctx, stack);
    stack.pop();
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CheckSource;

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    fn local(entries: &[(&str, &str)]) -> InterpolationContext {
        InterpolationContext::local(defs(entries))
    }
    fn both(local: &[(&str, &str)], workspace: &[(&str, &str)]) -> InterpolationContext {
        InterpolationContext::new(defs(local), defs(workspace))
    }

    #[test]
    fn resolves_a_local_name_reference() {
        assert_eq!(
            interpolate("Hello {{name}}", &local(&[("name", "World")])).unwrap(),
            "Hello World"
        );
    }

    #[test]
    fn resolves_global_name_from_workspace_definitions_only() {
        assert_eq!(
            interpolate("{{_.token}}", &both(&[], &[("token", "ws-tok")])).unwrap(),
            "ws-tok"
        );
    }

    #[test]
    fn global_name_does_not_resolve_from_local_definitions() {
        let error = interpolate("{{_.token}}", &both(&[("token", "local")], &[])).unwrap_err();
        assert!(error.contains("token"));
    }

    #[test]
    fn keeps_env_references_for_the_native_transport() {
        assert_eq!(
            interpolate(
                "Bearer {{!API_TOKEN}} {{nested}}",
                &local(&[("nested", "{{!OTHER}}")])
            )
            .unwrap(),
            "Bearer {{!API_TOKEN}} {{!OTHER}}"
        );
    }

    #[test]
    fn keeps_angle_brackets_as_literal_text() {
        assert_eq!(
            interpolate("a << b >> c", &local(&[])).unwrap(),
            "a << b >> c"
        );
    }

    #[test]
    fn local_definitions_shadow_workspace_for_plain_name() {
        assert_eq!(
            interpolate("{{x}}", &both(&[("x", "local")], &[("x", "workspace")])).unwrap(),
            "local"
        );
    }

    #[test]
    fn falls_back_to_workspace_for_name_when_absent_from_local() {
        assert_eq!(
            interpolate("{{x}}", &both(&[], &[("x", "workspace")])).unwrap(),
            "workspace"
        );
    }

    #[test]
    fn handles_multiple_references_and_literal_text_in_a_single_pass() {
        assert_eq!(
            interpolate("{{a}} and {{b}}", &local(&[("a", "foo"), ("b", "bar")])).unwrap(),
            "foo and bar"
        );
    }

    #[test]
    fn resolves_tokens_in_token_values() {
        assert_eq!(
            interpolate("{{a}}", &local(&[("a", "{{b}}"), ("b", "expanded")])).unwrap(),
            "expanded"
        );
    }

    #[test]
    fn resolves_global_tokens_referenced_by_token_values() {
        assert_eq!(
            interpolate(
                "{{requestUrl}}",
                &both(
                    &[("requestUrl", "https://{{_.host}}/v1")],
                    &[("host", "api.example.test")]
                )
            )
            .unwrap(),
            "https://api.example.test/v1"
        );
    }

    #[test]
    fn rejects_recursive_token_definitions_without_revealing_values() {
        let error = interpolate("{{a}}", &local(&[("a", "{{b}}"), ("b", "{{a}}")])).unwrap_err();
        assert!(error.contains("Circular token reference"));
    }

    #[test]
    fn throws_a_clear_error_naming_the_missing_reference() {
        assert!(
            interpolate("{{missing}}", &local(&[]))
                .unwrap_err()
                .contains("missing")
        );
    }

    #[test]
    fn error_messages_never_reveal_definition_values() {
        let message = interpolate("{{gone}}", &local(&[("secret", "hunter2")])).unwrap_err();
        assert!(message.contains("gone"));
        assert!(!message.contains("hunter2"));
    }

    #[test]
    fn returns_the_original_string_unchanged_when_there_are_no_tokens() {
        assert_eq!(
            interpolate("plain text", &local(&[])).unwrap(),
            "plain text"
        );
    }

    #[test]
    fn handles_an_empty_template() {
        assert_eq!(interpolate("", &local(&[])).unwrap(), "");
    }

    fn info(fetched: Option<u64>) -> ResponseTokenInfo {
        ResponseTokenInfo {
            request_label: "Login".into(),
            source: CheckSource::Json,
            path: ".access_token".into(),
            fetched_at_ms: fetched,
            environment: None,
            problem: None,
        }
    }

    #[test]
    fn a_response_token_without_a_value_names_its_request() {
        let mut ctx = both(&[], &[("access_token", "global-text")]);
        ctx.response_tokens
            .insert("access_token".into(), info(None));
        assert_eq!(
            interpolate("{{access_token}}", &ctx).unwrap_err(),
            "\"access_token\" has no current value. Send \"Login\" or the request that uses it."
        );
    }

    #[test]
    fn a_response_token_with_a_value_resolves() {
        let mut ctx = local(&[("access_token", "abc")]);
        ctx.response_tokens
            .insert("access_token".into(), info(Some(1)));
        assert_eq!(
            interpolate("Bearer {{access_token}}", &ctx).unwrap(),
            "Bearer abc"
        );
    }

    #[test]
    fn a_response_token_value_is_never_interpolated_again() {
        let mut ctx = local(&[("t", "{{missing}}")]);
        ctx.response_tokens.insert("t".into(), info(Some(1)));
        assert_eq!(interpolate("{{t}}", &ctx).unwrap(), "{{missing}}");
    }

    #[test]
    fn a_global_response_token_without_a_value_blocks_the_bare_name() {
        let mut ctx = both(&[], &[]);
        ctx.workspace_response_tokens
            .insert("csrf".into(), info(None));
        assert!(
            interpolate("{{csrf}}", &ctx)
                .unwrap_err()
                .starts_with("\"csrf\" has no current value")
        );
        assert!(
            interpolate("{{_.csrf}}", &ctx)
                .unwrap_err()
                .starts_with("\"csrf\" has no current value")
        );
    }
}
