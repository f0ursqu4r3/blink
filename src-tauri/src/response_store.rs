use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tempfile::NamedTempFile;

pub const STORE_ERROR: &str = "Cannot store the response. Check free disk space.";

/// Raw response bodies on disk, one temp file per response. Files do not
/// survive a restart: `new` clears the directory.
pub struct ResponseStore {
    dir: PathBuf,
    files: Mutex<HashMap<String, NamedTempFile>>,
}

impl ResponseStore {
    pub fn new(dir: PathBuf) -> Self {
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        Self {
            dir,
            files: Mutex::new(HashMap::new()),
        }
    }

    /// A new file in the store directory. Dropping it without `insert`
    /// deletes it.
    pub fn create(&self) -> Result<NamedTempFile, String> {
        fs::create_dir_all(&self.dir).map_err(|_| STORE_ERROR.to_string())?;
        tempfile::Builder::new()
            .prefix("body-")
            .tempfile_in(&self.dir)
            .map_err(|_| STORE_ERROR.to_string())
    }

    /// Keep a completed file. The file name is the id.
    pub fn insert(&self, file: NamedTempFile) -> String {
        let id = file
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.files.lock().unwrap().insert(id.clone(), file);
        id
    }

    pub fn release(&self, id: &str) {
        self.files.lock().unwrap().remove(id);
    }

    pub fn copy_body(&self, id: &str, dest: &Path) -> Result<(), String> {
        // Do not hold the lock while copying a large file.
        let source = self
            .files
            .lock()
            .unwrap()
            .get(id)
            .map(|file| file.path().to_path_buf())
            .ok_or_else(|| "The response body is no longer available".to_string())?;
        fs::copy(source, dest)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    #[cfg(test)]
    pub fn path(&self, id: &str) -> Option<PathBuf> {
        self.files
            .lock()
            .unwrap()
            .get(id)
            .map(|file| file.path().to_path_buf())
    }
}

#[tauri::command]
pub fn release_response(store: tauri::State<'_, ResponseStore>, body_id: String) {
    store.release(&body_id);
}

/// Rust opens the dialog, so the webview never supplies a file path.
/// `blocking_save_file` must not run on the main thread. Async commands run
/// on the async runtime, not the main thread.
fn pick_save_path(app: &tauri::AppHandle, suggested_name: &str) -> Option<PathBuf> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .set_file_name(suggested_name)
        .blocking_save_file()?
        .into_path()
        .ok()
}

#[tauri::command]
pub async fn save_response(
    app: tauri::AppHandle,
    store: tauri::State<'_, ResponseStore>,
    body_id: String,
    suggested_name: String,
) -> Result<bool, String> {
    let Some(dest) = pick_save_path(&app, &suggested_name) else {
        return Ok(false);
    };
    store.copy_body(&body_id, &dest)?;
    Ok(true)
}

#[tauri::command]
pub async fn save_response_text(
    app: tauri::AppHandle,
    text: String,
    suggested_name: String,
) -> Result<bool, String> {
    let Some(dest) = pick_save_path(&app, &suggested_name) else {
        return Ok(false);
    };
    fs::write(dest, text).map_err(|error| error.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn new_clears_files_left_by_an_earlier_run() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("responses");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("old"), b"stale").unwrap();
        let _store = ResponseStore::new(dir.clone());
        assert!(!dir.join("old").exists());
        assert!(dir.is_dir());
    }

    #[test]
    fn release_deletes_the_file_and_copy_keeps_bytes() {
        let root = tempfile::tempdir().unwrap();
        let store = ResponseStore::new(root.path().join("responses"));
        let mut file = store.create().unwrap();
        file.write_all(&[0, 159, 146, 150, 255]).unwrap();
        let id = store.insert(file);
        let path = store.path(&id).unwrap();
        let dest = root.path().join("saved.bin");
        store.copy_body(&id, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), vec![0, 159, 146, 150, 255]);
        store.release(&id);
        assert!(!path.exists());
        assert!(store.copy_body(&id, &dest).is_err());
        store.release("unknown");
    }
}
