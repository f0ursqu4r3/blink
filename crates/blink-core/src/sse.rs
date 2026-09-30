//! Port of `src/lib/sse.ts`.

use crate::model::SseEvent;

/// An incremental parser for the text/event-stream format (WHATWG HTML,
/// "Server-sent events"). Push text in any pieces; each push returns the
/// events it completed.
#[derive(Debug, Clone, Default)]
pub struct SseParser {
    buffer: String,
    data: Vec<String>,
    event: String,
    id: Option<String>,
    last_id: Option<String>,
}

impl SseParser {
    pub fn new() -> Self {
        SseParser::default()
    }

    fn line(&mut self, text: &str, events: &mut Vec<SseEvent>) {
        if text.is_empty() {
            if !self.data.is_empty() {
                let id = self.id.clone().or_else(|| self.last_id.clone());
                events.push(SseEvent {
                    event: if self.event.is_empty() {
                        "message".into()
                    } else {
                        self.event.clone()
                    },
                    data: self.data.join("\n"),
                    id: id.clone().filter(|id| !id.is_empty()),
                    at: None,
                });
                self.last_id = id;
            }
            self.data.clear();
            self.event.clear();
            self.id = None;
            return;
        }
        if text.starts_with(':') {
            return;
        }
        let (field, value) = text.split_once(':').unwrap_or((text, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "data" => self.data.push(value.to_string()),
            "event" => self.event = value.to_string(),
            "id" if !value.contains('\0') => self.id = Some(value.to_string()),
            _ => {}
        }
    }

    pub fn push(&mut self, text: &str) -> Vec<SseEvent> {
        self.buffer.push_str(text);
        let buffer = std::mem::take(&mut self.buffer);
        let bytes = buffer.as_bytes();
        let mut events = Vec::new();
        let mut start = 0;
        let mut i = 0;
        while i < bytes.len() {
            let end = match bytes[i] {
                b'\n' => Some(1),
                b'\r' if bytes.get(i + 1) == Some(&b'\n') => Some(2),
                // A trailing "\r" may be the first half of "\r\n".
                b'\r' if i + 1 < bytes.len() => Some(1),
                _ => None,
            };
            match end {
                Some(len) => {
                    self.line(&buffer[start..i], &mut events);
                    i += len;
                    start = i;
                }
                None => i += 1,
            }
        }
        self.buffer = buffer[start..].to_string();
        events
    }

    /// Dispatch an event left open when the stream ends.
    pub fn end(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();
        let buffer = std::mem::take(&mut self.buffer);
        if !buffer.is_empty() {
            self.line(buffer.strip_suffix('\r').unwrap_or(&buffer), &mut events);
        }
        self.line("", &mut events);
        events
    }
}

/// Parse a whole event-stream body.
pub fn parse_sse(text: &str) -> Vec<SseEvent> {
    let mut parser = SseParser::new();
    let mut events = parser.push(text);
    events.extend(parser.end());
    events
}

pub fn is_event_stream(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| value.trim().to_lowercase().starts_with("text/event-stream"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(event: &str, data: &str, id: Option<&str>) -> SseEvent {
        SseEvent {
            event: event.into(),
            data: data.into(),
            id: id.map(Into::into),
            at: None,
        }
    }

    #[test]
    fn parses_events_names_ids_and_multi_line_data() {
        assert_eq!(
            parse_sse(
                ": comment\nevent: tick\nid: 1\ndata: a\ndata: b\n\ndata:no space\n\nretry: 5\n\n"
            ),
            [
                event("tick", "a\nb", Some("1")),
                event("message", "no space", Some("1")),
            ]
        );
    }

    #[test]
    fn handles_pieces_split_anywhere_and_crlf() {
        let mut parser = SseParser::new();
        let mut events = Vec::new();
        for piece in ["da", "ta: x\r", "\n\r\n", "data: y\r\n", "\r\n"] {
            events.extend(parser.push(piece));
        }
        assert_eq!(
            events.iter().map(|e| e.data.as_str()).collect::<Vec<_>>(),
            ["x", "y"]
        );
    }

    #[test]
    fn dispatches_an_event_left_open_at_the_end() {
        assert_eq!(parse_sse("data: last"), [event("message", "last", None)]);
    }

    #[test]
    fn detects_event_stream_content_types() {
        assert!(is_event_stream(Some("text/event-stream; charset=utf-8")));
        assert!(!is_event_stream(Some("text/plain")));
        assert!(!is_event_stream(None));
    }
}
