//! The request tab strip. Port of `RequestTabs.vue`.

use gpui_kit::*;

use crate::store::Store;

pub struct RequestTabs {
    store: Entity<Store>,
    _subscriptions: Vec<Subscription>,
}

impl RequestTabs {
    pub fn new(store: Entity<Store>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        RequestTabs {
            store,
            _subscriptions,
        }
    }
}

impl Render for RequestTabs {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let _ = &self.store;
        div().size_full()
    }
}
