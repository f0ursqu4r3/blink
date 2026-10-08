use super::*;
use crate::model::RequestView;
use serde_json::json;

pub(super) fn validate(document: &ProjectDocument) -> Result<(), String> {
    let fail = || "Invalid project IDs, ancestry, or references.".to_string();
    if document.groups.is_empty()
        || document.groups.len() > crate::workspace::MAX_GROUPS
        || document.requests.len() > crate::workspace::MAX_REQUESTS
    {
        return Err(fail());
    }
    let groups: HashMap<_, _> = document.groups.iter().map(|g| (g.id, g)).collect();
    let requests: HashSet<_> = document.requests.iter().map(|r| r.id).collect();
    if groups.len() != document.groups.len() || requests.len() != document.requests.len() {
        return Err(fail());
    }
    let root = groups.get(&document.root_id).ok_or_else(fail)?;
    if root.parent_id.is_some() {
        return Err(fail());
    }
    for group in &document.groups {
        let mut current = group;
        let mut visited = HashSet::new();
        while current.id != root.id {
            if !visited.insert(current.id) || visited.len() >= MAX_DEPTH {
                return Err(fail());
            }
            current = groups
                .get(&current.parent_id.ok_or_else(fail)?)
                .ok_or_else(fail)?;
        }
        let mut token_ids = HashSet::new();
        for token in group.response_tokens.iter().flatten() {
            if token.id == 0
                || token.id >= 1_000_000_000
                || !token_ids.insert(token.id)
                || token.name.trim().is_empty()
                || !requests.contains(&token.request_id)
                || matches!(
                    token.source,
                    crate::model::CheckSource::Time | crate::model::CheckSource::Size
                )
            {
                return Err(fail());
            }
        }
    }
    if document
        .requests
        .iter()
        .any(|r| !groups.contains_key(&r.group_id))
    {
        return Err(fail());
    }
    // Reuse the established limits and draft/group validators without loading
    // a workspace, allocating IDs, sending requests, or opening upload paths.
    let mut tabs: Vec<_> = document
        .requests
        .iter()
        .map(|request| {
            json!({
                "id": request.id, "groupId": request.group_id, "draft": request.draft,
                "response": null, "error": "", "sentFingerprint": "", "interrupted": false,
                "view": RequestView::default(),
            })
        })
        .collect();
    if tabs.is_empty() {
        tabs.push(json!({
            "id": 1, "groupId": root.id, "draft": {"method":"GET", "url":"", "query":[], "headers":[],
                "bodyMode":"none", "body":"", "auth":"none", "token":"", "username":"", "password":""},
            "response": null, "error": "", "sentFingerprint": "", "interrupted": false,
            "view": RequestView::default(),
        }));
    }
    let mut shared_groups = document.groups.clone();
    for group in &mut shared_groups {
        group.collapsed = false;
        group.active_environment_id = None;
    }
    let snapshot = json!({
        "version": 4, "activeId": null, "openIds": [], "tabs": tabs,
        "groups": shared_groups, "globalDefinitions": {},
        "preferences": crate::preferences::default_preferences(),
    });
    crate::workspace::validate_workspace(&snapshot.to_string())
}
