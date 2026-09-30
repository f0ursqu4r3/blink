//! Port of `src/lib/text-location.ts`.

/// A parse error position. `line` and `column` are 1-based; `offset` is a
/// 0-based byte offset. `column` counts characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLocation {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
    pub reason: String,
}

fn floor_boundary(text: &str, offset: usize) -> usize {
    let mut at = offset.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

pub fn location_from_offset(text: &str, offset: usize, reason: impl Into<String>) -> TextLocation {
    let clamped = floor_boundary(text, offset);
    let before = &text[..clamped];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    let column = before[line_start..].chars().count() + 1;
    TextLocation {
        line,
        column,
        offset: clamped,
        reason: reason.into(),
    }
}

/// `column` counts characters from the line start.
pub fn location_from_line_column(
    text: &str,
    line: usize,
    column: usize,
    reason: impl Into<String>,
) -> TextLocation {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut offset = 0;
    for current in lines.iter().take(line.saturating_sub(1)) {
        offset += current.len() + 1;
    }
    let target = lines.get(line.saturating_sub(1)).copied().unwrap_or("");
    let steps = column.saturating_sub(1);
    let within = target.char_indices().nth(steps).map_or_else(
        // Past the line end: count the rest as single bytes, as the TS does.
        || target.len() + (steps - target.chars().count()),
        |(at, _)| at,
    );
    location_from_offset(text, offset + within, reason)
}

pub fn describe_location(location: &TextLocation) -> String {
    format!("line {}, column {}", location.line, location.column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_an_offset_to_a_1_based_line_and_column() {
        assert_eq!(
            location_from_offset("ab\ncd\nef", 4, "x"),
            TextLocation {
                line: 2,
                column: 2,
                offset: 4,
                reason: "x".into()
            }
        );
    }

    #[test]
    fn clamps_offsets_past_the_end_of_the_text() {
        let location = location_from_offset("ab", 99, "x");
        assert_eq!((location.line, location.column, location.offset), (1, 3, 2));
    }

    #[test]
    fn describes_a_location() {
        assert_eq!(
            describe_location(&location_from_offset("a\nb", 2, "x")),
            "line 2, column 1"
        );
    }

    #[test]
    fn converts_a_line_and_column_to_an_offset() {
        let location = location_from_line_column("ab\ncd\nef", 3, 2, "x");
        assert_eq!((location.line, location.column, location.offset), (3, 2, 7));
    }
}
