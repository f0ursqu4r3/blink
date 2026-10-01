//! The Browser rows and the selection rules of `RequestBrowser.vue`, without
//! a window.

use std::collections::HashMap;

use blink_core::model::{AuthKind, AuthorizationConfig, RequestGroup};
use blink_core::tree_guides::{TreeGuide, tree_guides};
use blink_core::workspace_state::Workspace;

/// One row of the tree: a group or a request, at an indent level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRow {
    Group { id: u64, level: usize },
    Request { id: u64, level: usize },
}

impl TreeRow {
    pub fn level(&self) -> usize {
        match self {
            TreeRow::Group { level, .. } | TreeRow::Request { level, .. } => *level,
        }
    }

    /// The Vue `rowKey`, also the drop indicator key of `blink_core::drag_drop`.
    pub fn key(&self) -> String {
        match self {
            TreeRow::Group { id, .. } => format!("group-{id}"),
            TreeRow::Request { id, .. } => format!("request-{id}"),
        }
    }
}

/// The group the Browser focuses, when it exists.
pub fn focused_group(workspace: &Workspace) -> Option<&RequestGroup> {
    workspace
        .focused_group_id()
        .and_then(|id| workspace.group(id))
}

/// The focused group always shows open; its saved state does not change.
pub fn is_open(group: &RequestGroup, focus: Option<u64>) -> bool {
    Some(group.id) == focus || !group.collapsed
}

/// The tree rows in display order: `rows` in `RequestBrowser.vue`.
pub fn tree_rows(workspace: &Workspace) -> Vec<TreeRow> {
    let focus = focused_group(workspace);
    let focus_id = focus.map(|group| group.id);
    let mut children: HashMap<Option<u64>, Vec<&RequestGroup>> = HashMap::new();
    for group in &workspace.groups {
        children.entry(group.parent_id).or_default().push(group);
    }
    let mut sessions: HashMap<Option<u64>, Vec<u64>> = HashMap::new();
    for session in &workspace.sessions {
        sessions
            .entry(session.group_id)
            .or_default()
            .push(session.id);
    }
    let mut items = Vec::new();
    if focus.is_none() {
        for id in sessions.get(&None).into_iter().flatten() {
            items.push(TreeRow::Request { id: *id, level: 0 });
        }
    }
    // Iterative depth-first walk, in the order of the recursive Vue `append`.
    let roots: Vec<&RequestGroup> = match focus {
        Some(group) => vec![group],
        None => children.get(&None).cloned().unwrap_or_default(),
    };
    let mut stack: Vec<(&RequestGroup, usize)> =
        roots.into_iter().rev().map(|group| (group, 0)).collect();
    // Guards a corrupt parent cycle.
    let mut seen = std::collections::HashSet::new();
    while let Some((group, level)) = stack.pop() {
        if !seen.insert(group.id) {
            continue;
        }
        items.push(TreeRow::Group {
            id: group.id,
            level,
        });
        if !is_open(group, focus_id) {
            continue;
        }
        for id in sessions.get(&Some(group.id)).into_iter().flatten() {
            items.push(TreeRow::Request {
                id: *id,
                level: level + 1,
            });
        }
        for child in children.get(&Some(group.id)).into_iter().flatten().rev() {
            stack.push((child, level + 1));
        }
    }
    items
}

/// Guides for the tree rows, in the same order.
pub fn row_guides(rows: &[TreeRow]) -> Vec<Option<TreeGuide>> {
    let levels: Vec<usize> = rows.iter().map(TreeRow::level).collect();
    tree_guides(&levels)
}

/// Left padding of a row at `level`.
pub fn indent(level: usize) -> f32 {
    const PX: [f32; 7] = [12., 32., 52., 72., 92., 112., 132.];
    PX[level.min(6)]
}

/// x of the line for children at `depth`: under the parent's folder icon.
pub fn guide_x(depth: usize) -> f32 {
    indent(depth.saturating_sub(1)) + 11.
}

/// Request ids in the tree, in display order.
pub fn visible_request_ids(rows: &[TreeRow]) -> Vec<u64> {
    rows.iter()
        .filter_map(|row| match row {
            TreeRow::Request { id, .. } => Some(*id),
            TreeRow::Group { .. } => None,
        })
        .collect()
}

/// The keys of the rows inside the group `key` (the rows the drag targets
/// with "into").
pub fn rows_inside(rows: &[TreeRow], key: &str) -> Vec<String> {
    let Some(start) = rows.iter().position(|row| row.key() == key) else {
        return Vec::new();
    };
    let level = rows[start].level();
    rows[start + 1..]
        .iter()
        .take_while(|row| row.level() > level)
        .map(TreeRow::key)
        .collect()
}

/// The selection after a click on a request row: `selectRequest`.
/// `toggle` is Cmd or Ctrl.
pub fn click_selection(
    selected: &[u64],
    anchor: Option<u64>,
    visible: &[u64],
    id: u64,
    shift: bool,
    toggle: bool,
) -> (Vec<u64>, Option<u64>) {
    if shift && let Some(anchor) = anchor {
        let start = visible.iter().position(|item| *item == anchor);
        let end = visible.iter().position(|item| *item == id);
        let ids = match (start, end) {
            (Some(start), Some(end)) => visible[start.min(end)..=start.max(end)].to_vec(),
            _ => vec![id],
        };
        return (ids, Some(anchor));
    }
    if toggle {
        let ids = if selected.contains(&id) {
            selected
                .iter()
                .copied()
                .filter(|item| *item != id)
                .collect()
        } else {
            selected.iter().copied().chain([id]).collect()
        };
        return (ids, Some(id));
    }
    (vec![id], Some(id))
}

/// The requests "Move selection here" moves: the selection, else the shown
/// active request.
pub fn selection_or_active(workspace: &Workspace) -> Vec<u64> {
    if !workspace.selected_ids.is_empty() {
        workspace.selected_ids.clone()
    } else {
        workspace.shown_active_id().into_iter().collect()
    }
}

/// True when the selection (or the active request) is all in `group_id`.
pub fn selection_already_in(workspace: &Workspace, group_id: Option<u64>) -> bool {
    let ids = selection_or_active(workspace);
    !ids.is_empty()
        && ids.iter().all(|id| {
            workspace
                .session(*id)
                .is_some_and(|session| session.group_id == group_id)
        })
}

/// True when requests are selected and not all of them are in `group_id`.
pub fn has_movable_selection(workspace: &Workspace, group_id: Option<u64>) -> bool {
    !workspace.selected_ids.is_empty() && !selection_already_in(workspace, group_id)
}

/// Move the selection (or the active request) into `group_id`.
pub fn move_selection(workspace: &mut Workspace, group_id: Option<u64>) {
    let ids = selection_or_active(workspace);
    move_targets(workspace, &ids, group_id);
}

/// `moveTargets`: several ids keep their order; one id keeps its place.
pub fn move_targets(workspace: &mut Workspace, ids: &[u64], group_id: Option<u64>) {
    match ids {
        [] => {}
        [id] => workspace.move_request(*id, group_id),
        _ => workspace.move_requests(ids, group_id, None),
    }
}

/// "Delete" or "Delete 3 requests", "Move to" or "Move 3 requests to".
pub fn count_label(verb: &str, count: usize, suffix: &str) -> String {
    if count > 1 {
        format!("{verb} {count} requests{suffix}")
    } else {
        format!("{verb}{suffix}")
    }
}

/// The request Authorization modes of the row menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    Inherit,
    None,
    Bearer,
    Basic,
}

impl AuthMode {
    pub const ALL: [AuthMode; 4] = [
        AuthMode::Inherit,
        AuthMode::None,
        AuthMode::Bearer,
        AuthMode::Basic,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AuthMode::Inherit => "Inherit",
            AuthMode::None => "No auth",
            AuthMode::Bearer => "Bearer",
            AuthMode::Basic => "Basic",
        }
    }

    pub fn of(local_auth: Option<&AuthorizationConfig>) -> AuthMode {
        match local_auth.map(AuthorizationConfig::kind) {
            None => AuthMode::Inherit,
            Some(AuthKind::None) => AuthMode::None,
            Some(AuthKind::Bearer) => AuthMode::Bearer,
            Some(AuthKind::Basic) => AuthMode::Basic,
        }
    }

    pub fn config(self) -> Option<AuthorizationConfig> {
        match self {
            AuthMode::Inherit => None,
            AuthMode::None => Some(AuthorizationConfig::None),
            AuthMode::Bearer => Some(AuthorizationConfig::Bearer {
                token: String::new(),
            }),
            AuthMode::Basic => Some(AuthorizationConfig::Basic {
                username: String::new(),
                password: String::new(),
            }),
        }
    }
}

/// The shared mode of the targets, or None when they differ.
pub fn shared_auth_mode(workspace: &Workspace, ids: &[u64]) -> Option<AuthMode> {
    let mut modes = ids.iter().map(|id| {
        AuthMode::of(
            workspace
                .session(*id)
                .and_then(|session| session.draft.local_auth.as_ref()),
        )
    });
    let first = modes.next()?;
    modes.all(|mode| mode == first).then_some(first)
}

/// Set the mode on each target. A target that already has it keeps its
/// credentials.
pub fn set_auth_mode(workspace: &mut Workspace, ids: &[u64], mode: AuthMode) {
    for id in ids {
        let current = AuthMode::of(
            workspace
                .session(*id)
                .and_then(|session| session.draft.local_auth.as_ref()),
        );
        if current != mode {
            workspace.set_request_local_auth(*id, mode.config());
        }
    }
}

/// The name of a group's parent for the delete confirmation.
pub fn parent_name(workspace: &Workspace, parent_id: Option<u64>) -> String {
    parent_id
        .and_then(|id| workspace.group(id))
        .map_or_else(|| "Browser".to_string(), |group| group.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use blink_core::session::create_session;

    fn workspace() -> (Workspace, [u64; 4], [u64; 3]) {
        let mut ws = Workspace::new();
        let first = ws.sessions[0].id;
        let platform = ws.add_group("Platform", None);
        let users = ws.add_group("Users", Some(platform));
        let other = ws.add_group("Other", None);
        let mut ids = [first, 0, 0, 0];
        for (index, group) in [(1, Some(platform)), (2, Some(users)), (3, Some(other))] {
            let mut session = create_session(None);
            session.group_id = group;
            ids[index] = session.id;
            ws.sessions.push(session);
        }
        (ws, ids, [platform, users, other])
    }

    #[test]
    fn lists_ungrouped_requests_then_nested_groups() {
        let (ws, [a, b, c, d], [platform, users, other]) = workspace();
        assert_eq!(
            tree_rows(&ws),
            vec![
                TreeRow::Request { id: a, level: 0 },
                TreeRow::Group {
                    id: platform,
                    level: 0
                },
                TreeRow::Request { id: b, level: 1 },
                TreeRow::Group {
                    id: users,
                    level: 1
                },
                TreeRow::Request { id: c, level: 2 },
                TreeRow::Group {
                    id: other,
                    level: 0
                },
                TreeRow::Request { id: d, level: 1 },
            ]
        );
    }

    #[test]
    fn hides_the_contents_of_a_collapsed_group() {
        let (mut ws, [a, _, _, d], [platform, _, other]) = workspace();
        ws.toggle_group(platform);
        assert_eq!(
            tree_rows(&ws),
            vec![
                TreeRow::Request { id: a, level: 0 },
                TreeRow::Group {
                    id: platform,
                    level: 0
                },
                TreeRow::Group {
                    id: other,
                    level: 0
                },
                TreeRow::Request { id: d, level: 1 },
            ]
        );
    }

    #[test]
    fn shows_only_the_focused_group_open() {
        let (mut ws, [_, b, c, _], [platform, users, _]) = workspace();
        ws.toggle_group(platform);
        ws.focus_group(platform);
        assert_eq!(
            tree_rows(&ws),
            vec![
                TreeRow::Group {
                    id: platform,
                    level: 0
                },
                TreeRow::Request { id: b, level: 1 },
                TreeRow::Group {
                    id: users,
                    level: 1
                },
                TreeRow::Request { id: c, level: 2 },
            ]
        );
    }

    #[test]
    fn selects_ranges_and_toggles() {
        let visible = [1, 2, 3, 4];
        assert_eq!(
            click_selection(&[1], Some(1), &visible, 3, true, false),
            (vec![1, 2, 3], Some(1))
        );
        assert_eq!(
            click_selection(&[3], Some(3), &visible, 1, true, false),
            (vec![1, 2, 3], Some(3))
        );
        assert_eq!(
            click_selection(&[1], Some(1), &visible, 3, false, true),
            (vec![1, 3], Some(3))
        );
        assert_eq!(
            click_selection(&[1, 3], Some(1), &visible, 3, false, true),
            (vec![1], Some(3))
        );
        assert_eq!(
            click_selection(&[1, 3], Some(1), &visible, 2, false, false),
            (vec![2], Some(2))
        );
        // No anchor: a plain selection.
        assert_eq!(
            click_selection(&[], None, &visible, 2, true, false),
            (vec![2], Some(2))
        );
    }

    #[test]
    fn finds_the_rows_inside_a_group() {
        let (ws, [_, b, c, _], [platform, users, _]) = workspace();
        let rows = tree_rows(&ws);
        assert_eq!(
            rows_inside(&rows, &format!("group-{platform}")),
            vec![
                format!("request-{b}"),
                format!("group-{users}"),
                format!("request-{c}")
            ]
        );
    }

    #[test]
    fn indents_and_places_guides() {
        assert_eq!(indent(0), 12.);
        assert_eq!(indent(9), 132.);
        assert_eq!(guide_x(1), 23.);
    }

    #[test]
    fn labels_counts() {
        assert_eq!(count_label("Move", 1, " to"), "Move to");
        assert_eq!(count_label("Move", 3, " to"), "Move 3 requests to");
        assert_eq!(count_label("Delete", 2, ""), "Delete 2 requests");
    }

    #[test]
    fn keeps_credentials_when_the_mode_does_not_change() {
        let (mut ws, [a, b, _, _], _) = workspace();
        ws.set_request_local_auth(
            a,
            Some(AuthorizationConfig::Bearer {
                token: "secret".into(),
            }),
        );
        assert_eq!(shared_auth_mode(&ws, &[a, b]), None);
        set_auth_mode(&mut ws, &[a, b], AuthMode::Bearer);
        assert_eq!(
            ws.session(a).unwrap().draft.local_auth,
            Some(AuthorizationConfig::Bearer {
                token: "secret".into()
            })
        );
        assert_eq!(shared_auth_mode(&ws, &[a, b]), Some(AuthMode::Bearer));
        set_auth_mode(&mut ws, &[a], AuthMode::Inherit);
        assert_eq!(ws.session(a).unwrap().draft.local_auth, None);
    }

    #[test]
    fn moves_the_selection() {
        let (mut ws, [a, _, _, _], [platform, _, _]) = workspace();
        ws.update_selection(vec![a], Some(a));
        assert!(has_movable_selection(&ws, Some(platform)));
        move_selection(&mut ws, Some(platform));
        assert_eq!(ws.session(a).unwrap().group_id, Some(platform));
        assert!(!has_movable_selection(&ws, Some(platform)));
        ws.update_selection(vec![], None);
        assert!(!has_movable_selection(&ws, None));
    }

    #[test]
    fn names_the_parent_for_delete() {
        let (ws, _, [platform, _, _]) = workspace();
        assert_eq!(parent_name(&ws, None), "Browser");
        assert_eq!(parent_name(&ws, Some(platform)), "Platform");
    }
}
