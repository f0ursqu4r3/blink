//! The cookie jar. Port of `CookiesDialog.vue`.

use gpui_kit::*;

use crate::store::Store;

/// Open the dialog.
pub fn open(store: Entity<Store>, _window: &mut Window, _cx: &mut App) {
    let _ = store;
}
