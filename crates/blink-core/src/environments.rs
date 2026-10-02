//! Named token sets on root groups. Port of `src/lib/environments.ts`.

use std::collections::{HashMap, HashSet};

use crate::ids;
use crate::model::{Definitions, Environment, EnvironmentColor, RequestGroup};

/// Theme colors an environment badge can use.
pub const ENVIRONMENT_COLORS: [EnvironmentColor; 5] = EnvironmentColor::ALL;

/// The saved id of a color, such as `info`.
pub fn environment_color_id(color: EnvironmentColor) -> &'static str {
    match color {
        EnvironmentColor::Destructive => "destructive",
        EnvironmentColor::Warning => "warning",
        EnvironmentColor::Success => "success",
        EnvironmentColor::Info => "info",
        EnvironmentColor::Keyword => "keyword",
    }
}

pub fn environment_color_label(color: EnvironmentColor) -> &'static str {
    color.label()
}

pub fn is_environment_color(value: &str) -> bool {
    ENVIRONMENT_COLORS
        .iter()
        .any(|color| environment_color_id(*color) == value)
}

pub const MAX_ENVIRONMENTS: usize = 20;
pub const MAX_ENVIRONMENT_NAME: usize = 24;

fn root_index(group_id: Option<u64>, groups: &[RequestGroup]) -> Option<usize> {
    let by_id: HashMap<u64, usize> = groups
        .iter()
        .enumerate()
        .map(|(index, group)| (group.id, index))
        .collect();
    let mut seen = HashSet::new();
    let mut index = *by_id.get(&group_id?)?;
    while let Some(parent_id) = groups[index].parent_id {
        if !seen.insert(groups[index].id) {
            break;
        }
        let Some(&parent) = by_id.get(&parent_id) else {
            break;
        };
        index = parent;
    }
    Some(index)
}

/// The root group of `group_id`, or None for an ungrouped request.
pub fn root_group(group_id: Option<u64>, groups: &[RequestGroup]) -> Option<&RequestGroup> {
    root_index(group_id, groups).map(|index| &groups[index])
}

/// The active environment of a root group. Nested groups have none.
pub fn active_environment(group: Option<&RequestGroup>) -> Option<&Environment> {
    let group = group?;
    if group.parent_id.is_some() {
        return None;
    }
    let active = group.active_environment_id?;
    group
        .environments
        .as_ref()?
        .iter()
        .find(|environment| environment.id == active)
}

fn active_environment_index(group: &RequestGroup) -> Option<usize> {
    if group.parent_id.is_some() {
        return None;
    }
    let active = group.active_environment_id?;
    group
        .environments
        .as_ref()?
        .iter()
        .position(|environment| environment.id == active)
}

/// The active environment that applies to a request in `group_id`.
pub fn request_environment(group_id: Option<u64>, groups: &[RequestGroup]) -> Option<&Environment> {
    active_environment(root_group(group_id, groups))
}

/// The base tokens of a group, with the active environment's values on top.
pub fn group_definitions(group: &RequestGroup) -> Definitions {
    let mut definitions = group.local_definitions.clone().unwrap_or_default();
    if let Some(environment) = active_environment(Some(group)) {
        definitions.extend(environment.values.clone());
    }
    definitions
}

pub fn create_environment(name: impl Into<String>, color: EnvironmentColor) -> Environment {
    Environment {
        id: ids::ENVIRONMENTS.next(),
        name: name.into(),
        color,
        protected: None,
        values: Definitions::new(),
    }
}

/// A color the group does not use yet, in palette order.
pub fn next_environment_color(environments: &[Environment]) -> EnvironmentColor {
    let used: HashSet<EnvironmentColor> = environments.iter().map(|e| e.color).collect();
    ENVIRONMENT_COLORS
        .iter()
        .rev()
        .copied()
        .find(|color| !used.contains(color))
        .unwrap_or(EnvironmentColor::Info)
}

/// Where captured values went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTarget {
    Environment,
    Group,
    Global,
}

/// Where captured values go for a request: the active environment of its
/// root group, else the root group's base tokens, else the global tokens.
/// The caller writes `Global` values.
pub fn apply_capture(
    group_id: Option<u64>,
    groups: &mut [RequestGroup],
    values: &Definitions,
) -> CaptureTarget {
    let Some(root) = root_index(group_id, groups) else {
        return CaptureTarget::Global;
    };
    let root = &mut groups[root];
    if let Some(index) = active_environment_index(root) {
        let environment = &mut root.environments.as_mut().unwrap()[index];
        environment.values.extend(values.clone());
        return CaptureTarget::Environment;
    }
    root.local_definitions
        .get_or_insert_with(Definitions::new)
        .extend(values.clone());
    CaptureTarget::Group
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authorization::resolve_token_definitions;
    use crate::model::WorkspacePreferences;
    use crate::session::create_session;
    use crate::workspace::{decode_workspace, encode_workspace};

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

    fn tree() -> (Vec<RequestGroup>, Environment, Environment) {
        let dev = Environment {
            values: defs(&[("host", "dev")]),
            ..create_environment("DEV", EnvironmentColor::Info)
        };
        let prod = Environment {
            protected: Some(true),
            values: defs(&[("host", "prod"), ("key", "p")]),
            ..create_environment("PROD", EnvironmentColor::Destructive)
        };
        let groups = vec![
            RequestGroup {
                name: "API".into(),
                local_definitions: Some(defs(&[("host", "base"), ("version", "v1")])),
                environments: Some(vec![dev.clone(), prod.clone()]),
                active_environment_id: Some(dev.id),
                ..group(1, None)
            },
            RequestGroup {
                name: "Users".into(),
                local_definitions: Some(defs(&[("version", "v2")])),
                ..group(2, Some(1))
            },
        ];
        (groups, dev, prod)
    }

    #[test]
    fn finds_the_root_group_and_its_active_environment() {
        let (groups, dev, _) = tree();
        assert_eq!(root_group(Some(2), &groups).map(|g| g.id), Some(1));
        assert!(root_group(None, &groups).is_none());
        assert_eq!(
            request_environment(Some(2), &groups).map(|e| e.id),
            Some(dev.id)
        );
    }

    #[test]
    fn puts_active_values_over_base_tokens_nested_groups_still_win() {
        let (mut groups, _, prod) = tree();
        assert_eq!(
            resolve_token_definitions(Some(2), &groups, &defs(&[])).definitions,
            defs(&[("host", "dev"), ("version", "v2")])
        );
        groups[0].active_environment_id = Some(prod.id);
        assert_eq!(
            resolve_token_definitions(Some(1), &groups, &defs(&[])).definitions,
            defs(&[("host", "prod"), ("version", "v1"), ("key", "p")])
        );
        groups[0].active_environment_id = None;
        assert_eq!(
            resolve_token_definitions(Some(1), &groups, &defs(&[])).definitions["host"],
            "base"
        );
    }

    #[test]
    fn ignores_environments_on_a_group_that_is_no_longer_a_root() {
        let (mut groups, _, _) = tree();
        groups.insert(
            0,
            RequestGroup {
                name: "Top".into(),
                ..group(3, None)
            },
        );
        groups[1].parent_id = Some(3);
        assert_eq!(
            resolve_token_definitions(Some(1), &groups, &defs(&[])).definitions["host"],
            "base"
        );
    }

    #[test]
    fn captures_into_the_active_environment_else_the_root_else_globals() {
        let (mut groups, _, _) = tree();
        assert_eq!(
            apply_capture(Some(2), &mut groups, &defs(&[("token", "a")])),
            CaptureTarget::Environment
        );
        let environments = groups[0].environments.as_ref().unwrap();
        assert_eq!(environments[0].values["token"], "a");
        assert!(!environments[1].values.contains_key("token"));
        groups[0].active_environment_id = None;
        assert_eq!(
            apply_capture(Some(2), &mut groups, &defs(&[("token", "b")])),
            CaptureTarget::Group
        );
        assert_eq!(groups[0].local_definitions.as_ref().unwrap()["token"], "b");
        assert_eq!(
            apply_capture(None, &mut groups, &defs(&[("token", "c")])),
            CaptureTarget::Global
        );
    }

    #[test]
    fn picks_an_unused_color() {
        let (_, dev, prod) = tree();
        assert_eq!(
            next_environment_color(&[dev, prod]),
            EnvironmentColor::Keyword
        );
    }

    #[test]
    fn round_trips_and_validates_environments() {
        let (groups, _, _) = tree();
        let session = create_session(None);
        let encoded = encode_workspace(
            std::slice::from_ref(&session),
            Some(session.id),
            &groups,
            &defs(&[]),
            &WorkspacePreferences::default(),
            &[session.id],
            &[],
        );
        assert_eq!(decode_workspace(&encoded).unwrap().groups, groups);
        let mut bad: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        bad["groups"][0]["activeEnvironmentId"] = 999.into();
        assert!(decode_workspace(&bad.to_string()).is_err());
        bad["groups"][0]["activeEnvironmentId"] = serde_json::Value::Null;
        bad["groups"][0]["environments"][0]["color"] = "#ff0000".into();
        assert!(decode_workspace(&bad.to_string()).is_err());
    }
}
