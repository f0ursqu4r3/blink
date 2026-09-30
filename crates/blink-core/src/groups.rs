//! Browser groups and tree moves. Port of `src/lib/groups.ts`.

use std::collections::{HashMap, HashSet};

use crate::ids;
use crate::model::{RequestGroup, RequestSession};

/// Anything placed in the Browser tree by group.
pub trait Grouped {
    fn id(&self) -> u64;
    fn group_id(&self) -> Option<u64>;
    fn set_group_id(&mut self, group_id: Option<u64>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupedSession {
    pub id: u64,
    pub group_id: Option<u64>,
}

impl Grouped for GroupedSession {
    fn id(&self) -> u64 {
        self.id
    }
    fn group_id(&self) -> Option<u64> {
        self.group_id
    }
    fn set_group_id(&mut self, group_id: Option<u64>) {
        self.group_id = group_id;
    }
}

impl Grouped for RequestSession {
    fn id(&self) -> u64 {
        self.id
    }
    fn group_id(&self) -> Option<u64> {
        self.group_id
    }
    fn set_group_id(&mut self, group_id: Option<u64>) {
        self.group_id = group_id;
    }
}

impl From<&RequestSession> for GroupedSession {
    fn from(session: &RequestSession) -> Self {
        GroupedSession {
            id: session.id,
            group_id: session.group_id,
        }
    }
}

pub fn create_group(name: &str, parent_id: Option<u64>) -> RequestGroup {
    RequestGroup {
        id: ids::GROUPS.next(),
        name: name.trim().to_string(),
        parent_id,
        collapsed: false,
        local_auth: None,
        local_definitions: None,
        default_method: None,
        default_url: None,
        environments: None,
        active_environment_id: None,
    }
}

pub fn can_nest_group(groups: &[RequestGroup], group_id: u64, parent_id: Option<u64>) -> bool {
    let Some(parent_id) = parent_id else {
        return true;
    };
    if group_id == parent_id {
        return false;
    }
    let parent_by_id: HashMap<u64, Option<u64>> = groups
        .iter()
        .map(|group| (group.id, group.parent_id))
        .collect();
    let mut cursor = Some(parent_id);
    let mut seen = HashSet::new();
    while let Some(id) = cursor {
        if id == group_id || seen.contains(&id) || !parent_by_id.contains_key(&id) {
            return false;
        }
        seen.insert(id);
        cursor = parent_by_id[&id];
    }
    true
}

/// Ids of `group_id` and all of its descendants. Empty for an unknown group.
pub fn group_subtree(groups: &[RequestGroup], group_id: u64) -> HashSet<u64> {
    let mut ids = HashSet::new();
    if !groups.iter().any(|group| group.id == group_id) {
        return ids;
    }
    ids.insert(group_id);
    let mut grew = true;
    while grew {
        grew = false;
        for group in groups {
            if let Some(parent) = group.parent_id
                && ids.contains(&parent)
                && !ids.contains(&group.id)
            {
                ids.insert(group.id);
                grew = true;
            }
        }
    }
    ids
}

/// Order of `sessions` after moving `ids` into `group_id`, before `before_id`
/// or after the last session of that group. Keeps the same items; the
/// caller assigns `group_id`.
pub fn sessions_after_move<T: Grouped>(
    sessions: Vec<T>,
    ids: &[u64],
    group_id: Option<u64>,
    before_id: Option<u64>,
) -> Vec<T> {
    let moving: HashSet<u64> = ids.iter().copied().collect();
    let (moved, mut remaining): (Vec<T>, Vec<T>) = sessions
        .into_iter()
        .partition(|session| moving.contains(&session.id()));
    let insert_at = before_id
        .and_then(|before| {
            remaining
                .iter()
                .position(|session| session.id() == before && session.group_id() == group_id)
        })
        .unwrap_or_else(|| {
            remaining
                .iter()
                .rposition(|session| session.group_id() == group_id)
                .map_or(remaining.len(), |last| last + 1)
        });
    remaining.splice(insert_at..insert_at, moved);
    remaining
}

/// Order of `groups` after moving `group_id` under `parent_id`, before
/// `before_group_id` or after its last new sibling. The caller assigns
/// `parent_id`. None when the move names an unknown group or would nest a
/// group inside itself.
pub fn groups_after_move(
    groups: &[RequestGroup],
    group_id: u64,
    parent_id: Option<u64>,
    before_group_id: Option<u64>,
) -> Option<Vec<RequestGroup>> {
    let source = groups.iter().find(|group| group.id == group_id)?;
    if !can_nest_group(groups, group_id, parent_id) {
        return None;
    }
    let mut remaining: Vec<RequestGroup> = groups
        .iter()
        .filter(|group| group.id != group_id)
        .cloned()
        .collect();
    let insert_at = before_group_id
        .and_then(|before| {
            remaining
                .iter()
                .position(|group| group.id == before && group.parent_id == parent_id)
        })
        .unwrap_or_else(|| {
            remaining
                .iter()
                .rposition(|group| group.parent_id == parent_id)
                .map_or(remaining.len(), |last| last + 1)
        });
    remaining.insert(insert_at, source.clone());
    Some(remaining)
}

/// Remove a group. Its child groups and requests move to its parent.
pub fn delete_group_and_promote_contents<T: Grouped>(
    groups: &mut Vec<RequestGroup>,
    sessions: &mut [T],
    group_id: u64,
) {
    let Some(index) = groups.iter().position(|group| group.id == group_id) else {
        return;
    };
    let removed = groups.remove(index);
    for group in groups.iter_mut() {
        if group.parent_id == Some(group_id) {
            group.parent_id = removed.parent_id;
        }
    }
    for session in sessions.iter_mut() {
        if session.group_id() == Some(group_id) {
            session.set_group_id(removed.parent_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: u64, parent_id: Option<u64>) -> RequestGroup {
        RequestGroup {
            id,
            name: format!("G{id}"),
            parent_id,
            collapsed: false,
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
    fn session_ids(items: &[GroupedSession]) -> Vec<u64> {
        items.iter().map(|item| item.id).collect()
    }
    fn group_ids(items: &[RequestGroup]) -> Vec<u64> {
        items.iter().map(|item| item.id).collect()
    }
    fn set(ids: &[u64]) -> HashSet<u64> {
        ids.iter().copied().collect()
    }

    // groups.test.ts

    #[test]
    fn collects_a_group_and_all_of_its_descendants() {
        let groups = [
            group(1, None),
            group(2, Some(1)),
            group(3, Some(2)),
            group(4, None),
        ];
        assert_eq!(group_subtree(&groups, 1), set(&[1, 2, 3]));
        assert_eq!(group_subtree(&groups, 2), set(&[2, 3]));
        assert_eq!(group_subtree(&groups, 4), set(&[4]));
        assert_eq!(group_subtree(&groups, 9), set(&[]));
    }

    #[test]
    fn creates_groups_with_distinct_identifiers_and_explicit_parents() {
        let root = create_group("Platform", None);
        let child = create_group("Identity", Some(root.id));
        assert_eq!((root.name.as_str(), root.parent_id), ("Platform", None));
        assert_eq!(
            (child.name.as_str(), child.parent_id),
            ("Identity", Some(root.id))
        );
        assert!(child.id > root.id);
    }

    #[test]
    fn rejects_moves_that_make_a_group_its_own_ancestor() {
        let groups = [group(1, None), group(2, Some(1)), group(3, Some(2))];
        assert!(!can_nest_group(&groups, 1, Some(3)));
        assert!(can_nest_group(&groups, 3, Some(1)));
    }

    #[test]
    fn promotes_child_groups_and_moves_requests_to_the_deleted_groups_parent() {
        ids::GROUPS.reserve(3);
        let parent = group(1, None);
        let child = group(3, Some(2));
        let mut groups = vec![parent.clone(), group(2, Some(1)), child.clone()];
        let mut sessions = [session(10, Some(2)), session(11, Some(3))];
        delete_group_and_promote_contents(&mut groups, &mut sessions, 2);
        assert_eq!(
            groups,
            vec![
                parent,
                RequestGroup {
                    parent_id: Some(1),
                    ..child
                }
            ]
        );
        assert_eq!(sessions, [session(10, Some(1)), session(11, Some(3))]);
    }

    // group-reparent.test.ts

    #[test]
    fn rejects_self_and_descendant_parents() {
        let groups = [group(1, None), group(2, Some(1)), group(3, Some(2))];
        assert!(!can_nest_group(&groups, 1, Some(1)));
        assert!(!can_nest_group(&groups, 1, Some(3)));
        assert!(can_nest_group(&groups, 3, None));
    }

    // groups-order.test.ts

    fn sessions() -> Vec<GroupedSession> {
        vec![
            session(1, None),
            session(2, Some(10)),
            session(3, None),
            session(4, Some(10)),
        ]
    }

    #[test]
    fn inserts_before_the_named_session() {
        assert_eq!(
            session_ids(&sessions_after_move(sessions(), &[4], Some(10), Some(2))),
            [1, 4, 2, 3]
        );
    }

    #[test]
    fn appends_after_the_last_session_of_the_group() {
        assert_eq!(
            session_ids(&sessions_after_move(sessions(), &[1], Some(10), None)),
            [2, 3, 4, 1]
        );
    }

    #[test]
    fn appends_to_the_end_for_an_empty_group() {
        assert_eq!(
            session_ids(&sessions_after_move(sessions(), &[1], Some(20), None)),
            [2, 3, 4, 1]
        );
    }

    #[test]
    fn ignores_a_before_id_from_another_group_and_appends() {
        assert_eq!(
            session_ids(&sessions_after_move(sessions(), &[3], Some(10), Some(1))),
            [1, 2, 4, 3]
        );
    }

    #[test]
    fn keeps_the_source_order_of_several_moved_sessions() {
        assert_eq!(
            session_ids(&sessions_after_move(sessions(), &[4, 1], None, Some(3))),
            [2, 1, 4, 3]
        );
    }

    #[test]
    fn returns_the_same_objects() {
        let moved = sessions_after_move(sessions(), &[1], None, None);
        assert_eq!(moved[0], sessions()[1]);
    }

    fn order_groups() -> Vec<RequestGroup> {
        vec![
            group(1, None),
            group(2, None),
            group(3, Some(1)),
            group(4, Some(1)),
        ]
    }

    #[test]
    fn reorders_among_siblings() {
        assert_eq!(
            group_ids(&groups_after_move(&order_groups(), 2, None, Some(1)).unwrap()),
            [2, 1, 3, 4]
        );
    }

    #[test]
    fn nests_at_the_end_of_the_new_parents_children() {
        assert_eq!(
            group_ids(&groups_after_move(&order_groups(), 2, Some(1), None).unwrap()),
            [1, 3, 4, 2]
        );
    }

    #[test]
    fn un_nests_before_a_root_group() {
        assert_eq!(
            group_ids(&groups_after_move(&order_groups(), 4, None, Some(1)).unwrap()),
            [4, 1, 2, 3]
        );
    }

    #[test]
    fn rejects_a_move_into_a_descendant() {
        assert!(groups_after_move(&order_groups(), 1, Some(3), None).is_none());
    }

    #[test]
    fn rejects_unknown_ids() {
        assert!(groups_after_move(&order_groups(), 99, None, None).is_none());
        assert!(groups_after_move(&order_groups(), 2, Some(99), None).is_none());
    }

    #[test]
    fn does_not_assign_parent_id() {
        let groups = order_groups();
        let moved = groups_after_move(&groups, 2, Some(1), None).unwrap();
        assert_eq!(groups[1].parent_id, None);
        assert_eq!(moved[3].parent_id, None);
    }
}
