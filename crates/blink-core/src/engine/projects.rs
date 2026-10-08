//! Project IO coordinated with the local workspace writer.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::{Engine, StoredCookie, cookies::Cookies};
use crate::project::{ProjectDisk, ProjectDocument};
use crate::workspace_state::Workspace;

/// A project keeps its last local definitions if its folder is unavailable.
pub type ProjectErrors = HashMap<u64, String>;

impl Engine {
    pub fn open_project(
        &self,
        path: PathBuf,
    ) -> impl Future<Output = Result<(String, ProjectDocument), String>> + Send + 'static {
        let engine = self.clone();
        self.blocking(
            move || engine.open_project_now(path, false),
            "Project open task failed.",
        )
    }

    pub fn reload_project_files(
        &self,
        path: PathBuf,
    ) -> impl Future<Output = Result<(String, ProjectDocument), String>> + Send + 'static {
        let engine = self.clone();
        self.blocking(
            move || engine.open_project_now(path, true),
            "Project reload task failed.",
        )
    }

    /// Drop write authorization after close or after a rejected reload.
    pub fn forget_project(&self, path: &str) {
        if let Ok(mut projects) = self.0.projects.lock() {
            projects.remove(path);
        }
    }

    fn open_project_now(
        &self,
        path: PathBuf,
        reload: bool,
    ) -> Result<(String, ProjectDocument), String> {
        let mut projects = self
            .0
            .projects
            .lock()
            .map_err(|_| "Project storage is unavailable.")?;
        let (disk, document) = ProjectDisk::open(&path)?;
        let path = disk
            .path()
            .to_str()
            .ok_or("Project path is not valid UTF-8.")?
            .to_string();
        if !reload && projects.contains_key(&path) {
            return Err("This project folder is already open.".into());
        }
        engine_cookie_init(self, &path)?;
        projects.insert(path.clone(), disk);
        Ok((path, document))
    }

    pub fn create_project(
        &self,
        path: PathBuf,
        document: ProjectDocument,
    ) -> impl Future<Output = Result<String, String>> + Send + 'static {
        let engine = self.clone();
        self.blocking(
            move || {
                let mut projects = engine
                    .0
                    .projects
                    .lock()
                    .map_err(|_| "Project storage is unavailable.")?;
                let disk = ProjectDisk::create(&path, &document)?;
                let path = disk
                    .path()
                    .to_str()
                    .ok_or("Project path is not valid UTF-8.")?
                    .to_string();
                engine_cookie_init(&engine, &path)?;
                projects.insert(path.clone(), disk);
                Ok(path)
            },
            "Project creation task failed.",
        )
    }

    /// Restore definitions from disk before enabling autosave. Missing projects
    /// keep their local recovery copy and receive a visible error.
    pub fn restore_project_workspace(
        &self,
        content: String,
    ) -> impl Future<Output = Result<(String, ProjectErrors), String>> + Send + 'static {
        let engine = self.clone();
        self.blocking(move || {
            let mut workspace = Workspace::decode(&content)?;
            let attachments = workspace.projects.clone();
            let mut errors = HashMap::new();
            for project in attachments {
                let local = workspace.project_document(project.root_id);
                let result = engine.open_project_now(project.path.clone().into(), true)
                    .and_then(|(_, document)| {
                        let local = local?;
                        if local != document && project.disk_baseline.as_ref() != Some(&local) {
                            return Err("Local project edits were not saved to disk. Blink kept the recovery copy. Reload only to discard those edits.".into());
                        }
                        workspace.reload_project(project.root_id, document)
                    });
                if let Err(error) = result {
                    engine.forget_project(&project.path);
                    errors.insert(project.root_id, error);
                }
            }
            Ok((workspace.encode(), errors))
        }, "Project restore task failed.")
    }

    /// Check files without accepting a new baseline. The UI decides whether
    /// local edits must be kept before it reloads an external change.
    pub fn changed_projects(
        &self,
        projects: Vec<(u64, String)>,
    ) -> impl Future<Output = Result<ProjectErrors, String>> + Send + 'static {
        let engine = self.clone();
        self.blocking(move || {
            let disks = engine.0.projects.lock().map_err(|_| "Project storage is unavailable.")?;
            let mut changed = HashMap::new();
            for (id, path) in projects {
                match disks.get(&path).ok_or_else(|| "Project is unavailable. Reload the project folder.".to_string()).and_then(ProjectDisk::changed) {
                    Ok(false) => (),
                    Ok(true) => { changed.insert(id, "Project files changed outside Blink. Reload the project to use them.".into()); },
                    Err(error) => { changed.insert(id, error); }
                }
            }
            Ok(changed)
        }, "Project check task failed.")
    }

    pub(super) fn save_project_definitions(&self, content: &str) -> Result<(), String> {
        // Legacy callers may save minimal v1 fixtures. Only project-bearing
        // snapshots need the complete typed model.
        let value: serde_json::Value =
            serde_json::from_str(content).map_err(|_| "Invalid workspace.")?;
        if value
            .get("projects")
            .is_none_or(|p| p.as_array().is_some_and(Vec::is_empty))
        {
            return Ok(());
        }
        let workspace = Workspace::decode(content)?;
        let mut disks = self
            .0
            .projects
            .lock()
            .map_err(|_| "Project storage is unavailable.")?;
        let mut errors = Vec::new();
        for project in &workspace.projects {
            let result = workspace
                .project_document(project.root_id)
                .and_then(|document| {
                    disks
                        .get_mut(&project.path)
                        .ok_or_else(|| {
                            "Project is unavailable. Reload the project folder.".to_string()
                        })?
                        .save(&document)
                });
            if let Err(error) = result {
                let name = workspace
                    .group(project.root_id)
                    .map(|g| g.name.as_str())
                    .unwrap_or("Project");
                errors.push(format!("{name}: {error}"));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }

    pub(super) fn scoped_cookies(&self, project: Option<&str>) -> Result<Arc<Cookies>, String> {
        let Some(project) = project else {
            return Ok(self.0.cookies.clone());
        };
        let mut jars = self
            .0
            .project_cookies
            .lock()
            .map_err(|_| "Project cookies are unavailable.")?;
        Ok(jars
            .entry(project.to_string())
            .or_insert_with(|| {
                let digest = Sha256::digest(project.as_bytes());
                let name = format!("{digest:x}.json");
                Arc::new(Cookies::load(
                    self.0.paths.data_dir.join("project-cookies").join(name),
                ))
            })
            .clone())
    }

    pub fn list_cookies_scoped(&self, project: Option<&str>) -> Result<Vec<StoredCookie>, String> {
        Ok(self.scoped_cookies(project)?.list())
    }

    pub fn delete_cookie_scoped(
        &self,
        project: Option<&str>,
        domain: &str,
        path: &str,
        name: &str,
    ) -> Result<(), String> {
        self.scoped_cookies(project)?.delete(domain, path, name)
    }

    pub fn clear_cookies_scoped(&self, project: Option<&str>) -> Result<(), String> {
        self.scoped_cookies(project)?.clear()
    }
}

fn engine_cookie_init(engine: &Engine, path: &str) -> Result<(), String> {
    engine.scoped_cookies(Some(path)).map(|_| ())
}

/// Resolve a portable upload reference. FileGrants still requires the user to
/// pick the resulting file before the HTTP engine can read it.
pub(super) fn resolve_project_files(
    request: &mut crate::model::RequestInput,
    project: Option<&str>,
) -> Result<(), String> {
    let Some(project) = project else {
        return Ok(());
    };
    let resolve = |file: &mut String| -> Result<(), String> {
        let path = std::path::Path::new(file);
        if path.is_absolute() {
            return Ok(());
        }
        if path.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        }) {
            return Err(
                "A project upload path cannot leave its folder. Pick the file explicitly.".into(),
            );
        }
        *file = std::path::Path::new(project)
            .join(path)
            .to_str()
            .ok_or("Upload path is not valid UTF-8.")?
            .to_string();
        Ok(())
    };
    if let Some(path) = &mut request.body_file {
        resolve(path)?;
    }
    for part in request
        .multipart
        .iter_mut()
        .flatten()
        .filter(|part| part.file)
    {
        resolve(&mut part.value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{Engine, Paths};
    use crate::workspace_state::Workspace;
    use futures::executor::block_on;

    fn engine(directory: &std::path::Path) -> Engine {
        Engine::new(Paths::with_base(directory.into())).unwrap()
    }

    #[test]
    fn project_save_and_restore_read_definitions_from_disk() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let engine = engine(data.path());
        let mut workspace = Workspace::new();
        let root = workspace.add_group("API", None);
        let request = workspace.create(Some(Some(root)));
        workspace.session_mut(request).unwrap().draft.url = "https://example.test/one".into();
        let document = workspace
            .convert_group_to_project(root, project.path().to_str().unwrap().into())
            .unwrap();
        let canonical = block_on(engine.create_project(project.path().into(), document)).unwrap();
        workspace.projects[0].path = canonical;
        block_on(engine.save_workspace(workspace.encode())).unwrap();
        workspace.session_mut(request).unwrap().draft.url = "https://example.test/two".into();
        block_on(engine.save_workspace(workspace.encode())).unwrap();
        let (_, disk) = crate::project::ProjectDisk::open(project.path()).unwrap();
        assert_eq!(disk.requests[0].draft.url, "https://example.test/two");
        assert!(
            !std::fs::read_to_string(project.path().join("blink.json"))
                .unwrap()
                .contains("response")
        );
    }

    #[test]
    fn duplicate_open_does_not_accept_external_changes_or_overwrite_them() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let engine = engine(data.path());
        let mut ws = Workspace::new();
        let root = ws.add_group("API", None);
        let doc = ws
            .convert_group_to_project(root, folder.path().to_string_lossy().into_owned())
            .unwrap();
        ws.projects[0].path = block_on(engine.create_project(folder.path().into(), doc)).unwrap();
        let (mut external, mut doc) = crate::project::ProjectDisk::open(folder.path()).unwrap();
        doc.groups[0].name = "External".into();
        external.save(&doc).unwrap();
        assert!(block_on(engine.open_project(folder.path().into())).is_err());
        assert!(block_on(engine.save_workspace(ws.encode())).is_err());
        assert_eq!(
            crate::project::ProjectDisk::open(folder.path())
                .unwrap()
                .1
                .groups[0]
                .name,
            "External"
        );
    }

    #[test]
    fn startup_keeps_recovery_edits_after_a_project_save_conflict() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let engine = engine(data.path());
        let mut ws = Workspace::new();
        let root = ws.add_group("API", None);
        let request = ws.create(Some(Some(root)));
        let doc = ws
            .convert_group_to_project(root, folder.path().to_string_lossy().into_owned())
            .unwrap();
        ws.projects[0].path = block_on(engine.create_project(folder.path().into(), doc)).unwrap();
        ws.session_mut(request).unwrap().draft.url = "https://local.test/unsaved".into();
        let (mut external, mut doc) = crate::project::ProjectDisk::open(folder.path()).unwrap();
        doc.groups[0].name = "External".into();
        external.save(&doc).unwrap();
        assert!(block_on(engine.save_workspace(ws.encode())).is_err());
        let recovery = block_on(engine.load_workspace()).unwrap().unwrap();
        let (restored, errors) = block_on(engine.restore_project_workspace(recovery)).unwrap();
        assert!(errors.contains_key(&root));
        let restored = Workspace::decode(&restored).unwrap();
        assert_eq!(
            restored.session(request).unwrap().draft.url,
            "https://local.test/unsaved"
        );
        assert!(block_on(engine.save_workspace(restored.encode())).is_err());
        assert_eq!(
            crate::project::ProjectDisk::open(folder.path())
                .unwrap()
                .1
                .groups[0]
                .name,
            "External"
        );
    }

    #[test]
    fn project_cookies_are_isolated_and_survive_engine_restart() {
        let data = tempfile::tempdir().unwrap();
        let first = engine(data.path());
        let a = first.scoped_cookies(Some("/project/a")).unwrap();
        a.jar()
            .lock()
            .unwrap()
            .parse(
                "session=one; Path=/",
                &"https://example.test/".parse().unwrap(),
            )
            .unwrap();
        a.save().unwrap();
        assert!(
            first
                .scoped_cookies(Some("/project/b"))
                .unwrap()
                .list()
                .is_empty()
        );
        assert!(first.list_cookies().is_empty());
        let second = engine(data.path());
        assert_eq!(
            second.scoped_cookies(Some("/project/a")).unwrap().list()[0].value,
            "one"
        );
        second.clear_cookies_scoped(Some("/project/a")).unwrap();
        assert!(
            second
                .list_cookies_scoped(Some("/project/a"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn upload_paths_resolve_inside_project_without_granting_access() {
        let mut request = crate::model::RequestInput {
            body_file: Some("fixtures/body.json".into()),
            ..Default::default()
        };
        super::resolve_project_files(&mut request, Some("/project")).unwrap();
        assert_eq!(
            request.body_file.as_deref(),
            Some("/project/fixtures/body.json")
        );
        request.body_file = Some("../private.json".into());
        assert!(super::resolve_project_files(&mut request, Some("/project")).is_err());
        request.body_file = Some("/picked/elsewhere.json".into());
        super::resolve_project_files(&mut request, Some("/project")).unwrap();
        assert_eq!(request.body_file.as_deref(), Some("/picked/elsewhere.json"));
    }
}
