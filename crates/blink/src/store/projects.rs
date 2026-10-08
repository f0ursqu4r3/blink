//! Project actions. Filesystem work runs on the engine, never in a render.

use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::*;

use super::{Store, StoreEvent};
use blink_core::workspace_state::Workspace;

impl Store {
    pub fn active_project_path(&self) -> Option<String> {
        self.workspace
            .active()
            .and_then(|request| self.workspace.project_for_group(request.group_id))
            .map(|project| project.path.clone())
    }

    fn project_error(&mut self, root: Option<u64>, error: String, cx: &mut Context<Self>) {
        if let Some(root) = root {
            self.project_errors.insert(root, error.clone());
        }
        self.notify_import(error, true, String::new(), cx);
    }

    fn begin_project_operation(&mut self, cx: &mut Context<Self>) -> bool {
        if self.collection_running {
            self.error = "Finish or cancel the collection run before changing projects.".into();
            cx.notify();
            return false;
        }
        if !self.ready || self.closing || self.project_operation {
            return false;
        }
        self.project_operation = true;
        self.ready = false;
        cx.notify();
        true
    }

    fn finish_project_operation(&mut self, cx: &mut Context<Self>) {
        self.project_operation = false;
        self.ready = true;
        self.saved_key = self.workspace.autosave_key();
        cx.emit(StoreEvent::Restored);
        self.flush(cx);
        self.watch_projects(cx);
        cx.notify();
    }

    pub fn open_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.begin_project_operation(cx) {
            return;
        }
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            if !wait_for_writer(&this, cx).await { return; }
            let result = engine.open_project(path).await;
            this.update(cx, |this, cx| {
                match result.and_then(|(path, document)| {
                    if let Some(project) = this.workspace.projects.iter().find(|p| p.path == path) {
                        let root = project.root_id;
                        this.engine.forget_project(&path);
                        this.project_errors.entry(root).or_insert_with(|| "This project is already open. Use Reload Project to resolve its disk state.".into());
                        return Err("This project folder is already open. Use its Reload Project action.".into());
                    }
                    let result = this.workspace.attach_project(path.clone(), document);
                    if result.is_err() { this.engine.forget_project(&path); }
                    result
                }) {
                    Ok(root) => {
                        this.workspace.expand_ancestors(Some(root));
                        this.project_errors.remove(&root);
                        this.notify_import("PROJECT OPENED".into(), false, String::new(), cx);
                    }
                    Err(error) => this.project_error(None, error, cx),
                }
                this.finish_project_operation(cx);
            }).ok();
        }).detach();
    }

    pub fn save_group_to_folder(&mut self, root: u64, path: PathBuf, cx: &mut Context<Self>) {
        if !self.begin_project_operation(cx) {
            return;
        }
        let engine = self.engine.clone();
        let preview = {
            let mut workspace = self.workspace.clone();
            workspace.convert_group_to_project(root, path.to_string_lossy().into_owned())
        };
        cx.spawn(async move |this, cx| {
            if !wait_for_writer(&this, cx).await {
                return;
            }
            let result = match preview {
                Ok(document) => engine.create_project(path, document).await,
                Err(error) => Err(error),
            };
            this.update(cx, |this, cx| {
                match result.and_then(|path| this.workspace.convert_group_to_project(root, path)) {
                    Ok(_) => this.notify_import(
                        "PROJECT SAVED TO FOLDER".into(),
                        false,
                        String::new(),
                        cx,
                    ),
                    Err(error) => this.project_error(None, error, cx),
                }
                this.finish_project_operation(cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn reload_project(&mut self, root: u64, cx: &mut Context<Self>) {
        let Some(project) = self.workspace.projects.iter().find(|p| p.root_id == root) else {
            return;
        };
        let path: PathBuf = project.path.clone().into();
        if self.workspace.sessions.iter().any(|s| {
            s.running()
                && self
                    .workspace
                    .project_for_group(s.group_id)
                    .is_some_and(|p| p.root_id == root)
        }) {
            self.project_error(
                Some(root),
                "Stop the project's requests before reloading.".into(),
                cx,
            );
            return;
        }
        if !self.begin_project_operation(cx) {
            return;
        }
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            if !wait_for_writer(&this, cx).await {
                return;
            }
            // Failed pending writes are superseded by this explicit reload.
            let result = engine.reload_project_files(path).await;
            this.update(cx, |this, cx| {
                match result.and_then(|(path, document)| {
                    let result = this.workspace.reload_project(root, document);
                    if result.is_err() {
                        this.engine.forget_project(&path);
                    }
                    result
                }) {
                    Ok(()) => {
                        this.project_errors.remove(&root);
                        this.pending = None;
                        this.notify_import("PROJECT RELOADED".into(), false, String::new(), cx);
                    }
                    Err(error) => this.project_error(Some(root), error, cx),
                }
                this.finish_project_operation(cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn close_project(&mut self, root: u64, cx: &mut Context<Self>) {
        if !self.ready || self.project_operation {
            return;
        }
        if !self.error.is_empty() || self.project_errors.contains_key(&root) {
            self.project_error(Some(root), "Resolve the project save error before closing. Local edits are still kept in Blink.".into(), cx);
            return;
        }
        if !self.begin_project_operation(cx) {
            return;
        }
        let engine = self.engine.clone();
        // Saving before detach prevents a close from dropping queued edits.
        let content = self.workspace.encode();
        cx.spawn(async move |this, cx| {
            if !wait_for_writer(&this, cx).await {
                return;
            }
            let result = engine.save_workspace(content).await;
            this.update(cx, |this, cx| {
                let path = this
                    .workspace
                    .projects
                    .iter()
                    .find(|p| p.root_id == root)
                    .map(|p| p.path.clone());
                match result.and_then(|()| this.workspace.close_project(root)) {
                    Ok(()) => {
                        this.project_errors.remove(&root);
                        if let Some(path) = path {
                            this.engine.forget_project(&path);
                        }
                    }
                    Err(error) => this.project_error(Some(root), error, cx),
                }
                this.finish_project_operation(cx);
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn record_project_save(&mut self, content: &str) {
        if self.workspace.projects.is_empty() {
            return;
        }
        let Ok(saved) = Workspace::decode(content) else {
            return;
        };
        for project in &mut self.workspace.projects {
            if let Ok(document) = saved.project_document(project.root_id) {
                project.accept_disk_baseline(document);
                self.project_errors.remove(&project.root_id);
            }
        }
    }

    pub(super) fn watch_projects(&mut self, cx: &mut Context<Self>) {
        if self.project_poll.is_some() || self.workspace.projects.is_empty() {
            return;
        }
        self.project_poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let Ok(check) = this.update(cx, |this, _| {
                    if !this.ready
                        || this.writing
                        || this.project_operation
                        || this.collection_running
                    {
                        return None;
                    }
                    let paths = this
                        .workspace
                        .projects
                        .iter()
                        .map(|p| (p.root_id, p.path.clone()))
                        .collect();
                    Some((this.engine.clone(), paths))
                }) else {
                    break;
                };
                let Some((engine, paths)) = check else {
                    continue;
                };
                if let Ok(changes) = engine.changed_projects(paths).await {
                    this.update(cx, |this, cx| {
                        if !this.ready || this.writing {
                            return;
                        }
                        for (root, error) in changes {
                            let clean = this
                                .workspace
                                .projects
                                .iter()
                                .find(|p| p.root_id == root)
                                .and_then(|p| p.disk_baseline.as_ref())
                                .is_some_and(|baseline| {
                                    this.workspace.project_document(root).as_ref() == Ok(baseline)
                                });
                            if clean && error.starts_with("Project files changed") {
                                this.reload_project(root, cx);
                                break;
                            }
                            this.project_errors.insert(root, error);
                        }
                        cx.notify();
                    })
                    .ok();
                }
            }
        }));
    }
}

async fn wait_for_writer(this: &WeakEntity<Store>, cx: &mut AsyncApp) -> bool {
    loop {
        match this.update(cx, |this, _| this.writing) {
            Ok(false) => return true,
            Ok(true) => {
                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await
            }
            Err(_) => return false,
        }
    }
}
