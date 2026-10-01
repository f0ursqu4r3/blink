//! Text selection over the shown rows of a read-only view.

use std::ops::Range;

/// A place in the text: a shown row and a byte offset in its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Spot {
    pub row: usize,
    pub offset: usize,
}

impl Spot {
    pub fn new(row: usize, offset: usize) -> Self {
        Spot { row, offset }
    }
}

/// The selected text from `anchor`, where the pointer went down, to `head`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Spot,
    pub head: Spot,
}

impl Selection {
    pub fn at(spot: Spot) -> Self {
        Selection {
            anchor: spot,
            head: spot,
        }
    }

    fn ordered(&self) -> (Spot, Spot) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The selected bytes of `row`, a row of `len` bytes.
    pub fn row_range(&self, row: usize, len: usize) -> Option<Range<usize>> {
        let (start, end) = self.ordered();
        if row < start.row || row > end.row {
            return None;
        }
        let from = if row == start.row { start.offset } else { 0 };
        let to = if row == end.row { end.offset } else { len };
        (from < to).then(|| from.min(len)..to.min(len))
    }

    /// The selected text, with a line break between rows. `row_text` gives
    /// the text of a shown row.
    pub fn text<'a>(&self, row_text: impl Fn(usize) -> &'a str) -> String {
        let (start, end) = self.ordered();
        (start.row..=end.row)
            .map(|row| {
                let text = row_text(row);
                let range = self.row_range(row, text.len()).unwrap_or_default();
                &text[range]
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The word at `offset` in `text`: letters, digits, and `_`, or the one
/// other character there.
pub fn word_at(text: &str, offset: usize) -> Range<usize> {
    let offset = offset.min(text.len());
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let Some(at) = text[offset..]
        .chars()
        .next()
        .or_else(|| text[..offset].chars().next_back())
    else {
        return offset..offset;
    };
    if !word(at) {
        let start = if offset < text.len() {
            offset
        } else {
            offset - at.len_utf8()
        };
        return start..start + at.len_utf8();
    }
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| word(*c))
        .last()
        .map_or(offset, |(index, _)| index);
    let end = text[offset..]
        .char_indices()
        .find(|(_, c)| !word(*c))
        .map_or(text.len(), |(index, _)| offset + index);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: [&str; 3] = ["{", "  \"name\": \"blink\",", "}"];

    #[test]
    fn selects_parts_of_the_first_and_last_rows_and_all_rows_between() {
        let selection = Selection {
            anchor: Spot::new(2, 1),
            head: Spot::new(0, 0),
        };
        assert_eq!(selection.row_range(0, 1), Some(0..1));
        assert_eq!(selection.row_range(1, 19), Some(0..19));
        assert_eq!(selection.row_range(2, 1), Some(0..1));
        assert_eq!(selection.text(|row| ROWS[row]), ROWS.join("\n"));

        let selection = Selection {
            anchor: Spot::new(1, 3),
            head: Spot::new(1, 7),
        };
        assert_eq!(selection.row_range(0, 1), None);
        assert_eq!(selection.text(|row| ROWS[row]), "name");
        assert!(Selection::at(Spot::new(1, 3)).row_range(1, 19).is_none());
        assert_eq!(Selection::at(Spot::new(1, 3)).text(|row| ROWS[row]), "");
    }

    #[test]
    fn a_word_is_letters_digits_and_underscores() {
        let text = "  \"user_id\": 42,";
        assert_eq!(&text[word_at(text, 5)], "user_id");
        assert_eq!(&text[word_at(text, 3)], "user_id");
        assert_eq!(&text[word_at(text, 9)], "user_id");
        assert_eq!(&text[word_at(text, 10)], "\"");
        assert_eq!(&text[word_at(text, 13)], "42");
        assert_eq!(&text[word_at(text, 11)], ":");
        assert_eq!(&text[word_at(text, text.len())], ",");
        assert_eq!(word_at("", 0), 0..0);
        assert_eq!(&"é"[word_at("é", 0)], "é");
    }
}
