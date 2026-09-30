//! The command center in the title bar. Port of `CommandCenter.vue`.

use gpui_kit::*;

use crate::store::Store;

pub struct CommandCenter {
    store: Entity<Store>,
    _subscriptions: Vec<Subscription>,
}

impl CommandCenter {
    pub fn new(store: Entity<Store>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        CommandCenter {
            store,
            _subscriptions,
        }
    }
}

impl CommandCenter {
    /// Open with `query`: "" searches requests, ">" lists commands.
    pub fn open(&mut self, _query: &str, _window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }
}

impl Render for CommandCenter {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let _ = &self.store;
        div().size_full()
    }
}
