//! Where the engine keeps its files: the app data dir for saved state and the
//! app cache dir for response bodies.

use std::fs;
use std::path::{Path, PathBuf};

/// The app identifier. The Tauri app used `com.kyle.blink`, so both apps can
/// run side by side with separate data.
pub const IDENTIFIER: &str = "com.kyle.blink.gpui";
const TAURI_IDENTIFIER: &str = "com.kyle.blink";
/// Replaces the data dir. The cache dir is its `cache` folder.
pub const DATA_DIR_ENV: &str = "BLINK_DATA_DIR";

/// Files copied from the Tauri app on first start. The workspace goes last:
/// its presence marks the copy as done.
const IMPORTED: [&str; 3] = ["file-grants.json", "cookies.json", "workspace-v1.json"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl Paths {
    /// The platform dirs, or `BLINK_DATA_DIR` when set. Copies the Tauri
    /// app's data once when this app has no workspace yet.
    pub fn resolve() -> Result<Self, String> {
        if let Some(base) = std::env::var_os(DATA_DIR_ENV).filter(|value| !value.is_empty()) {
            return Ok(Self::with_base(PathBuf::from(base)));
        }
        let data = dirs::data_dir().ok_or("Cannot find the application data directory.")?;
        let cache = dirs::cache_dir().unwrap_or_else(|| data.clone());
        let paths = Self {
            data_dir: data.join(IDENTIFIER),
            cache_dir: cache.join(IDENTIFIER),
        };
        // A failed copy starts with an empty workspace; the Tauri files stay.
        let _ = import_tauri_data(&data.join(TAURI_IDENTIFIER), &paths.data_dir);
        Ok(paths)
    }

    /// Everything under `base`, for tests and `BLINK_DATA_DIR`.
    pub fn with_base(base: PathBuf) -> Self {
        Self {
            cache_dir: base.join("cache"),
            data_dir: base,
        }
    }

    pub fn workspace(&self) -> PathBuf {
        self.data_dir.join(super::storage::FILE_NAME)
    }

    pub fn file_grants(&self) -> PathBuf {
        self.data_dir.join("file-grants.json")
    }

    pub fn cookies(&self) -> PathBuf {
        self.data_dir.join("cookies.json")
    }

    pub fn window_state(&self) -> PathBuf {
        self.data_dir.join("window-state.json")
    }

    pub fn responses(&self) -> PathBuf {
        self.cache_dir.join("responses")
    }
}

/// Copy the Tauri app's workspace, file grants, and cookies into `to` when
/// `to` has no workspace and `from` has one. Reads `from` only. Returns
/// whether a copy happened.
pub fn import_tauri_data(from: &Path, to: &Path) -> std::io::Result<bool> {
    let workspace = super::storage::FILE_NAME;
    if to.join(workspace).exists() || !from.join(workspace).is_file() {
        return Ok(false);
    }
    fs::create_dir_all(to)?;
    for name in IMPORTED {
        let source = from.join(name);
        let target = to.join(name);
        // Keep files this app already wrote, except the missing workspace.
        if source.is_file() && (name == workspace || !target.exists()) {
            fs::copy(&source, &target)?;
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_tauri_data_once_and_never_changes_it() {
        let root = tempfile::tempdir().unwrap();
        let tauri = root.path().join("tauri");
        let gpui = root.path().join("gpui");
        fs::create_dir_all(&tauri).unwrap();
        fs::write(tauri.join("workspace-v1.json"), "workspace").unwrap();
        fs::write(tauri.join("cookies.json"), "cookies").unwrap();
        fs::write(tauri.join("file-grants.json"), "grants").unwrap();

        assert!(import_tauri_data(&tauri, &gpui).unwrap());
        assert_eq!(
            fs::read_to_string(gpui.join("workspace-v1.json")).unwrap(),
            "workspace"
        );
        assert_eq!(
            fs::read_to_string(gpui.join("cookies.json")).unwrap(),
            "cookies"
        );
        assert_eq!(
            fs::read_to_string(gpui.join("file-grants.json")).unwrap(),
            "grants"
        );

        fs::write(gpui.join("workspace-v1.json"), "changed").unwrap();
        fs::write(tauri.join("workspace-v1.json"), "newer").unwrap();
        assert!(!import_tauri_data(&tauri, &gpui).unwrap());
        assert_eq!(
            fs::read_to_string(gpui.join("workspace-v1.json")).unwrap(),
            "changed"
        );
        assert_eq!(
            fs::read_to_string(tauri.join("cookies.json")).unwrap(),
            "cookies"
        );
    }

    #[test]
    fn skips_the_copy_without_a_tauri_workspace() {
        let root = tempfile::tempdir().unwrap();
        let tauri = root.path().join("tauri");
        fs::create_dir_all(&tauri).unwrap();
        fs::write(tauri.join("cookies.json"), "cookies").unwrap();
        let gpui = root.path().join("gpui");
        assert!(!import_tauri_data(&tauri, &gpui).unwrap());
        assert!(!gpui.exists());
    }

    #[test]
    fn base_dir_holds_data_and_cache() {
        let paths = Paths::with_base(PathBuf::from("/tmp/blink"));
        assert_eq!(
            paths.workspace(),
            PathBuf::from("/tmp/blink/workspace-v1.json")
        );
        assert_eq!(
            paths.responses(),
            PathBuf::from("/tmp/blink/cache/responses")
        );
    }
}
