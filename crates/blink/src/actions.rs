//! Application actions and their shortcuts. The shortcut table matches the
//! README and the command list in `App.vue`.

use gpui_kit::{App, KeyBinding, actions};

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
        Unfocus,
        Quit,
    ]
);

/// Key context of the root view. Global shortcuts bind here so any focused
/// descendant receives them.
pub const APP_CONTEXT: &str = "BlinkApp";

pub fn init(cx: &mut App) {
    let context = Some(APP_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("secondary-t", NewRequest, context),
        KeyBinding::new("secondary-shift-d", DuplicateRequest, context),
        KeyBinding::new("secondary-w", CloseTab, context),
        KeyBinding::new("ctrl-tab", NextTab, context),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, context),
        KeyBinding::new("secondary-enter", SendRequest, context),
        KeyBinding::new("secondary-.", CancelRequest, context),
        KeyBinding::new("secondary-\\", ToggleLayout, context),
        KeyBinding::new("secondary-p", SearchRequests, context),
        KeyBinding::new("secondary-shift-p", RunCommand, context),
        KeyBinding::new("secondary-l", FocusUrl, context),
        KeyBinding::new("secondary-,", OpenSettings, context),
        KeyBinding::new("secondary-shift-t", ReopenClosedTab, context),
        KeyBinding::new("secondary-=", ZoomIn, context),
        KeyBinding::new("secondary-+", ZoomIn, context),
        KeyBinding::new("secondary--", ZoomOut, context),
        KeyBinding::new("secondary-0", ZoomReset, context),
        KeyBinding::new("secondary-f", FindInResponse, context),
        KeyBinding::new("escape", Unfocus, context),
        KeyBinding::new("secondary-q", Quit, None),
    ]);
    // Undo delete only outside text fields: inputs keep their own Cmd+Z.
    cx.bind_keys([KeyBinding::new(
        "secondary-z",
        UndoDelete,
        Some("BlinkApp && !Input"),
    )]);
}
