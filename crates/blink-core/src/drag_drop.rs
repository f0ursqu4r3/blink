//! Drop targets for the Browser tree and the tab bar. Port of `src/lib/drag-drop.ts`.

use std::collections::HashSet;

use crate::groups::{GroupedSession, can_nest_group, groups_after_move, sessions_after_move};
use crate::model::RequestGroup;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropZone {
    Before,
    Into,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropBox {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DragPayload {
    Requests(Vec<u64>),
    Group(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootPosition {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeTarget {
    Request(u64),
    Group(u64),
    Root(RootPosition),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    pub sessions: Vec<GroupedSession>,
    pub groups: Vec<RequestGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeCommand {
    MoveRequests {
        ids: Vec<u64>,
        group_id: Option<u64>,
        before_id: Option<u64>,
    },
    MoveGroup {
        group_id: u64,
        parent_id: Option<u64>,
        before_group_id: Option<u64>,
    },
}

/// `key` names the row that shows the indicator: "root", "group-<id>", or "request-<id>".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeDrop {
    pub key: String,
    pub zone: DropZone,
    pub command: TreeCommand,
}

/// `key` is "tab-<id>", or "strip" when no tabs are open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabDrop {
    pub key: String,
    pub zone: DropZone,
    pub ids: Vec<u64>,
    pub before_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Request,
    Group,
    Tab,
}

/// Which part of a row the pointer is over.
pub fn hit_zone(bounds: DropBox, point: Point, kind: RowKind) -> DropZone {
    if kind == RowKind::Tab {
        return if point.x < bounds.left + bounds.width / 2.0 {
            DropZone::Before
        } else {
            DropZone::After
        };
    }
    let offset = (point.y - bounds.top) / bounds.height;
    if kind == RowKind::Request {
        return if offset < 0.5 {
            DropZone::Before
        } else {
            DropZone::After
        };
    }
    if offset < 0.25 {
        DropZone::Before
    } else if offset > 0.75 {
        DropZone::After
    } else {
        DropZone::Into
    }
}

/// First item after `index` that is not skipped and matches. `index` -1 starts
/// at the first item.
fn next_id<T>(
    items: &[T],
    index: isize,
    skip: &HashSet<u64>,
    id: impl Fn(&T) -> u64,
    matches: impl Fn(&T) -> bool,
) -> Option<u64> {
    let start = (index + 1).max(0) as usize;
    items
        .iter()
        .skip(start)
        .find(|item| !skip.contains(&id(item)) && matches(item))
        .map(id)
}

fn same_order(left: &[u64], right: &[u64]) -> bool {
    left == right
}

fn group_key(group_id: Option<u64>) -> String {
    match group_id {
        None => "root".into(),
        Some(id) => format!("group-{id}"),
    }
}

pub fn resolve_tree_drop(
    payload: &DragPayload,
    target: TreeTarget,
    zone: DropZone,
    tree: &Tree,
) -> Option<TreeDrop> {
    match payload {
        DragPayload::Requests(ids) => resolve_requests_drop(ids, target, zone, tree),
        DragPayload::Group(id) => resolve_group_drop(*id, target, zone, tree),
    }
}

fn resolve_requests_drop(
    request_ids: &[u64],
    target: TreeTarget,
    zone: DropZone,
    tree: &Tree,
) -> Option<TreeDrop> {
    let ids: Vec<u64> = request_ids
        .iter()
        .copied()
        .filter(|id| tree.sessions.iter().any(|session| session.id == *id))
        .collect();
    if ids.is_empty() {
        return None;
    }
    let moving: HashSet<u64> = ids.iter().copied().collect();
    let session_id = |session: &GroupedSession| session.id;
    let mut group_id = None;
    let mut before_id = None;
    let mut key = "root".to_string();
    let mut effective = DropZone::Into;
    match target {
        TreeTarget::Root(position) => {
            if position == RootPosition::Start {
                before_id = next_id(&tree.sessions, -1, &moving, session_id, |s| {
                    s.group_id.is_none()
                });
            }
        }
        TreeTarget::Group(id) => {
            if !tree.groups.iter().any(|group| group.id == id) {
                return None;
            }
            group_id = Some(id);
            key = group_key(group_id);
        }
        TreeTarget::Request(id) => {
            let index = tree.sessions.iter().position(|session| session.id == id)?;
            if zone == DropZone::Into {
                return None;
            }
            let anchor = tree.sessions[index];
            group_id = anchor.group_id;
            key = format!("request-{}", anchor.id);
            effective = zone;
            let from = if zone == DropZone::Before {
                index as isize - 1
            } else {
                index as isize
            };
            before_id = next_id(&tree.sessions, from, &moving, session_id, |s| {
                s.group_id == group_id
            });
        }
    }
    let in_group = |sessions: &[GroupedSession]| -> Vec<u64> {
        sessions
            .iter()
            .filter(|session| session.group_id == group_id)
            .map(|session| session.id)
            .collect()
    };
    let unchanged = tree
        .sessions
        .iter()
        .all(|session| !moving.contains(&session.id) || session.group_id == group_id)
        && same_order(
            &in_group(&sessions_after_move(
                tree.sessions.clone(),
                &ids,
                group_id,
                before_id,
            )),
            &in_group(&tree.sessions),
        );
    if unchanged {
        return None;
    }
    Some(TreeDrop {
        key,
        zone: effective,
        command: TreeCommand::MoveRequests {
            ids,
            group_id,
            before_id,
        },
    })
}

fn resolve_group_drop(
    group_id: u64,
    target: TreeTarget,
    zone: DropZone,
    tree: &Tree,
) -> Option<TreeDrop> {
    let source = tree.groups.iter().find(|group| group.id == group_id)?;
    let skip = HashSet::from([group_id]);
    let group_id_of = |group: &RequestGroup| group.id;
    let mut parent_id = None;
    let mut before_group_id = None;
    let mut key = "root".to_string();
    let mut effective = DropZone::Into;
    match target {
        TreeTarget::Root(position) => {
            if position == RootPosition::Start {
                before_group_id = next_id(&tree.groups, -1, &skip, group_id_of, |g| {
                    g.parent_id.is_none()
                });
            }
        }
        TreeTarget::Request(id) => {
            let anchor = tree.sessions.iter().find(|session| session.id == id)?;
            parent_id = anchor.group_id;
            key = group_key(parent_id);
        }
        TreeTarget::Group(id) => {
            let index = tree.groups.iter().position(|group| group.id == id)?;
            let anchor = &tree.groups[index];
            if anchor.id == group_id {
                return None;
            }
            let has_children = tree.groups.iter().any(|g| g.parent_id == Some(anchor.id))
                || tree
                    .sessions
                    .iter()
                    .any(|session| session.group_id == Some(anchor.id));
            effective = if zone == DropZone::After && !anchor.collapsed && has_children {
                DropZone::Into
            } else {
                zone
            };
            key = group_key(Some(anchor.id));
            if effective == DropZone::Into {
                parent_id = Some(anchor.id);
            } else {
                parent_id = anchor.parent_id;
                let from = if effective == DropZone::Before {
                    index as isize - 1
                } else {
                    index as isize
                };
                before_group_id = next_id(&tree.groups, from, &skip, group_id_of, |g| {
                    g.parent_id == parent_id
                });
            }
        }
    }
    if !can_nest_group(&tree.groups, group_id, parent_id) {
        return None;
    }
    let next = groups_after_move(&tree.groups, group_id, parent_id, before_group_id)?;
    let children = |groups: &[RequestGroup]| -> Vec<u64> {
        groups
            .iter()
            .filter(|group| group.parent_id == parent_id)
            .map(|group| group.id)
            .collect()
    };
    if source.parent_id == parent_id && same_order(&children(&next), &children(&tree.groups)) {
        return None;
    }
    Some(TreeDrop {
        key,
        zone: effective,
        command: TreeCommand::MoveGroup {
            group_id,
            parent_id,
            before_group_id,
        },
    })
}

/// Drop requests on the tab bar. `target_id` None means no tabs are open.
pub fn resolve_tab_drop(
    payload: &DragPayload,
    target_id: Option<u64>,
    zone: DropZone,
    open_ids: &[u64],
) -> Option<TabDrop> {
    let DragPayload::Requests(payload_ids) = payload else {
        return None;
    };
    if payload_ids.is_empty() {
        return None;
    }
    let mut ids = Vec::new();
    for id in payload_ids {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    let moving: HashSet<u64> = ids.iter().copied().collect();
    let index = match target_id {
        None => open_ids.len(),
        Some(target) => open_ids.iter().position(|id| *id == target)?,
    };
    let start = if zone == DropZone::Before {
        index
    } else {
        index + 1
    };
    let before_id = open_ids
        .iter()
        .skip(start)
        .find(|id| !moving.contains(id))
        .copied();
    let remaining: Vec<u64> = open_ids
        .iter()
        .copied()
        .filter(|id| !moving.contains(id))
        .collect();
    let at = before_id
        .and_then(|before| remaining.iter().position(|id| *id == before))
        .unwrap_or(remaining.len());
    let next: Vec<u64> = remaining[..at]
        .iter()
        .chain(&ids)
        .chain(&remaining[at..])
        .copied()
        .collect();
    if next == open_ids {
        return None;
    }
    Some(TabDrop {
        key: match target_id {
            None => "strip".into(),
            Some(target) => format!("tab-{target}"),
        },
        zone,
        ids,
        before_id,
    })
}

/// A new place in the tree: the group and the request to insert before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestStep {
    pub group_id: Option<u64>,
    pub before_id: Option<u64>,
}

/// Keyboard move: new place for `ids` one row up or down inside their group.
pub fn step_requests(
    sessions: &[GroupedSession],
    ids: &[u64],
    direction: i32,
) -> Option<RequestStep> {
    let moving: HashSet<u64> = ids.iter().copied().collect();
    let first = sessions
        .iter()
        .find(|session| moving.contains(&session.id))?;
    if sessions
        .iter()
        .any(|session| moving.contains(&session.id) && session.group_id != first.group_id)
    {
        return None;
    }
    let group_id = first.group_id;
    let siblings: Vec<&GroupedSession> = sessions
        .iter()
        .filter(|session| session.group_id == group_id)
        .collect();
    let positions: Vec<usize> = siblings
        .iter()
        .enumerate()
        .filter(|(_, session)| moving.contains(&session.id))
        .map(|(index, _)| index)
        .collect();
    if direction < 0 {
        let previous = positions[0].checked_sub(1).map(|index| siblings[index])?;
        return Some(RequestStep {
            group_id,
            before_id: Some(previous.id),
        });
    }
    let last = *positions.last()?;
    if last + 1 >= siblings.len() {
        return None;
    }
    Some(RequestStep {
        group_id,
        before_id: siblings.get(last + 2).map(|session| session.id),
    })
}

/// Keyboard move: new place for tab `id` one step left or right. The inner
/// value is the tab to insert before, None for the end.
pub fn step_tab(open_ids: &[u64], id: u64, direction: i32) -> Option<Option<u64>> {
    let index = open_ids.iter().position(|open| *open == id)? as isize;
    let target = index + direction as isize;
    if target < 0 || target as usize >= open_ids.len() {
        return None;
    }
    let target = target as usize;
    Some(if direction < 0 {
        Some(open_ids[target])
    } else {
        open_ids.get(target + 1).copied()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: DropBox = DropBox {
        left: 0.0,
        top: 100.0,
        width: 200.0,
        height: 40.0,
    };

    fn point(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    fn group(id: u64, parent_id: Option<u64>, collapsed: bool) -> RequestGroup {
        RequestGroup {
            id,
            name: format!("G{id}"),
            parent_id,
            collapsed,
            local_auth: None,
            local_definitions: None,
            default_method: None,
            default_url: None,
            environments: None,
            active_environment_id: None,
        }
    }

    fn session(id: u64, group_id: Option<u64>) -> GroupedSession {
        GroupedSession { id, group_id }
    }

    // Root: requests 1, 2. Group 10 (root) holds request 3 and group 11.
    // Group 11 is empty. Group 12 (root) is collapsed and empty.
    fn tree() -> Tree {
        Tree {
            sessions: vec![session(1, None), session(2, None), session(3, Some(10))],
            groups: vec![
                group(10, None, false),
                group(11, Some(10), false),
                group(12, None, true),
            ],
        }
    }

    fn requests(ids: &[u64]) -> DragPayload {
        DragPayload::Requests(ids.to_vec())
    }

    fn move_requests(ids: &[u64], group_id: Option<u64>, before_id: Option<u64>) -> TreeCommand {
        TreeCommand::MoveRequests {
            ids: ids.to_vec(),
            group_id,
            before_id,
        }
    }

    fn move_group(
        group_id: u64,
        parent_id: Option<u64>,
        before_group_id: Option<u64>,
    ) -> TreeCommand {
        TreeCommand::MoveGroup {
            group_id,
            parent_id,
            before_group_id,
        }
    }

    fn drop(key: &str, zone: DropZone, command: TreeCommand) -> Option<TreeDrop> {
        Some(TreeDrop {
            key: key.into(),
            zone,
            command,
        })
    }

    // hitZone

    #[test]
    fn splits_request_rows_in_half() {
        assert_eq!(
            hit_zone(BOX, point(5.0, 119.0), RowKind::Request),
            DropZone::Before
        );
        assert_eq!(
            hit_zone(BOX, point(5.0, 120.0), RowKind::Request),
            DropZone::After
        );
    }

    #[test]
    fn splits_group_rows_25_50_25() {
        assert_eq!(
            hit_zone(BOX, point(5.0, 109.0), RowKind::Group),
            DropZone::Before
        );
        assert_eq!(
            hit_zone(BOX, point(5.0, 110.0), RowKind::Group),
            DropZone::Into
        );
        assert_eq!(
            hit_zone(BOX, point(5.0, 130.0), RowKind::Group),
            DropZone::Into
        );
        assert_eq!(
            hit_zone(BOX, point(5.0, 131.0), RowKind::Group),
            DropZone::After
        );
    }

    #[test]
    fn splits_tabs_left_right() {
        assert_eq!(
            hit_zone(BOX, point(99.0, 0.0), RowKind::Tab),
            DropZone::Before
        );
        assert_eq!(
            hit_zone(BOX, point(100.0, 0.0), RowKind::Tab),
            DropZone::After
        );
    }

    // resolveTreeDrop – requests

    #[test]
    fn drops_before_a_request() {
        assert_eq!(
            resolve_tree_drop(
                &requests(&[2]),
                TreeTarget::Request(1),
                DropZone::Before,
                &tree()
            ),
            drop(
                "request-1",
                DropZone::Before,
                move_requests(&[2], None, Some(1))
            )
        );
    }

    #[test]
    fn drops_after_the_last_request_of_a_list() {
        assert_eq!(
            resolve_tree_drop(
                &requests(&[1]),
                TreeTarget::Request(2),
                DropZone::After,
                &tree()
            )
            .map(|d| d.command),
            Some(move_requests(&[1], None, None))
        );
    }

    #[test]
    fn returns_null_when_the_order_does_not_change() {
        assert_eq!(
            resolve_tree_drop(
                &requests(&[1]),
                TreeTarget::Request(1),
                DropZone::After,
                &tree()
            ),
            None
        );
        assert_eq!(
            resolve_tree_drop(
                &requests(&[1]),
                TreeTarget::Request(2),
                DropZone::Before,
                &tree()
            ),
            None
        );
    }

    #[test]
    fn drops_into_a_group_on_any_zone_of_the_group_row() {
        for zone in [DropZone::Before, DropZone::Into, DropZone::After] {
            assert_eq!(
                resolve_tree_drop(&requests(&[1]), TreeTarget::Group(11), zone, &tree()),
                drop(
                    "group-11",
                    DropZone::Into,
                    move_requests(&[1], Some(11), None)
                )
            );
        }
    }

    #[test]
    fn moves_a_multi_selection_across_folders() {
        assert_eq!(
            resolve_tree_drop(
                &requests(&[1, 3]),
                TreeTarget::Request(2),
                DropZone::Before,
                &tree()
            )
            .map(|d| d.command),
            Some(move_requests(&[1, 3], None, Some(2)))
        );
    }

    #[test]
    fn drops_at_the_start_of_root() {
        assert_eq!(
            resolve_tree_drop(
                &requests(&[3]),
                TreeTarget::Root(RootPosition::Start),
                DropZone::Into,
                &tree()
            ),
            drop("root", DropZone::Into, move_requests(&[3], None, Some(1)))
        );
    }

    #[test]
    fn drops_unknown_ids_as_null() {
        assert_eq!(
            resolve_tree_drop(
                &requests(&[99]),
                TreeTarget::Group(10),
                DropZone::Into,
                &tree()
            ),
            None
        );
    }

    // resolveTreeDrop – groups

    #[test]
    fn nests_a_group_into_another() {
        assert_eq!(
            resolve_tree_drop(
                &DragPayload::Group(12),
                TreeTarget::Group(11),
                DropZone::Into,
                &tree()
            ),
            drop("group-11", DropZone::Into, move_group(12, Some(11), None))
        );
    }

    #[test]
    fn reorders_before_a_sibling() {
        assert_eq!(
            resolve_tree_drop(
                &DragPayload::Group(12),
                TreeTarget::Group(10),
                DropZone::Before,
                &tree()
            )
            .map(|d| d.command),
            Some(move_group(12, None, Some(10)))
        );
    }

    #[test]
    fn treats_after_on_an_expanded_group_with_children_as_into() {
        let result = resolve_tree_drop(
            &DragPayload::Group(12),
            TreeTarget::Group(10),
            DropZone::After,
            &tree(),
        )
        .unwrap();
        assert_eq!(result.zone, DropZone::Into);
        assert!(matches!(
            result.command,
            TreeCommand::MoveGroup {
                parent_id: Some(10),
                ..
            }
        ));
    }

    #[test]
    fn keeps_after_on_a_collapsed_group() {
        let result = resolve_tree_drop(
            &DragPayload::Group(10),
            TreeTarget::Group(12),
            DropZone::After,
            &tree(),
        )
        .unwrap();
        assert_eq!(result.zone, DropZone::After);
        assert!(matches!(
            result.command,
            TreeCommand::MoveGroup {
                parent_id: None,
                before_group_id: None,
                ..
            }
        ));
    }

    #[test]
    fn rejects_a_group_into_itself_or_its_descendant() {
        assert_eq!(
            resolve_tree_drop(
                &DragPayload::Group(10),
                TreeTarget::Group(10),
                DropZone::Into,
                &tree()
            ),
            None
        );
        assert_eq!(
            resolve_tree_drop(
                &DragPayload::Group(10),
                TreeTarget::Group(11),
                DropZone::Into,
                &tree()
            ),
            None
        );
    }

    #[test]
    fn un_nests_to_root_end() {
        assert_eq!(
            resolve_tree_drop(
                &DragPayload::Group(11),
                TreeTarget::Root(RootPosition::End),
                DropZone::Into,
                &tree()
            ),
            drop("root", DropZone::Into, move_group(11, None, None))
        );
    }

    #[test]
    fn drops_onto_a_request_row_into_that_requests_group() {
        let result = resolve_tree_drop(
            &DragPayload::Group(12),
            TreeTarget::Request(3),
            DropZone::Before,
            &tree(),
        )
        .unwrap();
        assert_eq!(
            (result.key.as_str(), result.zone),
            ("group-10", DropZone::Into)
        );
        assert!(matches!(
            result.command,
            TreeCommand::MoveGroup {
                parent_id: Some(10),
                ..
            }
        ));
    }

    #[test]
    fn returns_null_for_a_no_op() {
        assert_eq!(
            resolve_tree_drop(
                &DragPayload::Group(12),
                TreeTarget::Root(RootPosition::End),
                DropZone::Into,
                &tree()
            ),
            None
        );
    }

    // resolveTabDrop

    const OPEN: [u64; 3] = [1, 2, 3];

    #[test]
    fn reorders_a_tab_after_another() {
        assert_eq!(
            resolve_tab_drop(&requests(&[1]), Some(2), DropZone::After, &OPEN),
            Some(TabDrop {
                key: "tab-2".into(),
                zone: DropZone::After,
                ids: vec![1],
                before_id: Some(3),
            })
        );
    }

    #[test]
    fn opens_a_closed_request_before_a_tab() {
        let result = resolve_tab_drop(&requests(&[9]), Some(1), DropZone::Before, &OPEN).unwrap();
        assert_eq!((result.ids, result.before_id), (vec![9], Some(1)));
    }

    #[test]
    fn appends_after_the_last_tab() {
        assert_eq!(
            resolve_tab_drop(&requests(&[1]), Some(3), DropZone::After, &OPEN)
                .unwrap()
                .before_id,
            None
        );
    }

    #[test]
    fn uses_the_strip_key_with_no_tabs() {
        assert_eq!(
            resolve_tab_drop(&requests(&[1]), None, DropZone::After, &[]),
            Some(TabDrop {
                key: "strip".into(),
                zone: DropZone::After,
                ids: vec![1],
                before_id: None,
            })
        );
    }

    #[test]
    fn returns_null_for_no_ops_and_groups() {
        assert_eq!(
            resolve_tab_drop(&requests(&[2]), Some(1), DropZone::After, &OPEN),
            None
        );
        assert_eq!(
            resolve_tab_drop(&DragPayload::Group(1), Some(1), DropZone::After, &OPEN),
            None
        );
    }

    // stepRequests

    fn flat() -> Vec<GroupedSession> {
        vec![session(1, None), session(2, None), session(3, None)]
    }

    fn step(group_id: Option<u64>, before_id: Option<u64>) -> Option<RequestStep> {
        Some(RequestStep {
            group_id,
            before_id,
        })
    }

    #[test]
    fn moves_up_before_the_previous_sibling() {
        assert_eq!(step_requests(&flat(), &[2], -1), step(None, Some(1)));
    }

    #[test]
    fn moves_down_past_the_next_sibling() {
        assert_eq!(step_requests(&flat(), &[1], 1), step(None, Some(3)));
        assert_eq!(step_requests(&flat(), &[2], 1), step(None, None));
    }

    #[test]
    fn stops_at_the_edges_and_across_groups() {
        assert_eq!(step_requests(&flat(), &[1], -1), None);
        assert_eq!(step_requests(&flat(), &[3], 1), None);
        let mut mixed = flat();
        mixed.push(session(4, Some(5)));
        assert_eq!(step_requests(&mixed, &[3, 4], 1), None);
    }

    // stepTab

    #[test]
    fn moves_left_and_right() {
        assert_eq!(step_tab(&[1, 2, 3], 2, -1), Some(Some(1)));
        assert_eq!(step_tab(&[1, 2, 3], 1, 1), Some(Some(3)));
        assert_eq!(step_tab(&[1, 2, 3], 2, 1), Some(None));
    }

    #[test]
    fn stops_at_the_edges() {
        assert_eq!(step_tab(&[1, 2], 1, -1), None);
        assert_eq!(step_tab(&[1, 2], 2, 1), None);
    }
}
