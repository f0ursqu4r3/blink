//! Authorization inheritance and resolved request context for Blink.
//! Port of `src/lib/authorization.ts`.
//!
//! AuthorizationConfig is a tagged enum:
//!   None                         — explicitly no auth
//!   Bearer { token }             — bearer token
//!   Basic { username, password } — basic auth
//!
//! Absence of `local_auth` on a draft or group means "inherit from parent".
//! Explicit `None` blocks inheritance.
//!
//! Resolution order: draft.local_auth > nearest group > ancestors > none.

use std::collections::{HashMap, HashSet};
use std::ops::Deref;

use crate::environments::group_definitions;
use crate::interpolation::InterpolationContext;
use crate::model::{AuthorizationConfig, Definitions, Draft, RequestGroup};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRequestContext {
    /// Fully resolved effective authorization for this request.
    pub auth: AuthorizationConfig,
    pub tokens: InterpolationContext,
}

impl Deref for ResolvedRequestContext {
    type Target = InterpolationContext;

    fn deref(&self) -> &InterpolationContext {
        &self.tokens
    }
}

/// The group ancestry of `group_id`, nearest first. Stops at a cycle or a
/// missing group.
fn ancestry(group_id: Option<u64>, groups: &[RequestGroup]) -> Vec<&RequestGroup> {
    let by_id: HashMap<u64, &RequestGroup> = groups.iter().map(|g| (g.id, g)).collect();
    let mut chain = Vec::new();
    let mut cursor = group_id;
    let mut seen = HashSet::new();
    while let Some(id) = cursor {
        if !seen.insert(id) {
            break;
        }
        let Some(group) = by_id.get(&id) else { break };
        chain.push(*group);
        cursor = group.parent_id;
    }
    chain
}

/// Walk the group ancestry (nearest first) and return the first
/// `local_auth` found. Returns `None` auth if none found.
pub fn resolve_authorization(
    draft_local_auth: Option<&AuthorizationConfig>,
    group_id: Option<u64>,
    groups: &[RequestGroup],
) -> AuthorizationConfig {
    // Draft overrides everything (including explicit none)
    if let Some(auth) = draft_local_auth {
        return auth.clone();
    }
    // Walk ancestry from direct group upward
    ancestry(group_id, groups)
        .into_iter()
        .find_map(|group| group.local_auth.clone())
        .unwrap_or(AuthorizationConfig::None)
}

/// Build the merged token definitions for interpolation.
/// Nearest group wins for duplicate keys; workspace global is always separate.
pub fn resolve_token_definitions(
    group_id: Option<u64>,
    groups: &[RequestGroup],
    workspace_global: &Definitions,
) -> InterpolationContext {
    // Merge: nearest wins (process chain in order, skip if key already set)
    let mut merged = Definitions::new();
    for group in ancestry(group_id, groups) {
        for (key, value) in group_definitions(group) {
            merged.entry(key).or_insert(value);
        }
    }
    InterpolationContext {
        definitions: merged,
        workspace_definitions: workspace_global.clone(),
        response_tokens: Default::default(),
        workspace_response_tokens: Default::default(),
    }
}

/// Build a fully resolved request context combining resolved auth and tokens.
pub fn build_resolved_request_context(
    draft: &Draft,
    group_id: Option<u64>,
    groups: &[RequestGroup],
    workspace_global: &Definitions,
) -> ResolvedRequestContext {
    ResolvedRequestContext {
        auth: resolve_authorization(draft.local_auth.as_ref(), group_id, groups),
        tokens: resolve_token_definitions(group_id, groups, workspace_global),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::create_draft;

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn group(id: u64, parent_id: Option<u64>) -> RequestGroup {
        RequestGroup {
            id,
            name: format!("G{id}"),
            parent_id,
            collapsed: false,
            local_auth: None,
            local_definitions: None,
            response_tokens: None,
            default_method: None,
            default_url: None,
            environments: None,
            active_environment_id: None,
        }
    }

    fn auth_group(
        id: u64,
        parent_id: Option<u64>,
        auth: Option<AuthorizationConfig>,
    ) -> RequestGroup {
        RequestGroup {
            local_auth: auth,
            ..group(id, parent_id)
        }
    }

    fn defs_group(id: u64, parent_id: Option<u64>, entries: &[(&str, &str)]) -> RequestGroup {
        RequestGroup {
            local_definitions: Some(defs(entries)),
            ..group(id, parent_id)
        }
    }

    fn bearer(token: &str) -> AuthorizationConfig {
        AuthorizationConfig::Bearer {
            token: token.into(),
        }
    }

    fn basic(username: &str, password: &str) -> AuthorizationConfig {
        AuthorizationConfig::Basic {
            username: username.into(),
            password: password.into(),
        }
    }

    // resolve_authorization — inheritance chain

    #[test]
    fn returns_none_when_draft_has_no_local_auth_and_no_groups_have_auth() {
        let groups = [auth_group(1, None, None)];
        assert_eq!(
            resolve_authorization(None, Some(1), &groups),
            AuthorizationConfig::None
        );
    }

    #[test]
    fn returns_draft_local_auth_when_set_overrides_groups() {
        let groups = [auth_group(1, None, Some(bearer("g-tok")))];
        assert_eq!(
            resolve_authorization(Some(&bearer("d-tok")), Some(1), &groups),
            bearer("d-tok")
        );
    }

    #[test]
    fn explicit_none_on_draft_blocks_group_auth() {
        let groups = [auth_group(1, None, Some(bearer("g-tok")))];
        assert_eq!(
            resolve_authorization(Some(&AuthorizationConfig::None), Some(1), &groups),
            AuthorizationConfig::None
        );
    }

    #[test]
    fn inherits_bearer_from_direct_group_when_draft_has_no_local_auth() {
        let groups = [auth_group(1, None, Some(bearer("g-tok")))];
        assert_eq!(
            resolve_authorization(None, Some(1), &groups),
            bearer("g-tok")
        );
    }

    #[test]
    fn inherits_from_parent_group_when_direct_group_has_no_local_auth() {
        let groups = [
            auth_group(1, None, Some(basic("u", "p"))),
            auth_group(2, Some(1), None),
        ];
        assert_eq!(
            resolve_authorization(None, Some(2), &groups),
            basic("u", "p")
        );
    }

    #[test]
    fn nearest_ancestor_wins_over_further_ancestor() {
        let groups = [
            auth_group(1, None, Some(bearer("root-tok"))),
            auth_group(2, Some(1), Some(bearer("mid-tok"))),
            auth_group(3, Some(2), None),
        ];
        assert_eq!(
            resolve_authorization(None, Some(3), &groups),
            bearer("mid-tok")
        );
    }

    #[test]
    fn returns_none_when_no_group_has_local_auth_ungrouped_request() {
        assert_eq!(
            resolve_authorization(None, None, &[]),
            AuthorizationConfig::None
        );
    }

    #[test]
    fn resolves_bearer_from_group_when_draft_inherits() {
        let groups = [auth_group(5, None, Some(bearer("tok")))];
        assert_eq!(resolve_authorization(None, Some(5), &groups), bearer("tok"));
    }

    // resolve_token_definitions — merging nearest-wins chain

    #[test]
    fn returns_workspace_global_definitions_when_not_in_any_group() {
        let result = resolve_token_definitions(None, &[], &defs(&[("baseUrl", "https://ws")]));
        assert_eq!(result.definitions, defs(&[]));
        assert_eq!(
            result.workspace_definitions,
            defs(&[("baseUrl", "https://ws")])
        );
    }

    #[test]
    fn merges_group_local_definitions_group_wins_over_workspace_for_same_key() {
        let groups = [defs_group(
            1,
            None,
            &[("env", "staging"), ("host", "g-host")],
        )];
        let result = resolve_token_definitions(
            Some(1),
            &groups,
            &defs(&[("env", "prod"), ("wsOnly", "yes")]),
        );
        assert_eq!(
            result.definitions,
            defs(&[("env", "staging"), ("host", "g-host")])
        );
        assert_eq!(
            result.workspace_definitions,
            defs(&[("env", "prod"), ("wsOnly", "yes")])
        );
    }

    #[test]
    fn nearer_group_definitions_win_over_parent_group_definitions_for_same_key() {
        let groups = [
            defs_group(1, None, &[("x", "parent-x"), ("y", "parent-y")]),
            defs_group(2, Some(1), &[("x", "child-x")]),
        ];
        let result = resolve_token_definitions(Some(2), &groups, &defs(&[]));
        assert_eq!(
            result.definitions,
            defs(&[("x", "child-x"), ("y", "parent-y")])
        );
    }

    #[test]
    fn accumulated_definitions_are_passed_as_local_workspace_global_separately() {
        let groups = [defs_group(1, None, &[("local", "yes")])];
        let result = resolve_token_definitions(Some(1), &groups, &defs(&[("global", "ws")]));
        assert_eq!(result.definitions, defs(&[("local", "yes")]));
        assert_eq!(result.workspace_definitions, defs(&[("global", "ws")]));
    }

    // build_resolved_request_context — combines auth + token resolution

    #[test]
    fn resolves_auth_and_definitions_from_group_ancestry_for_a_draft() {
        let groups = [RequestGroup {
            local_auth: Some(bearer("grp-tok")),
            local_definitions: Some(defs(&[("host", "api.example.com")])),
            ..group(1, None)
        }];
        let ctx = build_resolved_request_context(
            &create_draft(),
            Some(1),
            &groups,
            &defs(&[("apiVersion", "v2")]),
        );
        assert_eq!(ctx.auth, bearer("grp-tok"));
        assert_eq!(ctx.definitions, defs(&[("host", "api.example.com")]));
        assert_eq!(ctx.workspace_definitions, defs(&[("apiVersion", "v2")]));
    }

    #[test]
    fn draft_explicit_auth_overrides_group_auth() {
        let groups = [auth_group(1, None, Some(bearer("g-tok")))];
        let draft = Draft {
            local_auth: Some(basic("u", "p")),
            ..create_draft()
        };
        let ctx = build_resolved_request_context(&draft, Some(1), &groups, &defs(&[]));
        assert_eq!(ctx.auth, basic("u", "p"));
    }

    #[test]
    fn ungrouped_draft_with_no_local_auth_gets_auth_none_and_empty_definitions() {
        let ctx = build_resolved_request_context(&create_draft(), None, &[], &defs(&[]));
        assert_eq!(ctx.auth, AuthorizationConfig::None);
        assert!(ctx.definitions.is_empty());
        assert!(ctx.workspace_definitions.is_empty());
    }
}
