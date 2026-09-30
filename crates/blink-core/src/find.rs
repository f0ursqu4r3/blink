//! Port of the pure logic in `src/composables/useFind.ts` and the line and
//! header filters of `CodeView.vue` and `ResponsePanel.vue`.
//!
//! Find counts every occurrence in a list of row texts, moves between them,
//! and gives the ranges to paint in rendered rows. Ranges are byte ranges in
//! the original row text.

use std::ops::Range;

use crate::model::Header;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindOccurrence {
    pub row: usize,
    /// The occurrence's place within its row.
    pub ordinal: usize,
}

/// The needle for a query: trimmed, lower case.
pub fn find_needle(query: &str) -> String {
    query.trim().to_lowercase()
}

/// Occurrences of `needle` (lower case) in `text`, without case. They do
/// not overlap.
pub fn positions(text: &str, needle: &str) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return vec![];
    }
    // Lower case can change byte lengths, so map back to `text` offsets.
    let mut haystack = String::with_capacity(text.len());
    let mut origin = Vec::with_capacity(text.len() + 1);
    let mut boundary = Vec::with_capacity(text.len() + 1);
    for (at, c) in text.char_indices() {
        let start = haystack.len();
        haystack.extend(c.to_lowercase());
        for index in start..haystack.len() {
            origin.push(at);
            boundary.push(index == start);
        }
    }
    origin.push(text.len());
    boundary.push(true);
    let end_of = |index: usize| {
        (index..origin.len())
            .find(|&at| boundary[at])
            .map_or(text.len(), |at| origin[at])
    };
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(needle) {
        let at = from + offset;
        let end = at + needle.len();
        found.push(origin[at]..end_of(end));
        from = end;
    }
    found
}

/// Every occurrence of `needle` in `texts`, one text per row.
pub fn occurrences<S: AsRef<str>>(texts: &[S], needle: &str) -> Vec<FindOccurrence> {
    if needle.is_empty() {
        return vec![];
    }
    texts
        .iter()
        .enumerate()
        .flat_map(|(row, text)| {
            (0..positions(text.as_ref(), needle).len())
                .map(move |ordinal| FindOccurrence { row, ordinal })
        })
        .collect()
}

/// Find state for one list: the needle, its occurrences, and the current one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Finder {
    needle: String,
    occurrences: Vec<FindOccurrence>,
    current: usize,
}

impl Finder {
    pub fn new() -> Self {
        Finder::default()
    }

    /// Recount after the query or the texts change. A new needle moves to
    /// the first occurrence; returns its row to scroll to.
    pub fn update<S: AsRef<str>>(&mut self, texts: &[S], query: &str) -> Option<usize> {
        let needle = find_needle(query);
        let changed = needle != self.needle;
        self.needle = needle;
        self.occurrences = occurrences(texts, &self.needle);
        if changed {
            self.current = 0;
            return self.reveal();
        }
        if self.current >= self.occurrences.len() {
            self.current = 0;
        }
        None
    }

    fn reveal(&self) -> Option<usize> {
        self.occurrences
            .get(self.current)
            .map(|occurrence| occurrence.row)
    }

    /// Move to the next (`1`) or previous (`-1`) occurrence. Returns its row
    /// to scroll to.
    pub fn step(&mut self, direction: isize) -> Option<usize> {
        let count = self.occurrences.len();
        if count == 0 {
            return None;
        }
        self.current = (self.current as isize + direction).rem_euclid(count as isize) as usize;
        self.reveal()
    }

    pub fn needle(&self) -> &str {
        &self.needle
    }

    pub fn count(&self) -> usize {
        self.occurrences.len()
    }

    /// 0-based index of the current occurrence.
    pub fn current(&self) -> usize {
        self.current
    }

    pub fn target(&self) -> Option<FindOccurrence> {
        self.occurrences.get(self.current).copied()
    }

    /// Ranges to paint in a rendered row, each marked when it is the
    /// current occurrence.
    pub fn highlights(&self, row: usize, text: &str) -> Vec<(Range<usize>, bool)> {
        let target = self.target();
        positions(text, &self.needle)
            .into_iter()
            .enumerate()
            .map(|(ordinal, range)| (range, target == Some(FindOccurrence { row, ordinal })))
            .collect()
    }
}

/// The count shown beside the find field: "" with no query or while
/// filtering, "2 of 5", or "No results".
pub fn find_status(finder: &Finder, query: &str, filter_lines: bool) -> String {
    if query.trim().is_empty() || filter_lines {
        return String::new();
    }
    if finder.count() == 0 {
        return "No results".into();
    }
    format!("{} of {}", finder.current() + 1, finder.count())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilteredLine<'a> {
    pub text: &'a str,
    /// 1-based line number in the whole text.
    pub number: usize,
}

/// The lines of `text` that contain `filter` without case; every line when
/// the filter is blank.
pub fn filter_lines<'a>(text: &'a str, filter: &str) -> Vec<FilteredLine<'a>> {
    let filter = find_needle(filter);
    text.split('\n')
        .enumerate()
        .filter(|(_, line)| filter.is_empty() || line.to_lowercase().contains(&filter))
        .map(|(index, text)| FilteredLine {
            text,
            number: index + 1,
        })
        .collect()
}

/// Response headers whose "key: value" line contains `query` without case.
pub fn filter_headers<'a>(headers: &'a [Header], query: &str) -> Vec<&'a Header> {
    let query = find_needle(query);
    headers
        .iter()
        .filter(|header| {
            query.is_empty()
                || format!("{}: {}", header.key, header.value)
                    .to_lowercase()
                    .contains(&query)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_occurrences_without_case() {
        assert_eq!(positions("Abc abc ABC", "abc"), [0..3, 4..7, 8..11]);
        assert_eq!(positions("aaaa", "aa"), [0..2, 2..4]);
        assert!(positions("abc", "").is_empty());
    }

    #[test]
    fn maps_ranges_back_to_the_original_text() {
        // "İ" lowers to two characters; the range still covers it.
        let text = "xİy";
        assert_eq!(positions(text, "i̇y"), vec![1..4]);
        assert_eq!(&text[positions(text, "y")[0].clone()], "y");
    }

    #[test]
    fn counts_every_occurrence_across_rows() {
        let texts = ["one two", "none", "two two"];
        assert_eq!(
            occurrences(&texts, "two"),
            [
                FindOccurrence { row: 0, ordinal: 0 },
                FindOccurrence { row: 2, ordinal: 0 },
                FindOccurrence { row: 2, ordinal: 1 },
            ]
        );
    }

    #[test]
    fn moves_between_occurrences_and_wraps() {
        let texts = ["a", "b a", "a"];
        let mut finder = Finder::new();
        assert_eq!(finder.update(&texts, " A "), Some(0));
        assert_eq!(finder.count(), 3);
        assert_eq!(finder.step(1), Some(1));
        assert_eq!(finder.step(1), Some(2));
        assert_eq!(finder.step(1), Some(0));
        assert_eq!(finder.step(-1), Some(2));
        assert_eq!(find_status(&finder, "a", false), "3 of 3");
        // The same needle keeps the place.
        assert_eq!(finder.update(&texts, "a"), None);
        assert_eq!(finder.current(), 2);
        // Fewer occurrences move back to the first.
        assert_eq!(finder.update(&["a"], "a"), None);
        assert_eq!(finder.current(), 0);
        // A new needle starts over.
        finder.step(1);
        assert_eq!(finder.update(&texts, "b"), Some(1));
        assert_eq!(finder.current(), 0);
    }

    #[test]
    fn reports_the_find_status() {
        let mut finder = Finder::new();
        finder.update(&["abc"], "x");
        assert_eq!(find_status(&finder, "x", false), "No results");
        assert_eq!(find_status(&finder, " ", false), "");
        assert_eq!(find_status(&finder, "x", true), "");
        assert_eq!(finder.step(1), None);
    }

    #[test]
    fn marks_the_current_occurrence_in_a_row() {
        let texts = ["ab ab"];
        let mut finder = Finder::new();
        finder.update(&texts, "ab");
        finder.step(1);
        assert_eq!(
            finder.highlights(0, texts[0]),
            [(0..2, false), (3..5, true)]
        );
        assert_eq!(finder.highlights(1, "ab"), [(0..2, false)]);
    }

    #[test]
    fn filters_lines_and_keeps_their_numbers() {
        let lines = filter_lines("alpha\nBeta\ngamma\nbetamax", " BETA ");
        assert_eq!(
            lines,
            [
                FilteredLine {
                    text: "Beta",
                    number: 2
                },
                FilteredLine {
                    text: "betamax",
                    number: 4
                },
            ]
        );
        assert_eq!(filter_lines("a\nb", "").len(), 2);
    }

    #[test]
    fn filters_headers_by_key_and_value() {
        let headers = [
            Header {
                key: "Content-Type".into(),
                value: "application/json".into(),
            },
            Header {
                key: "X-Id".into(),
                value: "7".into(),
            },
        ];
        assert_eq!(filter_headers(&headers, "type: app").len(), 1);
        assert_eq!(filter_headers(&headers, "").len(), 2);
        assert!(filter_headers(&headers, "nothing").is_empty());
    }
}
