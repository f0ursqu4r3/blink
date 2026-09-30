//! Port of the WebSocket log and state rules from `src/lib/websocket.ts`
//! and `src/composables/useWebSocket.ts`. The transport lives in `engine`.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::ids::SOCKET_MESSAGES;
use crate::model::{
    Header, SOCKET_MESSAGE_LIMIT, SocketDirection, SocketMessage, SocketSession, SocketState,
};

pub fn is_web_socket_url(url: &str) -> bool {
    let url = url.trim().as_bytes();
    let scheme = |prefix: &[u8]| {
        url.len() >= prefix.len() && url[..prefix.len()].eq_ignore_ascii_case(prefix)
    };
    scheme(b"ws://") || scheme(b"wss://")
}

/// What the transport reports for a connection.
#[derive(Debug, Clone, PartialEq)]
pub enum SocketEvent {
    Open {
        status: u16,
        headers: Vec<(String, String)>,
    },
    Message {
        text: String,
        binary: bool,
        size: u64,
    },
    Close {
        code: Option<u16>,
        reason: String,
    },
    Error {
        message: String,
    },
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64() * 1000.0)
}

/// Add a message, dropping the oldest past the limit. `size` defaults to
/// the text length in bytes.
pub fn log(
    socket: &mut SocketSession,
    direction: SocketDirection,
    text: &str,
    binary: bool,
    size: Option<u64>,
) {
    socket.messages.push(SocketMessage {
        id: SOCKET_MESSAGES.next(),
        direction,
        text: text.to_string(),
        at: now_ms(),
        binary,
        size: size.unwrap_or(text.len() as u64),
    });
    if socket.messages.len() > SOCKET_MESSAGE_LIMIT {
        let excess = socket.messages.len() - SOCKET_MESSAGE_LIMIT;
        socket.messages.drain(..excess);
    }
}

pub fn is_active(socket: Option<&SocketSession>) -> bool {
    socket.is_some_and(|socket| matches!(socket.state, SocketState::Open | SocketState::Connecting))
}

/// A connecting session that keeps the earlier log. Call only when not
/// active.
pub fn begin_connect(previous: Option<SocketSession>, url: &str) -> SocketSession {
    let mut socket = SocketSession {
        state: SocketState::Connecting,
        messages: previous.map(|socket| socket.messages).unwrap_or_default(),
        headers: vec![],
    };
    log(
        &mut socket,
        SocketDirection::System,
        &format!("Connecting to {url}"),
        false,
        None,
    );
    socket
}

/// Apply a transport event. True when the connection ended, so the caller
/// drops its handle.
pub fn apply_event(socket: &mut SocketSession, event: SocketEvent) -> bool {
    match event {
        SocketEvent::Open { status, headers } => {
            socket.state = SocketState::Open;
            socket.headers = headers
                .into_iter()
                .map(|(key, value)| Header { key, value })
                .collect();
            log(
                socket,
                SocketDirection::System,
                &format!("Connected · {status}"),
                false,
                None,
            );
            false
        }
        SocketEvent::Message { text, binary, size } => {
            log(socket, SocketDirection::In, &text, binary, Some(size));
            false
        }
        SocketEvent::Close { code, reason } => {
            socket.state = SocketState::Closed;
            let code = code.map_or_else(String::new, |code| format!(" · {code}"));
            let reason = if reason.is_empty() {
                String::new()
            } else {
                format!(" · {reason}")
            };
            log(
                socket,
                SocketDirection::System,
                &format!("Closed{code}{reason}"),
                false,
                None,
            );
            true
        }
        SocketEvent::Error { message } => {
            socket.state = SocketState::Closed;
            log(socket, SocketDirection::System, &message, false, None);
            true
        }
    }
}

/// Mark the session closing; the caller then closes its handle.
pub fn begin_disconnect(socket: &mut SocketSession) {
    socket.state = SocketState::Closing;
}

/// Sending needs an open connection and some text.
pub fn can_send(socket: Option<&SocketSession>, text: &str) -> bool {
    socket.is_some_and(|socket| socket.state == SocketState::Open) && !text.is_empty()
}

/// Log the outcome of a send.
pub fn log_send(socket: &mut SocketSession, text: &str, result: Result<(), String>) {
    match result {
        Ok(()) => log(socket, SocketDirection::Out, text, false, None),
        Err(error) => log(socket, SocketDirection::System, &error, false, None),
    }
}

pub fn clear(socket: &mut SocketSession) {
    socket.messages.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ws_and_wss_urls() {
        assert!(is_web_socket_url(" wss://a.test/x"));
        assert!(is_web_socket_url("WS://a.test"));
        assert!(!is_web_socket_url("https://a.test"));
    }

    #[test]
    fn connects_logs_messages_both_ways_and_closes() {
        let mut socket = begin_connect(None, "ws://chat.test");
        assert_eq!(socket.state, SocketState::Connecting);
        assert!(!can_send(Some(&socket), "early"));
        assert!(!apply_event(
            &mut socket,
            SocketEvent::Open {
                status: 101,
                headers: vec![("x-a".into(), "1".into())]
            }
        ));
        assert_eq!(socket.state, SocketState::Open);
        assert_eq!(
            socket.headers,
            [Header {
                key: "x-a".into(),
                value: "1".into()
            }]
        );
        assert!(can_send(Some(&socket), "hi"));
        log_send(&mut socket, "hi", Ok(()));
        apply_event(
            &mut socket,
            SocketEvent::Message {
                text: "yo".into(),
                binary: false,
                size: 2,
            },
        );
        begin_disconnect(&mut socket);
        assert_eq!(socket.state, SocketState::Closing);
        assert!(apply_event(
            &mut socket,
            SocketEvent::Close {
                code: Some(1000),
                reason: "bye".into()
            }
        ));
        assert_eq!(socket.state, SocketState::Closed);
        let log: Vec<(SocketDirection, &str)> = socket
            .messages
            .iter()
            .map(|m| (m.direction, m.text.as_str()))
            .collect();
        assert_eq!(
            log,
            [
                (SocketDirection::System, "Connecting to ws://chat.test"),
                (SocketDirection::System, "Connected · 101"),
                (SocketDirection::Out, "hi"),
                (SocketDirection::In, "yo"),
                (SocketDirection::System, "Closed · 1000 · bye"),
            ]
        );
        let reconnected = begin_connect(Some(socket.clone()), "ws://chat.test");
        assert_eq!(reconnected.messages.len(), 6);
        clear(&mut socket);
        assert!(socket.messages.is_empty());
    }

    #[test]
    fn drops_the_oldest_messages_past_the_limit() {
        let mut socket = begin_connect(None, "ws://a.test");
        for index in 0..SOCKET_MESSAGE_LIMIT + 5 {
            log(
                &mut socket,
                SocketDirection::In,
                &index.to_string(),
                false,
                None,
            );
        }
        assert_eq!(socket.messages.len(), SOCKET_MESSAGE_LIMIT);
        // The connecting line and the first five messages dropped.
        assert_eq!(socket.messages[0].text, "5");
    }
}
