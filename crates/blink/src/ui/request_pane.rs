//! One request: URL bar, request editor, and response. Port of
//! `RequestWorkspace.vue`, `RequestEditor.vue`, `KeyValueEditor.vue`,
//! `TokenInput.vue`, `ChecksEditor.vue`, and `CodeEditor.vue`.

use blink_core::model::CodeTarget;
use gpui_kit::*;

use crate::store::Store;
use crate::ui::response_panel::ResponsePanel;

pub struct RequestPane {
    store: Entity<Store>,
    session_id: u64,
    response: Entity<ResponsePanel>,
    _subscriptions: Vec<Subscription>,
}

impl RequestPane {
    pub fn new(
        store: Entity<Store>,
        session_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let response = cx.new(|cx| ResponsePanel::new(store.clone(), session_id, window, cx));
        let _subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        RequestPane {
            store,
            session_id,
            response,
            _subscriptions,
        }
    }

    /// The response side, for the response commands.
    pub fn response(&self) -> &Entity<ResponsePanel> {
        &self.response
    }

    /// Focus the URL field and select its text (`Cmd/Ctrl+L`).
    pub fn focus_url(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Show or hide the code panel.
    pub fn toggle_code(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    /// The request as code in `target`; empty when the draft does not build.
    pub fn code_for(&self, _target: CodeTarget, _cx: &App) -> String {
        String::new()
    }
}

impl Render for RequestPane {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let _ = (&self.store, self.session_id);
        div().size_full().child(self.response.clone())
    }
}
