//! The request Browser sidebar. Port of `RequestBrowser.vue`, `GroupActionsMenu.vue`, `GroupMenuItems.vue`, `GroupMenuTree.vue`, `TreeGuides.vue`, `EnvironmentBadge.vue`, and `DragPreview.vue`.

use gpui_kit::*;

use crate::store::Store;

pub struct Browser {
    store: Entity<Store>,
    _subscriptions: Vec<Subscription>,
}

impl Browser {
    pub fn new(store: Entity<Store>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        Browser {
            store,
            _subscriptions,
        }
    }
}

impl Render for Browser {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let _ = &self.store;
        div().size_full()
    }
}
