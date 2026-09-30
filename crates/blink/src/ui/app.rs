//! The window shell. Port of `App.vue`.

use std::collections::HashMap;

use blink_core::command_center::Command;
use blink_core::environments::{active_environment, root_group};
use blink_core::import::parse_import;
use blink_core::model::{CodeTarget, PaneLayout};
use blink_core::preferences::step_zoom;
use blink_core::workspace_state::{IMPORT_LIMIT_BYTES, IMPORT_TOO_LARGE, import_failed_notice};
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::actions::*;
use crate::store::{Store, StoreEvent};
use crate::theme;
use crate::ui::browser::Browser;
use crate::ui::command_center::{CommandCenter, Picked};
use crate::ui::request_pane::RequestPane;
use crate::ui::tabs::RequestTabs;
use crate::ui::title_bar::{self, TitleBarProps};
use crate::ui::{cookies_dialog, group_settings, settings_dialog, status_bar};

/// The frame gap (`gap-1.5`).
pub const FRAME_GAP: f32 = 6.0;
/// At or below this width the Browser opens as an overlay (`max-[760px]`).
pub const NARROW_WIDTH: f32 = 760.0;

pub struct BlinkApp {
    focus_handle: FocusHandle,
    store: Entity<Store>,
    browser: Entity<Browser>,
    tabs: Entity<RequestTabs>,
    command_center: Entity<CommandCenter>,
    /// One pane per request, for its whole lifetime.
    panes: HashMap<u64, Entity<RequestPane>>,
    sidebar_collapsed: bool,
    mobile_browser_open: bool,
    _subscriptions: Vec<Subscription>,
}

impl BlinkApp {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let browser = cx.new(|cx| Browser::new(store.clone(), window, cx));
        let tabs = cx.new(|cx| RequestTabs::new(store.clone(), window, cx));
        let command_center = cx.new(|cx| CommandCenter::new(store.clone(), window, cx));
        let _subscriptions = vec![
            cx.observe_in(&store, window, |this, _, window, cx| {
                this.sync_panes(window, cx);
                this.apply_zoom(window, cx);
                cx.notify();
            }),
            // Save the window layout on blur too, so it survives a killed process.
            // Opening a request from the narrow overlay closes it, as `select` did.
            cx.subscribe(&browser, |this, _, _: &crate::ui::browser::BrowserEvent, cx| {
                this.mobile_browser_open = false;
                cx.notify();
            }),
            // A search pick closes the narrow overlay and focuses its tab
            // (`selectFromSearch`).
            cx.subscribe_in(&command_center, window, |this, _, _: &Picked, window, cx| {
                this.mobile_browser_open = false;
                this.focus_tabs(window, cx);
                cx.notify();
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    crate::save_window_state(&this.store.read(cx).engine, window, cx);
                }
            }),
            cx.subscribe_in(&store, window, |this, _, event, window, cx| {
                if let StoreEvent::Restored = event {
                    this.panes.clear();
                    this.sync_panes(window, cx);
                }
            }),
        ];
        let mut app = BlinkApp {
            focus_handle: cx.focus_handle(),
            store,
            browser,
            tabs,
            command_center,
            panes: HashMap::new(),
            sidebar_collapsed: false,
            mobile_browser_open: false,
            _subscriptions,
        };
        app.sync_panes(window, cx);
        app
    }

    /// Create panes for new requests and drop panes of deleted ones. A new
    /// pane that is active focuses its URL, as `RequestWorkspace` did on mount.
    fn sync_panes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Panes exist only for restored requests. A pane made for the
        // placeholder before the restore could share an id with a restored
        // request and reset its saved tab and scroll as a new response.
        if !self.store.read(cx).ready {
            self.panes.clear();
            return;
        }
        let workspace = &self.store.read(cx).workspace;
        let active_id = workspace.shown_active_id();
        let ids: Vec<u64> = workspace.sessions.iter().map(|session| session.id).collect();
        self.panes.retain(|id, _| ids.contains(id));
        for id in ids {
            if !self.panes.contains_key(&id) {
                let store = self.store.clone();
                let pane = cx.new(|cx| RequestPane::new(store, id, window, cx));
                if Some(id) == active_id {
                    pane.update(cx, |pane, cx| pane.focus_url(window, cx));
                }
                self.panes.insert(id, pane);
            }
        }
    }

    fn apply_zoom(&self, _window: &mut Window, cx: &mut App) {
        let zoom = self.store.read(cx).workspace.preferences.zoom as f32;
        theme::set_zoom(zoom, cx);
    }

    fn narrow(window: &Window) -> bool {
        window.viewport_size().width <= px(NARROW_WIDTH)
    }

    fn browser_visible(&self, window: &Window) -> bool {
        if Self::narrow(window) {
            self.mobile_browser_open
        } else {
            !self.sidebar_collapsed
        }
    }

    /// Show the Browser, as import, reveal, and new group do.
    fn show_browser(&mut self, window: &Window) {
        if Self::narrow(window) {
            self.mobile_browser_open = true;
        } else {
            self.sidebar_collapsed = false;
        }
    }

    fn active_pane(&self, cx: &App) -> Option<Entity<RequestPane>> {
        let id = self.store.read(cx).workspace.shown_active_id()?;
        self.panes.get(&id).cloned()
    }

    fn update_workspace<R>(
        &self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut blink_core::workspace_state::Workspace) -> R,
    ) -> R {
        self.store
            .update(cx, |store, cx| store.update_workspace(cx, change))
    }

    fn ready(&self, cx: &App) -> bool {
        self.store.read(cx).ready
    }

    // ── Actions ─────────────────────────────────────────────────────────────

    fn on_toggle_browser(&mut self, _: &ToggleBrowser, window: &mut Window, cx: &mut Context<Self>) {
        if Self::narrow(window) {
            self.mobile_browser_open = !self.mobile_browser_open;
            self.sidebar_collapsed = false;
        } else {
            self.sidebar_collapsed = !self.sidebar_collapsed;
        }
        cx.notify();
    }

    fn on_toggle_layout(&mut self, _: &ToggleLayout, _: &mut Window, cx: &mut Context<Self>) {
        self.update_workspace(cx, |workspace| workspace.toggle_layout());
    }

    fn on_new_request(&mut self, _: &NewRequest, _: &mut Window, cx: &mut Context<Self>) {
        if self.ready(cx) {
            self.update_workspace(cx, |workspace| workspace.create(None));
        }
    }

    fn on_duplicate(&mut self, _: &DuplicateRequest, _: &mut Window, cx: &mut Context<Self>) {
        if self.ready(cx) {
            self.update_workspace(cx, |workspace| workspace.duplicate(None));
        }
    }

    fn on_close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.store.read(cx).workspace.shown_active_id() {
            self.update_workspace(cx, |workspace| workspace.close(id));
            self.focus_tabs(window, cx);
        }
    }

    /// Focus the active tab, as `close`, `closeMany`, and `reopenTab` did.
    fn focus_tabs(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.focus_handle(cx).focus(window, cx);
    }

    fn on_next_tab(&mut self, _: &NextTab, _: &mut Window, cx: &mut Context<Self>) {
        self.update_workspace(cx, |workspace| workspace.cycle_tab(1));
    }

    fn on_previous_tab(&mut self, _: &PreviousTab, _: &mut Window, cx: &mut Context<Self>) {
        self.update_workspace(cx, |workspace| workspace.cycle_tab(-1));
    }

    fn on_send(&mut self, _: &SendRequest, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.store.read(cx).workspace.shown_active_id() else {
            return;
        };
        let busy = self
            .store
            .read(cx)
            .workspace
            .session(id)
            .is_some_and(|session| session.busy);
        self.store.update(cx, |store, cx| {
            if busy {
                store.cancel(id, cx);
            } else {
                store.send(id, window, cx);
            }
        });
    }

    fn on_cancel(&mut self, _: &CancelRequest, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.store.read(cx).workspace.shown_active_id() {
            self.store.update(cx, |store, cx| store.cancel(id, cx));
        }
    }

    fn open_command_center(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready(cx) {
            return;
        }
        let commands = self.commands(window, cx);
        self.command_center.update(cx, |center, cx| {
            center.set_commands(commands);
            center.open(query, window, cx);
        });
    }

    fn on_search(&mut self, _: &SearchRequests, window: &mut Window, cx: &mut Context<Self>) {
        self.open_command_center("", window, cx);
    }

    fn on_run_command(&mut self, _: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        self.open_command_center(blink_core::command_center::COMMAND_PREFIX, window, cx);
    }

    fn on_focus_url(&mut self, _: &FocusUrl, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane(cx) {
            pane.update(cx, |pane, cx| pane.focus_url(window, cx));
        }
    }

    fn on_open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        settings_dialog::open(self.store.clone(), window, cx);
    }

    fn on_reopen(&mut self, _: &ReopenClosedTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.update_workspace(cx, |workspace| workspace.reopen_tab()).is_some() {
            self.focus_tabs(window, cx);
        }
    }

    fn on_undo(&mut self, _: &UndoDelete, _: &mut Window, cx: &mut Context<Self>) {
        self.update_workspace(cx, |workspace| workspace.undo_deletion());
    }

    fn set_zoom(&mut self, zoom: f64, cx: &mut Context<Self>) {
        self.update_workspace(cx, |workspace| workspace.set_zoom(zoom));
    }

    fn on_zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        let zoom = self.store.read(cx).workspace.preferences.zoom;
        self.set_zoom(step_zoom(zoom, 1), cx);
    }

    fn on_zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        let zoom = self.store.read(cx).workspace.preferences.zoom;
        self.set_zoom(step_zoom(zoom, -1), cx);
    }

    fn on_zoom_reset(&mut self, _: &ZoomReset, _: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(1.0, cx);
    }

    fn on_find(&mut self, _: &FindInResponse, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane(cx) {
            let response = pane.read(cx).response().clone();
            response.update(cx, |response, cx| response.find(window, cx));
        }
    }

    fn on_unfocus(&mut self, _: &Unfocus, _: &mut Window, cx: &mut Context<Self>) {
        if self.store.read(cx).workspace.focused_group_id().is_some() {
            self.update_workspace(cx, |workspace| workspace.unfocus(true));
        } else {
            cx.propagate();
        }
    }

    fn on_open_group_settings(
        &mut self,
        action: &OpenGroupSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        group_settings::open(self.store.clone(), action.group_id, window, cx);
    }

    fn on_manage_cookies(&mut self, _: &ManageCookies, window: &mut Window, cx: &mut Context<Self>) {
        cookies_dialog::open(self.store.clone(), window, cx);
    }

    fn on_new_group(&mut self, _: &NewGroup, window: &mut Window, cx: &mut Context<Self>) {
        let group_id = self.update_workspace(cx, |workspace| workspace.new_group());
        self.show_browser(window);
        group_settings::open(self.store.clone(), group_id, window, cx);
    }

    fn on_collapse_groups(&mut self, _: &CollapseAllGroups, _: &mut Window, cx: &mut Context<Self>) {
        self.update_workspace(cx, |workspace| workspace.collapse_all_groups());
    }

    fn on_import(&mut self, _: &ImportFile, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready(cx) {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let store = self.store.clone();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let text = cx
                .background_executor()
                .spawn(async move {
                    let size = std::fs::metadata(&path)
                        .map_err(|error| error.to_string())?
                        .len();
                    if size > IMPORT_LIMIT_BYTES {
                        return Err(IMPORT_TOO_LARGE.to_string());
                    }
                    // As `File.text()`: invalid UTF-8 becomes U+FFFD.
                    std::fs::read(&path)
                        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                        .map_err(|error| error.to_string())
                })
                .await;
            let result = text.and_then(|text| parse_import(&text, &name));
            this.update_in(cx, |this, window, cx| {
                let imported = result.is_ok();
                store.update(cx, |store, cx| match result {
                    Ok(result) => {
                        let notice = store.update_workspace(cx, |workspace| workspace.import(&result));
                        store.notify_import(notice.message, false, notice.details, cx);
                    }
                    Err(error) => {
                        store.notify_import(import_failed_notice(&error), true, String::new(), cx)
                    }
                });
                // As `importFile`: only a successful import shows the Browser.
                if imported {
                    this.show_browser(window);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_run_command_id(&mut self, action: &RunCommandId, window: &mut Window, cx: &mut Context<Self>) {
        self.run_command(&action.id, window, cx);
    }

    fn on_reveal(&mut self, action: &RevealRequest, window: &mut Window, cx: &mut Context<Self>) {
        self.show_browser(window);
        let id = action.id;
        self.update_workspace(cx, |workspace| workspace.reveal(id));
        self.browser.update(cx, |browser, cx| browser.reveal(id, cx));
    }

    /// Save first; a failed save blocks quitting (`beforeExit`).
    fn on_quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        crate::save_window_state(&self.store.read(cx).engine, window, cx);
        self.store.update(cx, |store, cx| store.quit(cx));
    }

    // ── Commands ────────────────────────────────────────────────────────────

    /// The command list, in the order and wording of `App.vue`.
    pub fn commands(&self, window: &Window, cx: &App) -> Vec<Command> {
        let store = self.store.read(cx);
        let workspace = &store.workspace;
        let current = workspace.active();
        let none = current.is_none();
        let response = current.and_then(|session| session.response.as_ref());
        let visible = workspace.visible_ids().len();
        let zoom = workspace.preferences.zoom;
        let stacked = workspace.preferences.pane_layout == PaneLayout::Vertical;
        let browser_label = if self.browser_visible(window) {
            "Hide request browser"
        } else {
            "Show request browser"
        };
        let busy = current.is_some_and(|session| session.busy);
        let command = |id: &str, label: String, shortcut: &[&str], disabled: bool| Command {
            id: id.to_string(),
            label,
            shortcut: (!shortcut.is_empty())
                .then(|| shortcut.iter().map(|key| key.to_string()).collect()),
            disabled,
        };
        let mut commands = vec![
            command(
                "toggle-layout",
                format!("View: {}", title_bar::layout_toggle_label(stacked)),
                &["mod", "\\"],
                false,
            ),
            command("toggle-browser", format!("View: {browser_label}"), &[], false),
            command("zoom-in", "View: Zoom in".into(), &["mod", "="], false),
            command("zoom-out", "View: Zoom out".into(), &["mod", "-"], false),
            command(
                "zoom-reset",
                format!("View: Reset zoom ({}%)", (zoom * 100.0).round()),
                &["mod", "0"],
                zoom == 1.0,
            ),
            command("next-tab", "View: Next tab".into(), &["ctrl", "tab"], visible < 2),
            command(
                "previous-tab",
                "View: Previous tab".into(),
                &["ctrl", "shift", "tab"],
                visible < 2,
            ),
            command("new-request", "Request: New request".into(), &["mod", "t"], false),
            command(
                "send",
                if busy { "Request: Cancel request" } else { "Request: Send" }.into(),
                if busy { &["mod", "."] } else { &["mod", "enter"] },
                none,
            ),
            command("focus-url", "Request: Focus URL".into(), &["mod", "l"], none),
            command("show-code", "Request: Show code".into(), &[], none),
        ];
        for target in CodeTarget::ALL {
            commands.push(command(
                &format!("copy-as-{}", blink_core::codegen::code_target_id(target)),
                format!("Request: Copy as {}", target.label()),
                &[],
                none,
            ));
        }
        commands.extend([
            command(
                "duplicate-request",
                "Request: Duplicate request".into(),
                &["mod", "shift", "d"],
                none,
            ),
            command("reveal", "Request: Reveal in Browser".into(), &[], none),
            command("delete-request", "Request: Delete request".into(), &[], none || busy),
            command("close-tab", "Tabs: Close tab".into(), &["mod", "w"], none),
            command("close-other-tabs", "Tabs: Close other tabs".into(), &[], visible < 2),
            command("close-all-tabs", "Tabs: Close all tabs".into(), &[], visible == 0),
            command(
                "reopen-tab",
                "Tabs: Reopen closed tab".into(),
                &["mod", "shift", "t"],
                false,
            ),
            command(
                "find",
                "Response: Find".into(),
                &["mod", "f"],
                response.is_none_or(|response| response.is_binary()),
            ),
            command("copy-response", "Response: Copy".into(), &[], response.is_none()),
            command("toggle-history", "Response: Toggle history".into(), &[], none),
            command("save-response", "Response: Save body…".into(), &[], response.is_none()),
            command(
                "toggle-wrap",
                "Response: Toggle line wrap".into(),
                &[],
                response.is_none_or(|response| response.is_binary()),
            ),
            command(
                "toggle-pretty",
                "Response: Toggle pretty".into(),
                &[],
                response.is_none_or(|response| response.is_binary() || response.is_truncated()),
            ),
        ]);
        let root = current.and_then(|session| root_group(session.group_id, &workspace.groups));
        if let Some(root) = root {
            let active = active_environment(Some(root));
            if let Some(environments) = root.environments.as_ref().filter(|list| !list.is_empty()) {
                for environment in environments {
                    commands.push(command(
                        &format!("environment-{}", environment.id),
                        format!("Environment: Switch {} to {}", root.name, environment.name),
                        &[],
                        active.is_some_and(|active| active.id == environment.id),
                    ));
                }
                commands.push(command(
                    "environment-none",
                    format!("Environment: Use no environment for {}", root.name),
                    &[],
                    active.is_none(),
                ));
            }
            commands.push(command(
                "edit-environments",
                format!("Environment: Edit {} environments", root.name),
                &[],
                false,
            ));
        }
        commands.extend([
            command("new-group", "Browser: New group".into(), &[], false),
            command(
                "unfocus-group",
                "Browser: Unfocus group".into(),
                &[],
                workspace.focused_group_id().is_none(),
            ),
            command("import", "File: Import OpenAPI, Postman, or .http file…".into(), &[], false),
            command("manage-cookies", "Cookies: Manage cookies".into(), &[], false),
            command("clear-cookies", "Cookies: Clear all cookies".into(), &[], false),
            command(
                "collapse-groups",
                "Browser: Collapse all groups".into(),
                &[],
                workspace.groups.is_empty(),
            ),
            command(
                "undo-delete",
                "Edit: Undo delete".into(),
                &["mod", "z"],
                workspace.last_deletion().is_none(),
            ),
            command(
                "open-settings",
                "Preferences: Application settings".into(),
                &["mod", ","],
                false,
            ),
        ]);
        commands
    }

    /// `runCommand` in `App.vue`.
    pub fn run_command(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.active_pane(cx);
        let response = pane.as_ref().map(|pane| pane.read(cx).response().clone());
        let active_id = self.store.read(cx).workspace.shown_active_id();
        let root_id = {
            let workspace = &self.store.read(cx).workspace;
            workspace
                .active()
                .and_then(|session| root_group(session.group_id, &workspace.groups))
                .map(|group| group.id)
        };
        match id {
            "toggle-layout" => self.on_toggle_layout(&ToggleLayout, window, cx),
            "toggle-browser" => self.on_toggle_browser(&ToggleBrowser, window, cx),
            "zoom-in" => self.on_zoom_in(&ZoomIn, window, cx),
            "zoom-out" => self.on_zoom_out(&ZoomOut, window, cx),
            "zoom-reset" => self.set_zoom(1.0, cx),
            "next-tab" => self.on_next_tab(&NextTab, window, cx),
            "previous-tab" => self.on_previous_tab(&PreviousTab, window, cx),
            "new-request" => self.on_new_request(&NewRequest, window, cx),
            "send" => self.on_send(&SendRequest, window, cx),
            "focus-url" => self.on_focus_url(&FocusUrl, window, cx),
            "show-code" => {
                if let Some(pane) = pane {
                    pane.update(cx, |pane, cx| pane.toggle_code(window, cx));
                }
            }
            "duplicate-request" => self.on_duplicate(&DuplicateRequest, window, cx),
            "reveal" => {
                if let Some(id) = active_id {
                    self.on_reveal(&RevealRequest { id }, window, cx);
                }
            }
            "delete-request" => {
                if let Some(id) = active_id {
                    self.update_workspace(cx, |workspace| workspace.remove(id));
                }
            }
            "close-tab" => self.on_close_tab(&CloseTab, window, cx),
            "close-other-tabs" => {
                let ids: Vec<u64> = self
                    .store
                    .read(cx)
                    .workspace
                    .visible_ids()
                    .into_iter()
                    .filter(|open| Some(*open) != active_id)
                    .collect();
                self.update_workspace(cx, |workspace| workspace.close_many(&ids));
                self.focus_tabs(window, cx);
            }
            "close-all-tabs" => {
                let ids = self.store.read(cx).workspace.visible_ids();
                self.update_workspace(cx, |workspace| workspace.close_many(&ids));
                self.focus_tabs(window, cx);
            }
            "reopen-tab" => self.on_reopen(&ReopenClosedTab, window, cx),
            "find" => self.on_find(&FindInResponse, window, cx),
            "copy-response" => {
                if let Some(response) = response {
                    response.update(cx, |response, cx| response.copy_result(window, cx));
                }
            }
            "toggle-history" => {
                if let Some(response) = response {
                    response.update(cx, |response, cx| response.toggle_history(window, cx));
                }
            }
            "save-response" => {
                if let Some(response) = response {
                    response.update(cx, |response, cx| response.save_body(window, cx));
                }
            }
            "toggle-wrap" => {
                if let Some(response) = response {
                    response.update(cx, |response, cx| response.toggle_wrap(window, cx));
                }
            }
            "toggle-pretty" => {
                if let Some(response) = response {
                    response.update(cx, |response, cx| response.toggle_pretty(window, cx));
                }
            }
            "environment-none" => {
                if let Some(root_id) = root_id {
                    self.update_workspace(cx, |workspace| workspace.switch_environment(root_id, None));
                }
            }
            "edit-environments" => {
                if let Some(root_id) = root_id {
                    group_settings::open(self.store.clone(), root_id, window, cx);
                }
            }
            "new-group" => self.on_new_group(&NewGroup, window, cx),
            "unfocus-group" => {
                self.update_workspace(cx, |workspace| workspace.unfocus(true));
            }
            "import" => self.on_import(&ImportFile, window, cx),
            "manage-cookies" => self.on_manage_cookies(&ManageCookies, window, cx),
            "clear-cookies" => {
                let _ = self.store.read(cx).engine.clear_cookies();
            }
            "collapse-groups" => self.on_collapse_groups(&CollapseAllGroups, window, cx),
            "undo-delete" => self.on_undo(&UndoDelete, window, cx),
            "open-settings" => self.on_open_settings(&OpenSettings, window, cx),
            _ => {
                if let Some(target) = id.strip_prefix("copy-as-") {
                    if let (Some(pane), Some(target)) =
                        (pane, blink_core::codegen::code_target_from_id(target))
                    {
                        let code = pane.read(cx).code_for(target, cx);
                        self.store.update(cx, |store, cx| store.copy(code, cx));
                    }
                } else if let Some(environment) = id.strip_prefix("environment-")
                    && let (Some(root_id), Ok(environment)) = (root_id, environment.parse::<u64>())
                {
                    self.update_workspace(cx, |workspace| {
                        workspace.switch_environment(root_id, Some(environment))
                    });
                }
            }
        }
    }

    // ── Rendering ───────────────────────────────────────────────────────────

    /// The editor frame; narrow windows drop its rounding and side borders
    /// (`max-[760px]:rounded-none max-[760px]:border-x-0`).
    fn render_editor(&self, narrow: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let store = self.store.read(cx);
        let active = store.workspace.shown_active_id();
        let pane = active.and_then(|id| self.panes.get(&id).cloned());
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .when(!narrow, |this| this.rounded(px(8.)).border_1())
            .when(narrow, |this| this.border_y_1())
            .border_color(colors.border)
            .bg(colors.background)
            .child(self.tabs.clone())
            .map(|this| match pane {
                Some(pane) => this.child(div().flex().flex_col().flex_1().min_h_0().child(pane)),
                None => this.child(
                    div()
                        .flex()
                        .flex_1()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child("No open requests. Select a request in the browser.")
                        .child(
                            gpui_kit::component::button::Button::new("empty-new-request")
                                .secondary()
                                .label("New request")
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(NewRequest), cx)
                                }),
                        ),
                ),
            })
    }
}

impl Focusable for BlinkApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BlinkApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let ready = self.store.read(cx).ready;
        let closing = self.store.read(cx).closing;
        let stacked =
            self.store.read(cx).workspace.preferences.pane_layout == PaneLayout::Vertical;
        let narrow = Self::narrow(window);
        let browser_visible = self.browser_visible(window);
        // Keep the open command list current, as the Vue computed list was.
        let commands = self.commands(window, cx);
        self.command_center
            .update(cx, |center, _| center.set_commands(commands));
        let title = title_bar::render(
            TitleBarProps {
                browser_visible,
                stacked,
                command_center: if ready {
                    self.command_center.clone().into_any_element()
                } else {
                    div().into_any_element()
                },
                on_toggle_browser: Box::new(|window, cx| {
                    window.dispatch_action(Box::new(ToggleBrowser), cx)
                }),
                on_toggle_layout: Box::new(|window, cx| {
                    window.dispatch_action(Box::new(ToggleLayout), cx)
                }),
                on_settings: Box::new(|window, cx| {
                    window.dispatch_action(Box::new(OpenSettings), cx)
                }),
            },
            window,
            cx,
        );
        let notice = status_bar::render_storage_notice(&self.store, window, cx);
        let status = status_bar::render(&self.store, window, cx);
        div()
            .id("blink")
            .key_context(APP_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_toggle_browser))
            .on_action(cx.listener(Self::on_toggle_layout))
            .on_action(cx.listener(Self::on_new_request))
            .on_action(cx.listener(Self::on_duplicate))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(Self::on_next_tab))
            .on_action(cx.listener(Self::on_previous_tab))
            .on_action(cx.listener(Self::on_send))
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_search))
            .on_action(cx.listener(Self::on_run_command))
            .on_action(cx.listener(Self::on_focus_url))
            .on_action(cx.listener(Self::on_open_settings))
            .on_action(cx.listener(Self::on_reopen))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_zoom_in))
            .on_action(cx.listener(Self::on_zoom_out))
            .on_action(cx.listener(Self::on_zoom_reset))
            .on_action(cx.listener(Self::on_find))
            .on_action(cx.listener(Self::on_unfocus))
            .on_action(cx.listener(Self::on_open_group_settings))
            .on_action(cx.listener(Self::on_manage_cookies))
            .on_action(cx.listener(Self::on_new_group))
            .on_action(cx.listener(Self::on_collapse_groups))
            .on_action(cx.listener(Self::on_import))
            .on_action(cx.listener(Self::on_run_command_id))
            .on_action(cx.listener(Self::on_reveal))
            .on_action(cx.listener(Self::on_quit))
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.frame)
            .text_color(colors.foreground)
            .font_family(theme::SANS)
            .text_size(rems(theme::FONT_SIZE / theme::REM))
            .when(closing, |this| this.opacity(0.6))
            .child(title)
            .child(notice)
            .when(ready, |this| {
                this.child(
                    div()
                        .relative()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .gap(px(if narrow { 0. } else { FRAME_GAP }))
                        .px(px(if narrow { 0. } else { FRAME_GAP }))
                        .when(browser_visible && !narrow, |this| {
                            this.child(self.browser.clone())
                        })
                        .child(self.render_editor(narrow, cx))
                        .when(browser_visible && narrow, |this| {
                            this.child(
                                div()
                                    .id("browser-overlay")
                                    .absolute()
                                    .inset_0()
                                    .bg(black().opacity(0.5))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.mobile_browser_open = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .left_0()
                                    .flex()
                                    .child(self.browser.clone()),
                            )
                        }),
                )
            })
            .when(!ready, |this| this.child(div().flex_1()))
            .child(status)
    }
}
