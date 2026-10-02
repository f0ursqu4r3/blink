//! Show a field with each defined `{{name}}` replaced by its value, and map
//! edits on the shown text back to the raw text. Port of `src/lib/token-display.ts`.
//!
//! A shown value acts as one unit: the caret stays at its edges, and an edit
//! that touches part of it replaces the whole reference. Backspace at its end
//! (or Delete at its start) removes one raw character, so `users` turns back
//! into `{{endpoint}` and the user can edit the reference.
//!
//! Offsets are byte offsets into UTF-8 text.

use crate::interpolation::InterpolationContext;
use crate::token_hints::{TokenSpan, TokenState, token_spans, token_value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplaySegment {
    pub span: TokenSpan,
    /// Shown text: the value for a unit, else the raw text.
    pub text: String,
    /// True when the segment shows a value in place of its reference.
    pub unit: bool,
    pub from: usize,
    pub to: usize,
    pub raw_from: usize,
    pub raw_to: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenDisplay {
    pub text: String,
    pub segments: Vec<DisplaySegment>,
}

pub fn token_display(raw: &str, ctx: Option<&InterpolationContext>) -> TokenDisplay {
    let mut segments = Vec::new();
    let mut from = 0;
    let mut raw_from = 0;
    for span in token_spans(raw, ctx) {
        let value = match (&span.token, &span.name) {
            (Some(TokenState::Resolved), Some(name)) => token_value(name, ctx),
            _ => None,
        };
        // An empty value would leave nothing to see or edit.
        let value = value.filter(|value| !value.is_empty());
        let unit = value.is_some();
        let text = value.unwrap_or_else(|| span.text.clone());
        let raw_len = span.text.len();
        let len = text.len();
        segments.push(DisplaySegment {
            span,
            text,
            unit,
            from,
            to: from + len,
            raw_from,
            raw_to: raw_from + raw_len,
        });
        from += len;
        raw_from += raw_len;
    }
    TokenDisplay {
        text: segments.iter().map(|s| s.text.as_str()).collect(),
        segments,
    }
}

/// Raw offset for a shown offset. A unit's inside maps to its raw end.
pub fn raw_offset(segments: &[DisplaySegment], pos: usize) -> usize {
    for s in segments {
        if pos < s.from || pos > s.to {
            continue;
        }
        if !s.unit {
            return s.raw_from + (pos - s.from);
        }
        return if pos == s.from { s.raw_from } else { s.raw_to };
    }
    segments.last().map_or(0, |s| s.raw_to)
}

/// Shown offset for a raw offset. A unit's inside maps to its shown end.
pub fn display_offset(segments: &[DisplaySegment], raw: usize) -> usize {
    for s in segments {
        if raw < s.raw_from || raw > s.raw_to {
            continue;
        }
        if !s.unit {
            return s.from + (raw - s.raw_from);
        }
        return if raw == s.raw_from { s.from } else { s.to };
    }
    segments.last().map_or(0, |s| s.to)
}

fn unit_around(segments: &[DisplaySegment], pos: usize) -> Option<&DisplaySegment> {
    segments
        .iter()
        .find(|s| s.unit && s.from < pos && pos < s.to)
}

/// Move a caret out of a unit, to the edge in the direction it moved.
/// Returns `pos` when it is not inside a unit.
pub fn snap_caret(segments: &[DisplaySegment], pos: usize, previous: usize) -> usize {
    let Some(s) = unit_around(segments, pos) else {
        return pos;
    };
    if previous <= s.from {
        return s.to;
    }
    if previous >= s.to {
        return s.from;
    }
    if pos - s.from < s.to - pos {
        s.from
    } else {
        s.to
    }
}

/// Widen a shown range so it covers every unit it touches.
pub fn widen_range(
    segments: &[DisplaySegment],
    mut start: usize,
    mut end: usize,
) -> (usize, usize) {
    for s in segments {
        if !s.unit || s.from >= end || s.to <= start {
            continue;
        }
        start = start.min(s.from);
        end = end.max(s.to);
    }
    (start, end)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEdit {
    pub raw: String,
    pub caret: usize,
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

/// Apply a native edit, found by comparing the shown text before and after,
/// to the raw text. `caret` is the shown caret after the edit.
pub fn apply_display_edit(raw: &str, before: &TokenDisplay, after: &str, caret: usize) -> RawEdit {
    let old = before.text.as_str();
    let suffix = (after.len() - caret).min(old.len());
    let limit = caret.min(old.len() - suffix);
    let (old_bytes, after_bytes) = (old.as_bytes(), after.as_bytes());
    let mut start = 0;
    while start < limit && old_bytes[start] == after_bytes[start] {
        start += 1;
    }
    let start = floor_boundary(old, start);
    let old_end = ceil_boundary(old, old.len() - suffix);
    let inserted = &after[start..caret];

    let (mut from, mut to) = widen_range(&before.segments, start, old_end);
    // Text typed inside a unit goes after it.
    if from == to
        && let Some(inside) = unit_around(&before.segments, from)
    {
        from = inside.to;
        to = inside.to;
    }

    let raw_from = raw_offset(&before.segments, from);
    let raw_to = raw_offset(&before.segments, to).max(raw_from);
    RawEdit {
        raw: format!("{}{inserted}{}", &raw[..raw_from], &raw[raw_to..]),
        caret: raw_from + inserted.len(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteDirection {
    Backward,
    Forward,
}

/// Backspace just after a unit, or Delete just before one: remove one raw
/// character so the reference shows again. Returns None for other positions.
pub fn delete_into_unit(
    raw: &str,
    segments: &[DisplaySegment],
    pos: usize,
    direction: DeleteDirection,
) -> Option<RawEdit> {
    let s = segments.iter().find(|seg| {
        seg.unit
            && match direction {
                DeleteDirection::Backward => seg.to == pos,
                DeleteDirection::Forward => seg.from == pos,
            }
    })?;
    // A reference starts with `{{` and ends with `}}`, so the removed
    // character is always one byte.
    let at = match direction {
        DeleteDirection::Backward => s.raw_to - 1,
        DeleteDirection::Forward => s.raw_from,
    };
    Some(RawEdit {
        raw: format!("{}{}", &raw[..at], &raw[at + 1..]),
        caret: at,
    })
}

/// Raw text for a shown selection, with touched units copied whole.
pub fn raw_slice(raw: &str, segments: &[DisplaySegment], start: usize, end: usize) -> String {
    let (from, to) = widen_range(segments, start, end);
    raw[raw_offset(segments, from)..raw_offset(segments, to)].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpolation::ResponseTokenInfo;
    use crate::model::{CheckSource, Definitions};

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    fn ctx() -> InterpolationContext {
        InterpolationContext::new(
            defs(&[("endpoint", "users"), ("empty", "")]),
            defs(&[("host", "api.test")]),
        )
    }
    const RAW: &str = "https://{{_.host}}/{{endpoint}}";
    // Shown: "https://api.test/users"
    fn shown() -> TokenDisplay {
        token_display(RAW, Some(&ctx()))
    }

    #[test]
    fn shows_defined_tokens_as_values() {
        assert_eq!(shown().text, "https://api.test/users");
    }

    #[test]
    fn keeps_undefined_empty_and_environment_references_as_typed() {
        assert_eq!(
            token_display("{{nope}}{{empty}}{{!HOME}}", Some(&ctx())).text,
            "{{nope}}{{empty}}{{!HOME}}"
        );
    }

    #[test]
    fn maps_unit_edges_and_plain_text_both_ways() {
        let s = shown().segments;
        assert_eq!(raw_offset(&s, 8), 8);
        assert_eq!(raw_offset(&s, 16), 18);
        assert_eq!(raw_offset(&s, 22), RAW.len());
        assert_eq!(display_offset(&s, RAW.len()), 22);
        assert_eq!(display_offset(&s, 19), 17);
    }

    #[test]
    fn moves_the_caret_to_the_edge_in_the_direction_it_moved() {
        let s = shown().segments;
        assert_eq!(snap_caret(&s, 18, 17), 22);
        assert_eq!(snap_caret(&s, 21, 22), 17);
        assert_eq!(snap_caret(&s, 3, 2), 3);
    }

    #[test]
    fn removes_the_closing_brace_on_backspace_after_a_value() {
        assert_eq!(
            delete_into_unit(RAW, &shown().segments, 22, DeleteDirection::Backward),
            Some(RawEdit {
                raw: "https://{{_.host}}/{{endpoint}".into(),
                caret: 30
            })
        );
    }

    #[test]
    fn removes_the_opening_brace_on_delete_before_a_value() {
        assert_eq!(
            delete_into_unit(RAW, &shown().segments, 17, DeleteDirection::Forward),
            Some(RawEdit {
                raw: "https://{{_.host}}/{endpoint}}".into(),
                caret: 19
            })
        );
    }

    #[test]
    fn does_nothing_away_from_a_value() {
        assert_eq!(
            delete_into_unit(RAW, &shown().segments, 3, DeleteDirection::Backward),
            None
        );
    }

    #[test]
    fn applies_plain_edits_to_the_raw_text() {
        assert_eq!(
            apply_display_edit(RAW, &shown(), "https://api.test/users?x", 24),
            RawEdit {
                raw: format!("{RAW}?x"),
                caret: RAW.len() + 2
            }
        );
    }

    #[test]
    fn replaces_a_whole_value_when_an_edit_covers_part_of_it() {
        // Select "ers" and type "X".
        assert_eq!(
            apply_display_edit(RAW, &shown(), "https://api.test/uX", 19),
            RawEdit {
                raw: "https://{{_.host}}/X".into(),
                caret: 20
            }
        );
    }

    #[test]
    fn clears_the_field_when_all_text_is_replaced() {
        assert_eq!(
            apply_display_edit(RAW, &shown(), "a", 1),
            RawEdit {
                raw: "a".into(),
                caret: 1
            }
        );
    }

    #[test]
    fn copies_touched_values_as_their_references() {
        assert_eq!(
            raw_slice(RAW, &shown().segments, 10, 22),
            "{{_.host}}/{{endpoint}}"
        );
    }

    #[test]
    fn response_tokens_never_show_their_value() {
        let mut ctx = InterpolationContext::local(defs(&[("t", "secret")]));
        ctx.response_tokens.insert(
            "t".into(),
            ResponseTokenInfo {
                request_label: "Login".into(),
                source: CheckSource::Json,
                path: ".t".into(),
                fetched_at_ms: Some(0),
                environment: None,
                problem: None,
            },
        );
        let display = token_display("Bearer {{t}}", Some(&ctx));
        assert_eq!(display.text, "Bearer {{t}}");
        assert!(display.segments.iter().all(|s| !s.unit));
    }
}
