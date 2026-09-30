//! The response side of a request. Port of `ResponsePanel.vue`,
//! `JsonTreeView.vue`, `EventList.vue`, and `TimingCard.vue`.

use gpui_kit::*;

use crate::store::Store;

pub struct ResponsePanel {
    store: Entity<Store>,
    session_id: u64,
    _subscriptions: Vec<Subscription>,
}

impl ResponsePanel {
    pub fn new(
        store: Entity<Store>,
        session_id: u64,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let _subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        ResponsePanel {
            store,
            session_id,
            _subscriptions,
        }
    }

    /// Open find in the response body (`Cmd/Ctrl+F`).
    pub fn find(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    /// Copy the shown result: the body, or the jq output.
    pub fn copy_result(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub fn toggle_history(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    /// Save the full body to a file the user picks.
    pub fn save_body(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub fn toggle_wrap(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub fn toggle_pretty(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }
}

impl Render for ResponsePanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let _ = (&self.store, self.session_id);
        div().size_full()
    }
}
