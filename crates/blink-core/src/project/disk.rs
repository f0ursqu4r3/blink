use super::*;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Component;

const JOURNAL: &str = "transaction.json";

fn io(error: impl std::fmt::Display) -> String {
    format!("Project filesystem error: {error}")
}

pub(super) fn existing_root(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Project folder must be a directory, not a symlink.".into());
    }
    fs::canonicalize(path).map_err(io)
}

/// Pending journals must be resolved explicitly. Do not load a partly written
/// project or overwrite files that another process might have changed.
pub(super) fn ensure_no_interrupted(root: &Path) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(io)? {
        let entry = entry.map_err(io)?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(".blink-save-")
        {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(io)?;
        if metadata.file_type().is_symlink() {
            return Err("Unsafe project recovery folder symlink.".into());
        }
        if !metadata.is_dir() {
            continue;
        }
        let journal = entry.path().join(JOURNAL);
        match fs::symlink_metadata(&journal) {
            Ok(_) => {
                return Err(format!(
                    "Project save was interrupted. Recover the project files using {} before opening or saving. External edits have not been changed.",
                    journal.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(error)),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(io)
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub(super) fn create_root(path: &Path) -> Result<PathBuf, String> {
    let created = match fs::symlink_metadata(path) {
        Ok(_) => false,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(io)?;
            true
        }
        Err(error) => return Err(io(error)),
    };
    let path = existing_root(path)?;
    if created {
        sync_directory(path.parent().ok_or("Missing project parent folder.")?)?;
    }
    if fs::read_dir(&path).map_err(io)?.next().is_some() {
        return Err("Create Project requires an empty folder.".into());
    }
    Ok(path)
}

pub(super) fn relative(name: &str) -> Result<PathBuf, String> {
    let path = Path::new(name);
    if name.is_empty()
        || name.contains('\\')
        || name.contains(':')
        || name.contains('\0')
        || name
            .split('/')
            .next()
            .is_some_and(|part| part.starts_with(".blink-save-"))
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path.components().count() > MAX_DEPTH + 1
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Unsafe project file path.".into());
    }
    Ok(path.to_path_buf())
}

/// Check each existing component. A missing tail is allowed only for new files.
fn checked(root: &Path, relative_path: &Path, allow_missing: bool) -> Result<PathBuf, String> {
    existing_root(root)?;
    relative(&relative_path.to_string_lossy())?;
    let mut path = root.to_path_buf();
    let components: Vec<_> = relative_path.components().collect();
    for (index, component) in components.iter().enumerate() {
        path.push(component.as_os_str());
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || (index + 1 != components.len() && !metadata.is_dir())
                    || (index + 1 == components.len() && !metadata.is_file())
                {
                    return Err("Project path is a symlink or has an invalid file type.".into());
                }
            }
            Err(error) if allow_missing && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(error)),
        }
    }
    Ok(path)
}

pub(super) fn read(root: &Path, relative: &Path, budget: &mut usize) -> Result<Vec<u8>, String> {
    let path = checked(root, relative, false)?;
    let file = File::open(&path).map_err(io)?;
    if file.metadata().map_err(io)?.len() > *budget as u64 {
        return Err("Project exceeds the 64 MiB limit.".into());
    }
    let mut bytes = Vec::new();
    file.take(*budget as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > *budget {
        return Err("Project exceeds the 64 MiB limit.".into());
    }
    *budget -= bytes.len();
    Ok(bytes)
}

fn matches(root: &Path, relative: &Path, expected: Option<&Vec<u8>>) -> Result<bool, String> {
    let path = checked(root, relative, true)?;
    match expected {
        Some(bytes) => {
            let mut budget = MAX_BYTES;
            Ok(read(root, relative, &mut budget)? == *bytes)
        }
        None => match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(error) => Err(io(error)),
            Ok(_) => Ok(false),
        },
    }
}

fn ensure_parents(root: &Path, relative: &Path, created: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut current = root.to_path_buf();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            current.push(component);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => return Err("Unsafe project folder.".into()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    // Never create the project root after it has disappeared.
                    existing_root(root)?;
                    fs::create_dir(&current).map_err(io)?;
                    created.push(current.clone());
                    sync_directory(current.parent().ok_or("Missing project parent folder.")?)?;
                }
                Err(error) => return Err(io(error)),
            }
        }
    }
    Ok(())
}

fn staged(directory: &Path, bytes: &[u8]) -> Result<tempfile::NamedTempFile, String> {
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.as_file().sync_all().map_err(io)?;
    Ok(file)
}

struct Change {
    relative: PathBuf,
    replacement: Option<tempfile::NamedTempFile>,
    backup: Option<tempfile::NamedTempFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JournalEntry {
    path: String,
    backup: Option<String>,
    replacement: Option<String>,
    before_sha256: Option<String>,
    after_sha256: Option<String>,
}

fn write_journal(
    root: &Path,
    staging: &Path,
    changes: &[Change],
    baseline: &Files,
    desired: &Files,
) -> Result<(), String> {
    let name = |file: &tempfile::NamedTempFile| {
        file.path()
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    let digest = |bytes: &Vec<u8>| format!("{:x}", Sha256::digest(bytes));
    let entries: Vec<_> = changes
        .iter()
        .map(|change| JournalEntry {
            path: change.relative.to_string_lossy().into_owned(),
            backup: change.backup.as_ref().map(name),
            replacement: change.replacement.as_ref().map(name),
            before_sha256: baseline.get(&change.relative).map(digest),
            after_sha256: desired.get(&change.relative).map(digest),
        })
        .collect();
    let mut journal = File::create_new(staging.join(JOURNAL)).map_err(io)?;
    journal
        .write_all(&json(&serde_json::json!({"version": 1, "files": entries}))?)
        .map_err(io)?;
    journal.sync_all().map_err(io)?;
    // Persist backup names and the staging directory before touching managed files.
    sync_directory(staging)?;
    sync_directory(root)
}

fn clear_journal(staging: &Path) -> Result<(), String> {
    fs::remove_file(staging.join(JOURNAL)).map_err(io)?;
    sync_directory(staging)
}

fn restore_backup(
    backup: &mut Option<tempfile::NamedTempFile>,
    target: &Path,
) -> Result<(), String> {
    let file = backup.take().ok_or("Missing rollback backup.")?;
    match file.persist(target) {
        Ok(_) => Ok(()),
        Err(error) => {
            let message = io(&error.error);
            *backup = Some(error.file);
            Err(message)
        }
    }
}

/// Stage every new file and rollback copy before changing any managed path.
/// Individual replacements use atomic rename. Multi-file transactions cannot
/// be crash-atomic on ordinary folders; the manifest is committed last. A synced
/// journal preserves the originals and blocks loading after an interrupted save.
pub(super) fn commit(root: &Path, baseline: &Files, desired: &Files) -> Result<(), String> {
    commit_with(root, baseline, desired, |_| Ok(()))
}

fn commit_with(
    root: &Path,
    baseline: &Files,
    desired: &Files,
    mut before_write: impl FnMut(usize) -> Result<(), String>,
) -> Result<(), String> {
    existing_root(root)?;
    ensure_no_interrupted(root)?;
    let mut names: Vec<_> = baseline.keys().chain(desired.keys()).cloned().collect();
    names.sort();
    names.dedup();
    names.retain(|name| baseline.get(name) != desired.get(name));
    names.sort_by_key(|name| name == Path::new(MANIFEST));
    if names.is_empty() {
        return Ok(());
    }
    for name in &names {
        if !matches(root, name, baseline.get(name))? {
            return Err("Project changed on disk or a new path contains an unrelated file. Reload before saving.".into());
        }
    }
    let staging = tempfile::Builder::new()
        .prefix(".blink-save-")
        .tempdir_in(root)
        .map_err(io)?;
    let mut changes = vec![];
    for relative in names {
        changes.push(Change {
            replacement: desired
                .get(&relative)
                .map(|bytes| staged(staging.path(), bytes))
                .transpose()?,
            backup: baseline
                .get(&relative)
                .map(|bytes| staged(staging.path(), bytes))
                .transpose()?,
            relative,
        });
    }
    write_journal(root, staging.path(), &changes, baseline, desired)?;
    let mut created = vec![];
    let mut committed = 0;
    let result = (|| {
        // Check the complete baseline again after staging, including unchanged files.
        for (relative, bytes) in baseline {
            if !matches(root, relative, Some(bytes))? {
                return Err("Project changed on disk. Reload before saving.".into());
            }
        }
        for change in &mut changes {
            before_write(committed)?;
            if !matches(root, &change.relative, baseline.get(&change.relative))? {
                return Err("Project changed during save. Reload before saving.".into());
            }
            ensure_parents(root, &change.relative, &mut created)?;
            let target = checked(root, &change.relative, true)?;
            match change.replacement.take() {
                Some(file) => {
                    file.persist(&target).map_err(io)?;
                }
                None => fs::remove_file(&target).map_err(io)?,
            }
            committed += 1;
            sync_directory(target.parent().ok_or("Missing project parent folder.")?)?;
            sync_directory(staging.path())?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let mut rollback_failed = false;
        for change in changes[..committed].iter_mut().rev() {
            let restored = (|| {
                if !matches(root, &change.relative, desired.get(&change.relative))? {
                    return Err("External edit during rollback.".to_string());
                }
                let target = checked(root, &change.relative, true)?;
                if change.backup.is_some() {
                    // Keep the durable backup until the journal is cleared.
                    // A second crash during rollback must not consume it.
                    let mut copy = Some(staged(staging.path(), &baseline[&change.relative])?);
                    restore_backup(&mut copy, &target)?;
                } else {
                    fs::remove_file(&target).map_err(io)?;
                }
                sync_directory(target.parent().ok_or("Missing project parent folder.")?)?;
                sync_directory(staging.path())
            })();
            rollback_failed |= restored.is_err();
        }
        for directory in created.iter().rev() {
            let _ = fs::remove_dir(directory);
        }
        // The journal may be removed only after every restored directory is synced.
        rollback_failed |= !rollback_failed && clear_journal(staging.path()).is_err();
        if rollback_failed {
            // Retain staged backups for manual recovery when another process
            // changes files during rollback; never overwrite that process.
            let mut recovery_map = BTreeMap::new();
            for change in &mut changes {
                if let Some(backup) = change.backup.take() {
                    let name = backup
                        .path()
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    if backup.keep().is_ok() {
                        recovery_map.insert(change.relative.to_string_lossy().into_owned(), name);
                    }
                }
            }
            let _ = fs::write(
                staging.path().join("recovery.json"),
                json(&recovery_map).unwrap_or_default(),
            );
            let _ = sync_directory(staging.path());
            let _ = sync_directory(root);
            let recovery = staging.keep();
            return Err(format!(
                "{error} Some files could not be restored. Recovery files: {}",
                recovery.display()
            ));
        }
        return Err(error);
    }
    // Every managed mutation is durable. Removing the journal marks completion.
    // Preserve recovery files if completion cannot be made durable.
    if let Err(error) = clear_journal(staging.path()) {
        for change in &mut changes {
            if let Some(backup) = change.backup.take() {
                let _ = backup.keep();
            }
        }
        let recovery = staging.keep();
        return Err(format!(
            "{error} Project files were saved; recovery files remain at {}",
            recovery.display()
        ));
    }
    // Remove empty old managed directories only. Unrelated files retain their
    // folders. Never recursively remove a project directory.
    let mut old_parents: Vec<_> = baseline
        .keys()
        .filter_map(|p| p.parent())
        .flat_map(|p| p.ancestors())
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .collect();
    old_parents.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    old_parents.dedup();
    for relative in old_parents {
        if checked(root, &relative.join("group.json"), true).is_ok() {
            let _ = fs::remove_dir(root.join(relative));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_and_backups_exist_before_first_managed_write() {
        let dir = tempfile::tempdir().unwrap();
        let baseline = Files::from([(PathBuf::from("a.json"), b"original a".to_vec())]);
        fs::write(dir.path().join("a.json"), b"original a").unwrap();
        let desired = Files::from([(PathBuf::from("a.json"), b"updated a".to_vec())]);
        commit_with(dir.path(), &baseline, &desired, |_| {
            let staging = fs::read_dir(dir.path())
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| path.is_dir())
                .unwrap();
            let journal: Value =
                serde_json::from_slice(&fs::read(staging.join("transaction.json")).unwrap())
                    .unwrap();
            assert_eq!(journal["version"], 1);
            assert_eq!(journal["files"][0]["path"], "a.json");
            let backup = journal["files"][0]["backup"].as_str().unwrap();
            assert_eq!(fs::read(staging.join(backup)).unwrap(), b"original a");
            assert_eq!(fs::read(dir.path().join("a.json")).unwrap(), b"original a");
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_backup_rename_retains_original_bytes_for_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let mut backup = Some(staged(dir.path(), b"original bytes").unwrap());
        let target = dir.path().join("blocked-target");
        fs::create_dir(&target).unwrap();
        assert!(restore_backup(&mut backup, &target).is_err());
        let backup = backup.expect("failed rename must retain the backup");
        assert_eq!(fs::read(backup.path()).unwrap(), b"original bytes");
    }

    #[test]
    fn failed_second_write_rolls_back_first_write_and_preserves_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let baseline = Files::from([
            (PathBuf::from("a.json"), b"original a".to_vec()),
            (PathBuf::from("b.json"), b"original b".to_vec()),
            (PathBuf::from(MANIFEST), b"original manifest".to_vec()),
        ]);
        for (path, bytes) in &baseline {
            fs::write(dir.path().join(path), bytes).unwrap();
        }
        let desired = Files::from([
            (PathBuf::from("a.json"), b"updated a".to_vec()),
            (PathBuf::from("b.json"), b"updated b".to_vec()),
            (PathBuf::from(MANIFEST), b"updated manifest".to_vec()),
        ]);
        assert!(
            commit_with(dir.path(), &baseline, &desired, |index| {
                if index == 1 {
                    Err("Injected write failure".into())
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        for (path, bytes) in &baseline {
            assert_eq!(&fs::read(dir.path().join(path)).unwrap(), bytes);
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
    }

    #[test]
    fn rollback_preserves_concurrent_external_edit_and_retains_backup() {
        let dir = tempfile::tempdir().unwrap();
        let baseline = Files::from([
            (PathBuf::from("a.json"), b"original a".to_vec()),
            (PathBuf::from("b.json"), b"original b".to_vec()),
        ]);
        for (path, bytes) in &baseline {
            fs::write(dir.path().join(path), bytes).unwrap();
        }
        let desired = Files::from([
            (PathBuf::from("a.json"), b"updated a".to_vec()),
            (PathBuf::from("b.json"), b"updated b".to_vec()),
        ]);
        let error = commit_with(dir.path(), &baseline, &desired, |index| {
            if index == 1 {
                fs::write(dir.path().join("a.json"), "external a").unwrap();
                Err("Injected write failure".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(error.contains("Recovery files:"));
        assert_eq!(
            fs::read_to_string(dir.path().join("a.json")).unwrap(),
            "external a"
        );
        let recovery = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.is_dir())
            .unwrap();
        let map: BTreeMap<String, String> =
            serde_json::from_slice(&fs::read(recovery.join("recovery.json")).unwrap()).unwrap();
        assert_eq!(
            fs::read_to_string(recovery.join(&map["a.json"])).unwrap(),
            "original a"
        );
    }
}

#[cfg(test)]
mod crash_tests {
    use super::*;

    #[test]
    fn process_exit_during_group_move_leaves_durable_recovery_journal() {
        const ROOT_ENV: &str = "BLINK_PROJECT_CRASH_TEST_ROOT";
        if let Some(root) = std::env::var_os(ROOT_ENV) {
            let (disk, mut document) = ProjectDisk::open(Path::new(&root)).unwrap();
            document.groups[1].name = "zz moved group".into();
            let desired = encode(&document).unwrap();
            let _ = commit_with(disk.path(), &disk.baseline, &desired, |index| {
                if index == 2 {
                    // Process exit skips every destructor, like an abrupt crash.
                    std::process::exit(91);
                }
                Ok(())
            });
            panic!("crash injection did not run");
        }
        let dir = tempfile::tempdir().unwrap();
        let document = super::super::tests::document();
        let mut disk = ProjectDisk::create(dir.path(), &document).unwrap();
        let original_group = fs::read(dir.path().join("nested-group-2/group.json")).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "project::disk::crash_tests::process_exit_during_group_move_leaves_durable_recovery_journal", "--nocapture"])
            .env(ROOT_ENV, dir.path())
            .status().unwrap();
        assert_eq!(status.code(), Some(91));
        assert!(!dir.path().join("nested-group-2/group.json").exists());
        fs::write(dir.path().join("notes.txt"), "external edit after crash").unwrap();
        let error = ProjectDisk::open(dir.path()).unwrap_err();
        assert!(error.contains("interrupted"), "{error}");
        assert!(error.contains("transaction.json"), "{error}");
        assert!(disk.save(&document).unwrap_err().contains("interrupted"));
        let staging = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".blink-save-")
            })
            .unwrap();
        let journal: Value =
            serde_json::from_slice(&fs::read(staging.join(JOURNAL)).unwrap()).unwrap();
        let entry = journal["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["path"] == "nested-group-2/group.json")
            .unwrap();
        assert_eq!(
            fs::read(staging.join(entry["backup"].as_str().unwrap())).unwrap(),
            original_group
        );
        assert_eq!(
            entry["beforeSha256"],
            format!("{:x}", Sha256::digest(&original_group))
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("notes.txt")).unwrap(),
            "external edit after crash"
        );
    }
}
