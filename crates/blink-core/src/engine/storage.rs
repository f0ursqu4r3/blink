//! The workspace snapshot: one JSON file in the app data dir, replaced
//! atomically on each save.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

const LIMIT: u64 = 64 * 1024 * 1024;
pub const FILE_NAME: &str = "workspace-v1.json";

/// The snapshot path and a lock so saves and loads never overlap. Each save
/// takes a ticket when it is requested; a save whose ticket is older than the
/// last written one is skipped, so a slow background save can never replace
/// a newer snapshot (such as the one written on quit).
#[derive(Clone)]
pub struct Storage {
    path: PathBuf,
    /// The ticket of the last written snapshot.
    lock: Arc<Mutex<u64>>,
    tickets: Arc<AtomicU64>,
}

impl Storage {
    pub fn new(directory: &Path) -> Self {
        Self {
            path: directory.join(FILE_NAME),
            lock: Arc::new(Mutex::new(0)),
            tickets: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Reserve the order of a save. Take it when the save is requested, not
    /// when it runs.
    pub fn ticket(&self) -> u64 {
        self.tickets.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Blocking: run on a blocking thread.
    pub fn load(&self) -> Result<Option<String>, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Workspace storage lock failed.")?;
        load(&self.path)
    }

    /// Blocking: save now, after every save requested before it.
    pub fn save(&self, content: &str) -> Result<(), String> {
        self.save_ticketed(self.ticket(), content)
    }

    /// Blocking: run on a blocking thread. Skips the write when a newer
    /// snapshot is already on disk.
    pub fn save_ticketed(&self, ticket: u64, content: &str) -> Result<(), String> {
        let mut written = self
            .lock
            .lock()
            .map_err(|_| "Workspace storage lock failed.")?;
        if ticket < *written {
            return Ok(());
        }
        save(&self.path, content)?;
        *written = ticket;
        Ok(())
    }
}

fn load(path: &Path) -> Result<Option<String>, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(
                "Cannot read the saved workspace. Check application data permissions.".into(),
            );
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
    let version = value.get("version").and_then(|v| v.as_u64());
    let supports_groups = matches!(version, Some(2) | Some(3) | Some(4))
        && value.get("groups").is_some_and(|v| v.is_array());
    let supports_global_definitions = matches!(version, Some(3) | Some(4))
        && value
            .get("globalDefinitions")
            .is_some_and(|v| v.is_object());
    let supports_open_ids = version == Some(4)
        && value.get("openIds").is_some_and(|v| v.is_array())
        && value
            .get("activeId")
            .is_some_and(|v| v.is_null() || v.is_u64());
    // Any HTTP token, so custom methods such as PURGE are valid.
    let valid_method = |value: &serde_json::Value| {
        value.as_str().is_some_and(|method| {
            !method.is_empty()
                && method.len() <= 64
                && method
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
        })
    };
    let group_defaults_valid =
        value
            .get("groups")
            .and_then(|v| v.as_array())
            .is_none_or(|groups| {
                groups.iter().all(|group| {
                    group.get("defaultMethod").is_none_or(valid_method)
                        && group.get("defaultUrl").is_none_or(|url| {
                            url.as_str()
                                .is_some_and(|url| url.encode_utf16().count() <= 65536)
                        })
                })
            });
    let preferences_valid = value.get("preferences").is_none_or(|p| {
        p.get("defaultMethod").is_some_and(valid_method)
            && p.get("defaultBodyMode")
                .and_then(|v| v.as_str())
                .is_some_and(|mode| {
                    matches!(
                        mode,
                        "none" | "json" | "text" | "graphql" | "form" | "multipart" | "file"
                    )
                })
            && p.get("pretty").is_some_and(|v| v.is_boolean())
            && p.get("wrap").is_some_and(|v| v.is_boolean())
            && p.get("confirmCloseDrafts").is_some_and(|v| v.is_boolean())
    });
    if !matches!(version, Some(1) | Some(2) | Some(3) | Some(4))
        || !value.get("tabs").is_some_and(|v| v.is_array())
        || (matches!(version, Some(2) | Some(3) | Some(4)) && !supports_groups)
        || (matches!(version, Some(3) | Some(4)) && !supports_global_definitions)
        || (version == Some(4) && !supports_open_ids)
        || !preferences_valid
        || !group_defaults_valid
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

#[cfg(test)]
mod tests {
    use super::*;
    const FIRST: &str = r#"{"version":1,"activeId":1,"tabs":[{"name":"first"}]}"#;
    const SECOND: &str = r#"{"version":1,"activeId":2,"tabs":[{"name":"second"}]}"#;
    const GROUPED: &str =
        "{\"version\":2,\"activeId\":1,\"groups\":[],\"tabs\":[{\"name\":\"grouped\"}]}";
    const TOKENIZED: &str = "{\"version\":3,\"activeId\":1,\"groups\":[],\"globalDefinitions\":{},\"tabs\":[{\"name\":\"tokenized\"}]}";
    const OPEN_TABS: &str = "{\"version\":4,\"activeId\":null,\"openIds\":[],\"groups\":[],\"globalDefinitions\":{},\"tabs\":[{\"name\":\"closed\"}]}";
    #[test]
    fn preference_round_trip_and_invalid_values_preserve_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let mut value: serde_json::Value = serde_json::from_str(TOKENIZED).unwrap();
        value["preferences"] = serde_json::json!({
            "defaultMethod": "PATCH", "defaultBodyMode": "json",
            "pretty": false, "wrap": true, "confirmCloseDrafts": false
        });
        let valid = value.to_string();
        save(&path, &valid).unwrap();
        assert_eq!(load(&path).unwrap().as_deref(), Some(valid.as_str()));
        for (field, invalid) in [
            ("defaultMethod", serde_json::json!("BAD METHOD")),
            ("defaultBodyMode", serde_json::json!("xml")),
            ("pretty", serde_json::json!("false")),
            ("wrap", serde_json::json!(1)),
            ("confirmCloseDrafts", serde_json::Value::Null),
        ] {
            let mut malformed = value.clone();
            malformed["preferences"][field] = invalid;
            assert!(save(&path, &malformed.to_string()).is_err(), "{field}");
            assert_eq!(load(&path).unwrap().as_deref(), Some(valid.as_str()));
        }
    }

    #[test]
    fn group_defaults_round_trip_and_invalid_values_preserve_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let mut value: serde_json::Value = serde_json::from_str(TOKENIZED).unwrap();
        value["groups"] = serde_json::json!([{
            "id": 1, "name": "API", "parentId": null, "collapsed": false,
            "defaultMethod": "POST", "defaultUrl": "{{host}}/v1"
        }]);
        let valid = value.to_string();
        save(&path, &valid).unwrap();
        assert_eq!(load(&path).unwrap().as_deref(), Some(valid.as_str()));
        for (field, invalid) in [
            ("defaultMethod", serde_json::json!("BAD METHOD")),
            ("defaultMethod", serde_json::Value::Null),
            ("defaultUrl", serde_json::json!(false)),
            ("defaultUrl", serde_json::json!("x".repeat(65537))),
        ] {
            let mut malformed = value.clone();
            malformed["groups"][0][field] = invalid;
            assert!(save(&path, &malformed.to_string()).is_err(), "{field}");
            assert_eq!(load(&path).unwrap().as_deref(), Some(valid.as_str()));
        }
    }

    #[test]
    fn round_trip_and_atomic_replace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        assert_eq!(load(&path).unwrap(), None);
        save(&path, FIRST).unwrap();
        assert_eq!(load(&path).unwrap().as_deref(), Some(FIRST));
        save(&path, SECOND).unwrap();
        assert_eq!(load(&path).unwrap().as_deref(), Some(SECOND));
        save(&path, GROUPED).unwrap();
        assert_eq!(load(&path).unwrap().as_deref(), Some(GROUPED));
        save(&path, TOKENIZED).unwrap();
        assert_eq!(load(&path).unwrap().as_deref(), Some(TOKENIZED));
        save(&path, OPEN_TABS).unwrap();
        assert_eq!(load(&path).unwrap().as_deref(), Some(OPEN_TABS));
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
        assert!(save(&path, "{\"version\":3,\"tabs\":[]}").is_err());
        assert!(
            save(
                &path,
                "{\"version\":4,\"activeId\":1,\"groups\":[],\"globalDefinitions\":{},\"tabs\":[]}"
            )
            .is_err()
        );
        assert_eq!(load(&path).unwrap().as_deref(), Some(FIRST));
    }
    #[test]
    fn an_older_ticket_never_replaces_a_newer_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path());
        let older = storage.ticket();
        storage.save(SECOND).unwrap();
        storage.save_ticketed(older, FIRST).unwrap();
        assert_eq!(storage.load().unwrap().as_deref(), Some(SECOND));
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
                .args([
                    "--exact",
                    "engine::storage::tests::snapshot_child",
                    "--nocapture",
                ])
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
