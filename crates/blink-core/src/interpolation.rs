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

use regex::{Captures, Regex};

use crate::model::Definitions;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InterpolationContext {
    /// Local definitions (group or request level).
    pub definitions: Definitions,
    /// Workspace-global definitions.
    pub workspace_definitions: Definitions,
}

impl InterpolationContext {
    pub fn new(definitions: Definitions, workspace_definitions: Definitions) -> Self {
        InterpolationContext {
            definitions,
            workspace_definitions,
        }
    }

    /// Local definitions only.
    pub fn local(definitions: Definitions) -> Self {
        InterpolationContext {
            definitions,
            workspace_definitions: Definitions::new(),
        }
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
    let value = if workspace_only {
        ctx.workspace_definitions.get(name)
    } else {
        ctx.definitions
            .get(name)
            .or_else(|| ctx.workspace_definitions.get(name))
    };
    let Some(value) = value else {
        return Err(format!(
            "Undefined token reference: \"{name}\". Define it in your {} variables.",
            if workspace_only {
                "workspace"
            } else {
                "group or workspace"
            }
        ));
    };
    stack.push(key);
    let resolved = resolve(value, ctx, stack);
    stack.pop();
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
