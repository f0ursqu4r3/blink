//! Port of `src/lib/json.ts`.
//!
//! The TS formats with lossless-json. This is a port of its parser and
//! printer: numbers are copied as written, so large IDs and decimal values
//! stay exact. Error messages and positions match lossless-json, except that
//! positions are byte offsets.

use std::fmt;

use crate::text_location::{TextLocation, location_from_offset};

pub const JSON_HIGHLIGHT_LIMIT: usize = 64_000;

/// A parse error: "<reason> at position <offset>".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub reason: String,
    /// Byte offset in the text.
    pub offset: usize,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} at position {}", self.reason, self.offset)
    }
}

impl std::error::Error for JsonError {}

/// A parsed JSON value. Numbers keep their source text; strings keep the
/// escaped form that `JSON.stringify` writes, without the quotes.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    /// Keys in JavaScript property order: array-index keys first, ascending.
    Object(Vec<(String, JsonValue)>),
}

/// Keep large IDs and decimal values exact when inspecting or formatting JSON.
pub fn format_json(text: &str) -> Result<String, JsonError> {
    let value = parse(text)?;
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    write_value(&mut out, &value, Some("  "), "");
    Ok(out)
}

/// lossless-json reports errors as "<reason> at position <offset>".
pub fn json_error_location(text: &str, error: &JsonError) -> TextLocation {
    location_from_offset(text, error.offset, error.reason.clone())
}

/// Parse `text` as lossless-json does: duplicate keys with different
/// values are an error.
pub fn parse(text: &str) -> Result<JsonValue, JsonError> {
    Parser::new(text, false).document()
}

/// `JSON.stringify(JSON.parse(text))`: compact, with numbers as JavaScript
/// writes them. None when the text is not JSON.
pub fn canonical_json(text: &str) -> Option<String> {
    let value = Parser::new(text, true).document().ok()?;
    let mut out = String::new();
    write_value(&mut out, &value, None, "");
    Some(out)
}

/// Write `value` as JSON, indented by `indent` or compact.
pub fn stringify(value: &JsonValue, indent: Option<&str>) -> String {
    let mut out = String::new();
    write_value(&mut out, value, indent, "");
    out
}

fn write_value(out: &mut String, value: &JsonValue, space: Option<&str>, indent: &str) {
    match value {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(true) => out.push_str("true"),
        JsonValue::Bool(false) => out.push_str("false"),
        JsonValue::Number(number) => out.push_str(number),
        JsonValue::String(escaped) => {
            out.push('"');
            out.push_str(escaped);
            out.push('"');
        }
        JsonValue::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            let child = space.map(|space| format!("{indent}{space}"));
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                if let Some(child) = &child {
                    out.push('\n');
                    out.push_str(child);
                }
                write_value(out, item, space, child.as_deref().unwrap_or(""));
            }
            if space.is_some() {
                out.push('\n');
                out.push_str(indent);
            }
            out.push(']');
        }
        JsonValue::Object(entries) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            let child = space.map(|space| format!("{indent}{space}"));
            out.push('{');
            for (index, (key, item)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                if let Some(child) = &child {
                    out.push('\n');
                    out.push_str(child);
                }
                out.push('"');
                out.push_str(key);
                out.push_str(if space.is_some() { "\": " } else { "\":" });
                write_value(out, item, space, child.as_deref().unwrap_or(""));
            }
            if space.is_some() {
                out.push('\n');
                out.push_str(indent);
            }
            out.push('}');
        }
    }
}

/// `String(number)` in JavaScript: the shortest round-trip digits, in plain
/// notation from 1e-7 up to 1e21 and in exponent notation outside it.
pub fn js_number_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if value == 0.0 {
        return "0".into();
    }
    let exp = format!("{:e}", value.abs());
    let (mantissa, exponent) = exp.split_once('e').unwrap_or((&exp, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    // The decimal point sits after `n` digits.
    let n = exponent + 1;
    let sign = if value < 0.0 { "-" } else { "" };
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let e = if e >= 0 {
            format!("+{e}")
        } else {
            e.to_string()
        };
        if k == 1 {
            format!("{digits}e{e}")
        } else {
            format!("{}.{}e{e}", &digits[..1], &digits[1..])
        }
    };
    format!("{sign}{body}")
}

/// True for a key JavaScript orders as an array index.
fn is_index_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return false;
    }
    key.parse::<u64>()
        .is_ok_and(|index| index < u32::MAX as u64)
}

fn js_order(entries: &mut Vec<(String, JsonValue)>) {
    if !entries.iter().any(|(key, _)| is_index_key(key)) {
        return;
    }
    let (mut indices, rest): (Vec<_>, Vec<_>) =
        entries.drain(..).partition(|(key, _)| is_index_key(key));
    indices.sort_by_key(|(key, _)| key.parse::<u64>().unwrap_or(0));
    entries.extend(indices);
    entries.extend(rest);
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    i: usize,
    /// `JSON.parse` rules: the last duplicate key wins, numbers normalize.
    native: bool,
}

type Parsed<T> = Result<Option<T>, JsonError>;

impl<'a> Parser<'a> {
    fn new(text: &'a str, native: bool) -> Self {
        Parser {
            text,
            bytes: text.as_bytes(),
            i: 0,
            native,
        }
    }

    fn document(mut self) -> Result<JsonValue, JsonError> {
        let value = self.value()?;
        let Some(value) = value else {
            return Err(self.got_at("JSON value expected"));
        };
        if self.i < self.bytes.len() {
            return Err(self.got_at("Expected end of input"));
        }
        Ok(value)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.i).copied()
    }

    fn error(&self, reason: String) -> JsonError {
        JsonError {
            reason,
            offset: self.i,
        }
    }

    fn got_at(&self, reason: &str) -> JsonError {
        let got = match self.text[self.i.min(self.text.len())..].chars().next() {
            Some(c) => format!("but got '{c}'"),
            None => "but reached end of input".into(),
        };
        self.error(format!("{reason} {got}"))
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\t' | b'\r')) {
            self.i += 1;
        }
    }

    fn value(&mut self) -> Parsed<JsonValue> {
        self.skip_whitespace();
        let value = if let Some(s) = self.string()? {
            Some(JsonValue::String(s))
        } else if let Some(n) = self.number()? {
            Some(n)
        } else if let Some(o) = self.object()? {
            Some(o)
        } else if let Some(a) = self.array()? {
            Some(a)
        } else if self.keyword("true") {
            Some(JsonValue::Bool(true))
        } else if self.keyword("false") {
            Some(JsonValue::Bool(false))
        } else if self.keyword("null") {
            Some(JsonValue::Null)
        } else {
            None
        };
        self.skip_whitespace();
        Ok(value)
    }

    fn keyword(&mut self, name: &str) -> bool {
        if self.bytes[self.i..].starts_with(name.as_bytes()) {
            self.i += name.len();
            true
        } else {
            false
        }
    }

    fn object(&mut self) -> Parsed<JsonValue> {
        if self.peek() != Some(b'{') {
            return Ok(None);
        }
        self.i += 1;
        self.skip_whitespace();
        let mut entries: Vec<(String, JsonValue)> = Vec::new();
        let mut initial = true;
        while self.i < self.bytes.len() && self.peek() != Some(b'}') {
            if !initial {
                self.eat_comma()?;
                self.skip_whitespace();
            } else {
                initial = false;
            }
            let start = self.i;
            let Some(key) = self.string()? else {
                return Err(self.got_at("Quoted object key expected"));
            };
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(self.got_at("Colon ':' expected after property name"));
            }
            self.i += 1;
            let Some(value) = self.value()? else {
                return Err(self.error("Object value expected after ':'".into()));
            };
            match entries.iter_mut().find(|(existing, _)| *existing == key) {
                Some((_, existing)) if !self.native && !deep_equal(existing, &value) => {
                    return Err(JsonError {
                        reason: format!("Duplicate key '{}' encountered", unescape_lossy(&key)),
                        offset: start + 1,
                    });
                }
                Some((_, existing)) => *existing = value,
                None => entries.push((key, value)),
            }
        }
        if self.peek() != Some(b'}') {
            return Err(self.got_at("Quoted object key or end of object '}' expected"));
        }
        self.i += 1;
        js_order(&mut entries);
        Ok(Some(JsonValue::Object(entries)))
    }

    fn array(&mut self) -> Parsed<JsonValue> {
        if self.peek() != Some(b'[') {
            return Ok(None);
        }
        self.i += 1;
        self.skip_whitespace();
        let mut items = Vec::new();
        let mut initial = true;
        while self.i < self.bytes.len() && self.peek() != Some(b']') {
            if !initial {
                self.eat_comma()?;
            } else {
                initial = false;
            }
            let Some(value) = self.value()? else {
                return Err(self.got_at("Array item expected"));
            };
            items.push(value);
        }
        if self.peek() != Some(b']') {
            return Err(self.got_at("Array item or end of array ']' expected"));
        }
        self.i += 1;
        Ok(Some(JsonValue::Array(items)))
    }

    fn eat_comma(&mut self) -> Result<(), JsonError> {
        if self.peek() != Some(b',') {
            return Err(self.got_at("Comma ',' expected after value"));
        }
        self.i += 1;
        Ok(())
    }

    fn hex4(&self, at: usize) -> Option<u16> {
        let digits = self.bytes.get(at..at + 4)?;
        if !digits.iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        u16::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()
    }

    /// The string at the cursor, in `JSON.stringify` escaped form.
    fn string(&mut self) -> Parsed<String> {
        if self.peek() != Some(b'"') {
            return Ok(None);
        }
        self.i += 1;
        let mut out = String::new();
        while self.i < self.bytes.len() && self.peek() != Some(b'"') {
            if self.peek() == Some(b'\\') {
                let escape = self.bytes.get(self.i + 1).copied();
                let short = match escape {
                    Some(b'"') => Some('"'),
                    Some(b'\\') => Some('\\'),
                    Some(b'/') => Some('/'),
                    Some(b'b') => Some('\u{8}'),
                    Some(b'f') => Some('\u{c}'),
                    Some(b'n') => Some('\n'),
                    Some(b'r') => Some('\r'),
                    Some(b't') => Some('\t'),
                    _ => None,
                };
                if let Some(c) = short {
                    push_escaped(&mut out, c);
                    self.i += 2;
                    continue;
                }
                if escape == Some(b'u') {
                    let Some(unit) = self.hex4(self.i + 2) else {
                        let end = floor_char(self.text, (self.i + 6).min(self.text.len()));
                        let chars = &self.text[self.i..end];
                        return Err(self.error(format!("Invalid unicode character '{chars}'")));
                    };
                    self.i += 6;
                    if (0xd800..0xdc00).contains(&unit)
                        && self.bytes.get(self.i) == Some(&b'\\')
                        && self.bytes.get(self.i + 1) == Some(&b'u')
                        && let Some(low) = self.hex4(self.i + 2)
                        && (0xdc00..0xe000).contains(&low)
                    {
                        let code = 0x10000 + ((unit as u32 - 0xd800) << 10) + (low as u32 - 0xdc00);
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        self.i += 6;
                        continue;
                    }
                    match char::from_u32(unit as u32) {
                        Some(c) => push_escaped(&mut out, c),
                        // A lone surrogate: JSON.stringify writes it escaped.
                        None => out.push_str(&format!("\\u{unit:04x}")),
                    }
                    continue;
                }
                let next = self.text[self.i + 1..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8);
                let chars = &self.text[self.i..self.i + 1 + next];
                return Err(self.error(format!("Invalid escape character '{chars}'")));
            }
            let c = self.text[self.i..].chars().next().unwrap_or('\u{fffd}');
            if (c as u32) < 0x20 {
                return Err(self.error(format!("Invalid character '{c}'")));
            }
            push_escaped(&mut out, c);
            self.i += c.len_utf8();
        }
        if self.peek() != Some(b'"') {
            return Err(self.got_at("End of string '\"' expected"));
        }
        self.i += 1;
        Ok(Some(out))
    }

    fn expect_digit(&self, start: usize) -> Result<(), JsonError> {
        if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
            let so_far = &self.text[start..self.i];
            return Err(self.got_at(&format!("Invalid number '{so_far}', expecting a digit")));
        }
        Ok(())
    }

    fn digits(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.i += 1;
        }
    }

    fn number(&mut self) -> Parsed<JsonValue> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
            self.expect_digit(start)?;
        }
        if self.peek() == Some(b'0') {
            self.i += 1;
        } else if self.peek().is_some_and(|b| (b'1'..=b'9').contains(&b)) {
            self.i += 1;
            self.digits();
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            self.expect_digit(start)?;
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'-' | b'+')) {
                self.i += 1;
            }
            self.expect_digit(start)?;
            self.digits();
        }
        if self.i == start {
            return Ok(None);
        }
        let literal = &self.text[start..self.i];
        Ok(Some(JsonValue::Number(if self.native {
            js_number_string(literal.parse::<f64>().unwrap_or(f64::NAN))
        } else {
            literal.to_string()
        })))
    }
}

fn floor_char(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn push_escaped(out: &mut String, c: char) {
    match c {
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '\u{8}' => out.push_str("\\b"),
        '\u{c}' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
        c => out.push(c),
    }
}

/// The text of an escaped key, for messages.
fn unescape_lossy(escaped: &str) -> String {
    serde_json::from_str::<String>(&format!("\"{escaped}\""))
        .unwrap_or_else(|_| escaped.to_string())
}

fn deep_equal(a: &JsonValue, b: &JsonValue) -> bool {
    match (a, b) {
        (JsonValue::Array(a), JsonValue::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| deep_equal(a, b))
        }
        (JsonValue::Object(a), JsonValue::Object(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key, value)| {
                    b.iter()
                        .find(|(other, _)| other == key)
                        .is_some_and(|(_, other)| deep_equal(value, other))
                })
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_error(text: &str) -> JsonError {
        format_json(text).expect_err("expected format_json to fail")
    }

    #[test]
    fn keeps_large_numbers_exact() {
        assert_eq!(
            format_json(r#"{"id":12345678901234567890,"n":0.10000000000000000001,"e":1E+400}"#)
                .unwrap(),
            "{\n  \"id\": 12345678901234567890,\n  \"n\": 0.10000000000000000001,\n  \"e\": 1E+400\n}"
        );
    }

    #[test]
    fn formats_nested_values_with_two_spaces() {
        assert_eq!(
            format_json(r#" [1, {"a": [], "b": {}}, "x\/y\u00e9\ud83d\ude00", true, null] "#)
                .unwrap(),
            "[\n  1,\n  {\n    \"a\": [],\n    \"b\": {}\n  },\n  \"x/yé😀\",\n  true,\n  null\n]"
        );
    }

    #[test]
    fn escapes_strings_as_json_stringify_does() {
        assert_eq!(
            format_json(r#""a\u0001\"\\\n\ud800""#).unwrap(),
            r#""a\u0001\"\\\n\ud800""#
        );
    }

    #[test]
    fn orders_index_keys_first() {
        assert_eq!(
            canonical_json(r#"{"b":1,"2":2,"1":3,"01":4}"#).unwrap(),
            r#"{"1":3,"2":2,"b":1,"01":4}"#
        );
    }

    #[test]
    fn rejects_a_duplicate_key_with_another_value() {
        let error = json_error(r#"{"a":1,"a":2}"#);
        assert_eq!(
            error.to_string(),
            "Duplicate key 'a' encountered at position 8"
        );
        assert!(format_json(r#"{"a":1,"a":1}"#).is_ok());
    }

    #[test]
    fn canonical_json_normalizes_numbers_and_space() {
        assert_eq!(
            canonical_json(r#"{"a": 1.0, "b": [1e21, 0.0000001]}"#).unwrap(),
            r#"{"a":1,"b":[1e+21,1e-7]}"#
        );
        assert_eq!(canonical_json("abc"), None);
        assert_eq!(canonical_json(r#"{"a":1,"a":2}"#).unwrap(), r#"{"a":2}"#);
    }

    #[test]
    fn writes_numbers_as_javascript_does() {
        for (value, text) in [
            (120.0, "120"),
            (12.5, "12.5"),
            (-0.0, "0"),
            (1e21, "1e+21"),
            (123456789012345680000.0, "123456789012345680000"),
            (1.5e-7, "1.5e-7"),
            (0.000001, "0.000001"),
            (0.1 + 0.2, "0.30000000000000004"),
        ] {
            assert_eq!(js_number_string(value), text);
        }
    }

    #[test]
    fn reports_lossless_json_messages() {
        assert_eq!(
            json_error("").to_string(),
            "JSON value expected but reached end of input at position 0"
        );
        assert_eq!(
            json_error("[1,]").to_string(),
            "Array item expected but got ']' at position 3"
        );
        assert_eq!(
            json_error("01").to_string(),
            "Expected end of input but got '1' at position 1"
        );
        assert_eq!(
            json_error("{\"a\" 1}").to_string(),
            "Colon ':' expected after property name but got '1' at position 5"
        );
        assert_eq!(
            json_error("-x").to_string(),
            "Invalid number '-', expecting a digit but got 'x' at position 1"
        );
        assert_eq!(
            json_error("\"\\x\"").to_string(),
            "Invalid escape character '\\x' at position 1"
        );
    }

    // Ported from text-location.test.ts (jsonErrorLocation).
    #[test]
    fn locates_an_error_in_the_middle_of_the_text() {
        let text = "{\n  \"a\": 1,\n  \"b\": }";
        assert_eq!(
            json_error_location(text, &json_error(text)),
            TextLocation {
                line: 3,
                column: 8,
                offset: 19,
                reason: "Object value expected after ':'".into()
            }
        );
    }

    #[test]
    fn locates_an_error_at_the_end_of_the_text() {
        let text = "{\"a\":1";
        let location = json_error_location(text, &json_error(text));
        assert_eq!((location.line, location.column, location.offset), (1, 7, 6));
    }
}
