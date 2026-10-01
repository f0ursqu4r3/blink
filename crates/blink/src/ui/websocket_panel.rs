//! The WebSocket log and composer. Port of `WebSocketPanel.vue`.
//!
//! The message box edits the draft body. Connect and Disconnect live in the
//! URL bar (`store.send` / `store.cancel`).

use std::rc::Rc;

use blink_core::history::clock_time;
use blink_core::json::canonical_json;
use blink_core::model::{SocketDirection, SocketMessage, SocketSession, SocketState};
use blink_core::request::format_bytes;
use blink_core::runner::clear_socket;
use blink_core::shortcut::{IS_MAC, shortcut_label};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Enter, InputEvent, Textarea, TextareaState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::{Store, StoreEvent};
use crate::theme;
use crate::ui::response_panel::r;
use crate::ui::widgets::tracked;

/// `stateLabel` in `WebSocketPanel.vue`.
pub fn state_label(socket: Option<&SocketSession>) -> &'static str {
    match socket.map(|socket| socket.state) {
        Some(SocketState::Connecting) => "CONNECTING",
        Some(SocketState::Open) => "CONNECTED",
        Some(SocketState::Closing) => "CLOSING",
        Some(SocketState::Closed) => "CLOSED",
        None => "STANDBY",
    }
}

/// Received and sent message counts.
pub fn counts(messages: &[SocketMessage]) -> (usize, usize) {
    messages.iter().fold((0, 0), |(received, sent), message| {
        match message.direction {
            SocketDirection::In => (received + 1, sent),
            SocketDirection::Out => (received, sent + 1),
            SocketDirection::System => (received, sent),
        }
    })
}

/// JSON on one line; other text as sent.
pub fn display(message: &SocketMessage) -> String {
    if message.direction == SocketDirection::System {
        return message.text.clone();
    }
    canonical_json(&message.text).unwrap_or_else(|| message.text.clone())
}

pub struct WebSocketPanel {
    store: Entity<Store>,
    session_id: u64,
    message: Entity<TextareaState>,
    log: ListState,
    /// First message id and count at the last render.
    shown: (u64, usize),
    _subscriptions: Vec<Subscription>,
}

impl WebSocketPanel {
    pub fn new(
        store: Entity<Store>,
        session_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let body = store
            .read(cx)
            .workspace
            .session(session_id)
            .map(|session| session.draft.body.clone())
            .unwrap_or_default();
        let message = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(3)
                .placeholder("Message text or JSON")
                .default_value(body)
        });
        let log = ListState::new(0, ListAlignment::Top, px(400.));
        // Follow new messages while the log is at the bottom.
        log.set_follow_mode(FollowMode::Tail);
        let _subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe_in(&store, window, |this, store, event, window, cx| {
                if let StoreEvent::DraftReplaced(id) = event
                    && *id == this.session_id
                {
                    let body = store
                        .read(cx)
                        .workspace
                        .session(*id)
                        .map(|session| session.draft.body.clone())
                        .unwrap_or_default();
                    this.message
                        .update(cx, |message, cx| message.set_value(body, window, cx));
                }
            }),
            cx.subscribe(&message, |this, message, event, cx| {
                if let InputEvent::Change = event {
                    let text = message.read(cx).value().to_string();
                    let id = this.session_id;
                    this.store.update(cx, |store, cx| {
                        store.update_workspace(cx, |workspace| {
                            if let Some(session) = workspace.session_mut(id) {
                                session.draft.body = text;
                            }
                        })
                    });
                }
            }),
        ];
        WebSocketPanel {
            store,
            session_id,
            message,
            log,
            shown: (0, 0),
            _subscriptions,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.store.read(cx).workspace.session(self.session_id) else {
            return;
        };
        let open = session
            .socket
            .as_ref()
            .is_some_and(|socket| socket.state == SocketState::Open);
        let text = session.draft.body.clone();
        if open && !text.is_empty() {
            let id = self.session_id;
            self.store
                .update(cx, |store, cx| store.ws_send(id, text, cx));
        }
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        let id = self.session_id;
        self.store.update(cx, |store, cx| {
            store.update_workspace(cx, |workspace| {
                if let Some(session) = workspace.session_mut(id) {
                    clear_socket(session);
                }
            })
        });
    }

    fn sync_log(&mut self, messages: &[SocketMessage]) {
        let first = messages.first().map_or(0, |message| message.id);
        let (was_first, count) = self.shown;
        if first == was_first && messages.len() > count {
            self.log.splice(count..count, messages.len() - count);
        } else if (first, messages.len()) != self.shown {
            let following = self.log.is_following_tail();
            self.log.reset(messages.len());
            if following {
                self.log.set_follow_mode(FollowMode::Tail);
            }
        }
        self.shown = (first, messages.len());
    }
}

impl Render for WebSocketPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let socket = self
            .store
            .read(cx)
            .workspace
            .session(self.session_id)
            .and_then(|session| session.socket.clone());
        let messages: Rc<Vec<SocketMessage>> = Rc::new(
            socket
                .as_ref()
                .map(|socket| socket.messages.clone())
                .unwrap_or_default(),
        );
        self.sync_log(&messages);
        let open = socket
            .as_ref()
            .is_some_and(|socket| socket.state == SocketState::Open);
        let (received, sent) = counts(&messages);
        let has_message = !self.message.read(cx).value().is_empty();

        let header = div()
            .flex()
            .flex_none()
            .h(r(36.))
            .items_center()
            .gap(r(12.))
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.muted)
            .px(r(16.))
            .font_family(theme::MONO)
            .text_size(r(10.))
            .child(
                div()
                    .text_size(r(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors.foreground)
                    .child(tracked("WEBSOCKET", 0.12)),
            )
            .child(
                div()
                    .ml_auto()
                    .text_color(if open {
                        colors.success
                    } else {
                        colors.muted_foreground
                    })
                    .child(tracked(state_label(socket.as_ref()), 0.12)),
            );
        let count =
            |id: &'static str, icon: IconName, color: Hsla, value: usize, tip: &'static str| {
                div()
                    .id(id)
                    .flex()
                    .items_center()
                    .gap(r(4.))
                    .child(Icon::new(icon).size(r(12.)).text_color(color))
                    .child(value.to_string())
                    .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
            };
        let counters = div()
            .flex()
            .flex_none()
            .min_h(r(36.))
            .items_center()
            .gap(r(16.))
            .border_b_1()
            .border_color(colors.border)
            .px(r(16.))
            .font_family(theme::MONO)
            .text_size(r(11.))
            .text_color(colors.foreground)
            .child(count(
                "socket-received",
                IconName::ArrowDown,
                colors.info,
                received,
                "Received",
            ))
            .child(count(
                "socket-sent",
                IconName::ArrowUp,
                colors.primary,
                sent,
                "Sent",
            ))
            .child(
                Button::new("socket-clear")
                    .ghost()
                    .ml_auto()
                    .small()
                    .w(r(28.))
                    .h(r(28.))
                    .text_color(colors.muted_foreground)
                    .w(r(28.))
                    .h(r(28.))
                    .icon(Icon::new(IconName::Eraser).size(r(13.)))
                    .tooltip("Clear messages")
                    .disabled(messages.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
            );
        let rows = messages.clone();
        let log = list(self.log.clone(), move |ix, _, cx| {
            let colors = theme::colors(cx);
            let entry = &rows[ix];
            let (arrow, arrow_color) = match entry.direction {
                SocketDirection::In => ("↓", colors.info),
                SocketDirection::Out => ("↑", colors.primary),
                SocketDirection::System => ("·", colors.muted_foreground),
            };
            let system = entry.direction == SocketDirection::System;
            div()
                .flex()
                .items_start()
                .gap(r(8.))
                .border_b_1()
                .border_color(colors.border)
                .px(r(16.))
                .py(r(6.))
                .line_height(r(19.2))
                .text_color(if system {
                    colors.muted_foreground
                } else {
                    colors.foreground
                })
                .child(
                    div()
                        .w(r(72.))
                        .flex_none()
                        .text_color(colors.muted_foreground)
                        .child(clock_time(entry.at, false)),
                )
                .child(
                    div()
                        .w(r(16.))
                        .flex_none()
                        .text_color(arrow_color)
                        .child(arrow),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .flex_1()
                        .min_w_0()
                        .whitespace_normal()
                        .when(entry.binary, |this| {
                            this.child(
                                div()
                                    .mr(r(4.))
                                    .text_color(colors.muted_foreground)
                                    .child(format!("Binary · {} · ", format_bytes(entry.size))),
                            )
                        })
                        .child(display(entry)),
                )
                .into_any_element()
        })
        .size_full();
        let log = div()
            .flex_1()
            .min_h_0()
            .font_family(theme::MONO)
            .text_size(r(12.))
            .map(|this| {
                if messages.is_empty() {
                    this.child(
                        div()
                            .p(r(16.))
                            .text_size(r(12.))
                            .text_color(colors.muted_foreground)
                            .child("Connect to open the socket. Messages you send and receive show here."),
                    )
                } else {
                    this.relative().child(log).vertical_scrollbar(&self.log)
                }
            });
        let composer = div()
            .flex()
            .flex_none()
            .items_end()
            .gap(r(8.))
            .border_t_1()
            .border_color(colors.border)
            .p(r(8.))
            // Cmd/Ctrl+Enter sends; the window shortcut would connect or
            // disconnect instead.
            .capture_action(cx.listener(|this, action: &Enter, _, cx| {
                if action.secondary {
                    cx.stop_propagation();
                    this.submit(cx);
                }
            }))
            .child(
                div().flex_1().min_w_0().child(
                    Textarea::new(&self.message)
                        .h(r(60.))
                        .font_family(theme::MONO)
                        .text_size(r(12.)),
                ),
            )
            .child(
                Button::new("socket-send")
                    .primary()
                    .xsmall()
                    .h(r(28.))
                    .px(r(10.))
                    .font_family(theme::MONO)
                    .font_weight(FontWeight::MEDIUM)
                    .icon(Icon::new(IconName::SendHorizontal).size(r(14.)))
                    .label("Send")
                    .disabled(!open || !has_message)
                    .tooltip(format!(
                        "Send message · {}",
                        shortcut_label(&["mod", "enter"], IS_MAC)
                    ))
                    .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
            );
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(colors.background)
            .font_family(theme::SANS)
            .child(header)
            .child(counters)
            .child(log)
            .child(composer)
    }
}

#[cfg(test)]
mod ui_tests;

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` exports its own `test` attribute; use the standard one.
    use core::prelude::v1::test;

    fn message(direction: SocketDirection, text: &str) -> SocketMessage {
        SocketMessage {
            id: 1,
            direction,
            text: text.into(),
            at: 0.0,
            binary: false,
            size: text.len() as u64,
        }
    }

    #[test]
    fn labels_the_socket_state() {
        assert_eq!(state_label(None), "STANDBY");
        let socket = SocketSession {
            state: SocketState::Open,
            messages: vec![],
            headers: vec![],
        };
        assert_eq!(state_label(Some(&socket)), "CONNECTED");
    }

    #[test]
    fn counts_and_shows_messages() {
        let messages = [
            message(SocketDirection::In, "{ \"a\": 1 }"),
            message(SocketDirection::Out, "hi"),
            message(SocketDirection::System, "{ \"kept\": true }"),
        ];
        assert_eq!(counts(&messages), (1, 1));
        assert_eq!(display(&messages[0]), r#"{"a":1}"#);
        assert_eq!(display(&messages[2]), "{ \"kept\": true }");
    }
}
