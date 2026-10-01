//! Indent HTML response bodies for the pretty view.
//!
//! The formatter accepts any input and never fails: it puts tags, text, and
//! comments on indented lines, and does not repair, reorder, or drop markup.
//! Tag source, entities, and attributes stay as written. Script and style
//! content stays as received; pre and textarea elements stay on one line
//! with their content, because their whitespace is significant.

const INDENT: &str = "  ";

/// Elements that have no end tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Elements whose content is not parsed as markup here.
const RAW: &[&str] = &["script", "style", "pre", "textarea"];

#[derive(Debug, PartialEq)]
enum Token<'a> {
    Text(&'a str),
    /// A comment, doctype, or processing instruction.
    Other(&'a str),
    Start {
        name: String,
        source: &'a str,
        closed: bool,
    },
    End {
        name: String,
        source: &'a str,
    },
    /// The content of a `RAW` element.
    Raw(&'a str),
}

pub fn format_html(text: &str) -> String {
    let tokens = tokenize(text);
    let mut printer = Printer::default();
    let mut at = 0;
    while at < tokens.len() {
        match &tokens[at] {
            Token::Text(text) => {
                for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
                    printer.line(line);
                }
            }
            Token::Other(source) | Token::Raw(source) => printer.line(source),
            Token::End { name, source } => {
                if let Some(open) = printer.open.iter().rposition(|open| open == name) {
                    printer.open.truncate(open);
                }
                printer.line(source);
            }
            Token::Start {
                name,
                source,
                closed,
            } => {
                printer.close_implied(name);
                if *closed || VOID.contains(&name.as_str()) {
                    printer.line(source);
                } else if RAW.contains(&name.as_str()) {
                    let content = match tokens.get(at + 1) {
                        Some(Token::Raw(content)) => {
                            at += 1;
                            *content
                        }
                        _ => "",
                    };
                    let end = match tokens.get(at + 1) {
                        Some(Token::End { name: end, source }) if end == name => {
                            at += 1;
                            *source
                        }
                        _ => "",
                    };
                    printer.raw(name, source, content, end);
                } else if let Some((line, skip)) = inline_element(name, source, &tokens[at + 1..]) {
                    printer.line(&line);
                    at += skip;
                } else {
                    printer.line(source);
                    printer.open.push(name.clone());
                }
            }
        }
        at += 1;
    }
    printer.out
}

/// An element with only one line of text, or nothing, on one line: the
/// line and the number of tokens it uses after the start tag.
fn inline_element(name: &str, start: &str, next: &[Token]) -> Option<(String, usize)> {
    match next {
        [Token::End { name: end, source }, ..] if end == name => {
            Some((format!("{start}{source}"), 1))
        }
        [Token::Text(text), Token::End { name: end, source }, ..]
            if end == name && !text.trim().contains('\n') =>
        {
            Some((format!("{start}{}{source}", text.trim()), 2))
        }
        _ => None,
    }
}

#[derive(Default)]
struct Printer {
    out: String,
    /// Open elements; their count is the indent depth.
    open: Vec<String>,
}

impl Printer {
    fn line(&mut self, text: &str) {
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        for _ in 0..self.open.len() {
            self.out.push_str(INDENT);
        }
        self.out.push_str(text);
    }

    fn raw(&mut self, name: &str, start: &str, content: &str, end: &str) {
        if matches!(name, "pre" | "textarea") || content.trim().is_empty() {
            self.line(&format!("{start}{content}{end}"));
            return;
        }
        // Script and style: the content lines as received, without the
        // blank lines around them.
        let first = content.len() - content.trim_start().len();
        let from = content[..first]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        self.line(start);
        self.out.push('\n');
        self.out.push_str(content[from..].trim_end());
        if !end.is_empty() {
            self.line(end);
        }
    }

    /// Close the elements that a start tag `name` ends without end tags,
    /// such as an open `li` before the next `li`.
    fn close_implied(&mut self, name: &str) {
        let siblings: &[&str] = match name {
            "li" => &["li"],
            "dt" | "dd" => &["dt", "dd"],
            "td" | "th" => &["td", "th"],
            "tr" => &["tr", "td", "th"],
            "option" => &["option"],
            "p" => &["p"],
            _ => return,
        };
        while self
            .open
            .last()
            .is_some_and(|open| siblings.contains(&open.as_str()))
        {
            self.open.pop();
        }
    }
}

fn tokenize(text: &str) -> Vec<Token<'_>> {
    // ASCII lowercasing keeps byte offsets, so `lower` indexes like `text`.
    let lower = text.to_ascii_lowercase();
    let mut tokens = Vec::new();
    let mut at = 0;
    let mut text_start = 0;
    while at < text.len() {
        let Some((token, end)) = (text.as_bytes()[at] == b'<')
            .then(|| markup(text, &lower, at))
            .flatten()
        else {
            at += 1;
            continue;
        };
        if text_start < at {
            tokens.push(Token::Text(&text[text_start..at]));
        }
        let raw_end = match &token {
            Token::Start {
                name,
                closed: false,
                ..
            } if RAW.contains(&name.as_str()) => Some(format!("</{name}")),
            _ => None,
        };
        tokens.push(token);
        at = end;
        if let Some(raw_end) = raw_end {
            let content_end = lower[at..].find(&raw_end).map_or(text.len(), |i| at + i);
            if content_end > at {
                tokens.push(Token::Raw(&text[at..content_end]));
            }
            at = content_end;
        }
        text_start = at;
    }
    if text_start < text.len() {
        tokens.push(Token::Text(&text[text_start..]));
    }
    tokens
}

/// The markup that starts at `at` (a `<`) and the offset after it, or
/// `None` when the `<` is text.
fn markup<'a>(text: &'a str, lower: &str, at: usize) -> Option<(Token<'a>, usize)> {
    let rest = &text[at..];
    if let Some(comment) = rest.strip_prefix("<!--") {
        let end = comment.find("-->").map_or(text.len(), |i| at + 4 + i + 3);
        return Some((Token::Other(&text[at..end]), end));
    }
    if rest.starts_with("<!") || rest.starts_with("<?") {
        let end = rest.find('>').map_or(text.len(), |i| at + i + 1);
        return Some((Token::Other(&text[at..end]), end));
    }
    let closing = rest.starts_with("</");
    let name_start = at + if closing { 2 } else { 1 };
    let bytes = text.as_bytes();
    if !bytes.get(name_start).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let end = tag_end(bytes, name_start)?;
    let name_end = bytes[name_start..end]
        .iter()
        .position(|&b| b.is_ascii_whitespace() || b == b'/' || b == b'>')
        .map_or(end, |i| name_start + i);
    let name = lower[name_start..name_end].to_string();
    let source = &text[at..end];
    let token = if closing {
        Token::End { name, source }
    } else {
        Token::Start {
            name,
            source,
            closed: source.ends_with("/>"),
        }
    };
    Some((token, end))
}

/// The offset after the `>` that ends a tag, skipping quoted attribute
/// values. `None` when the tag does not end.
fn tag_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut at = from;
    while at < bytes.len() {
        match bytes[at] {
            b'>' => return Some(at + 1),
            quote @ (b'"' | b'\'') => {
                at += 1 + bytes[at + 1..].iter().position(|&b| b == quote)?;
            }
            _ => {}
        }
        at += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indents_nested_elements_and_keeps_short_text_inline() {
        let html = r#"<!DOCTYPE html><html lang="en"><head><meta charset="UTF-8"><title>Not found</title></head><body><div class="a"><p>Missing <b>page</b></p><br/></div></body></html>"#;
        assert_eq!(
            format_html(html),
            r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8">
    <title>Not found</title>
  </head>
  <body>
    <div class="a">
      <p>
        Missing
        <b>page</b>
      </p>
      <br/>
    </div>
  </body>
</html>"#
        );
    }

    #[test]
    fn keeps_script_and_style_content_as_received() {
        let html = "<head><script>var a = 1;\n  if (a < 2) { go(\"</div>\"); }\n</script><style>\n\n  p { color: red }\n</style><script src=\"x.js\"></script></head>";
        assert_eq!(
            format_html(html),
            "<head>\n  <script>\nvar a = 1;\n  if (a < 2) { go(\"</div>\"); }\n  </script>\n  <style>\n  p { color: red }\n  </style>\n  <script src=\"x.js\"></script>\n</head>"
        );
    }

    #[test]
    fn keeps_pre_and_textarea_whitespace() {
        let html = "<div><pre>a\n  <b>b</b>  </pre><TEXTAREA> x </textarea></div>";
        assert_eq!(
            format_html(html),
            "<div>\n  <pre>a\n  <b>b</b>  </pre>\n  <TEXTAREA> x </textarea>\n</div>"
        );
    }

    #[test]
    fn keeps_comments_attributes_and_entities_as_written() {
        let html = "<ul><!-- note --><li><a href='/?a=1&amp;b=\"2\"' data-x=\"a>b\">A &amp; B</a></li></ul>";
        assert_eq!(
            format_html(html),
            "<ul>\n  <!-- note -->\n  <li>\n    <a href='/?a=1&amp;b=\"2\"' data-x=\"a>b\">A &amp; B</a>\n  </li>\n</ul>"
        );
    }

    #[test]
    fn closes_implied_end_tags() {
        let html = "<ul><li>One<li>Two</ul><table><tr><td>1<td>2<tr><td>3</table>";
        assert_eq!(
            format_html(html),
            "<ul>\n  <li>\n    One\n  <li>\n    Two\n</ul>\n<table>\n  <tr>\n    <td>\n      1\n    <td>\n      2\n  <tr>\n    <td>\n      3\n</table>"
        );
    }

    #[test]
    fn accepts_markup_that_is_not_well_formed() {
        assert_eq!(
            format_html("</p><div>a < b<span"),
            "</p>\n<div>\n  a < b<span"
        );
        assert_eq!(
            format_html("<div><span>x</div>"),
            "<div>\n  <span>\n    x\n</div>"
        );
        assert_eq!(format_html("<!-- open"), "<!-- open");
        assert_eq!(format_html("<script>let a"), "<script>\nlet a");
        assert_eq!(format_html(""), "");
        assert_eq!(format_html("plain text"), "plain text");
    }

    #[test]
    fn splits_text_lines_and_drops_whitespace_between_tags() {
        assert_eq!(
            format_html("<div>\n   one\n\n   two  \n</div>\n\n<p>  x  </p>"),
            "<div>\n  one\n  two\n</div>\n<p>x</p>"
        );
    }
}
