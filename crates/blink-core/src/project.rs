//! Version 1 projects use `blink.json`, nested `group.json` files, and one JSON
//! file per request. The manifest inventories managed paths; other files are
//! never adopted or removed. IDs, rather than paths, identify definitions.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Draft, RequestGroup};

mod disk;
mod validation;

const VERSION: u32 = 1;
const MANIFEST: &str = "blink.json";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILES: usize = 20_001;
const MAX_DEPTH: usize = 64;
type Files = BTreeMap<PathBuf, Vec<u8>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDocument {
    pub root_id: u64,
    pub groups: Vec<RequestGroup>,
    pub requests: Vec<ProjectRequest>,
}

impl ProjectDocument {
    /// Validate shared definitions without loading files or executing requests.
    pub fn validate(&self) -> Result<(), String> {
        validation::validate(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRequest {
    pub id: u64,
    pub group_id: u64,
    pub draft: Draft,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    version: u32,
    root: Value,
    #[serde(default)]
    root_index: usize,
    groups: Vec<String>,
    requests: Vec<String>,
}

/// A project directory and the exact bytes last read or successfully written.
#[derive(Debug)]
pub struct ProjectDisk {
    path: PathBuf,
    baseline: Files,
}

impl ProjectDisk {
    /// Create in an absent or empty directory. Existing content is never adopted.
    pub fn create(path: &Path, document: &ProjectDocument) -> Result<Self, String> {
        let files = encode(document)?;
        let path = disk::create_root(path)?;
        let mut project = Self {
            path,
            baseline: Files::new(),
        };
        project.commit(files)?;
        Ok(project)
    }

    pub fn open(path: &Path) -> Result<(Self, ProjectDocument), String> {
        let path = disk::existing_root(path)?;
        disk::ensure_no_interrupted(&path)?;
        let mut baseline = Files::new();
        let mut budget = MAX_BYTES;
        let bytes = disk::read(&path, Path::new(MANIFEST), &mut budget)?;
        let manifest: Manifest = serde_json::from_slice(&bytes).map_err(invalid_json)?;
        if manifest.version != VERSION {
            return Err("Unsupported project version.".into());
        }
        if manifest.groups.len() + manifest.requests.len() + 1 > MAX_FILES {
            return Err("Project contains too many files.".into());
        }
        baseline.insert(MANIFEST.into(), bytes);
        let root = decode_group(manifest.root)?;
        let mut document = ProjectDocument {
            root_id: root.id,
            groups: vec![root],
            requests: vec![],
        };
        for name in &manifest.groups {
            let relative = disk::relative(name)?;
            if relative.file_name().and_then(|v| v.to_str()) != Some("group.json") {
                return Err("Group file must be named group.json.".into());
            }
            if baseline.contains_key(&relative) {
                return Err("Duplicate managed project path.".into());
            }
            let bytes = disk::read(&path, &relative, &mut budget)?;
            let value = serde_json::from_slice(&bytes).map_err(invalid_json)?;
            document.groups.push(decode_group(value)?);
            baseline.insert(relative, bytes);
        }
        for name in &manifest.requests {
            let relative = disk::relative(name)?;
            if relative.extension().and_then(|v| v.to_str()) != Some("json")
                || matches!(
                    relative.file_name().and_then(|v| v.to_str()),
                    Some("group.json" | MANIFEST)
                )
                || baseline.contains_key(&relative)
            {
                return Err("Invalid or duplicate request file path.".into());
            }
            let bytes = disk::read(&path, &relative, &mut budget)?;
            document
                .requests
                .push(serde_json::from_slice(&bytes).map_err(invalid_json)?);
            baseline.insert(relative, bytes);
        }
        if manifest.root_index >= document.groups.len() {
            return Err("Invalid root group position.".into());
        }
        let root = document.groups.remove(0);
        document.groups.insert(manifest.root_index, root);
        validation::validate(&document)?;
        Ok((Self { path, baseline }, document))
    }

    pub fn save(&mut self, document: &ProjectDocument) -> Result<(), String> {
        let files = encode(document)?;
        if self.changed()? {
            return Err("Project changed on disk. Reload before saving.".into());
        }
        self.commit(files)
    }

    /// Missing files and unsafe paths are errors, never a request to recreate.
    pub fn changed(&self) -> Result<bool, String> {
        disk::existing_root(&self.path)?;
        disk::ensure_no_interrupted(&self.path)?;
        let mut budget = MAX_BYTES;
        for (relative, baseline) in &self.baseline {
            if disk::read(&self.path, relative, &mut budget)? != *baseline {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn commit(&mut self, files: Files) -> Result<(), String> {
        disk::commit(&self.path, &self.baseline, &files)?;
        self.baseline = files;
        Ok(())
    }
}

fn invalid_json(error: impl std::fmt::Display) -> String {
    format!("Invalid project JSON: {error}")
}

fn group_value(group: &RequestGroup) -> Result<Value, String> {
    let mut value = serde_json::to_value(group).map_err(invalid_json)?;
    let fields = value.as_object_mut().ok_or("Invalid project group.")?;
    fields.remove("collapsed");
    fields.remove("activeEnvironmentId");
    Ok(value)
}

fn decode_group(mut value: Value) -> Result<RequestGroup, String> {
    let fields = value.as_object_mut().ok_or("Invalid project group.")?;
    fields.insert("collapsed".into(), false.into());
    fields.remove("activeEnvironmentId");
    serde_json::from_value(value).map_err(invalid_json)
}

fn json(value: &impl Serialize) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(invalid_json)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn slug(name: &str) -> String {
    let mut result = String::new();
    for c in name.chars().take(60) {
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
        } else if !result.ends_with('-') && !result.is_empty() {
            result.push('-');
        }
    }
    let result = result.trim_matches('-');
    if result.is_empty() {
        "item".into()
    } else {
        result.into()
    }
}

fn encode(document: &ProjectDocument) -> Result<Files, String> {
    validation::validate(document)?;
    let by_id: HashMap<_, _> = document.groups.iter().map(|g| (g.id, g)).collect();
    let root = by_id[&document.root_id];
    let mut folders = HashMap::new();
    folders.insert(root.id, PathBuf::new());
    for group in &document.groups {
        let mut ancestors = vec![];
        let mut current = group;
        while current.id != root.id {
            ancestors.push(format!("{}-{}", slug(&current.name), current.id));
            current = by_id[&current.parent_id.ok_or("Project group has no parent.")?];
        }
        let folder: PathBuf = ancestors.into_iter().rev().collect();
        folders.insert(group.id, folder);
    }
    let mut files = Files::new();
    let mut manifest = Manifest {
        version: VERSION,
        root: group_value(root)?,
        root_index: document
            .groups
            .iter()
            .position(|group| group.id == root.id)
            .unwrap_or(0),
        groups: vec![],
        requests: vec![],
    };
    for group in &document.groups {
        if group.id == root.id {
            continue;
        }
        let path = folders[&group.id].join("group.json");
        manifest
            .groups
            .push(path.to_string_lossy().replace('\\', "/"));
        files.insert(path, json(&group_value(group)?)?);
    }
    for request in &document.requests {
        // URL-independent names avoid exposing credentials in paths and keep
        // filenames stable when the endpoint changes.
        let path = folders[&request.group_id].join(format!(
            "{}-request-{}.json",
            slug(&request.draft.method),
            request.id
        ));
        manifest
            .requests
            .push(path.to_string_lossy().replace('\\', "/"));
        files.insert(path, json(request)?);
    }
    files.insert(MANIFEST.into(), json(&manifest)?);
    if files.values().map(Vec::len).sum::<usize>() > MAX_BYTES {
        return Err("Project exceeds the 64 MiB limit.".into());
    }
    Ok(files)
}

#[cfg(test)]
mod tests;
