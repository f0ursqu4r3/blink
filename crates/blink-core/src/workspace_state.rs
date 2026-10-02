//! The workspace state: port of `src/composables/useWorkspaceState.ts` and of
//! the parts of `src/App.vue` that change it (tab and selection actions,
//! group focus, captures, and environment switches).
//!
//! Saving, reading, and timers stay with the UI:
//! - `encode` gives the snapshot to write; `autosave_key` tells when a write
//!   is due (it ignores the response scroll offset).
//! - A deletion stays undoable for `UNDO_WINDOW`. After each deletion the UI
//!   reads `deletion_serial`, starts a timer, and when it fires calls
//!   `expire_deletion` with that serial.
//! - Response bodies that the workspace drops wait in a queue; the UI takes
//!   them with `take_released_bodies` and releases them in the engine.

use std::collections::HashSet;
use std::time::Duration;

use crate::environments::{
    CaptureTarget, active_environment, apply_capture, request_environment, root_group,
};
use crate::groups::{
    create_group, delete_group_and_promote_contents, group_subtree, groups_after_move,
    sessions_after_move,
};
use crate::import::count_requests;
use crate::model::{
    AuthorizationConfig, Definitions, Environment, ImportResult, ImportedGroup, PaneLayout,
    RequestGroup, RequestSession, ResponseToken, WorkspacePreferences,
};
use crate::preferences::{
    apply_new_request_defaults, default_preferences, resolve_new_request_defaults,
    valid_preferences,
};
use crate::request::{is_method, js_trim};
use crate::response_token_cache::ResponseTokenCache;
use crate::response_tokens::TokenSources;
use crate::session::{create_session, has_draft};
use crate::workspace::{decode_workspace, encode_workspace};

/// How long a deletion can be undone.
pub const UNDO_WINDOW: Duration = Duration::from_millis(10_000);
/// Closed tabs kept for reopening.
const CLOSED_TAB_LIMIT: usize = 50;
/// Files larger than this are not imported.
pub const IMPORT_LIMIT_BYTES: u64 = 16 * 1024 * 1024;
pub const IMPORT_TOO_LARGE: &str = "The file exceeds the 16 MiB import limit.";

/// The length of `text` as a JS string counts it: in UTF-16 units.
fn js_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// One deleted request.
#[derive(Debug, Clone, PartialEq)]
pub struct DeletedRequest {
    pub session: RequestSession,
    pub index: usize,
    /// None when the request was not open as a tab.
    pub open_index: Option<usize>,
    /// True when deleting it emptied the workspace.
    pub last: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Deletion {
    Requests(Vec<DeletedRequest>),
    Group {
        group: Box<RequestGroup>,
        index: usize,
        /// Groups and requests promoted to the parent of the deleted group.
        child_group_ids: Vec<u64>,
        session_ids: Vec<u64>,
    },
}

impl Deletion {
    /// The status bar label.
    pub fn label(&self) -> String {
        match self {
            Deletion::Group { group, .. } => format!("DELETED GROUP {}", group.name),
            Deletion::Requests(items) => {
                let count = items.len();
                format!(
                    "DELETED {count} {}",
                    if count == 1 { "REQUEST" } else { "REQUESTS" }
                )
            }
        }
    }
}

/// What a Browser delete of a request row asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteRequest {
    /// A target is sending; nothing happens.
    Blocked,
    /// Ask the user first, then call `remove_requests` with these ids.
    Confirm(Vec<u64>),
    /// The request was deleted.
    Deleted,
}

/// The changes the group settings dialog saves. `None` leaves a field alone;
/// `Some(None)` clears an optional field.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupSettingsChanges {
    pub name: Option<String>,
    pub local_auth: Option<Option<AuthorizationConfig>>,
    pub local_definitions: Option<Definitions>,
    pub parent_id: Option<Option<u64>>,
    /// Sets the default method and URL together.
    pub new_request_defaults: Option<(Option<String>, Option<String>)>,
    pub environments: Option<Option<Vec<Environment>>>,
    pub response_tokens: Option<Option<Vec<ResponseToken>>>,
}

/// The import notice: a message and the skipped-item details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportNotice {
    pub message: String,
    pub details: String,
}

pub fn import_failed_notice(error: &str) -> String {
    format!("IMPORT FAILED: {error}")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub sessions: Vec<RequestSession>,
    pub groups: Vec<RequestGroup>,
    /// Ids of requests open as tabs, in tab order. The Browser tree owns the requests.
    pub open_ids: Vec<u64>,
    /// None when no tab is open. The stored value: see `shown_active_id`.
    pub active_id: Option<u64>,
    /// Workspace-global token definitions, shared across all requests.
    pub global_definitions: Definitions,
    /// Workspace-global response tokens.
    pub global_response_tokens: Vec<ResponseToken>,
    pub preferences: WorkspacePreferences,
    /// Values read from responses. Saved by the store in its own file.
    pub response_cache: ResponseTokenCache,
    /// Selected Browser rows and the range anchor. Not saved.
    pub selected_ids: Vec<u64>,
    pub selection_anchor_id: Option<u64>,
    /// The Browser and tab bar show only this group and its descendants. Not saved.
    focused_group_id: Option<u64>,
    /// The active tab before focus started.
    focus_return_id: Option<u64>,
    /// Environments confirmed for sending since they became active.
    confirmed_environments: HashSet<u64>,
    /// Recently closed tabs, most recent last. Not saved.
    closed_ids: Vec<u64>,
    last_deletion: Option<Deletion>,
    deletion_serial: u64,
    released: Vec<String>,
    /// Nesting of public calls; the focus rules run when the outermost ends.
    depth: u32,
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace::new()
    }
}

impl Workspace {
    /// One blank request, open and active.
    pub fn new() -> Self {
        let session = create_session(None);
        let id = session.id;
        Workspace {
            sessions: vec![session],
            groups: Vec::new(),
            open_ids: vec![id],
            active_id: Some(id),
            global_definitions: Definitions::new(),
            global_response_tokens: Vec::new(),
            preferences: default_preferences(),
            response_cache: ResponseTokenCache::default(),
            selected_ids: Vec::new(),
            selection_anchor_id: None,
            focused_group_id: None,
            focus_return_id: None,
            confirmed_environments: HashSet::new(),
            closed_ids: Vec::new(),
            last_deletion: None,
            deletion_serial: 0,
            released: Vec::new(),
            depth: 0,
        }
    }

    /// Tokens, response tokens, and cached values at `now_ms`.
    pub fn token_sources(&self, now_ms: f64) -> TokenSources<'_> {
        TokenSources::new(
            &self.groups,
            &self.global_definitions,
            &self.global_response_tokens,
            &self.sessions,
            &self.response_cache,
        )
        .at(now_ms)
    }

    /// Every response token: groups first, then global.
    pub fn all_response_tokens(&self) -> impl Iterator<Item = &ResponseToken> {
        self.groups
            .iter()
            .flat_map(|group| group.response_tokens.iter().flatten())
            .chain(self.global_response_tokens.iter())
    }

    /// True when a group or global response token reads from request `id`.
    pub fn is_token_source(&self, id: u64) -> bool {
        let reads = |tokens: &[ResponseToken]| tokens.iter().any(|t| t.request_id == id);
        reads(&self.global_response_tokens)
            || self
                .groups
                .iter()
                .any(|group| group.response_tokens.as_deref().is_some_and(reads))
    }

    /// Recompute `stale` for one request. Returns it.
    pub fn refresh_stale(&mut self, id: u64) -> bool {
        let Some(session) = self.session(id) else {
            return false;
        };
        let stale =
            crate::runner::session_is_stale(session, &self.token_sources(crate::history::now_ms()));
        if let Some(session) = self.session_mut(id) {
            session.stale = stale;
        }
        stale
    }

    /// Recompute `stale` for every request from one set of token sources.
    pub fn refresh_all_stale(&mut self) {
        let flags: Vec<(u64, bool)> = {
            let sources = self.token_sources(crate::history::now_ms());
            self.sessions
                .iter()
                .map(|session| {
                    (
                        session.id,
                        crate::runner::session_is_stale(session, &sources),
                    )
                })
                .collect()
        };
        for (id, stale) in flags {
            if let Some(session) = self.session_mut(id) {
                session.stale = stale;
            }
        }
    }

    // ── Persistence ─────────────────────────────────────────────────────────

    pub fn encode(&self) -> String {
        encode_workspace(
            &self.sessions,
            self.active_id,
            &self.groups,
            &self.global_definitions,
            &self.preferences,
            &self.open_ids,
            &self.global_response_tokens,
        )
    }

    /// Changes when a save is due. Elapsed time and the response scroll
    /// offset are not part of it: a running request or a scroll must not
    /// cause disk writes. The next real change or quit saves the latest
    /// offset.
    pub fn autosave_key(&self) -> String {
        let sessions: Vec<RequestSession> = self
            .sessions
            .iter()
            .map(|session| {
                let mut session = session.clone();
                session.view.response_scroll = 0.0;
                session
            })
            .collect();
        encode_workspace(
            &sessions,
            self.active_id,
            &self.groups,
            &self.global_definitions,
            &self.preferences,
            &self.open_ids,
            &self.global_response_tokens,
        )
    }

    pub fn decode(content: &str) -> Result<Workspace, String> {
        let mut workspace = Workspace::new();
        workspace.restore(content)?;
        Ok(workspace)
    }

    /// Replace the saved state with a decoded snapshot.
    pub fn restore(&mut self, content: &str) -> Result<(), String> {
        let restored = decode_workspace(content)?;
        self.batch(|ws| {
            ws.sessions = restored.sessions;
            ws.groups = restored.groups;
            ws.open_ids = restored.open_ids;
            ws.active_id = restored.active_id;
            ws.global_definitions = restored.global_definitions;
            ws.global_response_tokens = restored.global_response_tokens;
            ws.preferences = restored.preferences;
        });
        Ok(())
    }

    /// One blank request, no groups or tokens, default preferences.
    pub fn reset(&mut self) {
        self.batch(|ws| {
            let session = create_session(None);
            ws.open_ids = vec![session.id];
            ws.active_id = Some(session.id);
            ws.sessions = vec![session];
            ws.groups.clear();
            ws.global_definitions = Definitions::new();
            ws.global_response_tokens.clear();
            ws.preferences = default_preferences();
        });
    }

    /// Response bodies dropped since the last call, for the engine to release.
    pub fn take_released_bodies(&mut self) -> Vec<String> {
        std::mem::take(&mut self.released)
    }

    // ── Lookups ─────────────────────────────────────────────────────────────

    pub fn session(&self, id: u64) -> Option<&RequestSession> {
        self.sessions.iter().find(|session| session.id == id)
    }

    pub fn session_mut(&mut self, id: u64) -> Option<&mut RequestSession> {
        self.sessions.iter_mut().find(|session| session.id == id)
    }

    pub fn group(&self, id: u64) -> Option<&RequestGroup> {
        self.groups.iter().find(|group| group.id == id)
    }

    fn group_mut(&mut self, id: u64) -> Option<&mut RequestGroup> {
        self.groups.iter_mut().find(|group| group.id == id)
    }

    fn has_session(&self, id: u64) -> bool {
        self.sessions.iter().any(|session| session.id == id)
    }

    // ── Focus ───────────────────────────────────────────────────────────────

    pub fn focused_group_id(&self) -> Option<u64> {
        self.focused_group_id
    }

    /// The groups the focus shows. None without focus.
    pub fn focus_ids(&self) -> Option<HashSet<u64>> {
        self.focused_group_id
            .map(|id| group_subtree(&self.groups, id))
    }

    fn in_focus_of(&self, ids: Option<&HashSet<u64>>, id: u64) -> bool {
        let Some(ids) = ids else { return true };
        self.session(id)
            .and_then(|session| session.group_id)
            .is_some_and(|group_id| ids.contains(&group_id))
    }

    /// True when the focus shows the request (always, without focus).
    pub fn in_focus(&self, id: u64) -> bool {
        self.in_focus_of(self.focus_ids().as_ref(), id)
    }

    /// Open tabs that the focus shows, in tab order.
    pub fn visible_ids(&self) -> Vec<u64> {
        let focus = self.focus_ids();
        self.open_ids
            .iter()
            .copied()
            .filter(|id| self.in_focus_of(focus.as_ref(), *id))
            .collect()
    }

    /// The shown active tab. None when the focus hides the active tab.
    pub fn shown_active_id(&self) -> Option<u64> {
        self.active_id.filter(|id| self.in_focus(*id))
    }

    /// The shown active request.
    pub fn active(&self) -> Option<&RequestSession> {
        self.shown_active_id().and_then(|id| self.session(id))
    }

    /// The requests of the shown tabs, in tab order.
    pub fn open_sessions(&self) -> Vec<&RequestSession> {
        self.visible_ids()
            .into_iter()
            .filter_map(|id| self.session(id))
            .collect()
    }

    /// The root group of the shown active request.
    pub fn active_root(&self) -> Option<&RequestGroup> {
        self.active()
            .and_then(|session| root_group(session.group_id, &self.groups))
    }

    /// The active environment of the shown active request.
    pub fn active_environment(&self) -> Option<&Environment> {
        active_environment(self.active_root())
    }

    pub fn focus_group(&mut self, id: u64) {
        self.batch(|ws| {
            if ws.focused_group_id.is_none() {
                ws.focus_return_id = ws.active_id;
            }
            ws.focused_group_id = Some(id);
            let focus = ws.focus_ids();
            let selected = ws
                .selected_ids
                .iter()
                .copied()
                .filter(|id| ws.in_focus_of(focus.as_ref(), *id))
                .collect();
            ws.update_selection(selected, None);
        });
    }

    /// Show all groups and tabs again. `restore` selects the tab active
    /// before focus.
    pub fn unfocus(&mut self, restore: bool) {
        self.batch(|ws| ws.unfocus_now(restore));
    }

    fn unfocus_now(&mut self, restore: bool) {
        if self.focused_group_id.is_none() {
            return;
        }
        self.focused_group_id = None;
        if restore
            && let Some(id) = self.focus_return_id
            && self.open_ids.contains(&id)
        {
            self.open_request_now(id);
            self.update_selection(vec![id], Some(id));
        }
        self.focus_return_id = None;
    }

    /// Run `change`, then the focus rules once the outermost call ends.
    fn batch<R>(&mut self, change: impl FnOnce(&mut Self) -> R) -> R {
        self.depth += 1;
        let result = change(self);
        self.depth -= 1;
        if self.depth == 0 {
            self.depth += 1;
            self.settle();
            self.depth -= 1;
        }
        result
    }

    /// The focus rules the app keeps after every change.
    fn settle(&mut self) {
        // A deleted focused group ends the focus.
        if self.focus_ids().is_some_and(|ids| ids.is_empty()) {
            self.unfocus_now(true);
        }
        // In focus, keep a shown tab active: the nearest one after the
        // hidden active tab.
        if self.focused_group_id.is_none() || self.shown_active_id().is_some() {
            return;
        }
        let visible = self.visible_ids();
        let Some(&last) = visible.last() else { return };
        // Like `indexOf`: -1 when absent.
        let position = |id: Option<u64>| {
            id.and_then(|id| self.open_ids.iter().position(|open| *open == id))
                .map_or(-1, |index| index as i64)
        };
        let at = position(self.active_id);
        let next = visible
            .iter()
            .copied()
            .find(|id| position(Some(*id)) > at)
            .unwrap_or(last);
        self.open_request_now(next);
    }

    // ── Selection ───────────────────────────────────────────────────────────

    pub fn update_selection(&mut self, ids: Vec<u64>, anchor_id: Option<u64>) {
        self.selected_ids = ids;
        self.selection_anchor_id = anchor_id;
    }

    /// The rows a Browser menu acts on: the selection when it holds the row
    /// and more than one request, else the row.
    pub fn menu_targets(&self, session_id: u64) -> Vec<u64> {
        if self.selected_ids.len() > 1 && self.selected_ids.contains(&session_id) {
            self.selected_ids.clone()
        } else {
            vec![session_id]
        }
    }

    // ── Tabs ────────────────────────────────────────────────────────────────

    /// Open a request as a tab, if not open already, and make it active.
    pub fn open_request(&mut self, id: u64) {
        self.batch(|ws| ws.open_request_now(id));
    }

    fn open_request_now(&mut self, id: u64) {
        if !self.has_session(id) {
            return;
        }
        if !self.open_ids.contains(&id) {
            self.open_ids.push(id);
        }
        self.active_id = Some(id);
    }

    fn remember_closed(&mut self, ids: &[u64]) {
        self.closed_ids.extend_from_slice(ids);
        if self.closed_ids.len() > CLOSED_TAB_LIMIT {
            let excess = self.closed_ids.len() - CLOSED_TAB_LIMIT;
            self.closed_ids.drain(..excess);
        }
    }

    /// Open the most recently closed tab that still exists.
    pub fn reopen_closed_tab(&mut self) -> Option<u64> {
        self.batch(|ws| {
            while let Some(id) = ws.closed_ids.pop() {
                if ws.has_session(id) && !ws.open_ids.contains(&id) {
                    ws.open_request_now(id);
                    return Some(id);
                }
            }
            None
        })
    }

    /// Close a tab. The request stays in the Browser tree.
    pub fn close_tab(&mut self, id: u64) {
        self.batch(|ws| ws.close_tab_now(id));
    }

    fn close_tab_now(&mut self, id: u64) {
        let Some(index) = self.open_ids.iter().position(|open| *open == id) else {
            return;
        };
        self.remember_closed(&[id]);
        self.open_ids.remove(index);
        if self.active_id == Some(id) {
            self.active_id = self
                .open_ids
                .get(index.min(self.open_ids.len().saturating_sub(1)))
                .copied();
        }
    }

    /// Close several tabs. The active tab stays active when it stays open;
    /// otherwise the next open tab at its position becomes active.
    pub fn close_tabs(&mut self, ids: &[u64]) {
        self.batch(|ws| {
            let closing: HashSet<u64> = ids.iter().copied().collect();
            let before = std::mem::take(&mut ws.open_ids);
            let remaining: Vec<u64> = before
                .iter()
                .copied()
                .filter(|id| !closing.contains(id))
                .collect();
            if remaining.len() == before.len() {
                ws.open_ids = before;
                return;
            }
            let closed: Vec<u64> = before
                .iter()
                .copied()
                .filter(|id| closing.contains(id))
                .collect();
            ws.remember_closed(&closed);
            ws.open_ids = remaining;
            let Some(active) = ws.active_id.filter(|id| closing.contains(id)) else {
                return;
            };
            let at = before.iter().position(|id| *id == active).unwrap_or(0);
            let position = before[..at]
                .iter()
                .filter(|id| !closing.contains(id))
                .count();
            ws.active_id = ws
                .open_ids
                .get(position.min(ws.open_ids.len().saturating_sub(1)))
                .copied();
        });
    }

    /// Open requests as tabs before `before_id`, or at the end. Open
    /// requests move there. The first request becomes active.
    pub fn open_requests(&mut self, ids: &[u64], before_id: Option<u64>) {
        self.batch(|ws| ws.open_requests_now(ids, before_id));
    }

    fn open_requests_now(&mut self, ids: &[u64], before_id: Option<u64>) {
        let mut known: Vec<u64> = Vec::new();
        for id in ids {
            if !known.contains(id) && self.has_session(*id) {
                known.push(*id);
            }
        }
        let Some(&first) = known.first() else { return };
        let mut remaining: Vec<u64> = self
            .open_ids
            .iter()
            .copied()
            .filter(|id| !known.contains(id))
            .collect();
        let index = before_id
            .and_then(|before| remaining.iter().position(|id| *id == before))
            .unwrap_or(remaining.len());
        remaining.splice(index..index, known);
        self.open_ids = remaining;
        self.active_id = Some(first);
    }

    // ── Deletion and undo ───────────────────────────────────────────────────

    /// The last deletion, kept so it can be undone.
    pub fn last_deletion(&self) -> Option<&Deletion> {
        self.last_deletion.as_ref()
    }

    /// Changes with each recorded deletion. Start the undo timer when it
    /// changes, and pass the value to `expire_deletion` when it fires.
    pub fn deletion_serial(&self) -> u64 {
        self.deletion_serial
    }

    /// The status bar label of the last deletion; empty when none.
    pub fn deletion_label(&self) -> String {
        self.last_deletion
            .as_ref()
            .map(Deletion::label)
            .unwrap_or_default()
    }

    /// Forget the last deletion. Its response bodies are released.
    pub fn discard_deletion(&mut self) {
        if let Some(Deletion::Requests(items)) = self.last_deletion.take() {
            self.released.extend(
                items
                    .into_iter()
                    .filter_map(|item| item.session.response.and_then(|r| r.body_id)),
            );
        }
    }

    /// The undo window of the deletion with `serial` ended. A later deletion
    /// keeps its own window.
    pub fn expire_deletion(&mut self, serial: u64) {
        if serial == self.deletion_serial {
            self.discard_deletion();
        }
    }

    /// `batched` joins request deletions made in the same action, such as a
    /// multi-select delete, into one deletion.
    fn record_deletion(&mut self, next: Deletion, batched: bool) {
        match (&mut self.last_deletion, next) {
            (Some(Deletion::Requests(current)), Deletion::Requests(items)) if batched => {
                current.extend(items);
            }
            (_, next) => {
                self.discard_deletion();
                self.last_deletion = Some(next);
            }
        }
        self.deletion_serial += 1;
    }

    /// Restore the last deleted requests or group.
    pub fn undo_delete(&mut self) -> bool {
        self.batch(|ws| ws.undo_delete_now())
    }

    fn undo_delete_now(&mut self) -> bool {
        let Some(deletion) = self.last_deletion.take() else {
            return false;
        };
        match deletion {
            Deletion::Requests(items) => {
                // A request created because the workspace became empty goes away again.
                let placeholder = (self.sessions.len() == 1 && !has_draft(&self.sessions[0]))
                    .then(|| self.sessions[0].id);
                if let Some(placeholder) = placeholder
                    && items.iter().any(|item| item.last)
                {
                    self.sessions.clear();
                    self.open_ids.retain(|id| *id != placeholder);
                }
                let group_ids: HashSet<u64> = self.groups.iter().map(|group| group.id).collect();
                let first = items
                    .iter()
                    .find(|item| item.open_index.is_some())
                    .or(items.first())
                    .map(|item| item.session.id);
                // Each index is from the moment of its deletion, so undo in reverse.
                let items: Vec<DeletedRequest> = items.into_iter().rev().collect();
                let mut reopen = Vec::new();
                for mut item in items {
                    if item
                        .session
                        .group_id
                        .is_some_and(|id| !group_ids.contains(&id))
                    {
                        item.session.group_id = None;
                    }
                    // Its wait ended when it was deleted.
                    item.session.waiting_on = None;
                    if let Some(open_index) = item.open_index {
                        reopen.push((open_index, item.session.id));
                    }
                    let at = item.index.min(self.sessions.len());
                    self.sessions.insert(at, item.session);
                }
                for (open_index, id) in reopen {
                    let at = open_index.min(self.open_ids.len());
                    self.open_ids.insert(at, id);
                }
                if let Some(first) = first {
                    self.open_request_now(first);
                }
            }
            Deletion::Group {
                mut group,
                index,
                child_group_ids,
                session_ids,
            } => {
                if let Some(parent) = group.parent_id
                    && !self.groups.iter().any(|g| g.id == parent)
                {
                    group.parent_id = None;
                }
                let parent_id = group.parent_id;
                let group_id = group.id;
                let at = index.min(self.groups.len());
                self.groups.insert(at, *group);
                for candidate in &mut self.groups {
                    if child_group_ids.contains(&candidate.id) && candidate.parent_id == parent_id {
                        candidate.parent_id = Some(group_id);
                    }
                }
                for session in &mut self.sessions {
                    if session_ids.contains(&session.id) && session.group_id == parent_id {
                        session.group_id = Some(group_id);
                    }
                }
            }
        }
        true
    }

    /// Remove a request from the workspace. The workspace always keeps one
    /// request.
    pub fn delete_request(&mut self, id: u64) {
        self.batch(|ws| ws.delete_request_now(id, false));
    }

    /// Remove several requests as one undoable deletion.
    pub fn delete_requests(&mut self, ids: &[u64]) {
        self.batch(|ws| {
            for (index, id) in ids.iter().enumerate() {
                ws.delete_request_now(*id, index > 0);
            }
        });
    }

    fn delete_request_now(&mut self, id: u64, batched: bool) {
        let Some(index) = self.sessions.iter().position(|session| session.id == id) else {
            return;
        };
        if self.sessions[index].running() {
            return;
        }
        let open_index = self.open_ids.iter().position(|open| *open == id);
        self.close_tab_now(id);
        let session = self.sessions.remove(index);
        let last = self.sessions.is_empty();
        self.record_deletion(
            Deletion::Requests(vec![DeletedRequest {
                session,
                index,
                open_index,
                last,
            }]),
            batched,
        );
        if self.sessions.is_empty() {
            let session = create_session(None);
            let id = session.id;
            self.sessions.push(session);
            self.open_request_now(id);
        }
    }

    // ── Groups ──────────────────────────────────────────────────────────────

    pub fn add_group(&mut self, name: &str, parent_id: Option<u64>) -> u64 {
        self.batch(|ws| ws.add_group_now(name, parent_id))
    }

    fn add_group_now(&mut self, name: &str, parent_id: Option<u64>) -> u64 {
        let group = create_group(name, parent_id);
        let id = group.id;
        self.groups.push(group);
        id
    }

    /// Add an imported group tree under `parent_id`. Returns the new root
    /// group id and the first new request id.
    pub fn import_group(
        &mut self,
        root: &ImportedGroup,
        parent_id: Option<u64>,
    ) -> (u64, Option<u64>) {
        self.batch(|ws| {
            let mut first = None;
            let id = ws.add_imported(root, parent_id, &mut first);
            (id, first)
        })
    }

    fn add_imported(
        &mut self,
        source: &ImportedGroup,
        parent: Option<u64>,
        first: &mut Option<u64>,
    ) -> u64 {
        let mut group = create_group(&source.name, parent);
        let group_id = group.id;
        // Local names cannot start with "_"; the saved workspace caps sizes.
        let definitions: Definitions = source
            .definitions
            .iter()
            .flatten()
            .filter(|(name, value)| {
                !name.is_empty()
                    && !name.starts_with('_')
                    && js_len(name) <= 256
                    && js_len(value) <= 65536
            })
            .take(500)
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        if !definitions.is_empty() {
            group.local_definitions = Some(definitions);
        }
        if let Some(auth) = &source.auth {
            group.local_auth = Some(auth.clone());
        }
        self.groups.push(group);
        for draft in &source.requests {
            let mut session = create_session(Some(draft));
            session.group_id = Some(group_id);
            first.get_or_insert(session.id);
            self.sessions.push(session);
        }
        for child in &source.groups {
            self.add_imported(child, Some(group_id), first);
        }
        group_id
    }

    pub fn rename_group(&mut self, id: u64, name: &str) {
        self.set_group_name(id, name);
    }

    pub fn toggle_group(&mut self, id: u64) {
        if let Some(group) = self.group_mut(id) {
            group.collapsed = !group.collapsed;
        }
    }

    pub fn move_request(&mut self, session_id: u64, group_id: Option<u64>) {
        self.batch(|ws| {
            if group_id.is_some_and(|id| ws.group(id).is_none()) {
                return;
            }
            if let Some(session) = ws.session_mut(session_id) {
                session.group_id = group_id;
            }
        });
    }

    pub fn move_requests(
        &mut self,
        session_ids: &[u64],
        group_id: Option<u64>,
        before_session_id: Option<u64>,
    ) {
        self.batch(|ws| ws.move_requests_now(session_ids, group_id, before_session_id));
    }

    fn move_requests_now(
        &mut self,
        session_ids: &[u64],
        group_id: Option<u64>,
        before_session_id: Option<u64>,
    ) {
        if group_id.is_some_and(|id| self.group(id).is_none()) {
            return;
        }
        let ids: HashSet<u64> = session_ids.iter().copied().collect();
        if !self
            .sessions
            .iter()
            .any(|session| ids.contains(&session.id))
        {
            return;
        }
        let sessions = std::mem::take(&mut self.sessions);
        self.sessions = sessions_after_move(sessions, session_ids, group_id, before_session_id);
        for session in &mut self.sessions {
            if ids.contains(&session.id) {
                session.group_id = group_id;
            }
        }
    }

    /// Reorder, nest, or un-nest a group. Rejects cycles and unknown ids.
    pub fn move_group(
        &mut self,
        group_id: u64,
        parent_id: Option<u64>,
        before_group_id: Option<u64>,
    ) {
        self.batch(|ws| ws.move_group_now(group_id, parent_id, before_group_id));
    }

    fn move_group_now(
        &mut self,
        group_id: u64,
        parent_id: Option<u64>,
        before_group_id: Option<u64>,
    ) {
        let Some(mut next) = groups_after_move(&self.groups, group_id, parent_id, before_group_id)
        else {
            return;
        };
        if let Some(group) = next.iter_mut().find(|group| group.id == group_id) {
            group.parent_id = parent_id;
        }
        self.groups = next;
    }

    /// Delete a group. Its child groups and requests move to its parent.
    pub fn delete_group(&mut self, id: u64) {
        self.batch(|ws| {
            let Some(index) = ws.groups.iter().position(|group| group.id == id) else {
                return;
            };
            let deletion = Deletion::Group {
                group: Box::new(ws.groups[index].clone()),
                index,
                child_group_ids: ws
                    .groups
                    .iter()
                    .filter(|group| group.parent_id == Some(id))
                    .map(|group| group.id)
                    .collect(),
                session_ids: ws
                    .sessions
                    .iter()
                    .filter(|session| session.group_id == Some(id))
                    .map(|session| session.id)
                    .collect(),
            };
            ws.record_deletion(deletion, false);
            delete_group_and_promote_contents(&mut ws.groups, &mut ws.sessions, id);
        });
    }

    // ── Narrow state setters ────────────────────────────────────────────────

    /// Set the local auth config on a request draft. None reverts to
    /// "inherit from group".
    pub fn set_request_local_auth(
        &mut self,
        session_id: u64,
        local_auth: Option<AuthorizationConfig>,
    ) {
        if let Some(session) = self.session_mut(session_id) {
            session.draft.local_auth = local_auth;
        }
    }

    /// Set the name of a group. Trims whitespace; no-op if blank, longer
    /// than 80, or not found.
    pub fn set_group_name(&mut self, group_id: u64, name: &str) {
        let trimmed = js_trim(name);
        if trimmed.is_empty() || js_len(trimmed) > 80 {
            return;
        }
        if let Some(group) = self.group_mut(group_id) {
            group.name = trimmed.to_string();
        }
    }

    /// Set the local auth override for a group. None reverts to inherit
    /// from parent.
    pub fn set_group_local_auth(&mut self, group_id: u64, local_auth: Option<AuthorizationConfig>) {
        if let Some(group) = self.group_mut(group_id) {
            group.local_auth = local_auth;
        }
    }

    /// Set (or clear) the token definitions for a group.
    pub fn set_group_local_definitions(
        &mut self,
        group_id: u64,
        local_definitions: Option<Definitions>,
    ) {
        if let Some(group) = self.group_mut(group_id) {
            group.local_definitions = local_definitions;
        }
    }

    /// Replace the workspace-wide global definitions map.
    pub fn set_global_response_tokens(&mut self, tokens: Vec<ResponseToken>) {
        self.global_response_tokens = tokens;
    }

    pub fn set_global_definitions(&mut self, definitions: Definitions) {
        self.global_definitions = definitions;
    }

    /// Apply valid preferences. False when `next` is not valid.
    pub fn set_preferences(&mut self, next: WorkspacePreferences) -> bool {
        let valid = serde_json::to_value(&next).is_ok_and(|value| valid_preferences(&value));
        if valid {
            self.preferences = next;
        }
        valid
    }

    /// Replace a root group's environments. Keeps a still-valid active one.
    pub fn set_group_environments(
        &mut self,
        group_id: u64,
        environments: Option<Vec<Environment>>,
    ) {
        let Some(group) = self.group_mut(group_id) else {
            return;
        };
        group.environments = environments.filter(|list| !list.is_empty());
        let active = group.active_environment_id;
        if !group
            .environments
            .iter()
            .flatten()
            .any(|environment| Some(environment.id) == active)
        {
            group.active_environment_id = None;
        }
    }

    /// Switch a root group's environment. None uses the base tokens only.
    pub fn set_active_environment(&mut self, group_id: u64, environment_id: Option<u64>) {
        let Some(group) = self.group_mut(group_id) else {
            return;
        };
        if group.parent_id.is_some() {
            return;
        }
        if let Some(id) = environment_id
            && !group
                .environments
                .iter()
                .flatten()
                .any(|environment| environment.id == id)
        {
            return;
        }
        group.active_environment_id = environment_id;
    }

    pub fn set_group_parent(&mut self, group_id: u64, parent_id: Option<u64>) {
        self.batch(|ws| {
            if ws
                .group(group_id)
                .is_some_and(|group| group.parent_id != parent_id)
            {
                ws.move_group_now(group_id, parent_id, None);
            }
        });
    }

    pub fn set_group_new_request_defaults(
        &mut self,
        group_id: u64,
        default_method: Option<String>,
        default_url: Option<String>,
    ) {
        if default_method
            .as_deref()
            .is_some_and(|method| !is_method(method))
        {
            return;
        }
        if default_url
            .as_deref()
            .is_some_and(|url| js_len(url) > 65536)
        {
            return;
        }
        if let Some(group) = self.group_mut(group_id) {
            group.default_method = default_method;
            group.default_url = default_url;
        }
    }

    /// Save the group settings dialog, in the order the app applies them.
    pub fn save_group_settings(&mut self, group_id: u64, changes: GroupSettingsChanges) {
        self.batch(|ws| {
            if let Some(name) = &changes.name {
                ws.set_group_name(group_id, name);
            }
            if let Some(local_auth) = changes.local_auth {
                ws.set_group_local_auth(group_id, local_auth);
            }
            if let Some(local_definitions) = changes.local_definitions {
                ws.set_group_local_definitions(group_id, Some(local_definitions));
            }
            if let Some(tokens) = changes.response_tokens
                && let Some(group) = ws.group_mut(group_id)
            {
                group.response_tokens = tokens;
            }
            if let Some(environments) = changes.environments {
                ws.set_group_environments(group_id, environments);
            }
            if let Some(parent_id) = changes.parent_id {
                ws.set_group_parent(group_id, parent_id);
            }
            if let Some((method, url)) = changes.new_request_defaults {
                ws.set_group_new_request_defaults(group_id, method, url);
            }
        });
    }

    // ── App actions ─────────────────────────────────────────────────────────

    /// Open a request and select it. A request outside the focus ends the
    /// focus.
    pub fn select(&mut self, id: u64) {
        self.batch(|ws| ws.select_now(id));
    }

    fn select_now(&mut self, id: u64) {
        if !self.in_focus(id) {
            self.unfocus_now(false);
        }
        self.open_request_now(id);
        self.update_selection(vec![id], Some(id));
    }

    /// Select the next (1) or previous (-1) shown tab, wrapping around.
    pub fn cycle_tab(&mut self, step: i64) {
        self.batch(|ws| {
            let ids = ws.visible_ids();
            if ids.is_empty() {
                return;
            }
            let index = ws
                .shown_active_id()
                .and_then(|active| ids.iter().position(|id| *id == active))
                .map_or(-1, |index| index as i64);
            let len = ids.len() as i64;
            let next = (index + step + len).rem_euclid(len) as usize;
            ws.select_now(ids[next]);
        });
    }

    /// Reopen the last closed tab and select it. Returns its id.
    pub fn reopen_tab(&mut self) -> Option<u64> {
        self.batch(|ws| {
            let id = ws.reopen_closed_tab()?;
            if !ws.in_focus(id) {
                ws.unfocus_now(false);
            }
            ws.update_selection(vec![id], Some(id));
            Some(id)
        })
    }

    /// Undo the last deletion and select the active tab.
    pub fn undo_deletion(&mut self) -> bool {
        self.batch(|ws| {
            if !ws.undo_delete_now() {
                return false;
            }
            if ws.active_id.is_some_and(|id| !ws.in_focus(id)) {
                ws.unfocus_now(false);
            }
            if let Some(id) = ws.shown_active_id() {
                ws.update_selection(vec![id], Some(id));
            }
            true
        })
    }

    /// Place requests in the tab bar (drag or Alt+Arrow) and select them.
    pub fn place_tabs(&mut self, ids: &[u64], before_id: Option<u64>) {
        self.batch(|ws| {
            ws.open_requests_now(ids, before_id);
            ws.update_selection(ids.to_vec(), ids.first().copied());
        });
    }

    pub fn close(&mut self, id: u64) {
        self.close_tab(id);
    }

    pub fn close_many(&mut self, ids: &[u64]) {
        self.close_tabs(ids);
    }

    /// Collapse every group in the Browser.
    pub fn collapse_all_groups(&mut self) {
        for group in &mut self.groups {
            group.collapsed = true;
        }
    }

    /// Expand every group from `group_id` up to the root.
    pub fn expand_ancestors(&mut self, group_id: Option<u64>) {
        let mut cursor = group_id;
        let mut seen = HashSet::new();
        while let Some(id) = cursor {
            if !seen.insert(id) {
                break;
            }
            let Some(group) = self.group_mut(id) else {
                break;
            };
            group.collapsed = false;
            cursor = group.parent_id;
        }
    }

    /// A Browser group from its header form, with the requests to move in.
    pub fn create_group(&mut self, name: &str, parent_id: Option<u64>, session_ids: &[u64]) -> u64 {
        self.batch(|ws| {
            let id = ws.add_group_now(name, parent_id);
            if !session_ids.is_empty() {
                ws.move_requests_now(session_ids, Some(id), None);
            }
            id
        })
    }

    /// A new top-level group: "New group", "New group 2", ... The UI shows
    /// the Browser and opens the group settings.
    pub fn new_group(&mut self) -> u64 {
        self.batch(|ws| {
            let names: HashSet<&str> = ws.groups.iter().map(|group| group.name.as_str()).collect();
            let mut name = "New group".to_string();
            let mut n = 2;
            while names.contains(name.as_str()) {
                name = format!("New group {n}");
                n += 1;
            }
            ws.add_group_now(&name, None)
        })
    }

    /// A new request with the new-request defaults, opened and selected.
    /// `in_group_id` None uses the active request's group; `Some(None)` is
    /// ungrouped. In focus, a group outside the focus becomes the focused
    /// group.
    pub fn create(&mut self, in_group_id: Option<Option<u64>>) -> u64 {
        self.batch(|ws| {
            let mut group_id = match in_group_id {
                Some(group_id) => group_id,
                None => ws.active().and_then(|session| session.group_id),
            };
            if let Some(focus) = ws.focus_ids()
                && group_id.is_none_or(|id| !focus.contains(&id))
            {
                group_id = ws.focused_group_id;
            }
            let mut session = create_session(None);
            session.group_id = group_id;
            let defaults = resolve_new_request_defaults(&ws.groups, group_id, &ws.preferences);
            apply_new_request_defaults(&mut session, &defaults);
            let id = session.id;
            ws.sessions.push(session);
            ws.expand_ancestors(group_id);
            ws.select_now(id);
            id
        })
    }

    /// Copy a request (the shown active one when None) into its group, then
    /// open and select the copy.
    pub fn duplicate(&mut self, session_id: Option<u64>) -> Option<u64> {
        self.batch(|ws| {
            let source = match session_id {
                Some(id) => ws.session(id),
                None => ws.active(),
            }?;
            let mut session = create_session(Some(&source.draft));
            session.group_id = source.group_id;
            session.view = source.view.clone();
            session.view.response_scroll = 0.0;
            let id = session.id;
            let group_id = session.group_id;
            ws.sessions.push(session);
            ws.expand_ancestors(group_id);
            ws.select_now(id);
            Some(id)
        })
    }

    /// Show a request in the Browser: expand its groups and select its row.
    /// False for an unknown request. The UI shows the Browser and scrolls
    /// to and focuses the row.
    pub fn reveal(&mut self, id: u64) -> bool {
        self.batch(|ws| {
            let Some(group_id) = ws.session(id).map(|session| session.group_id) else {
                return false;
            };
            if !ws.in_focus(id) {
                ws.unfocus_now(false);
            }
            ws.expand_ancestors(group_id);
            ws.update_selection(vec![id], Some(id));
            true
        })
    }

    /// Delete a request and drop it from the selection.
    pub fn remove(&mut self, id: u64) {
        self.remove_requests(&[id]);
    }

    /// Delete requests as one undoable deletion and drop them from the
    /// selection.
    pub fn remove_requests(&mut self, ids: &[u64]) {
        self.batch(|ws| {
            for (index, id) in ids.iter().enumerate() {
                ws.delete_request_now(*id, index > 0);
                let selected = ws
                    .selected_ids
                    .iter()
                    .copied()
                    .filter(|s| s != id)
                    .collect();
                let anchor = ws.selection_anchor_id.filter(|anchor| anchor != id);
                ws.update_selection(selected, anchor);
            }
        });
    }

    /// Delete from a Browser row menu. Several targets, or a request with a
    /// draft when `confirm_close_drafts` is on, ask first.
    pub fn request_delete(&mut self, session_id: u64) -> DeleteRequest {
        let ids = self.menu_targets(session_id);
        if ids
            .iter()
            .any(|id| self.session(*id).is_some_and(|session| session.running()))
        {
            return DeleteRequest::Blocked;
        }
        let draft = self.session(session_id).is_some_and(has_draft);
        if ids.len() > 1 || (self.preferences.confirm_close_drafts && draft) {
            return DeleteRequest::Confirm(ids);
        }
        self.remove(session_id);
        DeleteRequest::Deleted
    }

    /// Import a parsed file as a new top-level group and select its first
    /// request. The UI shows the Browser and the notice.
    pub fn import(&mut self, result: &ImportResult) -> ImportNotice {
        self.batch(|ws| {
            let (group_id, first) = ws.import_group(&result.root, None);
            ws.expand_ancestors(Some(group_id));
            if let Some(first) = first {
                ws.select_now(first);
            }
            let count = count_requests(&result.root);
            let name = ws
                .group(group_id)
                .map(|group| group.name.to_uppercase())
                .unwrap_or_default();
            let notes = if result.skipped.is_empty() {
                String::new()
            } else {
                format!(" · {} NOTES", result.skipped.len())
            };
            ImportNotice {
                message: format!(
                    "IMPORTED {count} {} INTO {name}{notes}",
                    if count == 1 { "REQUEST" } else { "REQUESTS" }
                ),
                details: result.skipped.join("\n"),
            }
        })
    }

    // ── Environments and captures ───────────────────────────────────────────

    /// Captured values go to the request's environment, so they never mix.
    pub fn capture(&mut self, group_id: Option<u64>, values: &Definitions) {
        if apply_capture(group_id, &mut self.groups, values) == CaptureTarget::Global {
            self.global_definitions
                .extend(values.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }

    /// Switch a root group's environment. A protected environment asks
    /// again after each switch to it.
    pub fn switch_environment(&mut self, group_id: u64, environment_id: Option<u64>) {
        self.set_active_environment(group_id, environment_id);
        if let Some(id) = environment_id {
            self.confirmed_environments.remove(&id);
        }
    }

    /// The user confirmed sending to a protected environment.
    pub fn confirm_environment(&mut self, id: u64) {
        self.confirmed_environments.insert(id);
    }

    /// True when the environment of a request in `group_id` was confirmed
    /// since it became active.
    pub fn environment_confirmed(&self, group_id: Option<u64>) -> bool {
        request_environment(group_id, &self.groups)
            .is_some_and(|environment| self.confirmed_environments.contains(&environment.id))
    }

    // ── Preferences ─────────────────────────────────────────────────────────

    pub fn toggle_layout(&mut self) {
        let mut next = self.preferences.clone();
        next.pane_layout = match next.pane_layout {
            PaneLayout::Vertical => PaneLayout::Horizontal,
            PaneLayout::Horizontal => PaneLayout::Vertical,
        };
        self.set_preferences(next);
    }

    pub fn set_zoom(&mut self, zoom: f64) {
        let mut next = self.preferences.clone();
        next.zoom = zoom;
        self.set_preferences(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environments::create_environment;
    use crate::model::{ApiResponse, EnvironmentColor};
    use crate::request::create_draft;

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn bearer(token: &str) -> AuthorizationConfig {
        AuthorizationConfig::Bearer {
            token: token.into(),
        }
    }

    fn ids(ws: &Workspace) -> Vec<u64> {
        ws.sessions.iter().map(|session| session.id).collect()
    }

    fn group_ids(ws: &Workspace) -> Vec<u64> {
        ws.groups.iter().map(|group| group.id).collect()
    }

    /// Add `count` blank requests; returns every request id.
    fn add_sessions(ws: &mut Workspace, count: usize) -> Vec<u64> {
        for _ in 0..count {
            ws.sessions.push(create_session(None));
        }
        ids(ws)
    }

    /// Add `count` requests with a URL, each opened as a tab.
    fn add_open_sessions(ws: &mut Workspace, count: usize) -> Vec<u64> {
        for index in 0..count {
            let mut session = create_session(None);
            session.draft.url = format!("https://example.test/{index}");
            let id = session.id;
            ws.sessions.push(session);
            ws.open_request(id);
        }
        ids(ws)
    }

    fn response(body_id: Option<&str>) -> ApiResponse {
        ApiResponse {
            status: 200,
            status_text: "OK".into(),
            duration_ms: 1.0,
            headers: vec![],
            body: String::new(),
            size_bytes: 0,
            body_id: body_id.map(str::to_string),
            truncated: None,
            binary: None,
            final_url: None,
            redirect_count: None,
            timing: None,
        }
    }

    // ── moves ───────────────────────────────────────────────────────────────

    #[test]
    fn move_requests_moves_into_a_group_and_assigns_group_id() {
        let mut ws = Workspace::new();
        let all = add_sessions(&mut ws, 1);
        let (first, second) = (all[0], all[1]);
        let group = ws.add_group("A", None);
        ws.move_requests(&[first], Some(group), None);
        assert_eq!(ids(&ws), vec![second, first]);
        assert_eq!(ws.sessions[1].group_id, Some(group));
    }

    #[test]
    fn move_requests_ignores_an_unknown_group() {
        let mut ws = Workspace::new();
        let all = add_sessions(&mut ws, 1);
        ws.move_requests(&[all[0]], Some(999_999), None);
        assert_eq!(ws.sessions[0].group_id, None);
    }

    #[test]
    fn move_group_nests_un_nests_and_reorders() {
        let mut ws = Workspace::new();
        let a = ws.add_group("A", None);
        let b = ws.add_group("B", None);
        ws.move_group(b, Some(a), None);
        assert_eq!(ws.group(b).unwrap().parent_id, Some(a));
        ws.move_group(b, None, Some(a));
        assert_eq!(group_ids(&ws), vec![b, a]);
        assert_eq!(ws.groups[0].parent_id, None);
    }

    #[test]
    fn move_group_rejects_a_cycle() {
        let mut ws = Workspace::new();
        let a = ws.add_group("A", None);
        let b = ws.add_group("B", Some(a));
        ws.move_group(a, Some(b), None);
        assert_eq!(ws.group(a).unwrap().parent_id, None);
    }

    #[test]
    fn set_group_parent_keeps_order_when_the_parent_does_not_change() {
        let mut ws = Workspace::new();
        let a = ws.add_group("A", None);
        let b = ws.add_group("B", None);
        ws.set_group_parent(a, None);
        assert_eq!(group_ids(&ws), vec![a, b]);
    }

    #[test]
    fn set_group_parent_does_not_reparent_into_a_missing_group_or_descendant() {
        let mut ws = Workspace::new();
        let parent = ws.add_group("Parent", None);
        let child = ws.add_group("Child", Some(parent));
        ws.set_group_parent(parent, Some(child));
        ws.set_group_parent(parent, Some(999_999));
        assert_eq!(ws.group(parent).unwrap().parent_id, None);
    }

    #[test]
    fn open_requests_opens_closed_requests_before_a_tab_and_activates_the_first() {
        let mut ws = Workspace::new();
        let all = add_sessions(&mut ws, 2);
        let (first, second, third) = (all[0], all[1], all[2]);
        ws.open_requests(&[second], None);
        ws.open_requests(&[third], Some(first));
        assert_eq!(ws.open_ids, vec![third, first, second]);
        assert_eq!(ws.active_id, Some(third));
    }

    #[test]
    fn open_requests_moves_an_open_tab_without_duplicating_it() {
        let mut ws = Workspace::new();
        let all = add_sessions(&mut ws, 1);
        ws.open_requests(&[all[1]], None);
        ws.open_requests(&[all[1]], Some(all[0]));
        assert_eq!(ws.open_ids, vec![all[1], all[0]]);
    }

    #[test]
    fn open_requests_ignores_unknown_ids() {
        let mut ws = Workspace::new();
        let before = ws.open_ids.clone();
        ws.open_requests(&[999_999], None);
        assert_eq!(ws.open_ids, before);
    }

    // ── reopen and undo ─────────────────────────────────────────────────────

    #[test]
    fn reopen_closed_tab_reopens_tabs_in_reverse_close_order() {
        let mut ws = Workspace::new();
        let all = add_open_sessions(&mut ws, 2);
        let (a, b, c) = (all[0], all[1], all[2]);
        ws.close_tab(b);
        ws.close_tabs(&[a, c]);
        assert!(ws.open_ids.is_empty());
        assert_eq!(ws.reopen_closed_tab(), Some(c));
        assert_eq!(ws.reopen_closed_tab(), Some(a));
        assert_eq!(ws.reopen_closed_tab(), Some(b));
        assert_eq!(ws.reopen_closed_tab(), None);
        assert_eq!(ws.active_id, Some(b));
    }

    #[test]
    fn reopen_closed_tab_skips_deleted_and_already_open_requests() {
        let mut ws = Workspace::new();
        let all = add_open_sessions(&mut ws, 1);
        let (a, b) = (all[0], all[1]);
        ws.close_tab(a);
        ws.close_tab(b);
        ws.delete_request(b);
        ws.open_request(a);
        assert_eq!(ws.reopen_closed_tab(), None);
    }

    #[test]
    fn undo_restores_a_deleted_request_at_its_place_and_tab() {
        let mut ws = Workspace::new();
        let all = add_open_sessions(&mut ws, 2);
        let (a, b, c) = (all[0], all[1], all[2]);
        ws.delete_request(b);
        assert!(matches!(ws.last_deletion(), Some(Deletion::Requests(_))));
        assert_eq!(ws.deletion_label(), "DELETED 1 REQUEST");
        assert!(ws.undo_delete());
        assert_eq!(ids(&ws), vec![a, b, c]);
        assert_eq!(ws.open_ids, vec![a, b, c]);
        assert_eq!(ws.active_id, Some(b));
        assert!(ws.last_deletion().is_none());
        assert!(!ws.undo_delete());
    }

    #[test]
    fn undo_restores_requests_deleted_together() {
        let mut ws = Workspace::new();
        let all = add_open_sessions(&mut ws, 2);
        let (a, b, c) = (all[0], all[1], all[2]);
        ws.delete_requests(&[a, c]);
        assert_eq!(ws.deletion_label(), "DELETED 2 REQUESTS");
        assert!(ws.undo_delete());
        assert_eq!(ids(&ws), vec![a, b, c]);
        // Separate deletions undo one at a time.
        ws.delete_request(a);
        ws.delete_request(b);
        ws.undo_delete();
        assert_eq!(ids(&ws), vec![b, c]);
    }

    #[test]
    fn undo_drops_the_placeholder_made_when_the_last_request_was_deleted() {
        let mut ws = Workspace::new();
        let only = ws.sessions[0].id;
        ws.sessions[0].draft.url = "https://example.test".into();
        ws.delete_request(only);
        assert_ne!(ws.sessions[0].id, only);
        ws.undo_delete();
        assert_eq!(ids(&ws), vec![only]);
        assert_eq!(ws.open_ids, vec![only]);
    }

    #[test]
    fn undo_restores_a_deleted_group_and_its_contents() {
        let mut ws = Workspace::new();
        let a = ws.sessions[0].id;
        let parent = ws.add_group("Parent", None);
        let group = ws.add_group("Child", Some(parent));
        let nested = ws.add_group("Nested", Some(group));
        ws.move_request(a, Some(group));
        ws.delete_group(group);
        assert_eq!(ws.sessions[0].group_id, Some(parent));
        assert_eq!(ws.deletion_label(), "DELETED GROUP Child");
        ws.undo_delete();
        let names: Vec<&str> = ws.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, vec!["Parent", "Child", "Nested"]);
        assert_eq!(ws.groups[2].parent_id, Some(group));
        assert_eq!(ws.sessions[0].group_id, Some(group));
        assert_eq!(ws.groups[2].id, nested);
    }

    #[test]
    fn deletion_expires_after_the_undo_window() {
        let mut ws = Workspace::new();
        let all = add_open_sessions(&mut ws, 1);
        ws.delete_request(all[1]);
        let serial = ws.deletion_serial();
        ws.expire_deletion(serial);
        assert!(ws.last_deletion().is_none());
        assert!(!ws.undo_delete());
    }

    #[test]
    fn an_old_timer_does_not_expire_a_later_deletion() {
        let mut ws = Workspace::new();
        let all = add_open_sessions(&mut ws, 2);
        ws.delete_request(all[1]);
        let old = ws.deletion_serial();
        ws.delete_request(all[2]);
        ws.expire_deletion(old);
        assert!(ws.last_deletion().is_some());
    }

    #[test]
    fn discarding_a_deletion_releases_its_response_bodies() {
        let mut ws = Workspace::new();
        let all = add_open_sessions(&mut ws, 1);
        ws.session_mut(all[1]).unwrap().response = Some(response(Some("body-1")));
        ws.delete_request(all[1]);
        assert!(ws.take_released_bodies().is_empty());
        ws.discard_deletion();
        assert_eq!(ws.take_released_bodies(), vec!["body-1".to_string()]);
    }

    #[test]
    fn a_busy_request_is_not_deleted() {
        let mut ws = Workspace::new();
        let id = ws.sessions[0].id;
        ws.sessions[0].busy = true;
        ws.delete_request(id);
        assert_eq!(ids(&ws), vec![id]);
        assert!(ws.last_deletion().is_none());
    }

    #[test]
    fn a_waiting_request_is_not_deleted() {
        let mut ws = Workspace::new();
        let id = ws.sessions[0].id;
        ws.sessions[0].waiting_on = Some("/login".into());
        ws.delete_request(id);
        assert_eq!(ids(&ws), vec![id]);
        assert!(ws.last_deletion().is_none());
        assert!(matches!(ws.request_delete(id), DeleteRequest::Blocked));
    }

    #[test]
    fn undo_restores_a_request_that_does_not_wait() {
        let mut ws = Workspace::new();
        let all = add_open_sessions(&mut ws, 1);
        ws.delete_request(all[1]);
        if let Some(Deletion::Requests(items)) = ws.last_deletion.as_mut() {
            items[0].session.waiting_on = Some("/login".into());
        }
        assert!(ws.undo_delete());
        assert_eq!(ws.session(all[1]).unwrap().waiting_on, None);
    }

    // ── autosave ────────────────────────────────────────────────────────────

    fn saved_scroll(content: &str) -> f64 {
        let value: serde_json::Value = serde_json::from_str(content).unwrap();
        value["tabs"][0]["view"]["responseScroll"].as_f64().unwrap()
    }

    #[test]
    fn autosave_key_ignores_the_response_scroll() {
        let mut ws = Workspace::new();
        let before = ws.autosave_key();
        ws.sessions[0].view.response_scroll = 480.0;
        assert_eq!(ws.autosave_key(), before);
    }

    #[test]
    fn a_real_change_saves_the_latest_scroll() {
        let mut ws = Workspace::new();
        let before = ws.autosave_key();
        ws.sessions[0].view.response_scroll = 480.0;
        ws.sessions[0].draft.url = "https://example.test/next".into();
        assert_ne!(ws.autosave_key(), before);
        assert_eq!(saved_scroll(&ws.encode()), 480.0);
    }

    #[test]
    fn encode_and_decode_round_trip() {
        let mut ws = Workspace::new();
        ws.set_global_definitions(defs(&[("host", "https://example.test")]));
        let mut preferences = default_preferences();
        preferences.default_method = "PATCH".into();
        preferences.wrap = true;
        preferences.confirm_close_drafts = false;
        assert!(ws.set_preferences(preferences.clone()));
        let group = ws.add_group("API", None);
        ws.move_request(ws.sessions[0].id, Some(group));
        let restored = Workspace::decode(&ws.encode()).unwrap();
        assert_eq!(restored.global_definitions, ws.global_definitions);
        assert_eq!(restored.preferences, preferences);
        assert_eq!(restored.groups, ws.groups);
        assert_eq!(restored.open_ids, ws.open_ids);
        assert_eq!(restored.active_id, ws.active_id);
        assert_eq!(ids(&restored), ids(&ws));
        assert!(Workspace::decode("not json").is_err());
    }

    #[test]
    fn reset_clears_tokens_preferences_and_groups() {
        let mut ws = Workspace::new();
        ws.set_global_definitions(defs(&[("host", "https://example.test")]));
        let mut preferences = default_preferences();
        preferences.wrap = true;
        ws.set_preferences(preferences);
        ws.add_group("API", None);
        ws.reset();
        assert!(ws.global_definitions.is_empty());
        assert_eq!(ws.preferences, default_preferences());
        assert!(ws.groups.is_empty());
        assert_eq!(ws.sessions.len(), 1);
        assert_eq!(ws.open_ids, vec![ws.sessions[0].id]);
        let saved: serde_json::Value = serde_json::from_str(&ws.encode()).unwrap();
        assert_eq!(saved["groups"], serde_json::json!([]));
        assert_eq!(saved["globalDefinitions"], serde_json::json!({}));
    }

    // ── setters ─────────────────────────────────────────────────────────────

    #[test]
    fn set_request_local_auth_sets_and_clears() {
        let mut ws = Workspace::new();
        let id = ws.sessions[0].id;
        assert!(ws.sessions[0].draft.local_auth.is_none());
        ws.set_request_local_auth(id, Some(bearer("tok")));
        assert_eq!(ws.sessions[0].draft.local_auth, Some(bearer("tok")));
        ws.set_request_local_auth(id, None);
        assert!(ws.sessions[0].draft.local_auth.is_none());
    }

    #[test]
    fn set_request_local_auth_ignores_an_unknown_request() {
        let mut ws = Workspace::new();
        let before = ws.sessions.clone();
        ws.set_request_local_auth(999_999, Some(AuthorizationConfig::None));
        assert_eq!(ws.sessions, before);
    }

    #[test]
    fn set_request_local_auth_does_not_affect_other_requests() {
        let mut ws = Workspace::new();
        let all = add_sessions(&mut ws, 1);
        ws.set_request_local_auth(all[0], Some(AuthorizationConfig::None));
        assert!(ws.session(all[1]).unwrap().draft.local_auth.is_none());
    }

    #[test]
    fn set_group_name_sets_and_trims() {
        let mut ws = Workspace::new();
        let group = ws.add_group("OldName", None);
        ws.set_group_name(group, "NewName");
        assert_eq!(ws.group(group).unwrap().name, "NewName");
        ws.set_group_name(group, "  Trimmed  ");
        assert_eq!(ws.group(group).unwrap().name, "Trimmed");
    }

    #[test]
    fn set_group_name_ignores_blank_oversized_and_unknown() {
        let mut ws = Workspace::new();
        let group = ws.add_group("Original", None);
        ws.set_group_name(group, "   ");
        assert_eq!(ws.group(group).unwrap().name, "Original");
        ws.set_group_name(group, &"x".repeat(81));
        assert_eq!(ws.group(group).unwrap().name, "Original");
        ws.rename_group(group, &"x".repeat(81));
        assert_eq!(ws.group(group).unwrap().name, "Original");
        ws.set_group_name(999_999, "Hack");
        assert_eq!(ws.group(group).unwrap().name, "Original");
    }

    #[test]
    fn set_group_local_auth_sets_and_clears() {
        let mut ws = Workspace::new();
        let group = ws.add_group("G", None);
        ws.set_group_local_auth(group, Some(bearer("g-tok")));
        assert_eq!(ws.group(group).unwrap().local_auth, Some(bearer("g-tok")));
        ws.set_group_local_auth(group, None);
        assert!(ws.group(group).unwrap().local_auth.is_none());
        ws.set_group_local_auth(999_999, Some(AuthorizationConfig::None));
    }

    #[test]
    fn set_group_local_definitions_sets_and_clears() {
        let mut ws = Workspace::new();
        let group = ws.add_group("G", None);
        let host = defs(&[("host", "api.example.com")]);
        ws.set_group_local_definitions(group, Some(host.clone()));
        assert_eq!(ws.group(group).unwrap().local_definitions, Some(host));
        ws.set_group_local_definitions(group, None);
        assert!(ws.group(group).unwrap().local_definitions.is_none());
        ws.set_group_local_definitions(999_999, Some(defs(&[("x", "y")])));
    }

    #[test]
    fn set_global_definitions_replaces_the_map() {
        let mut ws = Workspace::new();
        ws.set_global_definitions(defs(&[("apiVersion", "v3")]));
        assert_eq!(ws.global_definitions, defs(&[("apiVersion", "v3")]));
        ws.set_global_definitions(defs(&[("a", "1"), ("b", "2")]));
        ws.set_global_definitions(defs(&[("b", "updated")]));
        assert_eq!(ws.global_definitions, defs(&[("b", "updated")]));
    }

    #[test]
    fn set_preferences_rejects_invalid_preferences() {
        let mut ws = Workspace::new();
        let mut preferences = default_preferences();
        preferences.default_method = "BAD METHOD".into();
        assert!(!ws.set_preferences(preferences));
        assert_eq!(ws.preferences, default_preferences());
        assert!(Workspace::decode(&ws.encode()).is_ok());
    }

    #[test]
    fn set_group_new_request_defaults_rejects_invalid_values() {
        let mut ws = Workspace::new();
        let group = ws.add_group("API", None);
        ws.set_group_new_request_defaults(
            group,
            Some("POST".into()),
            Some("https://example.test".into()),
        );
        ws.set_group_new_request_defaults(
            group,
            Some("BAD METHOD".into()),
            Some("x".repeat(65537)),
        );
        let saved = ws.group(group).unwrap();
        assert_eq!(saved.default_method.as_deref(), Some("POST"));
        assert_eq!(saved.default_url.as_deref(), Some("https://example.test"));
        assert!(Workspace::decode(&ws.encode()).is_ok());
    }

    #[test]
    fn clears_overrides_and_resolves_group_location_independently() {
        let mut ws = Workspace::new();
        let parent = ws.add_group("Parent", None);
        let child = ws.add_group("Child", Some(parent));
        ws.set_group_new_request_defaults(
            child,
            Some("PUT".into()),
            Some("https://example.test".into()),
        );
        ws.set_group_parent(child, None);
        ws.set_group_new_request_defaults(child, None, None);
        let restored = Workspace::decode(&ws.encode()).unwrap();
        let saved = restored.group(child).unwrap();
        assert_eq!(saved.parent_id, None);
        assert!(saved.default_method.is_none());
    }

    #[test]
    fn set_group_environments_keeps_a_still_valid_active_one() {
        let mut ws = Workspace::new();
        let group = ws.add_group("API", None);
        let dev = create_environment("Dev", EnvironmentColor::Success);
        let prod = create_environment("Prod", EnvironmentColor::Destructive);
        ws.set_group_environments(group, Some(vec![dev.clone(), prod.clone()]));
        ws.set_active_environment(group, Some(prod.id));
        ws.set_group_environments(group, Some(vec![prod.clone()]));
        assert_eq!(
            ws.group(group).unwrap().active_environment_id,
            Some(prod.id)
        );
        ws.set_group_environments(group, Some(vec![dev]));
        assert_eq!(ws.group(group).unwrap().active_environment_id, None);
        ws.set_group_environments(group, Some(vec![]));
        assert!(ws.group(group).unwrap().environments.is_none());
    }

    #[test]
    fn set_active_environment_needs_a_root_group_and_a_known_environment() {
        let mut ws = Workspace::new();
        let root = ws.add_group("API", None);
        let nested = ws.add_group("Nested", Some(root));
        let dev = create_environment("Dev", EnvironmentColor::Success);
        ws.set_group_environments(root, Some(vec![dev.clone()]));
        ws.set_active_environment(root, Some(999_999));
        assert_eq!(ws.group(root).unwrap().active_environment_id, None);
        ws.set_active_environment(root, Some(dev.id));
        assert_eq!(ws.group(root).unwrap().active_environment_id, Some(dev.id));
        ws.set_active_environment(nested, None);
        assert_eq!(ws.group(root).unwrap().active_environment_id, Some(dev.id));
        ws.set_active_environment(root, None);
        assert_eq!(ws.group(root).unwrap().active_environment_id, None);
    }

    #[test]
    fn save_group_settings_applies_each_change() {
        let mut ws = Workspace::new();
        let parent = ws.add_group("Parent", None);
        let group = ws.add_group("G", None);
        ws.set_group_local_auth(group, Some(bearer("t")));
        ws.save_group_settings(
            group,
            GroupSettingsChanges {
                name: Some("Renamed".into()),
                local_auth: Some(None),
                local_definitions: Some(defs(&[("a", "1")])),
                parent_id: Some(Some(parent)),
                new_request_defaults: Some((Some("POST".into()), None)),
                environments: None,
                response_tokens: None,
            },
        );
        let saved = ws.group(group).unwrap();
        assert_eq!(saved.name, "Renamed");
        assert!(saved.local_auth.is_none());
        assert_eq!(saved.local_definitions, Some(defs(&[("a", "1")])));
        assert_eq!(saved.parent_id, Some(parent));
        assert_eq!(saved.default_method.as_deref(), Some("POST"));
    }

    // ── closeTabs ───────────────────────────────────────────────────────────

    fn open_four(ws: &mut Workspace) -> Vec<u64> {
        let ids: Vec<u64> = (0..4)
            .map(|_| {
                let session = create_session(None);
                let id = session.id;
                ws.sessions.push(session);
                id
            })
            .collect();
        ws.open_ids = ids.clone();
        ids
    }

    #[test]
    fn close_tabs_keeps_the_active_tab_when_it_stays_open() {
        let mut ws = Workspace::new();
        let t = open_four(&mut ws);
        ws.active_id = Some(t[1]);
        ws.close_tabs(&[t[0], t[2]]);
        assert_eq!(ws.open_ids, vec![t[1], t[3]]);
        assert_eq!(ws.active_id, Some(t[1]));
    }

    #[test]
    fn close_tabs_moves_the_active_tab_to_the_next_open_tab_at_the_same_position() {
        let mut ws = Workspace::new();
        let t = open_four(&mut ws);
        ws.active_id = Some(t[1]);
        ws.close_tabs(&[t[1], t[2]]);
        assert_eq!(ws.open_ids, vec![t[0], t[3]]);
        assert_eq!(ws.active_id, Some(t[3]));
    }

    #[test]
    fn close_tabs_clears_the_active_tab_when_all_close_and_keeps_the_requests() {
        let mut ws = Workspace::new();
        let t = open_four(&mut ws);
        let count = ws.sessions.len();
        ws.active_id = Some(t[0]);
        ws.close_tabs(&t);
        assert!(ws.open_ids.is_empty());
        assert_eq!(ws.active_id, None);
        assert_eq!(ws.sessions.len(), count);
    }

    #[test]
    fn close_tab_activates_the_tab_at_the_same_position() {
        let mut ws = Workspace::new();
        let t = open_four(&mut ws);
        ws.active_id = Some(t[3]);
        ws.close_tab(t[3]);
        assert_eq!(ws.active_id, Some(t[2]));
        ws.close_tab(t[0]);
        assert_eq!(ws.active_id, Some(t[2]));
    }

    // ── import ──────────────────────────────────────────────────────────────

    #[test]
    fn import_group_adds_the_tree_with_filtered_tokens_and_auth() {
        let mut ws = Workspace::new();
        let mut draft = create_draft();
        draft.url = "https://api.test/items".into();
        let long = "x".repeat(257);
        let root = ImportedGroup {
            name: "API".into(),
            definitions: Some(defs(&[
                ("host", "api.test"),
                ("_hidden", "1"),
                (&long, "v"),
                ("", "e"),
            ])),
            auth: Some(bearer("t")),
            groups: vec![ImportedGroup {
                name: "Items".into(),
                requests: vec![draft],
                ..ImportedGroup::default()
            }],
            requests: vec![],
        };
        let (group, first) = ws.import_group(&root, None);
        assert_eq!(ws.groups.len(), 2);
        let saved = ws.group(group).unwrap();
        assert_eq!(saved.local_definitions, Some(defs(&[("host", "api.test")])));
        assert_eq!(saved.local_auth, Some(bearer("t")));
        let child = ws.groups[1].id;
        assert_eq!(ws.groups[1].parent_id, Some(group));
        let first = ws.session(first.unwrap()).unwrap();
        assert_eq!(first.group_id, Some(child));
        assert_eq!(first.draft.url, "https://api.test/items");
    }

    #[test]
    fn import_selects_the_first_request_and_reports_the_count() {
        let mut ws = Workspace::new();
        let root = ImportedGroup {
            name: "Shop".into(),
            requests: vec![create_draft(), create_draft()],
            ..ImportedGroup::default()
        };
        let result = ImportResult {
            format: crate::model::ImportFormat::Http,
            root,
            skipped: vec!["a: why".into()],
        };
        let notice = ws.import(&result);
        assert_eq!(notice.message, "IMPORTED 2 REQUESTS INTO SHOP · 1 NOTES");
        assert_eq!(notice.details, "a: why");
        let active = ws.active().unwrap();
        assert_eq!(active.group_id, Some(ws.groups[0].id));
        assert_eq!(ws.selected_ids, vec![active.id]);
    }

    // ── App behavior ────────────────────────────────────────────────────────

    #[test]
    fn create_opens_a_request_in_the_active_group_with_its_defaults() {
        let mut ws = Workspace::new();
        let group = ws.add_group("API", None);
        ws.set_group_new_request_defaults(
            group,
            Some("POST".into()),
            Some("https://api.test".into()),
        );
        ws.toggle_group(group);
        ws.move_request(ws.sessions[0].id, Some(group));
        let id = ws.create(None);
        let session = ws.session(id).unwrap();
        assert_eq!(session.group_id, Some(group));
        assert_eq!(session.draft.method, "POST");
        assert_eq!(session.draft.url, "https://api.test");
        assert!(!ws.group(group).unwrap().collapsed);
        assert_eq!(ws.active_id, Some(id));
        assert_eq!(ws.open_ids.last(), Some(&id));
        assert_eq!(ws.selected_ids, vec![id]);
        assert_eq!(ws.selection_anchor_id, Some(id));
    }

    #[test]
    fn create_uses_the_preferences_outside_a_group() {
        let mut ws = Workspace::new();
        let mut preferences = default_preferences();
        preferences.default_method = "PATCH".into();
        preferences.wrap = true;
        ws.set_preferences(preferences);
        let id = ws.create(Some(None));
        let session = ws.session(id).unwrap();
        assert_eq!(session.draft.method, "PATCH");
        assert!(session.view.wrap);
    }

    #[test]
    fn duplicate_copies_the_draft_and_view_into_the_same_group() {
        let mut ws = Workspace::new();
        let group = ws.add_group("API", None);
        let source = ws.sessions[0].id;
        ws.move_request(source, Some(group));
        ws.sessions[0].draft.url = "https://api.test/a".into();
        ws.sessions[0].view.request_tab = "headers".into();
        ws.sessions[0].view.response_scroll = 200.0;
        let copy = ws.duplicate(None).unwrap();
        let session = ws.session(copy).unwrap();
        assert_ne!(copy, source);
        assert_eq!(session.group_id, Some(group));
        assert_eq!(session.draft.url, "https://api.test/a");
        assert_eq!(session.view.request_tab, "headers");
        assert_eq!(session.view.response_scroll, 0.0);
        assert_eq!(ws.active_id, Some(copy));
        assert_eq!(ws.duplicate(Some(999_999)), None);
    }

    #[test]
    fn close_others_closes_every_other_tab_and_keeps_the_requests() {
        let mut ws = Workspace::new();
        ws.create(None);
        ws.create(None);
        let tabs = ws.visible_ids();
        let second = tabs[1];
        let others: Vec<u64> = tabs.iter().copied().filter(|id| *id != second).collect();
        ws.close_many(&others);
        assert_eq!(ws.visible_ids(), vec![second]);
        assert!(tabs.iter().all(|id| ws.session(*id).is_some()));
    }

    #[test]
    fn reveal_expands_the_groups_and_selects_the_request() {
        let mut ws = Workspace::new();
        let parent = ws.add_group("Parent", None);
        let child = ws.add_group("Child", Some(parent));
        let id = ws.sessions[0].id;
        ws.move_request(id, Some(child));
        ws.collapse_all_groups();
        assert!(ws.groups.iter().all(|group| group.collapsed));
        assert!(ws.reveal(id));
        assert!(ws.groups.iter().all(|group| !group.collapsed));
        assert_eq!(ws.selected_ids, vec![id]);
        assert!(!ws.reveal(999_999));
    }

    #[test]
    fn cycle_tab_wraps_around_the_shown_tabs() {
        let mut ws = Workspace::new();
        let first = ws.sessions[0].id;
        let second = ws.create(None);
        let third = ws.create(None);
        ws.cycle_tab(1);
        assert_eq!(ws.active_id, Some(first));
        ws.cycle_tab(-1);
        assert_eq!(ws.active_id, Some(third));
        ws.cycle_tab(-1);
        assert_eq!(ws.active_id, Some(second));
        assert_eq!(ws.selected_ids, vec![second]);
    }

    #[test]
    fn place_tabs_opens_and_selects_the_requests() {
        let mut ws = Workspace::new();
        let all = add_sessions(&mut ws, 2);
        ws.place_tabs(&[all[2], all[1]], Some(all[0]));
        assert_eq!(ws.open_ids, vec![all[2], all[1], all[0]]);
        assert_eq!(ws.active_id, Some(all[2]));
        assert_eq!(ws.selected_ids, vec![all[2], all[1]]);
        assert_eq!(ws.selection_anchor_id, Some(all[2]));
    }

    #[test]
    fn new_group_picks_an_unused_name() {
        let mut ws = Workspace::new();
        let a = ws.new_group();
        let b = ws.new_group();
        let c = ws.new_group();
        assert_eq!(ws.group(a).unwrap().name, "New group");
        assert_eq!(ws.group(b).unwrap().name, "New group 2");
        assert_eq!(ws.group(c).unwrap().name, "New group 3");
        assert_eq!(ws.group(a).unwrap().parent_id, None);
    }

    #[test]
    fn create_group_moves_the_selected_requests_in() {
        let mut ws = Workspace::new();
        let all = add_sessions(&mut ws, 1);
        let group = ws.create_group("Platform", None, &[all[0]]);
        assert_eq!(ws.session(all[0]).unwrap().group_id, Some(group));
        assert_eq!(ws.session(all[1]).unwrap().group_id, None);
    }

    #[test]
    fn remove_drops_the_request_from_the_selection() {
        let mut ws = Workspace::new();
        let all = add_open_sessions(&mut ws, 2);
        ws.update_selection(vec![all[1], all[2]], Some(all[1]));
        ws.remove(all[1]);
        assert_eq!(ws.selected_ids, vec![all[2]]);
        assert_eq!(ws.selection_anchor_id, None);
        assert!(ws.session(all[1]).is_none());
    }

    #[test]
    fn request_delete_asks_for_several_targets_or_a_draft() {
        let mut ws = Workspace::new();
        let all = add_sessions(&mut ws, 2);
        // A blank request is deleted at once.
        assert_eq!(ws.request_delete(all[0]), DeleteRequest::Deleted);
        ws.session_mut(all[1]).unwrap().draft.url = "https://a.test".into();
        assert_eq!(
            ws.request_delete(all[1]),
            DeleteRequest::Confirm(vec![all[1]])
        );
        ws.update_selection(vec![all[1], all[2]], Some(all[1]));
        assert_eq!(
            ws.request_delete(all[2]),
            DeleteRequest::Confirm(vec![all[1], all[2]])
        );
        ws.session_mut(all[2]).unwrap().busy = true;
        assert_eq!(ws.request_delete(all[1]), DeleteRequest::Blocked);
        let mut preferences = default_preferences();
        preferences.confirm_close_drafts = false;
        ws.set_preferences(preferences);
        ws.update_selection(vec![], None);
        assert_eq!(ws.request_delete(all[1]), DeleteRequest::Deleted);
    }

    #[test]
    fn undo_deletion_selects_the_restored_tab() {
        let mut ws = Workspace::new();
        let all = add_open_sessions(&mut ws, 1);
        ws.remove(all[1]);
        assert!(ws.undo_deletion());
        assert_eq!(ws.active_id, Some(all[1]));
        assert_eq!(ws.selected_ids, vec![all[1]]);
    }

    #[test]
    fn reopen_tab_selects_the_reopened_tab() {
        let mut ws = Workspace::new();
        let all = add_open_sessions(&mut ws, 1);
        ws.close(all[1]);
        assert_eq!(ws.reopen_tab(), Some(all[1]));
        assert_eq!(ws.selected_ids, vec![all[1]]);
        assert_eq!(ws.reopen_tab(), None);
    }

    #[test]
    fn toggle_layout_and_zoom_update_the_preferences() {
        let mut ws = Workspace::new();
        ws.toggle_layout();
        assert_eq!(ws.preferences.pane_layout, PaneLayout::Vertical);
        ws.toggle_layout();
        assert_eq!(ws.preferences.pane_layout, PaneLayout::Horizontal);
        ws.set_zoom(1.25);
        assert_eq!(ws.preferences.zoom, 1.25);
        ws.set_zoom(3.0);
        assert_eq!(ws.preferences.zoom, 1.25);
    }

    #[test]
    fn toggle_group_flips_collapsed() {
        let mut ws = Workspace::new();
        let group = ws.add_group("Platform", None);
        ws.toggle_group(group);
        assert!(ws.group(group).unwrap().collapsed);
        ws.toggle_group(group);
        assert!(!ws.group(group).unwrap().collapsed);
    }

    // ── group focus ─────────────────────────────────────────────────────────

    /// One ungrouped tab with a URL, then one tab in Platform; the first is active.
    fn focus_setup() -> (Workspace, u64, u64, u64) {
        let mut ws = Workspace::new();
        let loose = ws.sessions[0].id;
        ws.sessions[0].draft.url = "https://example.test/loose".into();
        let platform = ws.add_group("Platform", None);
        let inside = ws.create(Some(Some(platform)));
        ws.select(loose);
        (ws, loose, inside, platform)
    }

    #[test]
    fn focus_shows_only_the_focused_groups_tabs_and_restores_the_rest() {
        let (mut ws, loose, inside, platform) = focus_setup();
        assert_eq!(ws.shown_active_id(), Some(loose));
        ws.focus_group(platform);
        assert_eq!(ws.visible_ids(), vec![inside]);
        assert_eq!(ws.shown_active_id(), Some(inside));
        assert_eq!(ws.focused_group_id(), Some(platform));
        ws.unfocus(true);
        assert_eq!(ws.focused_group_id(), None);
        assert_eq!(ws.visible_ids(), vec![loose, inside]);
        assert_eq!(ws.shown_active_id(), Some(loose));
    }

    #[test]
    fn selecting_a_request_outside_the_focus_unfocuses() {
        let (mut ws, loose, _, platform) = focus_setup();
        ws.focus_group(platform);
        ws.select(loose);
        assert_eq!(ws.focused_group_id(), None);
        assert_eq!(ws.shown_active_id(), Some(loose));
    }

    #[test]
    fn new_requests_go_to_the_focused_group() {
        let (mut ws, loose, inside, platform) = focus_setup();
        ws.focus_group(platform);
        let id = ws.create(Some(None));
        assert_eq!(ws.session(id).unwrap().group_id, Some(platform));
        assert_eq!(ws.visible_ids(), vec![inside, id]);
        assert_eq!(ws.focused_group_id(), Some(platform));
        assert!(!ws.in_focus(loose));
    }

    #[test]
    fn focus_drops_hidden_rows_from_the_selection() {
        let (mut ws, loose, inside, platform) = focus_setup();
        ws.update_selection(vec![loose, inside], Some(loose));
        ws.focus_group(platform);
        assert_eq!(ws.selected_ids, vec![inside]);
        assert_eq!(ws.selection_anchor_id, None);
    }

    #[test]
    fn deleting_the_focused_group_ends_the_focus() {
        let (mut ws, loose, _, platform) = focus_setup();
        ws.focus_group(platform);
        ws.delete_group(platform);
        assert_eq!(ws.focused_group_id(), None);
        assert_eq!(ws.shown_active_id(), Some(loose));
    }

    #[test]
    fn closing_the_shown_tab_in_focus_keeps_a_shown_tab_active() {
        let (mut ws, _, inside, platform) = focus_setup();
        let other = ws.create(Some(Some(platform)));
        ws.focus_group(platform);
        ws.select(inside);
        ws.close(inside);
        assert_eq!(ws.shown_active_id(), Some(other));
    }

    // ── captures and environments ───────────────────────────────────────────

    #[test]
    fn capture_writes_to_the_environment_group_or_globals() {
        let mut ws = Workspace::new();
        let root = ws.add_group("API", None);
        let nested = ws.add_group("Items", Some(root));
        ws.capture(None, &defs(&[("token", "g")]));
        assert_eq!(ws.global_definitions, defs(&[("token", "g")]));
        ws.capture(Some(nested), &defs(&[("token", "r")]));
        assert_eq!(
            ws.group(root).unwrap().local_definitions,
            Some(defs(&[("token", "r")]))
        );
        let dev = create_environment("Dev", EnvironmentColor::Success);
        ws.set_group_environments(root, Some(vec![dev.clone()]));
        ws.switch_environment(root, Some(dev.id));
        ws.capture(Some(nested), &defs(&[("token", "e")]));
        let env = &ws.group(root).unwrap().environments.as_ref().unwrap()[0];
        assert_eq!(env.values, defs(&[("token", "e")]));
        assert_eq!(ws.global_definitions, defs(&[("token", "g")]));
    }

    #[test]
    fn a_protected_environment_asks_again_after_each_switch() {
        let mut ws = Workspace::new();
        let root = ws.add_group("API", None);
        let mut prod = create_environment("Prod", EnvironmentColor::Destructive);
        prod.protected = Some(true);
        let dev = create_environment("Dev", EnvironmentColor::Success);
        ws.set_group_environments(root, Some(vec![prod.clone(), dev.clone()]));
        ws.switch_environment(root, Some(prod.id));
        assert!(!ws.environment_confirmed(Some(root)));
        ws.confirm_environment(prod.id);
        assert!(ws.environment_confirmed(Some(root)));
        ws.switch_environment(root, Some(dev.id));
        ws.switch_environment(root, Some(prod.id));
        assert!(!ws.environment_confirmed(Some(root)));
    }

    #[test]
    fn active_root_and_environment_follow_the_active_request() {
        let mut ws = Workspace::new();
        let root = ws.add_group("API", None);
        let nested = ws.add_group("Items", Some(root));
        let dev = create_environment("Dev", EnvironmentColor::Success);
        ws.set_group_environments(root, Some(vec![dev.clone()]));
        ws.switch_environment(root, Some(dev.id));
        assert!(ws.active_root().is_none());
        ws.move_request(ws.sessions[0].id, Some(nested));
        assert_eq!(ws.active_root().map(|g| g.id), Some(root));
        assert_eq!(ws.active_environment().map(|e| e.id), Some(dev.id));
    }
}
