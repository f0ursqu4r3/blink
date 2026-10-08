//! Response comparisons and a private, durable baseline outside request history.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::HistoryEntry;

#[derive(Debug, Clone, PartialEq)]
pub struct FieldChange {
    /// RFC 6901 JSON pointer. The empty string is the document root.
    pub path: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// Compare JSON by field. `None` requests the existing text diff for either
/// malformed JSON or responses whose bodies were not retained.
pub fn compare_json(
    before: &HistoryEntry,
    after: &HistoryEntry,
    ignored_paths: &[String],
) -> Option<Vec<FieldChange>> {
    if before.error.is_some()
        || after.error.is_some()
        || before.body_omitted == Some(true)
        || after.body_omitted == Some(true)
    {
        return None;
    }
    let before: Value = serde_json::from_str(&before.body).ok()?;
    let after: Value = serde_json::from_str(&after.body).ok()?;
    let mut changes = Vec::new();
    compare_values(Some(&before), Some(&after), "", ignored_paths, &mut changes);
    Some(changes)
}

/// Validate pointer escapes so a mistyped path never silently disables a rule.
pub fn validate_pointer(path: &str) -> Result<(), String> {
    if !path.is_empty() && !path.starts_with('/') {
        return Err(format!("Use a JSON pointer that starts with /: {path}"));
    }
    let mut chars = path.chars();
    while let Some(ch) = chars.next() {
        if ch == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return Err(format!(
                "Use ~0 for ~ and ~1 for / in JSON pointers: {path}"
            ));
        }
    }
    Ok(())
}

fn child_path(parent: &str, key: &str) -> String {
    format!("{parent}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn compare_values(
    before: Option<&Value>,
    after: Option<&Value>,
    path: &str,
    ignored: &[String],
    changes: &mut Vec<FieldChange>,
) {
    if ignored.iter().any(|ignored| ignored == path) || before == after {
        return;
    }
    match (before, after) {
        (Some(Value::Object(a)), Some(Value::Object(b))) => {
            let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
            for key in keys {
                compare_values(
                    a.get(key),
                    b.get(key),
                    &child_path(path, key),
                    ignored,
                    changes,
                );
            }
        }
        (Some(Value::Array(a)), Some(Value::Array(b))) => {
            for index in 0..a.len().max(b.len()) {
                compare_values(
                    a.get(index),
                    b.get(index),
                    &child_path(path, &index.to_string()),
                    ignored,
                    changes,
                );
            }
        }
        (Some(Value::Object(object)), None) | (None, Some(Value::Object(object)))
            if !object.is_empty() =>
        {
            for (key, value) in object {
                compare_values(
                    before.map(|_| value),
                    after.map(|_| value),
                    &child_path(path, key),
                    ignored,
                    changes,
                );
            }
        }
        (Some(Value::Array(array)), None) | (None, Some(Value::Array(array)))
            if !array.is_empty() =>
        {
            for (index, value) in array.iter().enumerate() {
                compare_values(
                    before.map(|_| value),
                    after.map(|_| value),
                    &child_path(path, &index.to_string()),
                    ignored,
                    changes,
                );
            }
        }
        _ => changes.push(FieldChange {
            path: path.into(),
            before: before.cloned(),
            after: after.cloned(),
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinnedResponse {
    pub label: String,
    /// A copy independent of the history limit and project attachment ids.
    pub entry: HistoryEntry,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonState {
    #[serde(default)]
    pub baseline: Option<PinnedResponse>,
    #[serde(default)]
    pub ignored_paths: Vec<String>,
}

impl ComparisonState {
    pub fn load(data_dir: &Path) -> Result<Self, String> {
        let path = data_dir.join("response-comparison.json");
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("Cannot read response comparison settings: {error}")),
        };
        let state: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Cannot read response comparison settings: {error}"))?;
        for path in &state.ignored_paths {
            validate_pointer(path)?;
        }
        Ok(state)
    }

    /// Atomic replacement. The file is app-local and never enters a project.
    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        for path in &self.ignored_paths {
            validate_pointer(path)?;
        }
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            std::fs::create_dir_all(data_dir)?;
            let mut file = tempfile::NamedTempFile::new_in(data_dir)?;
            serde_json::to_writer_pretty(&mut file, self)?;
            file.flush()?;
            file.as_file().sync_all()?;
            file.persist(data_dir.join("response-comparison.json"))?;
            Ok(())
        };
        write().map_err(|error| format!("Cannot save response comparison settings: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{HistoryOutcome, history_entry};
    use crate::model::{ApiResponse, RequestInput};

    fn entry(body: &str) -> HistoryEntry {
        history_entry(
            1,
            0.,
            &RequestInput {
                method: "GET".into(),
                url: "https://staging.test".into(),
                headers: vec![],
                body: None,
                body_file: None,
                multipart: None,
            },
            HistoryOutcome::Response(&ApiResponse {
                status: 200,
                status_text: "OK".into(),
                duration_ms: 1.,
                headers: vec![],
                body: body.into(),
                size_bytes: body.len() as u64,
                body_id: None,
                truncated: None,
                binary: None,
                final_url: None,
                redirect_count: None,
                timing: None,
            }),
        )
    }

    #[test]
    fn json_key_order_does_not_change_fields() {
        assert_eq!(
            compare_json(
                &entry(r#"{"a":1,"b":{"x":2,"y":3}}"#),
                &entry(r#"{"b":{"y":3,"x":2},"a":1}"#),
                &[]
            ),
            Some(vec![])
        );
    }

    #[test]
    fn ignores_subtrees_and_preserves_array_indices() {
        let changes = compare_json(
            &entry(r#"{"time":{"at":1},"items":[1,2,3]}"#),
            &entry(r#"{"time":{"at":9,"new":true},"items":[9,4]}"#),
            &["/time".into(), "/items/0".into()],
        )
        .unwrap();
        assert_eq!(
            changes.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(),
            ["/items/1", "/items/2"]
        );
        assert_eq!(changes[1].after, None);
    }

    #[test]
    fn ignores_fields_in_added_subtrees_and_escapes_keys() {
        let changes = compare_json(
            &entry("{}"),
            &entry(r#"{"meta":{"time":1},"a/b~c":2}"#),
            &["/meta/time".into()],
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "/a~1b~0c");
    }

    #[test]
    fn reports_types_null_and_empty_containers() {
        let changes = compare_json(
            &entry(r#"{"a":null,"b":1}"#),
            &entry(r#"{"b":"1","c":{}}"#),
            &[],
        )
        .unwrap();
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].before, Some(Value::Null));
        assert_eq!(changes[0].after, None);
        assert_eq!(
            compare_json(&entry("1"), &entry("2"), &[String::new()]),
            Some(vec![])
        );
    }

    #[test]
    fn malformed_json_and_unavailable_bodies_use_text() {
        assert!(compare_json(&entry("{bad"), &entry("{}"), &[]).is_none());
        assert!(compare_json(&entry("{}"), &entry("plain text"), &[]).is_none());
        let mut missing = entry("{}");
        missing.body_omitted = Some(true);
        assert!(compare_json(&missing, &entry("{}"), &[]).is_none());
    }

    #[test]
    fn validates_json_pointer_escapes() {
        for pointer in ["", "/time", "/a~1b/~0/0"] {
            assert!(validate_pointer(pointer).is_ok());
        }
        for pointer in ["time", "/a~2", "/a~"] {
            assert!(validate_pointer(pointer).is_err());
        }
    }

    #[test]
    fn baseline_survives_history_clear_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let mut history = vec![entry(r#"{"time":1}"#)];
        let state = ComparisonState {
            baseline: Some(PinnedResponse {
                label: "Staging".into(),
                entry: history[0].clone(),
            }),
            ignored_paths: vec!["/time".into()],
        };
        state.save(dir.path()).unwrap();
        history.clear();
        assert_eq!(ComparisonState::load(dir.path()).unwrap(), state);
        std::fs::write(dir.path().join("response-comparison.json"), "corrupt").unwrap();
        assert!(ComparisonState::load(dir.path()).is_err());
    }
    #[test]
    fn project_reopen_and_history_pruning_keep_the_pinned_snapshot_private() {
        use crate::workspace_state::Workspace;
        let dir = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::new();
        let root = workspace.add_group("Project", None);
        let request = workspace.create(Some(Some(root)));
        let baseline = entry(r#"{"secretResponseValue":"private-baseline"}"#);
        workspace
            .session_mut(request)
            .unwrap()
            .history
            .push(baseline.clone());
        let state = ComparisonState {
            baseline: Some(PinnedResponse {
                label: "Staging".into(),
                entry: baseline.clone(),
            }),
            ignored_paths: vec![],
        };
        state.save(dir.path()).unwrap();
        let document = workspace
            .convert_group_to_project(root, "/comparison-project".into())
            .unwrap();
        assert!(
            !serde_json::to_string(&document)
                .unwrap()
                .contains("private-baseline")
        );
        for id in 2..=30 {
            let mut next = entry("{}");
            next.id = id;
            let session = workspace.session_mut(request).unwrap();
            session.history = crate::history::add_history(&session.history, next);
        }
        assert!(
            !workspace
                .session(request)
                .unwrap()
                .history
                .iter()
                .any(|item| item.id == 1)
        );
        workspace.close_project(root).unwrap();
        workspace
            .attach_project("/comparison-project".into(), document)
            .unwrap();
        assert_eq!(
            ComparisonState::load(dir.path())
                .unwrap()
                .baseline
                .unwrap()
                .entry,
            baseline
        );
    }
}
