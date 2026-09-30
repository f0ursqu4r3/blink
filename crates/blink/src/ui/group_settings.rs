//! Group settings. Port of `GroupSettingsDialog.vue` and `EnvironmentTokensEditor.vue`.

use gpui_kit::*;

use crate::store::Store;

/// Open the dialog.
pub fn open(store: Entity<Store>, group_id: u64, _window: &mut Window, _cx: &mut App) {
    let _ = store;
}
