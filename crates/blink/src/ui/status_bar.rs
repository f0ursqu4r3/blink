//! The 24 px status bar of `App.vue`, and `WorkspaceStorageNotice.vue`.

use gpui_kit::*;

use crate::store::Store;

/// The status bar for the current store state.
pub fn render(store: &Entity<Store>, _window: &mut Window, _cx: &mut App) -> AnyElement {
    let _ = store;
    div().into_any_element()
}

/// The storage notice above the workspace: load and save failures.
pub fn render_storage_notice(store: &Entity<Store>, _window: &mut Window, _cx: &mut App) -> AnyElement {
    let _ = store;
    div().into_any_element()
}
