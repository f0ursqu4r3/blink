use super::*;
use crate::{groups::create_group, request::create_draft};

pub(super) fn document() -> ProjectDocument {
    let mut root = create_group("Example", None);
    root.id = 1;
    let mut child = create_group("Nested group", Some(1));
    child.id = 2;
    let mut draft = create_draft();
    draft.url = "https://example.com/items".into();
    ProjectDocument {
        root_id: 1,
        groups: vec![root, child],
        requests: vec![ProjectRequest {
            id: 3,
            group_id: 2,
            draft,
        }],
    }
}

#[test]
fn roundtrip_nested_and_edit_save_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = document();
    let mut disk = ProjectDisk::create(dir.path(), &doc).unwrap();
    assert_eq!(ProjectDisk::open(dir.path()).unwrap().1, doc);
    assert!(dir.path().join("nested-group-2/group.json").is_file());
    doc.requests[0].draft.body = "edited".into();
    disk.save(&doc).unwrap();
    assert_eq!(ProjectDisk::open(dir.path()).unwrap().1, doc);
    assert!(!disk.changed().unwrap());
}

#[test]
fn conflicts_preserve_external_content_and_missing_folder_is_not_recreated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project");
    let doc = document();
    let mut disk = ProjectDisk::create(&path, &doc).unwrap();
    std::fs::write(path.join("blink.json"), "external").unwrap();
    assert!(disk.changed().unwrap());
    assert!(disk.save(&doc).is_err());
    assert_eq!(
        std::fs::read_to_string(path.join("blink.json")).unwrap(),
        "external"
    );
    std::fs::remove_dir_all(&path).unwrap();
    assert!(disk.save(&doc).is_err());
    assert!(!path.exists());
}

#[test]
fn unrelated_files_survive_and_unchanged_files_keep_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = document();
    let mut disk = ProjectDisk::create(dir.path(), &doc).unwrap();
    let manifest = dir.path().join("blink.json");
    let modified = std::fs::metadata(&manifest).unwrap().modified().unwrap();
    std::fs::write(dir.path().join("notes.txt"), "keep me").unwrap();
    disk.save(&doc).unwrap();
    assert_eq!(
        std::fs::metadata(&manifest).unwrap().modified().unwrap(),
        modified
    );
    doc.requests.clear();
    doc.groups.truncate(1);
    disk.save(&doc).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes.txt")).unwrap(),
        "keep me"
    );
}

#[test]
fn rejects_duplicate_ids_cycles_unknown_versions_and_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = document();
    doc.groups.push(doc.groups[1].clone());
    assert!(ProjectDisk::create(dir.path(), &doc).is_err());
    doc = document();
    doc.groups[1].parent_id = Some(2);
    assert!(ProjectDisk::create(dir.path(), &doc).is_err());
    ProjectDisk::create(dir.path(), &document()).unwrap();
    let manifest = dir.path().join("blink.json");
    let original = std::fs::read_to_string(&manifest).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&original).unwrap();
    value["version"] = 999.into();
    std::fs::write(&manifest, value.to_string()).unwrap();
    assert!(ProjectDisk::open(dir.path()).is_err());
    value = serde_json::from_str(&original).unwrap();
    value["groups"][0] = "../outside.json".into();
    std::fs::write(&manifest, value.to_string()).unwrap();
    assert!(ProjectDisk::open(dir.path()).is_err());
}

#[cfg(unix)]
#[test]
fn rejects_symlink_files_folders_and_root() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    ProjectDisk::create(&root, &document()).unwrap();
    symlink(&root, dir.path().join("alias")).unwrap();
    assert!(ProjectDisk::open(&dir.path().join("alias")).is_err());
    let group = root.join("nested-group-2/group.json");
    std::fs::rename(&group, dir.path().join("group.json")).unwrap();
    symlink(dir.path().join("group.json"), &group).unwrap();
    assert!(ProjectDisk::open(&root).is_err());
}

#[test]
fn private_group_view_state_is_omitted() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = document();
    doc.groups[0].collapsed = true;
    doc.groups[0].active_environment_id = Some(42);
    ProjectDisk::create(dir.path(), &doc).unwrap();
    let manifest = std::fs::read_to_string(dir.path().join("blink.json")).unwrap();
    assert!(!manifest.contains("collapsed"));
    assert!(!manifest.contains("activeEnvironmentId"));
    let loaded = ProjectDisk::open(dir.path()).unwrap().1;
    assert!(!loaded.groups[0].collapsed);
    assert_eq!(loaded.groups[0].active_environment_id, None);
}

#[test]
fn failed_new_path_collision_preserves_all_managed_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = document();
    let mut disk = ProjectDisk::create(dir.path(), &doc).unwrap();
    let before = disk.baseline.clone();
    doc.requests[0].draft.method = "POST".into();
    doc.groups[0].name = "New name".into();
    let collision = dir.path().join("nested-group-2/post-request-3.json");
    std::fs::write(&collision, "unrelated").unwrap();
    assert!(disk.save(&doc).is_err());
    for (relative, bytes) in before {
        assert_eq!(std::fs::read(dir.path().join(relative)).unwrap(), bytes);
    }
    assert_eq!(std::fs::read_to_string(collision).unwrap(), "unrelated");
}

#[test]
fn empty_project_and_manual_field_edits_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = document();
    doc.requests.clear();
    ProjectDisk::create(dir.path(), &doc).unwrap();
    let group_file = dir.path().join("nested-group-2/group.json");
    let mut group: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&group_file).unwrap()).unwrap();
    group["name"] = "Manually edited".into();
    std::fs::write(group_file, group.to_string()).unwrap();
    let (mut disk, mut loaded) = ProjectDisk::open(dir.path()).unwrap();
    assert_eq!(loaded.groups[1].name, "Manually edited");
    loaded.groups[1].default_url = Some("https://example.com".into());
    disk.save(&loaded).unwrap();
    assert_eq!(ProjectDisk::open(dir.path()).unwrap().1, loaded);
}

#[test]
fn rejects_bad_request_membership_ids_json_and_nonempty_create() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = document();
    doc.requests[0].group_id = 99;
    assert!(ProjectDisk::create(dir.path(), &doc).is_err());
    doc = document();
    doc.requests.push(doc.requests[0].clone());
    assert!(ProjectDisk::create(dir.path(), &doc).is_err());
    doc = document();
    doc.groups[0].id = 0;
    doc.root_id = 0;
    doc.groups[1].parent_id = Some(0);
    assert!(ProjectDisk::create(dir.path(), &doc).is_err());
    std::fs::write(dir.path().join("notes.txt"), "keep").unwrap();
    assert!(ProjectDisk::create(dir.path(), &document()).is_err());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes.txt")).unwrap(),
        "keep"
    );
    std::fs::write(dir.path().join("blink.json"), "{ broken").unwrap();
    assert!(ProjectDisk::open(dir.path()).is_err());
}

#[test]
fn missing_manifest_and_oversized_file_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let doc = document();
    let mut disk = ProjectDisk::create(dir.path(), &doc).unwrap();
    let manifest = dir.path().join("blink.json");
    std::fs::remove_file(&manifest).unwrap();
    assert!(disk.save(&doc).is_err());
    assert!(!manifest.exists());
    let file = std::fs::File::create(&manifest).unwrap();
    file.set_len(MAX_BYTES as u64 + 1).unwrap();
    assert!(ProjectDisk::open(dir.path()).is_err());
}

#[cfg(unix)]
#[test]
fn replaced_nested_folder_symlink_cannot_escape_on_save() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let mut doc = document();
    let mut disk = ProjectDisk::create(dir.path(), &doc).unwrap();
    let child = dir.path().join("nested-group-2");
    std::fs::rename(&child, outside.path().join("original")).unwrap();
    symlink(outside.path().join("original"), &child).unwrap();
    doc.requests[0].draft.body = "changed".into();
    assert!(disk.save(&doc).is_err());
    assert!(ProjectDisk::open(dir.path()).is_err());
    let request = outside.path().join("original/get-request-3.json");
    assert!(
        !std::fs::read_to_string(request)
            .unwrap()
            .contains("changed")
    );
}

#[test]
fn preserves_group_order_and_accepts_inventory_moves() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = document();
    doc.groups.swap(0, 1);
    ProjectDisk::create(dir.path(), &doc).unwrap();
    assert_eq!(ProjectDisk::open(dir.path()).unwrap().1, doc);
    let manifest = dir.path().join("blink.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    std::fs::rename(
        dir.path().join("nested-group-2"),
        dir.path().join("manual-folder"),
    )
    .unwrap();
    value["groups"][0] = "manual-folder/group.json".into();
    value["requests"][0] = "manual-folder/get-request-3.json".into();
    std::fs::write(manifest, value.to_string()).unwrap();
    assert_eq!(ProjectDisk::open(dir.path()).unwrap().1, doc);
}

#[test]
fn rejects_response_references_outside_project() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = document();
    doc.groups[0].response_tokens = Some(vec![crate::model::ResponseToken {
        id: 1,
        name: "access_token".into(),
        request_id: 900,
        source: crate::model::CheckSource::Body,
        path: ".token".into(),
        max_age_secs: None,
    }]);
    assert!(ProjectDisk::create(dir.path(), &doc).is_err());
    doc.groups[0].response_tokens.as_mut().unwrap()[0].request_id = 3;
    ProjectDisk::create(dir.path(), &doc).unwrap();
    assert_eq!(ProjectDisk::open(dir.path()).unwrap().1, doc);
}

#[test]
fn interrupted_transaction_is_reported_before_reading_broken_manifest_members() {
    let dir = tempfile::tempdir().unwrap();
    let doc = document();
    let mut disk = ProjectDisk::create(dir.path(), &doc).unwrap();
    let recovery = dir.path().join(".blink-save-interrupted");
    std::fs::create_dir(&recovery).unwrap();
    let group = dir.path().join("nested-group-2/group.json");
    std::fs::copy(&group, recovery.join("original-group")).unwrap();
    std::fs::write(recovery.join("transaction.json"), serde_json::json!({
        "version": 1, "files": [{"path": "nested-group-2/group.json", "backup": "original-group", "replacement": null}]
    }).to_string()).unwrap();
    std::fs::remove_file(&group).unwrap();
    std::fs::write(dir.path().join("notes.txt"), "external edit").unwrap();
    let error = ProjectDisk::open(dir.path()).unwrap_err();
    assert!(error.contains("interrupted"), "{error}");
    assert!(error.contains("transaction.json"), "{error}");
    let error = disk.save(&doc).unwrap_err();
    assert!(error.contains("interrupted"), "{error}");
    assert!(recovery.join("original-group").is_file());
    assert!(!group.exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes.txt")).unwrap(),
        "external edit"
    );
}
