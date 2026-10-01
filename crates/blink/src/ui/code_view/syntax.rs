//! Line highlighting for response bodies. The Vue app ran highlight.js on
//! each line; this is a small tokenizer per language with the same roles
//! (`style.css` maps highlight.js classes to `SyntaxRole`s).

use std::ops::Range;

use blink_core::response_content::ResponseLanguage;
use blink_core::theme::SyntaxRole;

pub type Spans = Vec<(Range<usize>, SyntaxRole)>;

/// Colored ranges of one line. Plain text gets no range.
pub fn highlight_line(text: &str, language: ResponseLanguage) -> Spans {
    use ResponseLanguage::*;
    if text.is_empty() {
        return vec![];
    }
    match language {
        Plaintext | Markdown => vec![],
        Json => json(text),
        Xml | Html => xml(text),
        Yaml => yaml(text),
        Ini => ini(text),
        Css => generic(text, &Rules::CSS),
        Javascript => generic(text, &Rules::JAVASCRIPT),
        Graphql => generic(text, &Rules::GRAPHQL),
        Sql => generic(text, &Rules::SQL),
        Bash => generic(text, &Rules::BASH),
    }
}

/// End of a quoted string starting at `start` (the quote), past the
/// closing quote or at the end of the line.
fn string_end(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut at = start + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            c if c == quote => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

fn number_end(bytes: &[u8], start: usize) -> usize {
    let mut at = start;
    if bytes.get(at) == Some(&b'-') {
        at += 1;
    }
    while at < bytes.len()
        && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'.' | b'+' | b'-'))
    {
        // A sign only follows an exponent.
        if matches!(bytes[at], b'+' | b'-') && !matches!(bytes[at - 1], b'e' | b'E') {
            break;
        }
        at += 1;
    }
    at
}

fn word_end(bytes: &[u8], start: usize) -> usize {
    let mut at = start;
    while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
        at += 1;
    }
    at
}

fn json(text: &str) -> Spans {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let c = bytes[at];
        if c == b'"' {
            let end = string_end(bytes, at);
            // A key is followed by a colon: `hljs-attr`, plain color.
            let rest = text[end..].trim_start();
            if !rest.starts_with(':') {
                spans.push((at..end, SyntaxRole::String));
            }
            at = end;
        } else if c == b'-' || c.is_ascii_digit() {
            let end = number_end(bytes, at).max(at + 1);
            spans.push((at..end, SyntaxRole::Number));
            at = end;
        } else if c.is_ascii_alphabetic() {
            let end = word_end(bytes, at);
            if matches!(&text[at..end], "true" | "false" | "null") {
                spans.push((at..end, SyntaxRole::Number));
            }
            at = end;
        } else {
            at += text[at..].chars().next().map_or(1, char::len_utf8);
        }
    }
    spans
}

fn xml(text: &str) -> Spans {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if text[at..].starts_with("<!--") {
            let end = text[at..]
                .find("-->")
                .map_or(bytes.len(), |end| at + end + 3);
            spans.push((at..end, SyntaxRole::Comment));
            at = end;
        } else if text[at..].starts_with("<?") || text[at..].starts_with("<!") {
            let end = text[at..].find('>').map_or(bytes.len(), |end| at + end + 1);
            spans.push((at..end, SyntaxRole::Comment));
            at = end;
        } else if bytes[at] == b'<' {
            // Tag: the name is `hljs-name`, attributes plain, values strings.
            let mut cursor = at + 1;
            if bytes.get(cursor) == Some(&b'/') {
                cursor += 1;
            }
            let name_end = tag_name_end(bytes, cursor);
            spans.push((at..name_end, SyntaxRole::Keyword));
            cursor = name_end;
            while cursor < bytes.len() && bytes[cursor] != b'>' {
                match bytes[cursor] {
                    b'"' | b'\'' => {
                        let end = string_end(bytes, cursor);
                        spans.push((cursor..end, SyntaxRole::String));
                        cursor = end;
                    }
                    _ => cursor += text[cursor..].chars().next().map_or(1, char::len_utf8),
                }
            }
            if cursor < bytes.len() {
                let start = if cursor > 0 && bytes[cursor - 1] == b'/' {
                    cursor - 1
                } else {
                    cursor
                };
                spans.push((start..cursor + 1, SyntaxRole::Keyword));
                cursor += 1;
            }
            at = cursor;
        } else {
            at += text[at..].chars().next().map_or(1, char::len_utf8);
        }
    }
    spans
}

fn tag_name_end(bytes: &[u8], start: usize) -> usize {
    let mut at = start;
    while at < bytes.len()
        && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'-' | b'_' | b':' | b'.'))
    {
        at += 1;
    }
    at
}

fn scalar(text: &str, offset: usize, spans: &mut Spans) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let start = offset + (text.len() - text.trim_start().len());
    let range = start..start + trimmed.len();
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        spans.push((range, SyntaxRole::String));
    } else if matches!(
        trimmed,
        "true" | "false" | "null" | "~" | "yes" | "no" | "on" | "off"
    ) || trimmed.parse::<f64>().is_ok()
    {
        spans.push((range, SyntaxRole::Number));
    } else {
        spans.push((range, SyntaxRole::String));
    }
}

fn comment_start(text: &str, marker: char) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut quote = None;
    for (at, c) in text.char_indices() {
        match quote {
            Some(q) if c == q && (at == 0 || bytes[at - 1] != b'\\') => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == marker && (at == 0 || bytes[at - 1].is_ascii_whitespace()) => {
                return Some(at);
            }
            None => {}
        }
    }
    None
}

fn yaml(text: &str) -> Spans {
    let mut spans = Vec::new();
    let body_end = comment_start(text, '#').unwrap_or(text.len());
    let body = &text[..body_end];
    let trimmed = body.trim_start();
    if trimmed.starts_with("---") || trimmed.starts_with("...") {
        spans.push((0..body_end, SyntaxRole::Comment));
    } else {
        let item = body.len() - trimmed.len();
        let content = if let Some(rest) = trimmed.strip_prefix("- ") {
            (item + 2, rest)
        } else {
            (item, trimmed)
        };
        match content
            .1
            .find(": ")
            .or_else(|| content.1.strip_suffix(':').map(str::len))
        {
            // `key: value`: the key is `hljs-attr`, plain.
            Some(colon) => {
                let value_start = (colon + 1).min(content.1.len());
                scalar(
                    &content.1[value_start..],
                    content.0 + value_start,
                    &mut spans,
                );
            }
            None => scalar(content.1, content.0, &mut spans),
        }
    }
    if body_end < text.len() {
        spans.push((body_end..text.len(), SyntaxRole::Comment));
    }
    spans
}

fn ini(text: &str) -> Spans {
    let trimmed = text.trim_start();
    let offset = text.len() - trimmed.len();
    if trimmed.starts_with(';') || trimmed.starts_with('#') {
        return vec![(offset..text.len(), SyntaxRole::Comment)];
    }
    let mut spans = Vec::new();
    if let Some(equals) = text.find('=') {
        scalar(&text[equals + 1..], equals + 1, &mut spans);
    }
    spans
}

struct Rules {
    line_comment: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    keywords: &'static [&'static str],
    literals: &'static [&'static str],
    case_insensitive: bool,
}

impl Rules {
    const JAVASCRIPT: Rules = Rules {
        line_comment: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: &[
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "default",
            "delete",
            "do",
            "else",
            "export",
            "extends",
            "finally",
            "for",
            "function",
            "if",
            "import",
            "in",
            "instanceof",
            "let",
            "new",
            "return",
            "static",
            "super",
            "switch",
            "this",
            "throw",
            "try",
            "typeof",
            "var",
            "void",
            "while",
            "with",
            "yield",
            "from",
            "of",
        ],
        literals: &["true", "false", "null", "undefined", "NaN", "Infinity"],
        case_insensitive: false,
    };
    const CSS: Rules = Rules {
        line_comment: &[],
        block_comment: Some(("/*", "*/")),
        keywords: &[
            "@media",
            "@import",
            "@font-face",
            "@keyframes",
            "!important",
        ],
        literals: &[],
        case_insensitive: false,
    };
    const GRAPHQL: Rules = Rules {
        line_comment: &["#"],
        block_comment: None,
        keywords: &[
            "query",
            "mutation",
            "subscription",
            "fragment",
            "on",
            "type",
            "input",
            "enum",
            "interface",
            "union",
            "scalar",
            "schema",
            "extend",
            "directive",
            "implements",
        ],
        literals: &["true", "false", "null"],
        case_insensitive: false,
    };
    const SQL: Rules = Rules {
        line_comment: &["--"],
        block_comment: Some(("/*", "*/")),
        keywords: &[
            "select", "from", "where", "and", "or", "not", "insert", "into", "values", "update",
            "set", "delete", "create", "table", "drop", "alter", "join", "left", "right", "inner",
            "outer", "on", "group", "by", "order", "having", "limit", "offset", "as", "distinct",
            "union", "all", "in", "is", "like", "between", "exists", "case", "when", "then",
            "else", "end", "primary", "key", "index", "view", "asc", "desc",
        ],
        literals: &["true", "false", "null"],
        case_insensitive: true,
    };
    const BASH: Rules = Rules {
        line_comment: &["#"],
        block_comment: None,
        keywords: &[
            "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case",
            "esac", "in", "function", "return", "export", "local",
        ],
        literals: &["true", "false"],
        case_insensitive: false,
    };
}

fn generic(text: &str, rules: &Rules) -> Spans {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let rest = &text[at..];
        if rules
            .line_comment
            .iter()
            .any(|marker| rest.starts_with(marker))
        {
            spans.push((at..bytes.len(), SyntaxRole::Comment));
            break;
        }
        if let Some((open, close)) = rules.block_comment
            && rest.starts_with(open)
        {
            let end = rest[open.len()..]
                .find(close)
                .map_or(bytes.len(), |end| at + open.len() + end + close.len());
            spans.push((at..end, SyntaxRole::Comment));
            at = end;
            continue;
        }
        let c = bytes[at];
        if matches!(c, b'"' | b'\'' | b'`') {
            let end = string_end(bytes, at);
            spans.push((at..end, SyntaxRole::String));
            at = end;
        } else if c.is_ascii_digit() && (at == 0 || !is_word(bytes[at - 1])) {
            let end = number_end(bytes, at);
            spans.push((at..end, SyntaxRole::Number));
            at = end;
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'@' || c == b'!' {
            let end = word_end(bytes, at + 1);
            let word = &text[at..end];
            let matches = |list: &[&str]| {
                list.iter().any(|candidate| {
                    if rules.case_insensitive {
                        candidate.eq_ignore_ascii_case(word)
                    } else {
                        *candidate == word
                    }
                })
            };
            if matches(rules.keywords) {
                spans.push((at..end, SyntaxRole::Keyword));
            } else if matches(rules.literals) {
                spans.push((at..end, SyntaxRole::Number));
            }
            at = end;
        } else {
            at += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    spans
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` exports its own `test` attribute; use the standard one.
    use core::prelude::v1::test;

    fn roles(text: &str, language: ResponseLanguage) -> Vec<(&str, SyntaxRole)> {
        highlight_line(text, language)
            .into_iter()
            .map(|(range, role)| (&text[range], role))
            .collect()
    }

    #[test]
    fn colors_json_values_but_not_keys() {
        assert_eq!(
            roles(
                r#"  "id": 12, "ok": true, "name": "a\"b""#,
                ResponseLanguage::Json
            ),
            [
                ("12", SyntaxRole::Number),
                ("true", SyntaxRole::Number),
                (r#""a\"b""#, SyntaxRole::String),
            ]
        );
    }

    #[test]
    fn colors_xml_tags_values_and_comments() {
        assert_eq!(
            roles(r#"<a href="x">t</a><!-- c -->"#, ResponseLanguage::Xml),
            [
                ("<a", SyntaxRole::Keyword),
                (r#""x""#, SyntaxRole::String),
                (">", SyntaxRole::Keyword),
                ("</a", SyntaxRole::Keyword),
                (">", SyntaxRole::Keyword),
                ("<!-- c -->", SyntaxRole::Comment),
            ]
        );
    }

    #[test]
    fn colors_yaml_scalars_and_comments() {
        assert_eq!(
            roles("count: 3 # note", ResponseLanguage::Yaml),
            [("3", SyntaxRole::Number), ("# note", SyntaxRole::Comment)]
        );
    }

    #[test]
    fn plain_text_has_no_colors() {
        assert!(highlight_line("anything", ResponseLanguage::Plaintext).is_empty());
    }
}
