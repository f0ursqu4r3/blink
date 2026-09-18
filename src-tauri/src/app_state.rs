use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{AppHandle, Emitter, Manager};

const LIMIT: u64 = 64 * 1024 * 1024;
const FILE_NAME: &str = "workspace-v1.json";

pub struct AppState {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
    pub ready: AtomicBool,
    pub allow_exit: AtomicBool,
}
impl AppState {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            path: directory.join(FILE_NAME),
            lock: Arc::new(Mutex::new(())),
            ready: AtomicBool::new(false),
            allow_exit: AtomicBool::new(false),
        }
    }
}
fn load(path: &Path) -> Result<Option<String>, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(
                "Cannot read the saved workspace. Check application data permissions.".into(),
            )
        }
    };
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read the saved workspace.")?;
    if bytes.len() as u64 > LIMIT {
        return Err("Saved workspace exceeds the 64 MiB limit.".into());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "Saved workspace is not valid UTF-8. The file has not been changed.".into())
}
fn save(path: &Path, content: &str) -> Result<(), String> {
    if content.len() as u64 > LIMIT {
        return Err("Workspace exceeds the 64 MiB save limit.".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(content).map_err(|_| "Workspace data is not valid JSON.")?;
    if value.get("version").and_then(|v| v.as_u64()) != Some(1)
        || !value.get("tabs").is_some_and(|v| v.is_array())
    {
        return Err("Unsupported workspace format.".into());
    }
    let directory = path.parent().ok_or("Invalid workspace path.")?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(directory)
        .map_err(|_| "Cannot create the application data directory.")?;
    // NamedTempFile is owner-only on Unix. Persist replaces the target atomically.
    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|_| "Cannot create the workspace save file.")?;
    temporary
        .write_all(content.as_bytes())
        .map_err(|_| "Cannot write workspace data. Check disk space.")?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| "Cannot sync workspace data to disk.")?;
    temporary
        .persist(path)
        .map_err(|_| "Cannot replace the workspace file. The previous save was preserved.")?;
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|dir| dir.sync_all())
        .map_err(|_| "Workspace was written, but the directory could not be synced.")?;
    Ok(())
}

#[tauri::command]
pub async fn load_app_state(app: AppHandle) -> Result<Option<String>, String> {
    let state = app.state::<AppState>();
    let path = state.path.clone();
    let lock = state.lock.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock.lock().map_err(|_| "Workspace storage lock failed.")?;
        load(&path)
    })
    .await
    .map_err(|_| "Workspace load task failed.".to_string())?
}
#[tauri::command]
pub async fn save_app_state(app: AppHandle, content: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let path = state.path.clone();
    let lock = state.lock.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock.lock().map_err(|_| "Workspace storage lock failed.")?;
        save(&path, &content)
    })
    .await
    .map_err(|_| "Workspace save task failed.".to_string())?
}
#[tauri::command]
pub fn app_state_ready(app: AppHandle, ready: bool) {
    app.state::<AppState>().ready.store(ready, Ordering::SeqCst);
}
#[tauri::command]
pub fn finish_app_exit(app: AppHandle) {
    app.state::<AppState>()
        .allow_exit
        .store(true, Ordering::SeqCst);
    app.exit(0);
}
pub fn request_exit(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    if state.ready.load(Ordering::SeqCst) && !state.allow_exit.load(Ordering::SeqCst) {
        return app.emit("workspace:before-exit", ()).is_ok();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIRST: &str = r#"{"version":1,"activeId":1,"tabs":[{"name":"first"}]}"#;
    const SECOND: &str = r#"{"version":1,"activeId":2,"tabs":[{"name":"second"}]}"#;
    #[test]
    fn round_trip_and_atomic_replace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        assert_eq!(load(&path).unwrap(), None);
        save(&path, FIRST).unwrap();
        assert_eq!(load(&path).unwrap().as_deref(), Some(FIRST));
        save(&path, SECOND).unwrap();
        assert_eq!(load(&path).unwrap().as_deref(), Some(SECOND));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn failed_save_preserves_previous_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        save(&path, FIRST).unwrap();
        assert!(save(&path, "invalid").is_err());
        assert!(save(&path, r#"{"version":2,"tabs":[]}"#).is_err());
        assert_eq!(load(&path).unwrap().as_deref(), Some(FIRST));
    }
    #[test]
    fn corrupt_reads_never_reset_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(path).unwrap(), [0xff, 0xfe]);
    }
    #[test]
    fn detects_unwritable_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        fs::create_dir(&path).unwrap();
        assert!(save(&path, FIRST).is_err());
        assert!(path.is_dir());
    }
    #[test]
    fn snapshot_survives_process_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let exe = std::env::current_exe().unwrap();
        for mode in ["write", "read"] {
            let status = std::process::Command::new(&exe)
                .args(["--exact", "app_state::tests::snapshot_child", "--nocapture"])
                .env("BLINK_TEST_STATE_PATH", &path)
                .env("BLINK_TEST_STATE_MODE", mode)
                .status()
                .unwrap();
            assert!(status.success());
        }
    }
    #[test]
    fn snapshot_child() {
        let Ok(path) = std::env::var("BLINK_TEST_STATE_PATH") else {
            return;
        };
        if std::env::var("BLINK_TEST_STATE_MODE").unwrap() == "write" {
            save(Path::new(&path), FIRST).unwrap();
        } else {
            assert_eq!(load(Path::new(&path)).unwrap().as_deref(), Some(FIRST));
        }
    }
}
