//! Headless flow through the real WebSocket panel against a local echo server.

use blink_core::model::{SocketDirection, SocketState};
use gpui_kit::{Entity, TestAppContext};

use super::{WebSocketPanel, counts};
use crate::actions::SendRequest;
use crate::test_support::{self, Harness, wait};

/// A WebSocket server that echoes every text message.
fn echo_server() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            std::thread::spawn(move || {
                let Ok(mut socket) = tungstenite::accept(stream) else {
                    return;
                };
                while let Ok(message) = socket.read() {
                    match message {
                        tungstenite::Message::Text(text) => {
                            let reply = format!("echo: {text}");
                            if socket.send(tungstenite::Message::text(reply)).is_err() {
                                return;
                            }
                        }
                        tungstenite::Message::Close(_) => {
                            let _ = socket.flush();
                            return;
                        }
                        _ => {}
                    }
                }
            });
        }
    });
    format!("ws://{address}/socket")
}

fn panel(harness: &Harness, cx: &TestAppContext) -> Entity<WebSocketPanel> {
    let id = harness.active_id(cx);
    cx.read(|cx| {
        harness
            .app
            .read(cx)
            .pane(id)
            .expect("pane")
            .read(cx)
            .websocket()
            .expect("the WebSocket panel is shown for a ws:// URL")
            .clone()
    })
}

fn socket_state(harness: &Harness, cx: &TestAppContext) -> Option<SocketState> {
    harness.session(cx, |s| s.socket.as_ref().map(|socket| socket.state))
}

#[gpui_kit::test]
fn connects_sends_logs_clears_and_disconnects(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let url = echo_server();
    let harness = test_support::open(cx, &engine);
    harness.edit_draft(cx, |draft| draft.url = url);
    harness.draw(cx);
    let panel = panel(&harness, cx);

    // Connect with the primary action.
    harness.dispatch(cx, SendRequest);
    wait(cx, "open", |cx| {
        let id = harness.store.read(cx).workspace.shown_active_id().unwrap();
        let session = harness.store.read(cx).workspace.session(id).unwrap();
        session
            .socket
            .as_ref()
            .is_some_and(|s| s.state == SocketState::Open)
    });
    harness.draw(cx);
    let label = harness.session(cx, |s| super::state_label(s.socket.as_ref()));
    assert_eq!(label, "CONNECTED");

    // Type in the panel's message box, then send from the panel.
    harness.update(cx, |window, cx| {
        let message = panel.read(cx).message.clone();
        message.update(cx, |message, cx| message.focus(window, cx));
    });
    harness.draw(cx);
    cx.simulate_input(harness.window, "hello");
    harness.draw(cx);
    assert_eq!(harness.session(cx, |s| s.draft.body.clone()), "hello");
    panel.update(cx, |panel, cx| panel.submit(cx));
    wait(cx, "echo", |cx| {
        let id = harness.store.read(cx).workspace.shown_active_id().unwrap();
        let session = harness.store.read(cx).workspace.session(id).unwrap();
        session.socket.as_ref().is_some_and(|socket| {
            socket
                .messages
                .iter()
                .any(|m| m.direction == SocketDirection::In && m.text == "echo: hello")
        })
    });
    harness.draw(cx);
    let messages = harness.session(cx, |s| s.socket.as_ref().unwrap().messages.clone());
    let out = messages
        .iter()
        .position(|m| m.direction == SocketDirection::Out && m.text == "hello")
        .expect("the sent message is logged");
    let echo = messages
        .iter()
        .position(|m| m.direction == SocketDirection::In && m.text == "echo: hello")
        .unwrap();
    assert!(out < echo);
    assert_eq!(counts(&messages), (1, 1), "received, sent");
    // A sent message clears the message box.
    assert_eq!(harness.session(cx, |s| s.draft.body.clone()), "");
    assert_eq!(
        cx.read(|cx| panel.read(cx).message.read(cx).value().to_string()),
        ""
    );
    assert_eq!(cx.read(|cx| panel.read(cx).shown.1), messages.len());

    // Clear the log.
    panel.update(cx, |panel, cx| panel.clear(cx));
    harness.draw(cx);
    let messages = harness.session(cx, |s| s.socket.as_ref().unwrap().messages.clone());
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(counts(&messages), (0, 0));
    assert_eq!(cx.read(|cx| panel.read(cx).shown.1), 0);
    assert_eq!(socket_state(&harness, cx), Some(SocketState::Open));

    // Disconnect with the primary action.
    harness.dispatch(cx, SendRequest);
    wait(cx, "closed", |cx| {
        let id = harness.store.read(cx).workspace.shown_active_id().unwrap();
        let session = harness.store.read(cx).workspace.session(id).unwrap();
        session
            .socket
            .as_ref()
            .is_some_and(|s| s.state == SocketState::Closed)
    });
    harness.draw(cx);
    assert!(cx.read(|cx| harness.store.read(cx).sockets.is_empty()));
    let label = harness.session(cx, |s| super::state_label(s.socket.as_ref()));
    assert_eq!(label, "CLOSED");
    // Sending while closed does nothing.
    harness.update(cx, |window, cx| {
        let message = panel.read(cx).message.clone();
        message.update(cx, |message, cx| message.set_value("late", window, cx));
    });
    panel.update(cx, |panel, cx| panel.submit(cx));
    harness.draw(cx);
    let messages = harness.session(cx, |s| s.socket.as_ref().unwrap().messages.clone());
    assert!(!messages.iter().any(|m| m.text == "late"));
}
