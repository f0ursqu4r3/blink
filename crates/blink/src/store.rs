//! The single owner of domain state. Port of the reactive and IO parts of
//! `useWorkspaceState`; the state logic itself is
//! `blink_core::workspace_state::Workspace`.
//!
//! Views change state only through `Store::update_workspace` (or the send
//! methods in `runner.rs`) and re-render by observing the store. Every change
//! schedules a save, as the Vue watcher did.

use std::collections::{HashMap, HashSet};

use blink_core::engine::Engine;
use blink_core::runner::PrimaryAction;
use blink_core::workspace_state::{UNDO_WINDOW, Workspace};
use gpui_kit::*;

use crate::runner::InFlight;

/// A semantic change a view reacts to beyond re-rendering.
#[derive(Debug, Clone)]
pub enum StoreEvent {
    /// A request's draft was replaced from outside its editor (cURL import,
    /// format, undo, restore), so its inputs must reload.
    DraftReplaced(u64),
    /// The whole workspace was restored or reset.
    Restored,
}

/// A request waiting for a source request to give a response token a value.
/// See `runner.rs`.
pub(crate) struct Waiting {
    /// The source request it waits for.
    pub dependency: u64,
    /// The token that needs it, for messages.
    pub token: String,
    /// This wait sent the source request, so cancelling it may cancel that.
    pub started: bool,
    /// What to do once every token has a value.
    pub then: PrimaryAction,
}

pub struct Store {
    pub workspace: Workspace,
    pub engine: Engine,
    /// The snapshot loaded, or the user started fresh.
    pub ready: bool,
    /// Load or save failure, shown in the storage notice.
    pub error: String,
    pub saving: bool,
    /// A failed save blocked quitting.
    pub exit_blocked: bool,
    /// Quitting: the UI is inert until the last save lands.
    pub closing: bool,
    /// The user chose Quit without saving: the exit writes nothing.
    discard_on_exit: bool,
    revision: u64,
    /// Autosave key of the last snapshot handed to the writer.
    saved_key: String,
    /// The snapshot waiting for the writer; newest wins.
    pending: Option<String>,
    writing: bool,
    undo_timer: Option<(u64, Task<()>)>,
    /// Status bar COPIED, for 1.6 s after a copy.
    pub copied: bool,
    copied_timer: Option<Task<()>>,
    /// Status bar import result, for 8 s.
    pub import_notice: String,
    pub import_details: String,
    pub import_failed: bool,
    import_timer: Option<Task<()>>,
    /// Requests waiting for the protected-environment confirmation.
    pub(crate) confirming: HashSet<u64>,
    /// Sends in flight, by request id. See `runner.rs`.
    pub(crate) in_flight: HashMap<u64, InFlight>,
    /// Requests waiting for a source request, by request id.
    pub(crate) waiting: HashMap<u64, Waiting>,
    /// Live WebSocket connections: engine connection id and event task.
    pub(crate) sockets: HashMap<u64, (String, Task<()>)>,
}

impl EventEmitter<StoreEvent> for Store {}

impl Store {
    pub fn new(engine: Engine, cx: &mut Context<Self>) -> Self {
        let mut store = Store {
            workspace: Workspace::new(),
            engine,
            ready: false,
            error: String::new(),
            saving: false,
            exit_blocked: false,
            closing: false,
            discard_on_exit: false,
            revision: 0,
            saved_key: String::new(),
            pending: None,
            writing: false,
            undo_timer: None,
            copied: false,
            copied_timer: None,
            import_notice: String::new(),
            import_details: String::new(),
            import_failed: false,
            import_timer: None,
            confirming: HashSet::new(),
            in_flight: HashMap::new(),
            waiting: HashMap::new(),
            sockets: HashMap::new(),
        };
        store.restore(cx);
        store
    }

    /// Footer text: `status` in `useWorkspaceState`.
    pub fn status(&self) -> &'static str {
        if !self.error.is_empty() {
            "NOT SAVED"
        } else if !self.ready {
            "RESTORING"
        } else if self.saving {
            "SAVING"
        } else {
            "SAVED LOCALLY"
        }
    }

    /// Load the snapshot. A corrupt or unsupported one is never replaced:
    /// the notice offers Retry and Start fresh.
    pub fn restore(&mut self, cx: &mut Context<Self>) {
        self.ready = false;
        cx.notify();
        let load = self.engine.load_workspace();
        cx.spawn(async move |this, cx| {
            let result = load.await;
            this.update(cx, |this, cx| {
                match result.and_then(|content| match content {
                    Some(content) => this.workspace.restore(&content),
                    None => Ok(()),
                }) {
                    Ok(()) => {
                        this.workspace.response_cache = this.engine.load_response_tokens();
                        let live = this.workspace.sessions.iter().map(|s| s.id).collect();
                        this.workspace.response_cache.prune(&live);
                        this.workspace.refresh_all_stale();
                        this.error.clear();
                        this.ready = true;
                        this.saved_key = this.workspace.autosave_key();
                        cx.emit(StoreEvent::Restored);
                        // As the Vue watcher did when `ready` turned true:
                        // write the restored state at once, so an older
                        // snapshot version is upgraded and interrupted
                        // requests are saved idle.
                        this.flush(cx);
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Save the response token cache. A failure is not a workspace error:
    /// the cache is rebuilt by the next send.
    pub fn save_response_cache(&self, cx: &mut Context<Self>) {
        let save = self
            .engine
            .save_response_tokens(self.workspace.response_cache.clone());
        cx.spawn(async move |_, _| {
            let _ = save.await;
        })
        .detach();
    }

    /// Start fresh: save an empty workspace, then use it.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        let mut fresh = Workspace::new();
        fresh.reset();
        let save = self.engine.save_workspace(fresh.encode());
        cx.spawn(async move |this, cx| {
            let result = save.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.workspace = fresh;
                        this.save_response_cache(cx);
                        this.error.clear();
                        this.ready = true;
                        this.saved_key = this.workspace.autosave_key();
                        cx.emit(StoreEvent::Restored);
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Change the workspace, then re-render and save if the saved state changed.
    pub fn update_workspace<R>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Workspace) -> R,
    ) -> R {
        let deletion_before = self.workspace.deletion_serial();
        let result = change(&mut self.workspace);
        for body_id in self.workspace.take_released_bodies() {
            self.engine.release_response(&body_id);
        }
        if self.workspace.deletion_serial() != deletion_before {
            self.arm_undo_timer(cx);
        }
        self.changed(cx);
        result
    }

    /// Re-render, and save when the saved state changed. Elapsed time and
    /// scroll offsets are not part of the key, so a running request never
    /// causes timer-driven disk writes.
    pub fn changed(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        if !self.ready {
            return;
        }
        let key = self.workspace.autosave_key();
        if key != self.saved_key {
            self.saved_key = key;
            self.flush(cx);
        }
    }

    /// Queue the current snapshot. One disk write at a time; the newest
    /// queued snapshot wins, never an older one.
    pub fn flush(&mut self, cx: &mut Context<Self>) {
        if !self.ready {
            return;
        }
        self.revision += 1;
        self.pending = Some(self.workspace.encode());
        self.saving = true;
        cx.notify();
        if !self.writing {
            self.drain(cx);
        }
    }

    fn drain(&mut self, cx: &mut Context<Self>) {
        let Some(content) = self.pending.take() else {
            self.writing = false;
            return;
        };
        self.writing = true;
        let revision = self.revision;
        let save = self.engine.save_workspace(content.clone());
        cx.spawn(async move |this, cx| {
            let result = save.await;
            this.update(cx, |this, cx| {
                let current = revision == this.revision;
                match result {
                    Ok(()) => {
                        if current {
                            this.error.clear();
                        }
                    }
                    Err(error) => {
                        // Keep the failed snapshot unless a newer one is queued.
                        this.pending.get_or_insert(content);
                        if current {
                            this.error = error;
                        }
                        this.writing = false;
                        this.saving = false;
                        cx.notify();
                        return;
                    }
                }
                if this.pending.is_some() {
                    this.drain(cx);
                } else {
                    this.writing = false;
                    this.saving = false;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Retry after a failed load or save.
    pub fn retry(&mut self, cx: &mut Context<Self>) {
        if self.ready {
            self.flush(cx);
        } else {
            self.restore(cx);
        }
    }

    /// Write the latest snapshot synchronously before quitting. A failed
    /// restore never overwrites the existing file. Returns false when the
    /// save failed and quitting must wait for the user.
    pub fn save_before_exit(&mut self, cx: &mut Context<Self>) -> bool {
        // Already saved by an earlier exit step, or the user discards changes.
        if !self.ready || self.closing || self.discard_on_exit {
            return true;
        }
        self.closing = true;
        match self.engine.save_workspace_now(&self.workspace.encode()) {
            Ok(()) => true,
            Err(error) => {
                self.error = error;
                self.exit_blocked = true;
                self.closing = false;
                cx.notify();
                false
            }
        }
    }

    /// Quit now and write nothing: the storage notice's Quit without saving.
    pub fn quit_without_saving(&mut self, cx: &mut Context<Self>) {
        self.discard_on_exit = true;
        cx.quit();
    }

    /// Save the latest snapshot, then quit. A failed save blocks quitting and
    /// the storage notice offers Retry and Quit without saving.
    pub fn quit(&mut self, cx: &mut Context<Self>) {
        if self.save_before_exit(cx) {
            cx.quit();
        }
    }

    /// Copy text and show COPIED in the status bar. `useClipboard`.
    pub fn copy(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = true;
        self.copied_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1600))
                .await;
            this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Show an import result in the status bar for 8 s. `notifyImport`.
    pub fn notify_import(
        &mut self,
        message: String,
        failed: bool,
        details: String,
        cx: &mut Context<Self>,
    ) {
        self.import_notice = message;
        self.import_details = details;
        self.import_failed = failed;
        self.import_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(8))
                .await;
            this.update(cx, |this, cx| {
                this.import_notice.clear();
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Deletions can be undone for 10 s, then they expire.
    fn arm_undo_timer(&mut self, cx: &mut Context<Self>) {
        if self.workspace.last_deletion().is_none() {
            self.undo_timer = None;
            return;
        }
        let serial = self.workspace.deletion_serial();
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(UNDO_WINDOW).await;
            this.update(cx, |this, cx| {
                this.update_workspace(cx, |workspace| workspace.expire_deletion(serial));
            })
            .ok();
        });
        self.undo_timer = Some((serial, task));
    }
}
