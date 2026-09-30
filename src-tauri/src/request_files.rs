use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::Serialize;

/// Files a request body may read. Only files the user picked in Blink's own
/// open dialog are readable, so the webview cannot read arbitrary files.
/// Grants persist, so saved requests keep working after a restart.
pub struct FileGrants {
    path: PathBuf,
    files: Mutex<HashSet<PathBuf>>,
}

impl FileGrants {
    pub fn load(path: PathBuf) -> Self {
        let files = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<PathBuf>>(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            files: Mutex::new(files.into_iter().collect()),
        }
    }

    pub fn grant(&self, file: &Path) -> Result<PathBuf, String> {
        let file = file
            .canonicalize()
            .map_err(|_| "Cannot open the selected file.".to_string())?;
        let mut files = self.files.lock().unwrap();
        if files.insert(file.clone()) {
            let list: Vec<&PathBuf> = files.iter().collect();
            let content = serde_json::to_vec(&list).map_err(|error| error.to_string())?;
            if let Some(directory) = self.path.parent() {
                let _ = fs::create_dir_all(directory);
            }
            fs::write(&self.path, content)
                .map_err(|_| "Cannot save file access. Check application data permissions.")?;
        }
        Ok(file)
    }

    /// The granted file for `path`, or an error that names only the file.
    pub fn check(&self, path: &str) -> Result<PathBuf, String> {
        let name = file_name(Path::new(path));
        let file = Path::new(path)
            .canonicalize()
            .map_err(|_| format!("File not found: {name}. Choose it again."))?;
        if self.files.lock().unwrap().contains(&file) {
            Ok(file)
        } else {
            Err(format!("Choose {name} again to allow Blink to read it."))
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "the file".to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedFile {
    path: String,
    name: String,
    size_bytes: u64,
}

/// Rust opens the dialog, so a file is readable only after the user picks it.
/// `blocking_pick_file` must not run on the main thread. Async commands run
/// on the async runtime, not the main thread.
#[tauri::command]
pub async fn pick_request_file(
    app: tauri::AppHandle,
    grants: tauri::State<'_, FileGrants>,
) -> Result<Option<PickedFile>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(path) = app
        .dialog()
        .file()
        .blocking_pick_file()
        .and_then(|file| file.into_path().ok())
    else {
        return Ok(None);
    };
    let file = grants.grant(&path)?;
    let size_bytes = fs::metadata(&file).map(|meta| meta.len()).unwrap_or(0);
    Ok(Some(PickedFile {
        name: file_name(&file),
        path: file.to_string_lossy().into_owned(),
        size_bytes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_granted_files_are_readable_and_grants_persist() {
        let root = tempfile::tempdir().unwrap();
        let granted = root.path().join("granted.bin");
        let other = root.path().join("other.bin");
        fs::write(&granted, b"ok").unwrap();
        fs::write(&other, b"secret").unwrap();
        let store = root.path().join("data").join("file-grants.json");

        let grants = FileGrants::load(store.clone());
        grants.grant(&granted).unwrap();
        assert!(grants.check(granted.to_str().unwrap()).is_ok());
        let error = grants.check(other.to_str().unwrap()).unwrap_err();
        assert_eq!(error, "Choose other.bin again to allow Blink to read it.");

        let reloaded = FileGrants::load(store);
        assert!(reloaded.check(granted.to_str().unwrap()).is_ok());
    }

    #[test]
    fn a_missing_file_names_only_the_file() {
        let root = tempfile::tempdir().unwrap();
        let grants = FileGrants::load(root.path().join("grants.json"));
        let missing = root.path().join("private").join("gone.json");
        assert_eq!(
            grants.check(missing.to_str().unwrap()).unwrap_err(),
            "File not found: gone.json. Choose it again."
        );
    }
}
