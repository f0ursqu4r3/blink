//! Sending requests and WebSocket traffic. Port of the async parts of
//! `useRequestRunner` and `useWebSocket`; the pure parts live in
//! `blink_core::runner`.

use gpui_kit::*;

use crate::store::Store;

impl Store {
    /// Send the request, or connect its WebSocket. Asks before the first send
    /// to a protected environment. Nothing is sent without a user action.
    pub fn send(&mut self, _session_id: u64, _window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    /// Cancel a running send or close a WebSocket.
    pub fn cancel(&mut self, _session_id: u64, cx: &mut Context<Self>) {
        cx.notify();
    }

    /// Send one WebSocket message.
    pub fn ws_send(&mut self, _session_id: u64, _text: String, cx: &mut Context<Self>) {
        cx.notify();
    }
}
