//! Application settings. Port of `ApplicationSettingsDialog.vue` and `ThemeSettings.vue`.

use gpui_kit::*;

use crate::store::Store;

/// Open the dialog.
pub fn open(store: Entity<Store>, _window: &mut Window, _cx: &mut App) {
    let _ = store;
}
