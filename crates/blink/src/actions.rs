//! Application actions and their shortcuts. The shortcut table matches the
//! README and the command list in `App.vue`.

use gpui_kit::component::input;
use gpui_kit::{Action, App, KeyBinding, Menu, MenuItem, OsAction, SystemMenuType, actions};

actions!(
    blink,
    [
        NewRequest,
        DuplicateRequest,
        CloseTab,
        NextTab,
        PreviousTab,
        SendRequest,
        CancelRequest,
        ToggleLayout,
        SearchRequests,
        RunCommand,
        FocusUrl,
        OpenSettings,
        ReopenClosedTab,
        UndoDelete,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        FindInResponse,
        ShowCompletions,
        Unfocus,
        Quit,
    ]
);

actions!(blink, [Hide, HideOthers, ShowAll, Minimize, Zoom]);

actions!(
    blink,
    [
        ToggleBrowser,
        ImportFile,
        ManageCookies,
        NewGroup,
        CheckForUpdates,
        CollapseAllGroups,
    ]
);

/// Open group settings for a group. Dispatched by the Browser, the
/// environment badge, and the command center; the root view opens the dialog.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = blink, no_json)]
pub struct OpenGroupSettings {
    pub group_id: u64,
}

/// Show a request in the Browser: open the Browser, expand its groups, and
/// select its row.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = blink, no_json)]
pub struct RevealRequest {
    pub id: u64,
}

/// Run a command-center command by id, such as `copy-as-curl`.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = blink, no_json)]
pub struct RunCommandId {
    pub id: String,
}

/// Key context of the root view. Global shortcuts bind here so any focused
/// descendant receives them.
pub const APP_CONTEXT: &str = "BlinkApp";
/// Key context of the open command center popover.
pub const COMMAND_CENTER_CONTEXT: &str = "CommandCenter";
/// App shortcuts: not while the command center or a menu is open, as
/// `onKey` in `App.vue` returned early for those surfaces.
const SHORTCUTS: &str = "BlinkApp && !CommandCenter && !PopupMenu";
/// Key context of the read-only response body text.
pub const CODE_VIEW_CONTEXT: &str = "CodeView";
/// Key context of the body and variables editors.
pub const BODY_EDITOR_CONTEXT: &str = "BodyEditor";
/// Undo delete and Escape unfocus: also not in text fields, which keep their
/// own Cmd+Z and Escape (`isEditable`).
const OUTSIDE_INPUTS: &str = "BlinkApp && !CommandCenter && !PopupMenu && !Input";

/// The macOS menu bar, as Tauri's default menu gave the Vue app.
fn menus() -> Vec<Menu> {
    vec![
        Menu::new("Blink").items([
            MenuItem::action("Check for Updates…", CheckForUpdates),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Hide Blink", Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit Blink", Quit),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", input::Undo, OsAction::Undo),
            MenuItem::os_action("Redo", input::Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", input::Cut, OsAction::Cut),
            MenuItem::os_action("Copy", input::Copy, OsAction::Copy),
            MenuItem::os_action("Paste", input::Paste, OsAction::Paste),
            MenuItem::os_action("Select All", input::SelectAll, OsAction::SelectAll),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", Zoom),
        ]),
    ]
}

pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Minimize, cx| {
        if let Some(window) = cx.active_window() {
            window
                .update(cx, |_, window, _| window.minimize_window())
                .ok();
        }
    });
    cx.on_action(|_: &Zoom, cx| {
        if let Some(window) = cx.active_window() {
            window.update(cx, |_, window, _| window.zoom_window()).ok();
        }
    });
    cx.set_menus(menus());
    let context = Some(SHORTCUTS);
    // The request and response keys of `RequestWorkspace.vue` and
    // `ResponsePanel.vue` stood down only for dialogs and menus.
    let pane = Some("BlinkApp && !PopupMenu");
    cx.bind_keys([
        KeyBinding::new("secondary-t", NewRequest, context),
        KeyBinding::new("secondary-shift-d", DuplicateRequest, context),
        KeyBinding::new("secondary-w", CloseTab, context),
        KeyBinding::new("ctrl-tab", NextTab, context),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, context),
        KeyBinding::new("secondary-enter", SendRequest, pane),
        KeyBinding::new("secondary-.", CancelRequest, pane),
        KeyBinding::new("secondary-\\", ToggleLayout, context),
        KeyBinding::new("secondary-p", SearchRequests, context),
        KeyBinding::new("secondary-shift-p", RunCommand, context),
        KeyBinding::new("secondary-l", FocusUrl, pane),
        KeyBinding::new("secondary-,", OpenSettings, context),
        KeyBinding::new("secondary-shift-t", ReopenClosedTab, context),
        KeyBinding::new("secondary-=", ZoomIn, context),
        KeyBinding::new("secondary-+", ZoomIn, context),
        KeyBinding::new("secondary--", ZoomOut, context),
        KeyBinding::new("secondary-0", ZoomReset, context),
        KeyBinding::new("secondary-f", FindInResponse, pane),
        KeyBinding::new("secondary-z", UndoDelete, Some(OUTSIDE_INPUTS)),
        KeyBinding::new("escape", Unfocus, Some(OUTSIDE_INPUTS)),
        KeyBinding::new("secondary-q", Quit, None),
        // CodeMirror's `startCompletion`.
        KeyBinding::new("ctrl-space", ShowCompletions, Some(BODY_EDITOR_CONTEXT)),
        // The Edit menu's Copy and Select All, for the response body text.
        KeyBinding::new("secondary-c", input::Copy, Some(CODE_VIEW_CONTEXT)),
        KeyBinding::new("secondary-a", input::SelectAll, Some(CODE_VIEW_CONTEXT)),
    ]);
}
