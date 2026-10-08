//! Folder picking and project confirmations shared by Browser menus.

use crate::store::Store;
use gpui_kit::*;

pub fn open_folder(store: Entity<Store>, cx: &mut App) {
    pick_folder(store, None, cx);
}

fn pick_folder(store: Entity<Store>, root: Option<u64>, cx: &mut App) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some(
            if root.is_some() {
                "Save project here"
            } else {
                "Open project"
            }
            .into(),
        ),
    });
    store.update(cx, |_, cx| {
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |this, cx| match root {
                Some(root) => this.save_group_to_folder(root, path, cx),
                None => this.open_project(path, cx),
            })
            .ok();
        })
        .detach();
    });
}

pub fn save_group(store: Entity<Store>, root: u64, window: &mut Window, cx: &mut App) {
    let workspace = &store.read(cx).workspace;
    let Some(group) = workspace.group(root) else {
        return;
    };
    let group_ids = blink_core::groups::group_subtree(&workspace.groups, root);
    let requests = workspace
        .sessions
        .iter()
        .filter(|s| s.group_id.is_some_and(|id| group_ids.contains(&id)))
        .count();
    let detail = format!(
        "Save {} requests and {} groups as JSON files in an empty folder. Request bodies, headers, and ordinary token values will be shared. Authorization credentials and captured values stay in Blink's local storage. Use token references for other secrets. Review the files before committing them to Git.",
        requests,
        group_ids.len()
    );
    let answer = window.prompt(
        PromptLevel::Info,
        &format!("Save {} as a project?", group.name),
        Some(&detail),
        &["Choose Folder…", "Cancel"],
        cx,
    );
    store.update(cx, |_, cx| {
        cx.spawn(async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            if let Some(store) = this.upgrade() {
                cx.update(|cx| pick_folder(store, Some(root), cx));
            }
        })
        .detach();
    });
}

pub fn reload(store: Entity<Store>, root: u64, window: &mut Window, cx: &mut App) {
    let answer = window.prompt(
        PromptLevel::Warning, "Reload project from disk?",
        Some("This replaces this project's request definitions with the files on disk. Unsaved definition edits will be discarded. Local credentials and request history stay in Blink."),
        &["Reload", "Cancel"], cx,
    );
    store.update(cx, |_, cx| {
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(0) {
                this.update(cx, |this, cx| this.reload_project(root, cx))
                    .ok();
            }
        })
        .detach();
    });
}

pub fn transfer(
    store: Entity<Store>,
    ids: Vec<u64>,
    target: Option<u64>,
    window: &mut Window,
    cx: &mut App,
) {
    let workspace = &store.read(cx).workspace;
    let target_name = target
        .and_then(|id| workspace.group(id))
        .map(|g| g.name.as_str())
        .unwrap_or("Local Ungrouped");
    let mut preview = workspace.clone();
    if let Err(error) = preview.transfer_requests(&ids, target) {
        store.update(cx, |s, cx| s.notify_import(error, true, String::new(), cx));
        return;
    }
    let detail = format!(
        "Move {} requests to {target_name}. This changes their storage location. Blink checked token dependencies and inherited request settings. Responses and history stay local. Project cookie jars remain separate.",
        ids.len()
    );
    let answer = window.prompt(
        PromptLevel::Warning,
        "Move requests to this project?",
        Some(&detail),
        &["Move", "Cancel"],
        cx,
    );
    store.update(cx, |_, cx| {
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(0) {
                this.update(cx, |s, cx| {
                    let result = s.update_workspace(cx, |w| w.transfer_requests(&ids, target));
                    if let Err(error) = result {
                        s.notify_import(error, true, String::new(), cx);
                    } else {
                        cx.emit(crate::store::StoreEvent::Restored);
                    }
                })
                .ok();
            }
        })
        .detach();
    });
}
