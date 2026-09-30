//! WebSocket connections. The UI opens one per request, receives events
//! over a channel, and sends text messages by connection id.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest,
    http::{HeaderName, HeaderValue},
    protocol::{CloseFrame, Message, frame::coding::CloseCode},
};

use super::http::resolve_environment_references;
use crate::model::Header;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SocketEvent {
    Open {
        status: u16,
        headers: Vec<(String, String)>,
    },
    Message {
        /// The text, or a hex preview of a binary message.
        text: String,
        binary: bool,
        size: usize,
    },
    Close {
        code: Option<u16>,
        reason: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug)]
pub enum Outgoing {
    Text(String),
    Close,
}

/// Open connections by the id the UI gave them.
#[derive(Default, Clone)]
pub struct Sockets(Arc<Mutex<HashMap<String, mpsc::UnboundedSender<Outgoing>>>>);

fn hex_preview(bytes: &[u8]) -> String {
    let shown: Vec<String> = bytes.iter().take(64).map(|b| format!("{b:02x}")).collect();
    let more = if bytes.len() > 64 { " …" } else { "" };
    format!("{}{more}", shown.join(" "))
}

/// Connect, then relay messages both ways until either side closes.
pub async fn run(
    url: String,
    headers: Vec<Header>,
    connect_timeout: Duration,
    emit: impl Fn(SocketEvent),
    mut outgoing: mpsc::UnboundedReceiver<Outgoing>,
) {
    let fail = |message: String| emit(SocketEvent::Error { message });
    let mut request = match url.as_str().into_client_request() {
        Ok(request) => request,
        Err(_) => return fail("Enter a ws:// or wss:// URL.".into()),
    };
    for header in headers {
        let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(header.key.trim().as_bytes()),
            HeaderValue::from_str(&header.value),
        ) else {
            return fail(format!("Invalid header: {}", header.key));
        };
        request.headers_mut().append(name, value);
    }
    let connected =
        tokio::time::timeout(connect_timeout, tokio_tungstenite::connect_async(request)).await;
    let (stream, response) = match connected {
        Err(_) => {
            return fail(format!(
                "Connection timed out after {} s.",
                connect_timeout.as_secs()
            ));
        }
        Ok(Err(error)) => return fail(format!("Connection failed: {error}")),
        Ok(Ok(pair)) => pair,
    };
    emit(SocketEvent::Open {
        status: response.status().as_u16(),
        headers: response
            .headers()
            .iter()
            .map(|(key, value)| {
                (
                    key.to_string(),
                    value.to_str().unwrap_or("[binary value]").to_string(),
                )
            })
            .collect(),
    });
    let (mut write, mut read) = stream.split();
    let mut sending = true;
    loop {
        tokio::select! {
            incoming = read.next() => match incoming {
                Some(Ok(Message::Text(text))) => emit(SocketEvent::Message {
                    size: text.len(),
                    text: text.to_string(),
                    binary: false,
                }),
                Some(Ok(Message::Binary(bytes))) => emit(SocketEvent::Message {
                    size: bytes.len(),
                    text: hex_preview(&bytes),
                    binary: true,
                }),
                Some(Ok(Message::Close(frame))) => {
                    emit(SocketEvent::Close {
                        code: frame.as_ref().map(|frame| u16::from(frame.code)),
                        reason: frame.map(|frame| frame.reason.to_string()).unwrap_or_default(),
                    });
                    return;
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => return fail(format!("Connection lost: {error}")),
                None => {
                    emit(SocketEvent::Close { code: None, reason: String::new() });
                    return;
                }
            },
            next = outgoing.recv(), if sending => match next {
                Some(Outgoing::Text(text)) => {
                    if let Err(error) = write.send(Message::Text(text.into())).await {
                        return fail(format!("Cannot send: {error}"));
                    }
                }
                // Close, or the app dropped the connection: close cleanly and
                // wait for the server's close frame.
                _ => {
                    sending = false;
                    let frame = CloseFrame { code: CloseCode::Normal, reason: "".into() };
                    if write.send(Message::Close(Some(frame))).await.is_err() {
                        emit(SocketEvent::Close { code: Some(1000), reason: String::new() });
                        return;
                    }
                }
            },
        }
    }
}

impl Sockets {
    /// The URL and headers with environment references resolved, or an
    /// error for a URL that is not ws:// or wss://.
    pub fn prepare(url: &str, headers: Vec<Header>) -> Result<(String, Vec<Header>), String> {
        let url = resolve_environment_references(url)?;
        let parsed = reqwest::Url::parse(&url).map_err(|_| "Enter a ws:// or wss:// URL.")?;
        if !matches!(parsed.scheme(), "ws" | "wss") {
            return Err("Enter a ws:// or wss:// URL.".into());
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("Use the Auth tab instead of credentials in the URL.".into());
        }
        let headers = headers
            .into_iter()
            .map(|header| {
                Ok(Header {
                    value: resolve_environment_references(&header.value)?,
                    key: header.key,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok((url, headers))
    }

    /// Register a connection. `run` takes the receiver.
    pub fn insert(&self, connection_id: String) -> mpsc::UnboundedReceiver<Outgoing> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.0.lock().unwrap().insert(connection_id, sender);
        receiver
    }

    pub fn remove(&self, connection_id: &str) {
        self.0.lock().unwrap().remove(connection_id);
    }

    pub fn send(&self, connection_id: &str, text: String) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .get(connection_id)
            .ok_or("Not connected.")?
            .send(Outgoing::Text(text))
            .map_err(|_| "Not connected.".to_string())
    }

    pub fn close(&self, connection_id: &str) {
        if let Some(sender) = self.0.lock().unwrap().get(connection_id) {
            let _ = sender.send(Outgoing::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// A server that echoes text and closes when it receives "bye".
    async fn echo_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            while let Some(Ok(message)) = socket.next().await {
                match message {
                    Message::Text(text) if text.as_str() == "bye" => {
                        let frame = CloseFrame {
                            code: CloseCode::Away,
                            reason: "done".into(),
                        };
                        let _ = socket.close(Some(frame)).await;
                        break;
                    }
                    Message::Text(text) => {
                        socket
                            .send(Message::Text(format!("echo {text}").into()))
                            .await
                            .unwrap();
                        socket
                            .send(Message::Binary(vec![1u8, 255].into()))
                            .await
                            .unwrap();
                    }
                    _ => {}
                }
            }
        });
        format!("ws://{address}")
    }

    #[tokio::test]
    async fn relays_messages_and_reports_the_close() {
        let url = echo_server().await;
        let (sender, receiver) = mpsc::unbounded_channel();
        let events = Arc::new(Mutex::new(Vec::new()));
        let log = events.clone();
        sender.send(Outgoing::Text("hi".into())).unwrap();
        sender.send(Outgoing::Text("bye".into())).unwrap();
        run(
            url,
            vec![Header {
                key: "X-Test".into(),
                value: "1".into(),
            }],
            Duration::from_secs(5),
            move |event| log.lock().unwrap().push(event),
            receiver,
        )
        .await;
        let events = events.lock().unwrap();
        assert!(matches!(events[0], SocketEvent::Open { status: 101, .. }));
        assert!(
            matches!(&events[1], SocketEvent::Message { text, binary: false, .. } if text == "echo hi")
        );
        assert!(
            matches!(&events[2], SocketEvent::Message { text, binary: true, size: 2 } if text == "01 ff")
        );
        assert!(
            matches!(&events[3], SocketEvent::Close { code: Some(1001), reason } if reason == "done")
        );
    }

    #[tokio::test]
    async fn reports_a_failed_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        drop(listener);
        let (_sender, receiver) = mpsc::unbounded_channel();
        let events = Arc::new(Mutex::new(Vec::new()));
        let log = events.clone();
        run(
            url,
            vec![],
            Duration::from_secs(5),
            move |event| log.lock().unwrap().push(event),
            receiver,
        )
        .await;
        assert!(
            matches!(&events.lock().unwrap()[0], SocketEvent::Error { message } if message.starts_with("Connection failed"))
        );
    }
}
