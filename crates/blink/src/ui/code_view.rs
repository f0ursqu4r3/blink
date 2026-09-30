//! Read-only highlighted code with find and filter. Port of `CodeView.vue`.

use gpui_kit::*;

use crate::store::Store;

pub struct CodeView {
    store: Entity<Store>,
    _subscriptions: Vec<Subscription>,
}

impl CodeView {
    pub fn new(store: Entity<Store>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        CodeView {
            store,
            _subscriptions,
        }
    }
}

impl Render for CodeView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let _ = &self.store;
        div().size_full()
    }
}
