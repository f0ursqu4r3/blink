use std::{
    collections::BTreeSet,
    fs,
    io::Read as _,
    path::{Component, Path, PathBuf},
};

const LIMIT: u64 = 64 * 1024;
const BUNDLED: &str = "/Applications/Ghostty.app/Contents/Resources/ghostty/themes";

/// User themes first, so they win over bundled themes with the same name.
fn theme_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".config/ghostty/themes"));
    }
    dirs.push(PathBuf::from(BUNDLED));
    dirs
}

fn valid_name(name: &str) -> bool {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains(':')
    {
        return false;
    }
    // Reject a name that resolves to something other than a single plain
    // path segment (e.g. a Windows drive prefix like `C:foo`, `.`, or `/`).
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn list(dirs: &[PathBuf]) -> Vec<String> {
    let mut names = BTreeSet::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let is_file = fs::metadata(entry.path())
                .map(|meta| meta.is_file())
                .unwrap_or(false);
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if is_file && valid_name(&name) && !name.starts_with('.') {
                names.insert(name);
            }
        }
    }
    names.into_iter().collect()
}

fn read(dirs: &[PathBuf], name: &str) -> Result<String, String> {
    if !valid_name(name) {
        return Err("Invalid theme name.".into());
    }
    for dir in dirs {
        let path: &Path = &dir.join(name);
        let Ok(meta) = fs::metadata(path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if meta.len() > LIMIT {
            return Err("Theme file is larger than 64 KiB.".into());
        }
        let file =
            fs::File::open(path).map_err(|error| format!("Could not read theme: {error}"))?;
        let mut contents = String::new();
        let read = file
            .take(LIMIT + 1)
            .read_to_string(&mut contents)
            .map_err(|error| format!("Could not read theme: {error}"))?;
        if read as u64 > LIMIT {
            return Err("Theme file is larger than 64 KiB.".into());
        }
        return Ok(contents);
    }
    Err(format!("Theme {name} not found."))
}

#[tauri::command]
pub fn list_ghostty_themes() -> Vec<String> {
    list(&theme_dirs())
}

#[tauri::command]
pub fn read_ghostty_theme(name: String) -> Result<String, String> {
    read(&theme_dirs(), &name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs() -> (tempfile::TempDir, tempfile::TempDir, Vec<PathBuf>) {
        let user = tempfile::tempdir().unwrap();
        let bundled = tempfile::tempdir().unwrap();
        let paths = vec![user.path().to_path_buf(), bundled.path().to_path_buf()];
        (user, bundled, paths)
    }

    #[test]
    fn lists_sorted_unique_names_and_skips_missing_dirs() {
        let (user, bundled, mut paths) = dirs();
        fs::write(user.path().join("Zenburn"), "background = #3f3f3f").unwrap();
        fs::write(bundled.path().join("Zenburn"), "background = #000000").unwrap();
        fs::write(bundled.path().join("Monokai Pro"), "background = #262427").unwrap();
        fs::write(bundled.path().join(".DS_Store"), "").unwrap();
        fs::create_dir(bundled.path().join("folder")).unwrap();
        paths.push(PathBuf::from("/missing/ghostty/themes"));
        assert_eq!(list(&paths), vec!["Monokai Pro", "Zenburn"]);
    }

    #[test]
    fn user_theme_wins_over_bundled_theme() {
        let (user, bundled, paths) = dirs();
        fs::write(user.path().join("Zenburn"), "background = #3f3f3f").unwrap();
        fs::write(bundled.path().join("Zenburn"), "background = #000000").unwrap();
        assert_eq!(read(&paths, "Zenburn").unwrap(), "background = #3f3f3f");
    }

    #[test]
    fn rejects_path_names_large_files_and_missing_themes() {
        let (_user, bundled, paths) = dirs();
        for name in ["", "../secret", "a/b", "a\\b", "..", "C:foo"] {
            assert_eq!(read(&paths, name), Err("Invalid theme name.".into()));
        }
        fs::write(bundled.path().join("Huge"), vec![b'#'; 64 * 1024 + 1]).unwrap();
        assert_eq!(
            read(&paths, "Huge"),
            Err("Theme file is larger than 64 KiB.".into())
        );
        assert_eq!(read(&paths, "Nope"), Err("Theme Nope not found.".into()));
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinks_and_skips_broken_ones() {
        use std::os::unix::fs::symlink;

        let (user, _bundled, paths) = dirs();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("Real"), "background = #123456").unwrap();
        symlink(outside.path().join("Real"), user.path().join("Linked")).unwrap();
        symlink(outside.path().join("Missing"), user.path().join("Broken")).unwrap();

        assert_eq!(list(&paths), vec!["Linked"]);
        assert_eq!(read(&paths, "Linked").unwrap(), "background = #123456");
    }
}
