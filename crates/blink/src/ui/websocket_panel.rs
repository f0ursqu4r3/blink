//! The WebSocket log and composer. Port of `WebSocketPanel.vue`.

use gpui_kit::*;

use crate::store::Store;

pub struct WebSocketPanel {
    store: Entity<Store>,
    session_id: u64,
    _subscriptions: Vec<Subscription>,
}

impl WebSocketPanel {
    pub fn new(store: Entity<Store>, session_id: u64, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        WebSocketPanel {
            store,
            session_id,
            _subscriptions,
        }
    }
}

impl Render for WebSocketPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let _ = &self.store;
        div().size_full()
    }
}
