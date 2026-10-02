//! Sending requests and WebSocket traffic. Port of the async parts of
//! `useRequestRunner`, `useWebSocket`, and the primary action of
//! `RequestWorkspace.vue`; the pure parts live in `blink_core::runner`.

use blink_core::history::now_ms;
use blink_core::response_tokens::values_for;
use std::time::Duration;

use blink_core::engine::{SocketEvent, StreamMessage};
use blink_core::environments::request_environment;
use blink_core::runner::{
    PrimaryAction, SendTicket, apply_socket_event, apply_stream_message, begin_send,
    can_send_socket, connect_socket, disconnect_socket, finish_send, orphaned_body, prepare_send,
    primary_action, socket_active, socket_sent, tick,
};
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use futures::{FutureExt as _, StreamExt as _, select_biased};
use gpui_kit::*;

use crate::store::Store;

/// A send in flight: the engine request id and the pure send state.
pub struct InFlight {
    request_id: String,
    ticket: SendTicket,
    _task: Task<()>,
    _clock: Task<()>,
}

/// How often the elapsed time of a running send updates.
const CLOCK: Duration = Duration::from_millis(100);

impl Store {
    /// A protected environment waits for this before the first send.
    pub fn confirming(&self, session_id: u64) -> bool {
        self.confirming.contains(&session_id)
    }

    /// Send the request, or connect or disconnect its WebSocket. Asks before
    /// the first send to a protected environment. `primary` in
    /// `RequestWorkspace.vue`.
    pub fn send(&mut self, session_id: u64, _window: &mut Window, cx: &mut Context<Self>) {
        let workspace = &self.workspace;
        let Some(session) = workspace.session(session_id) else {
            return;
        };
        let websocket = blink_core::websocket_log::is_web_socket_url(&session.draft.url);
        let environment = request_environment(session.group_id, &workspace.groups);
        let confirmed = workspace.environment_confirmed(session.group_id);
        match primary_action(session, websocket, environment, confirmed) {
            PrimaryAction::ConfirmEnvironment => {
                self.confirming.insert(session_id);
                cx.notify();
            }
            PrimaryAction::Send => self.send_http(session_id, cx),
            PrimaryAction::Connect => self.connect(session_id, cx),
            PrimaryAction::Disconnect => self.disconnect(session_id, cx),
        }
    }

    /// The user confirmed the protected environment: remember it, then run.
    pub fn confirm_and_send(
        &mut self,
        session_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirming.remove(&session_id);
        let environment_id = self.workspace.session(session_id).and_then(|session| {
            request_environment(session.group_id, &self.workspace.groups).map(|env| env.id)
        });
        if let Some(id) = environment_id {
            self.update_workspace(cx, |workspace| workspace.confirm_environment(id));
        }
        self.send(session_id, window, cx);
    }

    pub fn dismiss_confirm(&mut self, session_id: u64, cx: &mut Context<Self>) {
        if self.confirming.remove(&session_id) {
            cx.notify();
        }
    }

    /// Cancel a running send or close a WebSocket. The draft stays editable.
    pub fn cancel(&mut self, session_id: u64, cx: &mut Context<Self>) {
        if let Some(flight) = self.in_flight.get(&session_id) {
            self.engine.cancel_request(&flight.request_id);
        } else if self
            .workspace
            .session(session_id)
            .is_some_and(socket_active)
        {
            self.disconnect(session_id, cx);
        }
    }

    fn send_http(&mut self, session_id: u64, cx: &mut Context<Self>) {
        let workspace = &self.workspace;
        let Some(session) = workspace.session(session_id) else {
            return;
        };
        let sources = workspace.token_sources(now_ms());
        let prepared = prepare_send(session, &sources, &workspace.preferences);
        let options = prepared.options.clone();
        let Some(ticket) = self.update_workspace(cx, |workspace| {
            begin_send(workspace.session_mut(session_id)?, &prepared.http, &options)
        }) else {
            return;
        };
        if let Some(body_id) = &ticket.released {
            self.engine.release_response(body_id);
        }
        let request_id = self.engine.next_id("request");
        let (sender, receiver) = unbounded();
        let response = self.engine.send_request(
            ticket.request.clone(),
            options,
            request_id.clone(),
            Some(sender),
        );
        let task = cx.spawn(async move |this, cx| {
            let mut stream: UnboundedReceiver<StreamMessage> = receiver;
            let mut response = Box::pin(response).fuse();
            let result = loop {
                select_biased! {
                    message = stream.next() => {
                        let Some(message) = message else { continue };
                        if this
                            .update(cx, |this, cx| this.apply_stream(session_id, message, cx))
                            .is_err()
                        {
                            return;
                        }
                    }
                    result = response => break result,
                }
            };
            // Messages that arrived before the result still belong to it.
            while let Ok(message) = stream.try_recv() {
                if this
                    .update(cx, |this, cx| this.apply_stream(session_id, message, cx))
                    .is_err()
                {
                    return;
                }
            }
            this.update(cx, |this, cx| this.finish(session_id, result, cx))
                .ok();
        });
        let clock = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(CLOCK).await;
                let running = this
                    .update(cx, |this, cx| {
                        let Some(flight) = this.in_flight.get(&session_id) else {
                            return false;
                        };
                        if let Some(session) = this.workspace.session_mut(session_id) {
                            tick(session, &flight.ticket);
                        }
                        // Elapsed time is not saved: re-render only.
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        });
        self.in_flight.insert(
            session_id,
            InFlight {
                request_id,
                ticket,
                _task: task,
                _clock: clock,
            },
        );
    }

    /// A live event stream message. Not saved: re-render only.
    fn apply_stream(&mut self, session_id: u64, message: StreamMessage, cx: &mut Context<Self>) {
        let Some(flight) = self.in_flight.get_mut(&session_id) else {
            return;
        };
        if let Some(session) = self.workspace.session_mut(session_id) {
            apply_stream_message(session, &mut flight.ticket, message);
            cx.notify();
        }
    }

    fn finish(
        &mut self,
        session_id: u64,
        result: Result<blink_core::model::ApiResponse, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(flight) = self.in_flight.remove(&session_id) else {
            return;
        };
        let group_id = self.workspace.session(session_id).map(|s| s.group_id);
        let Some(group_id) = group_id else {
            // A result for a deleted request has no owner, so free its body.
            if let Some(body_id) = orphaned_body(&result) {
                self.engine.release_response(&body_id);
            }
            return;
        };
        let recorded = self.update_workspace(cx, |workspace| {
            let Some(session) = workspace.session_mut(session_id) else {
                return false;
            };
            let fingerprint = session.sent_fingerprint.clone();
            let completed = result.is_ok();
            let outcome = finish_send(session, flight.ticket, result);
            let values = workspace
                .session(session_id)
                .and_then(|session| session.response.as_ref())
                .filter(|response| completed && (200..=299).contains(&response.status))
                .map(|response| values_for(session_id, workspace.all_response_tokens(), response));
            if !outcome.captured.is_empty() {
                workspace.capture(group_id, &outcome.captured);
            }
            let recorded = match values {
                Some(values) if workspace.is_token_source(session_id) => {
                    workspace.response_cache.record(
                        session_id,
                        &fingerprint,
                        now_ms() as u64,
                        values,
                    );
                    true
                }
                _ => false,
            };
            workspace.refresh_all_stale();
            recorded
        });
        if recorded {
            self.save_response_cache();
        }
    }

    // ── WebSocket ───────────────────────────────────────────────────────────

    fn connect(&mut self, session_id: u64, cx: &mut Context<Self>) {
        let workspace = &self.workspace;
        let Some(session) = workspace.session(session_id) else {
            return;
        };
        let sources = workspace.token_sources(now_ms());
        let prepared = prepare_send(session, &sources, &workspace.preferences);
        let Ok(request) = prepared.socket else {
            return;
        };
        let timeout = prepared.options.connect_timeout_seconds;
        let started = self.update_workspace(cx, |workspace| {
            workspace
                .session_mut(session_id)
                .is_some_and(|session| connect_socket(session, &request))
        });
        if !started {
            return;
        }
        let connection_id = self.engine.next_id("socket");
        match self.engine.ws_connect(
            connection_id.clone(),
            &request.url,
            request.headers.clone(),
            timeout,
        ) {
            Ok(mut events) => {
                let task = cx.spawn(async move |this, cx| {
                    while let Some(event) = events.next().await {
                        let ended = this
                            .update(cx, |this, cx| this.apply_socket(session_id, event, cx))
                            .unwrap_or(true);
                        if ended {
                            break;
                        }
                    }
                });
                self.sockets.insert(session_id, (connection_id, task));
            }
            Err(message) => {
                self.apply_socket(session_id, SocketEvent::Error { message }, cx);
            }
        }
    }

    /// A socket event. True when the connection ended.
    fn apply_socket(
        &mut self,
        session_id: u64,
        event: SocketEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let ended = self
            .workspace
            .session_mut(session_id)
            .is_none_or(|session| apply_socket_event(session, event));
        if ended {
            self.sockets.remove(&session_id);
        }
        // The socket log is not saved: re-render only.
        cx.notify();
        ended
    }

    fn disconnect(&mut self, session_id: u64, cx: &mut Context<Self>) {
        let connection = self.sockets.get(&session_id).map(|(id, _)| id.clone());
        let close = self
            .workspace
            .session_mut(session_id)
            .is_some_and(|session| disconnect_socket(session, connection.is_some()));
        if let (true, Some(connection)) = (close, connection) {
            self.engine.ws_close(&connection);
        }
        cx.notify();
    }

    /// Send one WebSocket message; a sent message clears the message box.
    pub fn ws_send(&mut self, session_id: u64, text: String, cx: &mut Context<Self>) {
        let Some(connection) = self.sockets.get(&session_id).map(|(id, _)| id.clone()) else {
            return;
        };
        let can_send = self
            .workspace
            .session(session_id)
            .is_some_and(|session| can_send_socket(session, true, &text));
        if !can_send {
            return;
        }
        let result = self.engine.ws_send(&connection, text.clone());
        self.update_workspace(cx, |workspace| {
            if let Some(session) = workspace.session_mut(session_id) {
                socket_sent(session, &text, result);
            }
        });
        cx.emit(crate::store::StoreEvent::DraftReplaced(session_id));
    }
}
