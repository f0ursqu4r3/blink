//! Past sends of a request. Port of `src/lib/history.ts`.

use crate::json::format_json;
use crate::model::{ApiResponse, HistoryEntry, RequestInput};

pub const HISTORY_LIMIT: usize = 25;
/// History keeps bodies up to this size, so the workspace stays small.
pub const HISTORY_BODY_LIMIT: usize = 64 * 1024;

/// How a send ended.
#[derive(Debug, Clone, Copy)]
pub enum HistoryOutcome<'a> {
    Response(&'a ApiResponse),
    Error { error: &'a str, duration_ms: f64 },
}

pub fn history_entry(
    id: u64,
    sent_at: f64,
    request: &RequestInput,
    outcome: HistoryOutcome,
) -> HistoryEntry {
    let base = HistoryEntry {
        id,
        sent_at,
        method: request.method.clone(),
        url: request.url.clone(),
        status: None,
        status_text: None,
        error: None,
        duration_ms: 0.0,
        size_bytes: 0,
        headers: Vec::new(),
        body: String::new(),
        body_omitted: None,
        timing: None,
    };
    let response = match outcome {
        HistoryOutcome::Error { error, duration_ms } => {
            return HistoryEntry {
                error: Some(error.to_string()),
                duration_ms,
                ..base
            };
        }
        HistoryOutcome::Response(response) => response,
    };
    // The limit counts UTF-16 units, as the TS string length does.
    let omit = response.is_truncated()
        || response.is_binary()
        || response
            .body
            .encode_utf16()
            .nth(HISTORY_BODY_LIMIT)
            .is_some();
    HistoryEntry {
        status: Some(response.status),
        status_text: Some(response.status_text.clone()),
        duration_ms: response.duration_ms,
        size_bytes: response.size_bytes,
        headers: response.headers.clone(),
        body: if omit {
            String::new()
        } else {
            response.body.clone()
        },
        body_omitted: omit.then_some(true),
        timing: response.timing,
        ..base
    }
}

/// The newest entry first, capped at HISTORY_LIMIT.
pub fn add_history(history: &[HistoryEntry], entry: HistoryEntry) -> Vec<HistoryEntry> {
    std::iter::once(entry)
        .chain(history.iter().cloned())
        .take(HISTORY_LIMIT)
        .collect()
}

pub fn next_history_id(history: &[HistoryEntry]) -> u64 {
    history.iter().map(|entry| entry.id).max().unwrap_or(0) + 1
}

/// Body lines for a diff: formatted JSON when the body parses.
pub fn diff_text(entry: &HistoryEntry) -> Vec<String> {
    if let Some(error) = entry.error.as_deref().filter(|error| !error.is_empty()) {
        return vec![format!("Error: {error}")];
    }
    if entry.body_omitted == Some(true) {
        return vec!["(body not kept)".into()];
    }
    let text = format_json(&entry.body).unwrap_or_else(|_| entry.body.clone());
    text.split('\n').map(String::from).collect()
}

/// Header lines, sorted by name, for a diff.
pub fn header_lines(entry: &HistoryEntry) -> Vec<String> {
    let mut lines: Vec<String> = entry
        .headers
        .iter()
        .map(|header| format!("{}: {}", header.key.to_lowercase(), header.value))
        .collect();
    // JavaScript sorts strings by UTF-16 code units.
    lines.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    lines
}

/// Epoch milliseconds now.
pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64() * 1000.0)
}

pub fn relative_time(sent_at: f64, now: f64) -> String {
    let seconds = ((now - sent_at) / 1000.0).round().max(0.0);
    if seconds < 45.0 {
        return "just now".into();
    }
    let minutes = (seconds / 60.0).round();
    if minutes < 60.0 {
        return format!("{minutes} min ago");
    }
    let hours = (minutes / 60.0).round();
    if hours < 24.0 {
        return format!("{hours} h ago");
    }
    local_date(sent_at)
}

/// The local date, as `toLocaleDateString` writes it in the en-US locale.
fn local_date(epoch_ms: f64) -> String {
    use chrono::{Datelike, Local, TimeZone};
    match Local.timestamp_millis_opt(epoch_ms as i64).single() {
        Some(date) => format!("{}/{}/{}", date.month(), date.day(), date.year()),
        None => "Invalid Date".into(),
    }
}

/// Local time of day, as `toLocaleTimeString` writes it with two-digit hour,
/// minute, and second in the en-US locale: "03:04:05 PM", or "15:04:05"
/// without `hour12`.
pub fn clock_time(epoch_ms: f64, hour12: bool) -> String {
    use chrono::{Local, TimeZone, Timelike};
    let Some(time) = Local.timestamp_millis_opt(epoch_ms as i64).single() else {
        return "Invalid Date".into();
    };
    if hour12 {
        let (pm, hour) = time.hour12();
        format!(
            "{hour:02}:{:02}:{:02} {}",
            time.minute(),
            time.second(),
            if pm { "PM" } else { "AM" }
        )
    } else {
        format!("{:02}:{:02}:{:02}", time.hour(), time.minute(), time.second())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Header;

    fn request() -> RequestInput {
        RequestInput {
            method: "GET".into(),
            url: "https://example.test".into(),
            headers: vec![],
            body: None,
            body_file: None,
            multipart: None,
        }
    }
    fn response(body: &str) -> ApiResponse {
        ApiResponse {
            status: 200,
            status_text: "OK".into(),
            duration_ms: 5.0,
            headers: vec![Header {
                key: "A".into(),
                value: "1".into(),
            }],
            body: body.into(),
            size_bytes: body.len() as u64,
            body_id: Some("x".into()),
            truncated: None,
            binary: None,
            final_url: None,
            redirect_count: None,
            timing: None,
        }
    }

    #[test]
    fn keeps_the_response_without_its_body_id() {
        let entry = history_entry(
            1,
            100.0,
            &request(),
            HistoryOutcome::Response(&response("{}")),
        );
        assert_eq!(entry.status, Some(200));
        assert_eq!(entry.body, "{}");
        assert_eq!(entry.method, "GET");
        assert!(!serde_json::to_string(&entry).unwrap().contains("bodyId"));
    }

    #[test]
    fn omits_large_truncated_and_binary_bodies() {
        let large = response(&"x".repeat(HISTORY_BODY_LIMIT + 1));
        let truncated = ApiResponse {
            truncated: Some(true),
            ..response("x")
        };
        let binary = ApiResponse {
            binary: Some(true),
            ..response("")
        };
        for r in [large, truncated, binary] {
            let entry = history_entry(1, 0.0, &request(), HistoryOutcome::Response(&r));
            assert_eq!(entry.body, "");
            assert_eq!(entry.body_omitted, Some(true));
        }
        let exact = response(&"x".repeat(HISTORY_BODY_LIMIT));
        let entry = history_entry(1, 0.0, &request(), HistoryOutcome::Response(&exact));
        assert_eq!(entry.body_omitted, None);
    }

    #[test]
    fn records_errors() {
        let entry = history_entry(
            2,
            0.0,
            &request(),
            HistoryOutcome::Error {
                error: "Boom",
                duration_ms: 3.0,
            },
        );
        assert_eq!(entry.error.as_deref(), Some("Boom"));
        assert_eq!(entry.duration_ms, 3.0);
        assert_eq!(diff_text(&entry), ["Error: Boom"]);
    }

    #[test]
    fn puts_the_newest_first_and_caps_the_list() {
        let mut list = Vec::new();
        for i in 1..=HISTORY_LIMIT as u64 + 3 {
            let entry = history_entry(
                i,
                i as f64,
                &request(),
                HistoryOutcome::Response(&response("")),
            );
            list = add_history(&list, entry);
        }
        assert_eq!(list.len(), HISTORY_LIMIT);
        assert_eq!(list[0].id, HISTORY_LIMIT as u64 + 3);
        assert_eq!(next_history_id(&list), HISTORY_LIMIT as u64 + 4);
        assert_eq!(next_history_id(&[]), 1);
    }

    #[test]
    fn formats_json_bodies_for_diffs() {
        let entry = history_entry(
            1,
            0.0,
            &request(),
            HistoryOutcome::Response(&response("{\"a\":1}")),
        );
        assert_eq!(diff_text(&entry), ["{", "  \"a\": 1", "}"]);
    }

    #[test]
    fn describes_relative_times() {
        assert_eq!(relative_time(0.0, 10_000.0), "just now");
        assert_eq!(relative_time(0.0, 5.0 * 60_000.0), "5 min ago");
        assert_eq!(relative_time(0.0, 3.0 * 3_600_000.0), "3 h ago");
    }
}
