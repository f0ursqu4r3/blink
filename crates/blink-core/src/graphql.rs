//! GraphQL formatting. Port of `src/lib/graphql.ts`, which runs Prettier's
//! GraphQL printer; this module reproduces its output (print width 60) with
//! a port of the graphql-js parser and the Prettier doc printer.

use std::fmt;

use crate::text_location::{TextLocation, location_from_line_column};

/// A GraphQL parse error, worded as Prettier reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphqlError {
    /// "Syntax Error: … (line:column)", or another formatter error.
    pub message: String,
    /// 1-based line and column (in characters) of a syntax error.
    pub start: Option<(usize, usize)>,
}

impl fmt::Display for GraphqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GraphqlError {}

/// Format a GraphQL document.
pub fn format_graphql(text: &str) -> Result<String, GraphqlError> {
    let bom = text.starts_with('\u{FEFF}');
    let body = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    // Prettier returns blank input unchanged, as an empty document.
    if crate::curl_import::js_trim(body).is_empty() {
        return Ok(String::new());
    }
    let body = body.replace("\r\n", "\n").replace('\r', "\n");
    let chars: Vec<char> = body.chars().collect();
    let (nodes, root, comments) = parse(&chars).map_err(|error| {
        let (line, column) = line_column(&chars, error.position);
        GraphqlError {
            message: format!("Syntax Error: {} ({line}:{column})", error.reason),
            start: Some((line, column)),
        }
    })?;
    let mut printer = Printer::new(&chars, nodes, comments);
    printer.attach_comments(root);
    let doc = printer.main_print(root);
    if let Some(comment) = printer.comments.iter().find(|comment| !comment.printed) {
        return Err(GraphqlError {
            message: format!(
                "Comment \"{}\" was not printed. Please report this error!",
                comment.value.trim()
            ),
            start: None,
        });
    }
    let mut formatted = print_doc(doc);
    if bom {
        formatted.insert(0, '\u{FEFF}');
    }
    if formatted.ends_with('\n') {
        formatted.pop();
    }
    Ok(formatted)
}

/// Syntax errors carry a start and a "Syntax Error: … (l:c)" message.
pub fn graphql_error_location(text: &str, error: &GraphqlError) -> Option<TextLocation> {
    let (line, column) = error.start?;
    let first = error.message.split('\n').next().unwrap_or("");
    let reason = first
        .strip_prefix("Syntax Error:")
        .map(|rest| rest.trim_start())
        .unwrap_or(first);
    let reason = strip_position_suffix(reason);
    Some(location_from_line_column(text, line, column, reason))
}

/// `.replace(/\s*\(\d+:\d+\)\s*$/, '')`
fn strip_position_suffix(text: &str) -> &str {
    let trimmed = text.trim_end();
    let Some(inner) = trimmed.strip_suffix(')') else {
        return text;
    };
    let Some(open) = inner.rfind('(') else {
        return text;
    };
    let numbers = &inner[open + 1..];
    let valid = numbers.split_once(':').is_some_and(|(line, column)| {
        !line.is_empty()
            && !column.is_empty()
            && line.bytes().all(|b| b.is_ascii_digit())
            && column.bytes().all(|b| b.is_ascii_digit())
    });
    if valid {
        inner[..open].trim_end()
    } else {
        text
    }
}

/// graphql-js `getLocation` on text with `\n` line ends: 1-based line and column.
fn line_column(chars: &[char], position: usize) -> (usize, usize) {
    let before = &chars[..position.min(chars.len())];
    let line = before.iter().filter(|c| **c == '\n').count() + 1;
    let line_start = before
        .iter()
        .rposition(|c| *c == '\n')
        .map_or(0, |at| at + 1);
    (line, position + 1 - line_start)
}

// ── Lexer (graphql-js) ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tk {
    Sof,
    Eof,
    Bang,
    Dollar,
    Amp,
    ParenL,
    ParenR,
    Spread,
    Colon,
    Equals,
    At,
    BracketL,
    BracketR,
    BraceL,
    Pipe,
    BraceR,
    Name,
    Int,
    Float,
    String,
    BlockString,
}

impl Tk {
    fn desc(self) -> &'static str {
        match self {
            Tk::Sof => "<SOF>",
            Tk::Eof => "<EOF>",
            Tk::Bang => "\"!\"",
            Tk::Dollar => "\"$\"",
            Tk::Amp => "\"&\"",
            Tk::ParenL => "\"(\"",
            Tk::ParenR => "\")\"",
            Tk::Spread => "\"...\"",
            Tk::Colon => "\":\"",
            Tk::Equals => "\"=\"",
            Tk::At => "\"@\"",
            Tk::BracketL => "\"[\"",
            Tk::BracketR => "\"]\"",
            Tk::BraceL => "\"{\"",
            Tk::Pipe => "\"|\"",
            Tk::BraceR => "\"}\"",
            Tk::Name => "Name",
            Tk::Int => "Int",
            Tk::Float => "Float",
            Tk::String => "String",
            Tk::BlockString => "BlockString",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub kind: Tk,
    /// Character offsets.
    pub start: usize,
    pub end: usize,
    pub value: Option<String>,
}

fn token_desc(token: &Token) -> String {
    match &token.value {
        Some(value) => format!("{} \"{value}\"", token.kind.desc()),
        None => token.kind.desc().to_string(),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SyntaxError {
    /// Character offset.
    pub position: usize,
    pub reason: String,
}

fn error<T>(position: usize, reason: impl Into<String>) -> Result<T, SyntaxError> {
    Err(SyntaxError {
        position,
        reason: reason.into(),
    })
}

#[derive(Debug, Clone)]
pub(crate) struct Comment {
    pub start: usize,
    pub end: usize,
    pub value: String,
}

fn is_digit(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_digit())
}

fn is_name_start(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

fn is_name_continue(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn hex(c: Option<char>) -> i32 {
    c.and_then(|c| c.to_digit(16)).map_or(-1, |d| d as i32)
}

/// graphql-js `printCodePointAt`.
fn print_char(chars: &[char], at: usize) -> String {
    match chars.get(at) {
        None => "<EOF>".into(),
        Some('"') => "'\"'".into(),
        Some(c) if (' '..='~').contains(c) => format!("\"{c}\""),
        Some(c) => format!("U+{:04X}", *c as u32),
    }
}

/// Lexes on demand, as graphql-js does, so errors surface in reading order.
pub(crate) struct Lexer<'a> {
    chars: &'a [char],
    tokens: Vec<Token>,
    pub comments: Vec<Comment>,
    at: usize,
    last: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(chars: &'a [char]) -> Self {
        Lexer {
            chars,
            tokens: vec![Token {
                kind: Tk::Sof,
                start: 0,
                end: 0,
                value: None,
            }],
            comments: Vec::new(),
            at: 0,
            last: 0,
        }
    }

    pub fn token(&self) -> &Token {
        &self.tokens[self.at]
    }

    pub fn last_token(&self) -> &Token {
        &self.tokens[self.last]
    }

    fn ensure_next(&mut self) -> Result<(), SyntaxError> {
        if self.at + 1 >= self.tokens.len() && self.tokens[self.at].kind != Tk::Eof {
            let next = self.read(self.tokens[self.at].end)?;
            self.tokens.push(next);
        }
        Ok(())
    }

    pub fn advance(&mut self) -> Result<(), SyntaxError> {
        self.last = self.at;
        self.ensure_next()?;
        if self.tokens[self.at].kind != Tk::Eof {
            self.at += 1;
        }
        Ok(())
    }

    pub fn lookahead(&mut self) -> Result<Token, SyntaxError> {
        self.ensure_next()?;
        Ok(self
            .tokens
            .get(self.at + 1)
            .unwrap_or(&self.tokens[self.at])
            .clone())
    }

    fn get(&self, at: usize) -> Option<char> {
        self.chars.get(at).copied()
    }

    fn slice(&self, start: usize, end: usize) -> String {
        self.chars[start.min(self.chars.len())..end.min(self.chars.len())]
            .iter()
            .collect()
    }

    fn punct(kind: Tk, start: usize, size: usize) -> Result<Token, SyntaxError> {
        Ok(Token {
            kind,
            start,
            end: start + size,
            value: None,
        })
    }

    /// Read the next non-comment token from `from`.
    fn read(&mut self, from: usize) -> Result<Token, SyntaxError> {
        let length = self.chars.len();
        let mut i = from;
        while i < length {
            let c = self.chars[i];
            match c {
                '\u{FEFF}' | '\t' | ' ' | ',' | '\n' | '\r' => {
                    i += 1;
                    continue;
                }
                '#' => {
                    let mut end = i + 1;
                    while end < length && !matches!(self.chars[end], '\n' | '\r') {
                        end += 1;
                    }
                    self.comments.push(Comment {
                        start: i,
                        end,
                        value: self.slice(i + 1, end),
                    });
                    i = end;
                    continue;
                }
                '!' => return Self::punct(Tk::Bang, i, 1),
                '$' => return Self::punct(Tk::Dollar, i, 1),
                '&' => return Self::punct(Tk::Amp, i, 1),
                '(' => return Self::punct(Tk::ParenL, i, 1),
                ')' => return Self::punct(Tk::ParenR, i, 1),
                '.' => {
                    let next = self.get(i + 1);
                    if next == Some('.') && self.get(i + 2) == Some('.') {
                        return Self::punct(Tk::Spread, i, 3);
                    }
                    if next == Some('.') {
                        return error(i, "Unexpected \"..\", did you mean \"...\"?");
                    }
                    if is_digit(next) {
                        let end = self.read_digits(i + 1)?;
                        let digits = self.slice(i + 1, end);
                        return error(
                            i,
                            format!(
                                "Invalid number, expected digit before \".\", did you mean \"0.{digits}\"?"
                            ),
                        );
                    }
                }
                ':' => return Self::punct(Tk::Colon, i, 1),
                '=' => return Self::punct(Tk::Equals, i, 1),
                '@' => return Self::punct(Tk::At, i, 1),
                '[' => return Self::punct(Tk::BracketL, i, 1),
                ']' => return Self::punct(Tk::BracketR, i, 1),
                '{' => return Self::punct(Tk::BraceL, i, 1),
                '|' => return Self::punct(Tk::Pipe, i, 1),
                '}' => return Self::punct(Tk::BraceR, i, 1),
                '"' => {
                    return if self.get(i + 1) == Some('"') && self.get(i + 2) == Some('"') {
                        self.read_block_string(i)
                    } else {
                        self.read_string(i)
                    };
                }
                _ => {}
            }
            if c.is_ascii_digit() || c == '-' {
                return self.read_number(i);
            }
            if is_name_start(Some(c)) {
                let mut end = i + 1;
                while is_name_continue(self.get(end)) {
                    end += 1;
                }
                return Ok(Token {
                    kind: Tk::Name,
                    start: i,
                    end,
                    value: Some(self.slice(i, end)),
                });
            }
            return error(
                i,
                if c == '\'' {
                    "Unexpected single quote character ('), did you mean to use a double quote (\")?"
                        .to_string()
                } else {
                    format!("Unexpected character: {}.", print_char(self.chars, i))
                },
            );
        }
        Ok(Token {
            kind: Tk::Eof,
            start: length,
            end: length,
            value: None,
        })
    }

    fn read_digits(&self, start: usize) -> Result<usize, SyntaxError> {
        if !is_digit(self.get(start)) {
            return error(
                start,
                format!(
                    "Invalid number, expected digit but got: {}.",
                    print_char(self.chars, start)
                ),
            );
        }
        let mut i = start + 1;
        while is_digit(self.get(i)) {
            i += 1;
        }
        Ok(i)
    }

    fn read_number(&self, start: usize) -> Result<Token, SyntaxError> {
        let mut i = start;
        let mut c = self.get(i);
        let mut float = false;
        if c == Some('-') {
            i += 1;
            c = self.get(i);
        }
        if c == Some('0') {
            i += 1;
            c = self.get(i);
            if is_digit(c) {
                return error(
                    i,
                    format!(
                        "Invalid number, unexpected digit after 0: {}.",
                        print_char(self.chars, i)
                    ),
                );
            }
        } else {
            i = self.read_digits(i)?;
            c = self.get(i);
        }
        if c == Some('.') {
            float = true;
            i = self.read_digits(i + 1)?;
            c = self.get(i);
        }
        if matches!(c, Some('E' | 'e')) {
            float = true;
            i += 1;
            c = self.get(i);
            if matches!(c, Some('+' | '-')) {
                i += 1;
            }
            i = self.read_digits(i)?;
            c = self.get(i);
        }
        if c == Some('.') || is_name_start(c) {
            return error(
                i,
                format!(
                    "Invalid number, expected digit but got: {}.",
                    print_char(self.chars, i)
                ),
            );
        }
        Ok(Token {
            kind: if float { Tk::Float } else { Tk::Int },
            start,
            end: i,
            value: Some(self.slice(start, i)),
        })
    }

    fn read_string(&self, start: usize) -> Result<Token, SyntaxError> {
        let length = self.chars.len();
        let mut i = start + 1;
        let mut value = String::new();
        while i < length {
            let c = self.chars[i];
            if c == '"' {
                return Ok(Token {
                    kind: Tk::String,
                    start,
                    end: i + 1,
                    value: Some(value),
                });
            }
            if c == '\\' {
                let (escaped, size) = if self.get(i + 1) == Some('u') {
                    if self.get(i + 2) == Some('{') {
                        self.read_unicode_variable(i)?
                    } else {
                        self.read_unicode_fixed(i)?
                    }
                } else {
                    self.read_escaped(i)?
                };
                value.push(escaped);
                i += size;
                continue;
            }
            if c == '\n' || c == '\r' {
                break;
            }
            value.push(c);
            i += 1;
        }
        error(i, "Unterminated string.")
    }

    fn read_unicode_variable(&self, start: usize) -> Result<(char, usize), SyntaxError> {
        let mut point: i32 = 0;
        let mut size = 3;
        while size < 12 {
            let c = self.get(start + size);
            size += 1;
            if c == Some('}') {
                if size < 5 {
                    break;
                }
                match u32::try_from(point).ok().and_then(char::from_u32) {
                    Some(char) => return Ok((char, size)),
                    None => break,
                }
            }
            point = point.wrapping_shl(4) | hex(c);
            if point < 0 {
                break;
            }
        }
        error(
            start,
            format!(
                "Invalid Unicode escape sequence: \"{}\".",
                self.slice(start, start + size)
            ),
        )
    }

    fn read_hex4(&self, at: usize) -> i32 {
        let digits = [
            hex(self.get(at)),
            hex(self.get(at + 1)),
            hex(self.get(at + 2)),
            hex(self.get(at + 3)),
        ];
        if digits.contains(&-1) {
            -1
        } else {
            digits.iter().fold(0, |acc, d| (acc << 4) | d)
        }
    }

    fn read_unicode_fixed(&self, start: usize) -> Result<(char, usize), SyntaxError> {
        let code = self.read_hex4(start + 2);
        if let Some(char) = u32::try_from(code).ok().and_then(char::from_u32) {
            return Ok((char, 6));
        }
        if (0xD800..=0xDBFF).contains(&code)
            && self.get(start + 6) == Some('\\')
            && self.get(start + 7) == Some('u')
        {
            let trail = self.read_hex4(start + 8);
            if (0xDC00..=0xDFFF).contains(&trail) {
                let point = 0x10000 + ((code as u32 - 0xD800) << 10) + (trail as u32 - 0xDC00);
                if let Some(char) = char::from_u32(point) {
                    return Ok((char, 12));
                }
            }
        }
        error(
            start,
            format!(
                "Invalid Unicode escape sequence: \"{}\".",
                self.slice(start, start + 6)
            ),
        )
    }

    fn read_escaped(&self, start: usize) -> Result<(char, usize), SyntaxError> {
        let char = match self.get(start + 1) {
            Some('"') => '"',
            Some('\\') => '\\',
            Some('/') => '/',
            Some('b') => '\u{8}',
            Some('f') => '\u{C}',
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            _ => {
                return error(
                    start,
                    format!(
                        "Invalid character escape sequence: \"{}\".",
                        self.slice(start, start + 2)
                    ),
                );
            }
        };
        Ok((char, 2))
    }

    fn read_block_string(&self, start: usize) -> Result<Token, SyntaxError> {
        let length = self.chars.len();
        let mut i = start + 3;
        let mut line = String::new();
        let mut lines: Vec<String> = Vec::new();
        while i < length {
            let c = self.chars[i];
            if c == '"' && self.get(i + 1) == Some('"') && self.get(i + 2) == Some('"') {
                lines.push(line);
                return Ok(Token {
                    kind: Tk::BlockString,
                    start,
                    end: i + 3,
                    value: Some(dedent_block_string_lines(lines).join("\n")),
                });
            }
            if c == '\\'
                && self.get(i + 1) == Some('"')
                && self.get(i + 2) == Some('"')
                && self.get(i + 3) == Some('"')
            {
                line.push_str("\"\"\"");
                i += 4;
                continue;
            }
            if c == '\n' || c == '\r' {
                lines.push(std::mem::take(&mut line));
                i += if c == '\r' && self.get(i + 1) == Some('\n') {
                    2
                } else {
                    1
                };
                continue;
            }
            line.push(c);
            i += 1;
        }
        error(i, "Unterminated string.")
    }
}

fn dedent_block_string_lines(lines: Vec<String>) -> Vec<String> {
    let mut common = usize::MAX;
    let mut first: Option<usize> = None;
    let mut last: isize = -1;
    for (i, line) in lines.iter().enumerate() {
        let indent = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        if indent == line.chars().count() {
            continue;
        }
        first.get_or_insert(i);
        last = i as isize;
        if i != 0 && indent < common {
            common = indent;
        }
    }
    let start = first.unwrap_or(0);
    let end = (last + 1) as usize;
    lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                line
            } else {
                line.chars().skip(common).collect()
            }
        })
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

// ── Parser (graphql-js, experimental fragment arguments on) ────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Document,
    OperationDefinition,
    VariableDefinition,
    Variable,
    SelectionSet,
    Field,
    Argument,
    FragmentArgument,
    FragmentSpread,
    InlineFragment,
    FragmentDefinition,
    IntValue,
    FloatValue,
    StringValue,
    BooleanValue,
    NullValue,
    EnumValue,
    ListValue,
    ObjectValue,
    ObjectField,
    Directive,
    NamedType,
    ListType,
    NonNullType,
    Name,
    SchemaDefinition,
    OperationTypeDefinition,
    ScalarTypeDefinition,
    ObjectTypeDefinition,
    FieldDefinition,
    InputValueDefinition,
    InterfaceTypeDefinition,
    UnionTypeDefinition,
    EnumTypeDefinition,
    EnumValueDefinition,
    InputObjectTypeDefinition,
    DirectiveDefinition,
    SchemaExtension,
    ScalarTypeExtension,
    ObjectTypeExtension,
    InterfaceTypeExtension,
    UnionTypeExtension,
    EnumTypeExtension,
    InputObjectTypeExtension,
    DirectiveExtension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Description,
    Name,
    Alias,
    Variable,
    VariableDefinitions,
    Directives,
    SelectionSet,
    Selections,
    Arguments,
    Value,
    Values,
    Fields,
    Type,
    TypeCondition,
    DefaultValue,
    Definitions,
    Interfaces,
    OperationTypes,
    Types,
    Locations,
}

#[derive(Debug, Clone)]
struct Node {
    kind: Kind,
    start: usize,
    end: usize,
    /// Name value, literal value, or operation keyword.
    text: String,
    /// Block string, boolean value, or `repeatable`.
    flag: bool,
    props: Vec<(Key, Vec<usize>)>,
}

impl Node {
    fn list(&self, key: Key) -> &[usize] {
        self.props
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(&[], |(_, ids)| ids.as_slice())
    }

    fn one(&self, key: Key) -> Option<usize> {
        self.list(key).first().copied()
    }
}

type Parsed = Result<usize, SyntaxError>;

const DIRECTIVE_LOCATIONS: [&str; 21] = [
    "QUERY",
    "MUTATION",
    "SUBSCRIPTION",
    "FIELD",
    "FRAGMENT_DEFINITION",
    "FRAGMENT_SPREAD",
    "INLINE_FRAGMENT",
    "VARIABLE_DEFINITION",
    "FRAGMENT_VARIABLE_DEFINITION",
    "SCHEMA",
    "SCALAR",
    "OBJECT",
    "FIELD_DEFINITION",
    "ARGUMENT_DEFINITION",
    "INTERFACE",
    "UNION",
    "ENUM",
    "ENUM_VALUE",
    "INPUT_OBJECT",
    "INPUT_FIELD_DEFINITION",
    "DIRECTIVE_DEFINITION",
];

struct Parser<'a> {
    lexer: Lexer<'a>,
    nodes: Vec<Node>,
}

/// Props of a node under construction; absent lists stay absent.
#[derive(Default)]
struct Props(Vec<(Key, Vec<usize>)>);

impl Props {
    fn one(mut self, key: Key, id: usize) -> Self {
        self.0.push((key, vec![id]));
        self
    }

    fn opt(mut self, key: Key, id: Option<usize>) -> Self {
        if let Some(id) = id {
            self.0.push((key, vec![id]));
        }
        self
    }

    fn many(mut self, key: Key, ids: Option<Vec<usize>>) -> Self {
        if let Some(ids) = ids {
            self.0.push((key, ids));
        }
        self
    }
}

type Rule<'a> = fn(&mut Parser<'a>) -> Parsed;

impl<'a> Parser<'a> {
    fn token(&self) -> &Token {
        self.lexer.token()
    }

    fn peek(&self, kind: Tk) -> bool {
        self.token().kind == kind
    }

    fn node(&mut self, start: usize, kind: Kind, text: &str, flag: bool, props: Props) -> usize {
        self.nodes.push(Node {
            kind,
            start,
            end: self.lexer.last_token().end,
            text: text.to_string(),
            flag,
            props: props.0,
        });
        self.nodes.len() - 1
    }

    fn expect_token(&mut self, kind: Tk) -> Result<Token, SyntaxError> {
        let token = self.token().clone();
        if token.kind == kind {
            self.lexer.advance()?;
            return Ok(token);
        }
        error(
            token.start,
            format!("Expected {}, found {}.", kind.desc(), token_desc(&token)),
        )
    }

    fn expect_optional_token(&mut self, kind: Tk) -> Result<bool, SyntaxError> {
        if self.peek(kind) {
            self.lexer.advance()?;
            return Ok(true);
        }
        Ok(false)
    }

    fn is_keyword(&self, value: &str) -> bool {
        self.token().kind == Tk::Name && self.token().value.as_deref() == Some(value)
    }

    fn expect_keyword(&mut self, value: &str) -> Result<(), SyntaxError> {
        if self.is_keyword(value) {
            return self.lexer.advance();
        }
        let token = self.token().clone();
        error(
            token.start,
            format!("Expected \"{value}\", found {}.", token_desc(&token)),
        )
    }

    fn expect_optional_keyword(&mut self, value: &str) -> Result<bool, SyntaxError> {
        if self.is_keyword(value) {
            self.lexer.advance()?;
            return Ok(true);
        }
        Ok(false)
    }

    fn unexpected<T>(&self, token: Option<&Token>) -> Result<T, SyntaxError> {
        let token = token.unwrap_or(self.token());
        error(token.start, format!("Unexpected {}.", token_desc(token)))
    }

    fn any(
        &mut self,
        open: Tk,
        rule: impl Fn(&mut Self) -> Parsed,
        close: Tk,
    ) -> Result<Vec<usize>, SyntaxError> {
        self.expect_token(open)?;
        let mut items = Vec::new();
        while !self.expect_optional_token(close)? {
            items.push(rule(self)?);
        }
        Ok(items)
    }

    fn optional_many(
        &mut self,
        open: Tk,
        rule: Rule<'a>,
        close: Tk,
    ) -> Result<Option<Vec<usize>>, SyntaxError> {
        if !self.expect_optional_token(open)? {
            return Ok(None);
        }
        let mut items = Vec::new();
        loop {
            items.push(rule(self)?);
            if self.expect_optional_token(close)? {
                return Ok(Some(items));
            }
        }
    }

    fn many(&mut self, open: Tk, rule: Rule<'a>, close: Tk) -> Result<Vec<usize>, SyntaxError> {
        self.expect_token(open)?;
        let mut items = Vec::new();
        loop {
            items.push(rule(self)?);
            if self.expect_optional_token(close)? {
                return Ok(items);
            }
        }
    }

    fn delimited_many(&mut self, delimiter: Tk, rule: Rule<'a>) -> Result<Vec<usize>, SyntaxError> {
        self.expect_optional_token(delimiter)?;
        let mut items = Vec::new();
        loop {
            items.push(rule(self)?);
            if !self.expect_optional_token(delimiter)? {
                return Ok(items);
            }
        }
    }

    fn parse_name(&mut self) -> Parsed {
        let token = self.expect_token(Tk::Name)?;
        let value = token.value.clone().unwrap_or_default();
        Ok(self.node(token.start, Kind::Name, &value, false, Props::default()))
    }

    fn parse_document(&mut self) -> Parsed {
        let start = self.token().start;
        let definitions = self.many(Tk::Sof, Self::parse_definition, Tk::Eof)?;
        Ok(self.node(
            start,
            Kind::Document,
            "",
            false,
            Props::default().many(Key::Definitions, Some(definitions)),
        ))
    }

    fn peek_description(&self) -> bool {
        self.peek(Tk::String) || self.peek(Tk::BlockString)
    }

    fn parse_definition(&mut self) -> Parsed {
        if self.peek(Tk::BraceL) {
            return self.parse_operation_definition();
        }
        let has_description = self.peek_description();
        let keyword = if has_description {
            self.lexer.lookahead()?
        } else {
            self.token().clone()
        };
        if has_description && keyword.kind == Tk::BraceL {
            return error(
                self.token().start,
                "Unexpected description, descriptions are not supported on shorthand queries.",
            );
        }
        if keyword.kind == Tk::Name {
            match keyword.value.as_deref() {
                Some("schema") => return self.parse_schema_definition(),
                Some("scalar") => return self.parse_scalar_type_definition(),
                Some("type") => {
                    return self.parse_object_type_definition(Kind::ObjectTypeDefinition, "type");
                }
                Some("interface") => {
                    return self
                        .parse_object_type_definition(Kind::InterfaceTypeDefinition, "interface");
                }
                Some("union") => return self.parse_union_type_definition(),
                Some("enum") => return self.parse_enum_type_definition(),
                Some("input") => return self.parse_input_object_type_definition(),
                Some("directive") => return self.parse_directive_definition(),
                Some("query" | "mutation" | "subscription") => {
                    return self.parse_operation_definition();
                }
                Some("fragment") => return self.parse_fragment_definition(),
                _ => {}
            }
            if has_description {
                return error(
                    self.token().start,
                    "Unexpected description, only GraphQL definitions support descriptions.",
                );
            }
            if keyword.value.as_deref() == Some("extend") {
                return self.parse_type_system_extension();
            }
        }
        self.unexpected(Some(&keyword))
    }

    fn parse_operation_definition(&mut self) -> Parsed {
        let start = self.token().start;
        if self.peek(Tk::BraceL) {
            let selection_set = self.parse_selection_set()?;
            return Ok(self.node(
                start,
                Kind::OperationDefinition,
                "query",
                false,
                Props::default().one(Key::SelectionSet, selection_set),
            ));
        }
        let description = self.parse_description()?;
        let operation = self.parse_operation_type()?;
        let name = if self.peek(Tk::Name) {
            Some(self.parse_name()?)
        } else {
            None
        };
        let variables = self.parse_variable_definitions()?;
        let directives = self.parse_directives(false)?;
        let selection_set = self.parse_selection_set()?;
        Ok(self.node(
            start,
            Kind::OperationDefinition,
            &operation,
            false,
            Props::default()
                .opt(Key::Description, description)
                .opt(Key::Name, name)
                .many(Key::VariableDefinitions, variables)
                .many(Key::Directives, directives)
                .one(Key::SelectionSet, selection_set),
        ))
    }

    fn parse_operation_type(&mut self) -> Result<String, SyntaxError> {
        let token = self.expect_token(Tk::Name)?;
        match token.value.as_deref() {
            Some(value @ ("query" | "mutation" | "subscription")) => Ok(value.to_string()),
            _ => self.unexpected(Some(&token)),
        }
    }

    fn parse_variable_definitions(&mut self) -> Result<Option<Vec<usize>>, SyntaxError> {
        self.optional_many(Tk::ParenL, Self::parse_variable_definition, Tk::ParenR)
    }

    fn parse_variable_definition(&mut self) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        let variable = self.parse_variable()?;
        self.expect_token(Tk::Colon)?;
        let kind = self.parse_type_reference()?;
        let default = if self.expect_optional_token(Tk::Equals)? {
            Some(self.parse_value_literal(true)?)
        } else {
            None
        };
        let directives = self.parse_directives(true)?;
        Ok(self.node(
            start,
            Kind::VariableDefinition,
            "",
            false,
            Props::default()
                .opt(Key::Description, description)
                .one(Key::Variable, variable)
                .one(Key::Type, kind)
                .opt(Key::DefaultValue, default)
                .many(Key::Directives, directives),
        ))
    }

    fn parse_variable(&mut self) -> Parsed {
        let start = self.token().start;
        self.expect_token(Tk::Dollar)?;
        let name = self.parse_name()?;
        Ok(self.node(
            start,
            Kind::Variable,
            "",
            false,
            Props::default().one(Key::Name, name),
        ))
    }

    fn parse_selection_set(&mut self) -> Parsed {
        let start = self.token().start;
        let selections = self.many(Tk::BraceL, Self::parse_selection, Tk::BraceR)?;
        Ok(self.node(
            start,
            Kind::SelectionSet,
            "",
            false,
            Props::default().many(Key::Selections, Some(selections)),
        ))
    }

    fn parse_selection(&mut self) -> Parsed {
        if self.peek(Tk::Spread) {
            self.parse_fragment()
        } else {
            self.parse_field()
        }
    }

    fn parse_field(&mut self) -> Parsed {
        let start = self.token().start;
        let first = self.parse_name()?;
        let (alias, name) = if self.expect_optional_token(Tk::Colon)? {
            (Some(first), self.parse_name()?)
        } else {
            (None, first)
        };
        let arguments = self.optional_many(Tk::ParenL, Self::parse_argument, Tk::ParenR)?;
        let directives = self.parse_directives(false)?;
        let selection_set = if self.peek(Tk::BraceL) {
            Some(self.parse_selection_set()?)
        } else {
            None
        };
        Ok(self.node(
            start,
            Kind::Field,
            "",
            false,
            Props::default()
                .opt(Key::Alias, alias)
                .one(Key::Name, name)
                .many(Key::Arguments, arguments)
                .many(Key::Directives, directives)
                .opt(Key::SelectionSet, selection_set),
        ))
    }

    fn parse_argument_with(&mut self, kind: Kind, constant: bool) -> Parsed {
        let start = self.token().start;
        let name = self.parse_name()?;
        self.expect_token(Tk::Colon)?;
        let value = self.parse_value_literal(constant)?;
        Ok(self.node(
            start,
            kind,
            "",
            false,
            Props::default().one(Key::Name, name).one(Key::Value, value),
        ))
    }

    fn parse_argument(&mut self) -> Parsed {
        self.parse_argument_with(Kind::Argument, false)
    }

    fn parse_const_argument(&mut self) -> Parsed {
        self.parse_argument_with(Kind::Argument, true)
    }

    fn parse_fragment_argument(&mut self) -> Parsed {
        self.parse_argument_with(Kind::FragmentArgument, false)
    }

    fn parse_fragment(&mut self) -> Parsed {
        let start = self.token().start;
        self.expect_token(Tk::Spread)?;
        let has_type_condition = self.expect_optional_keyword("on")?;
        if !has_type_condition && self.peek(Tk::Name) {
            let name = self.parse_fragment_name()?;
            let arguments = if self.peek(Tk::ParenL) {
                self.optional_many(Tk::ParenL, Self::parse_fragment_argument, Tk::ParenR)?
            } else {
                None
            };
            let directives = self.parse_directives(false)?;
            return Ok(self.node(
                start,
                Kind::FragmentSpread,
                "",
                false,
                Props::default()
                    .one(Key::Name, name)
                    .many(Key::Arguments, arguments)
                    .many(Key::Directives, directives),
            ));
        }
        let type_condition = if has_type_condition {
            Some(self.parse_named_type()?)
        } else {
            None
        };
        let directives = self.parse_directives(false)?;
        let selection_set = self.parse_selection_set()?;
        Ok(self.node(
            start,
            Kind::InlineFragment,
            "",
            false,
            Props::default()
                .opt(Key::TypeCondition, type_condition)
                .many(Key::Directives, directives)
                .one(Key::SelectionSet, selection_set),
        ))
    }

    fn parse_fragment_definition(&mut self) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        self.expect_keyword("fragment")?;
        let name = self.parse_fragment_name()?;
        let variables = self.parse_variable_definitions()?;
        self.expect_keyword("on")?;
        let type_condition = self.parse_named_type()?;
        let directives = self.parse_directives(false)?;
        let selection_set = self.parse_selection_set()?;
        Ok(self.node(
            start,
            Kind::FragmentDefinition,
            "",
            false,
            Props::default()
                .opt(Key::Description, description)
                .one(Key::Name, name)
                .many(Key::VariableDefinitions, variables)
                .one(Key::TypeCondition, type_condition)
                .many(Key::Directives, directives)
                .one(Key::SelectionSet, selection_set),
        ))
    }

    fn parse_fragment_name(&mut self) -> Parsed {
        if self.token().value.as_deref() == Some("on") {
            return self.unexpected(None);
        }
        self.parse_name()
    }

    fn parse_value_literal(&mut self, constant: bool) -> Parsed {
        let token = self.token().clone();
        let value = token.value.clone().unwrap_or_default();
        match token.kind {
            Tk::BracketL => {
                let items = self.any(
                    Tk::BracketL,
                    |p| p.parse_value_literal(constant),
                    Tk::BracketR,
                )?;
                Ok(self.node(
                    token.start,
                    Kind::ListValue,
                    "",
                    false,
                    Props::default().many(Key::Values, Some(items)),
                ))
            }
            Tk::BraceL => {
                let fields =
                    self.any(Tk::BraceL, |p| p.parse_object_field(constant), Tk::BraceR)?;
                Ok(self.node(
                    token.start,
                    Kind::ObjectValue,
                    "",
                    false,
                    Props::default().many(Key::Fields, Some(fields)),
                ))
            }
            Tk::Int => {
                self.lexer.advance()?;
                Ok(self.node(token.start, Kind::IntValue, &value, false, Props::default()))
            }
            Tk::Float => {
                self.lexer.advance()?;
                Ok(self.node(
                    token.start,
                    Kind::FloatValue,
                    &value,
                    false,
                    Props::default(),
                ))
            }
            Tk::String | Tk::BlockString => self.parse_string_literal(),
            Tk::Name => {
                self.lexer.advance()?;
                Ok(match value.as_str() {
                    "true" => {
                        self.node(token.start, Kind::BooleanValue, "", true, Props::default())
                    }
                    "false" => {
                        self.node(token.start, Kind::BooleanValue, "", false, Props::default())
                    }
                    "null" => self.node(token.start, Kind::NullValue, "", false, Props::default()),
                    _ => self.node(
                        token.start,
                        Kind::EnumValue,
                        &value,
                        false,
                        Props::default(),
                    ),
                })
            }
            Tk::Dollar => {
                if constant {
                    self.expect_token(Tk::Dollar)?;
                    if self.token().kind == Tk::Name {
                        let name = self.token().value.clone().unwrap_or_default();
                        return error(
                            token.start,
                            format!("Unexpected variable \"${name}\" in constant value."),
                        );
                    }
                    return self.unexpected(Some(&token));
                }
                self.parse_variable()
            }
            _ => self.unexpected(None),
        }
    }

    fn parse_string_literal(&mut self) -> Parsed {
        let token = self.token().clone();
        self.lexer.advance()?;
        Ok(self.node(
            token.start,
            Kind::StringValue,
            token.value.as_deref().unwrap_or_default(),
            token.kind == Tk::BlockString,
            Props::default(),
        ))
    }

    fn parse_object_field(&mut self, constant: bool) -> Parsed {
        let start = self.token().start;
        let name = self.parse_name()?;
        self.expect_token(Tk::Colon)?;
        let value = self.parse_value_literal(constant)?;
        Ok(self.node(
            start,
            Kind::ObjectField,
            "",
            false,
            Props::default().one(Key::Name, name).one(Key::Value, value),
        ))
    }

    fn parse_directives(&mut self, constant: bool) -> Result<Option<Vec<usize>>, SyntaxError> {
        let mut directives = Vec::new();
        while self.peek(Tk::At) {
            let start = self.token().start;
            self.expect_token(Tk::At)?;
            let name = self.parse_name()?;
            let rule: Rule<'a> = if constant {
                Self::parse_const_argument
            } else {
                Self::parse_argument
            };
            let arguments = self.optional_many(Tk::ParenL, rule, Tk::ParenR)?;
            directives.push(
                self.node(
                    start,
                    Kind::Directive,
                    "",
                    false,
                    Props::default()
                        .one(Key::Name, name)
                        .many(Key::Arguments, arguments),
                ),
            );
        }
        Ok((!directives.is_empty()).then_some(directives))
    }

    fn parse_type_reference(&mut self) -> Parsed {
        let start = self.token().start;
        let inner = if self.expect_optional_token(Tk::BracketL)? {
            let inner = self.parse_type_reference()?;
            self.expect_token(Tk::BracketR)?;
            self.node(
                start,
                Kind::ListType,
                "",
                false,
                Props::default().one(Key::Type, inner),
            )
        } else {
            self.parse_named_type()?
        };
        if self.expect_optional_token(Tk::Bang)? {
            return Ok(self.node(
                start,
                Kind::NonNullType,
                "",
                false,
                Props::default().one(Key::Type, inner),
            ));
        }
        Ok(inner)
    }

    fn parse_named_type(&mut self) -> Parsed {
        let start = self.token().start;
        let name = self.parse_name()?;
        Ok(self.node(
            start,
            Kind::NamedType,
            "",
            false,
            Props::default().one(Key::Name, name),
        ))
    }

    fn parse_description(&mut self) -> Result<Option<usize>, SyntaxError> {
        if self.peek_description() {
            return Ok(Some(self.parse_string_literal()?));
        }
        Ok(None)
    }

    fn parse_schema_definition(&mut self) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        self.expect_keyword("schema")?;
        let directives = self.parse_directives(true)?;
        let operation_types = self.many(
            Tk::BraceL,
            Self::parse_operation_type_definition,
            Tk::BraceR,
        )?;
        Ok(self.node(
            start,
            Kind::SchemaDefinition,
            "",
            false,
            Props::default()
                .opt(Key::Description, description)
                .many(Key::Directives, directives)
                .many(Key::OperationTypes, Some(operation_types)),
        ))
    }

    fn parse_operation_type_definition(&mut self) -> Parsed {
        let start = self.token().start;
        let operation = self.parse_operation_type()?;
        self.expect_token(Tk::Colon)?;
        let kind = self.parse_named_type()?;
        Ok(self.node(
            start,
            Kind::OperationTypeDefinition,
            &operation,
            false,
            Props::default().one(Key::Type, kind),
        ))
    }

    fn parse_scalar_type_definition(&mut self) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        self.expect_keyword("scalar")?;
        let name = self.parse_name()?;
        let directives = self.parse_directives(true)?;
        Ok(self.node(
            start,
            Kind::ScalarTypeDefinition,
            "",
            false,
            Props::default()
                .opt(Key::Description, description)
                .one(Key::Name, name)
                .many(Key::Directives, directives),
        ))
    }

    fn parse_implements_interfaces(&mut self) -> Result<Option<Vec<usize>>, SyntaxError> {
        if self.expect_optional_keyword("implements")? {
            return Ok(Some(self.delimited_many(Tk::Amp, Self::parse_named_type)?));
        }
        Ok(None)
    }

    /// Object and interface type definitions.
    fn parse_object_type_definition(&mut self, kind: Kind, keyword: &str) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        self.expect_keyword(keyword)?;
        let name = self.parse_name()?;
        let interfaces = self.parse_implements_interfaces()?;
        let directives = self.parse_directives(true)?;
        let fields = self.optional_many(Tk::BraceL, Self::parse_field_definition, Tk::BraceR)?;
        Ok(self.node(
            start,
            kind,
            "",
            false,
            Props::default()
                .opt(Key::Description, description)
                .one(Key::Name, name)
                .many(Key::Interfaces, interfaces)
                .many(Key::Directives, directives)
                .many(Key::Fields, fields),
        ))
    }

    fn parse_field_definition(&mut self) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        let name = self.parse_name()?;
        let arguments = self.optional_many(Tk::ParenL, Self::parse_input_value_def, Tk::ParenR)?;
        self.expect_token(Tk::Colon)?;
        let kind = self.parse_type_reference()?;
        let directives = self.parse_directives(true)?;
        Ok(self.node(
            start,
            Kind::FieldDefinition,
            "",
            false,
            Props::default()
                .opt(Key::Description, description)
                .one(Key::Name, name)
                .many(Key::Arguments, arguments)
                .one(Key::Type, kind)
                .many(Key::Directives, directives),
        ))
    }

    fn parse_input_value_def(&mut self) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        let name = self.parse_name()?;
        self.expect_token(Tk::Colon)?;
        let kind = self.parse_type_reference()?;
        let default = if self.expect_optional_token(Tk::Equals)? {
            Some(self.parse_value_literal(true)?)
        } else {
            None
        };
        let directives = self.parse_directives(true)?;
        Ok(self.node(
            start,
            Kind::InputValueDefinition,
            "",
            false,
            Props::default()
                .opt(Key::Description, description)
                .one(Key::Name, name)
                .one(Key::Type, kind)
                .opt(Key::DefaultValue, default)
                .many(Key::Directives, directives),
        ))
    }

    fn parse_union_member_types(&mut self) -> Result<Option<Vec<usize>>, SyntaxError> {
        if self.expect_optional_token(Tk::Equals)? {
            return Ok(Some(self.delimited_many(Tk::Pipe, Self::parse_named_type)?));
        }
        Ok(None)
    }

    fn parse_union_type_definition(&mut self) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        self.expect_keyword("union")?;
        let name = self.parse_name()?;
        let directives = self.parse_directives(true)?;
        let types = self.parse_union_member_types()?;
        Ok(self.node(
            start,
            Kind::UnionTypeDefinition,
            "",
            false,
            Props::default()
                .opt(Key::Description, description)
                .one(Key::Name, name)
                .many(Key::Directives, directives)
                .many(Key::Types, types),
        ))
    }

    fn parse_enum_values(&mut self) -> Result<Option<Vec<usize>>, SyntaxError> {
        self.optional_many(Tk::BraceL, Self::parse_enum_value_definition, Tk::BraceR)
    }

    fn parse_enum_type_definition(&mut self) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        self.expect_keyword("enum")?;
        let name = self.parse_name()?;
        let directives = self.parse_directives(true)?;
        let values = self.parse_enum_values()?;
        Ok(self.node(
            start,
            Kind::EnumTypeDefinition,
            "",
            false,
            Props::default()
                .opt(Key::Description, description)
                .one(Key::Name, name)
                .many(Key::Directives, directives)
                .many(Key::Values, values),
        ))
    }

    fn parse_enum_value_definition(&mut self) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        if matches!(
            self.token().value.as_deref(),
            Some("true" | "false" | "null")
        ) {
            let token = self.token().clone();
            return error(
                token.start,
                format!(
                    "{} is reserved and cannot be used for an enum value.",
                    token_desc(&token)
                ),
            );
        }
        let name = self.parse_name()?;
        let directives = self.parse_directives(true)?;
        Ok(self.node(
            start,
            Kind::EnumValueDefinition,
            "",
            false,
            Props::default()
                .opt(Key::Description, description)
                .one(Key::Name, name)
                .many(Key::Directives, directives),
        ))
    }

    fn parse_input_fields(&mut self) -> Result<Option<Vec<usize>>, SyntaxError> {
        self.optional_many(Tk::BraceL, Self::parse_input_value_def, Tk::BraceR)
    }

    fn parse_input_object_type_definition(&mut self) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        self.expect_keyword("input")?;
        let name = self.parse_name()?;
        let directives = self.parse_directives(true)?;
        let fields = self.parse_input_fields()?;
        Ok(self.node(
            start,
            Kind::InputObjectTypeDefinition,
            "",
            false,
            Props::default()
                .opt(Key::Description, description)
                .one(Key::Name, name)
                .many(Key::Directives, directives)
                .many(Key::Fields, fields),
        ))
    }

    fn parse_directive_definition(&mut self) -> Parsed {
        let start = self.token().start;
        let description = self.parse_description()?;
        self.expect_keyword("directive")?;
        self.expect_token(Tk::At)?;
        let name = self.parse_name()?;
        let arguments = self.optional_many(Tk::ParenL, Self::parse_input_value_def, Tk::ParenR)?;
        let directives = self.parse_directives(true)?;
        let repeatable = self.expect_optional_keyword("repeatable")?;
        self.expect_keyword("on")?;
        let locations = self.delimited_many(Tk::Pipe, Self::parse_directive_location)?;
        Ok(self.node(
            start,
            Kind::DirectiveDefinition,
            "",
            repeatable,
            Props::default()
                .opt(Key::Description, description)
                .one(Key::Name, name)
                .many(Key::Arguments, arguments)
                .many(Key::Directives, directives)
                .many(Key::Locations, Some(locations)),
        ))
    }

    fn parse_directive_location(&mut self) -> Parsed {
        let token = self.token().clone();
        let name = self.parse_name()?;
        if DIRECTIVE_LOCATIONS.contains(&self.nodes[name].text.as_str()) {
            return Ok(name);
        }
        self.unexpected(Some(&token))
    }

    fn parse_type_system_extension(&mut self) -> Parsed {
        let keyword = self.lexer.lookahead()?;
        if keyword.kind == Tk::Name {
            let start = self.token().start;
            let value = keyword.value.clone().unwrap_or_default();
            let known = [
                "schema",
                "scalar",
                "type",
                "interface",
                "union",
                "enum",
                "input",
                "directive",
            ];
            if known.contains(&value.as_str()) {
                self.expect_keyword("extend")?;
                self.expect_keyword(&value)?;
                return self.parse_extension_body(start, &value);
            }
        }
        self.unexpected(Some(&keyword))
    }

    fn parse_extension_body(&mut self, start: usize, keyword: &str) -> Parsed {
        let props;
        let kind;
        let empty;
        match keyword {
            "schema" => {
                let directives = self.parse_directives(true)?;
                let operation_types = self.optional_many(
                    Tk::BraceL,
                    Self::parse_operation_type_definition,
                    Tk::BraceR,
                )?;
                empty = directives.is_none() && operation_types.is_none();
                kind = Kind::SchemaExtension;
                props = Props::default()
                    .many(Key::Directives, directives)
                    .many(Key::OperationTypes, operation_types);
            }
            "scalar" => {
                let name = self.parse_name()?;
                let directives = self.parse_directives(true)?;
                empty = directives.is_none();
                kind = Kind::ScalarTypeExtension;
                props = Props::default()
                    .one(Key::Name, name)
                    .many(Key::Directives, directives);
            }
            "type" | "interface" => {
                let name = self.parse_name()?;
                let interfaces = self.parse_implements_interfaces()?;
                let directives = self.parse_directives(true)?;
                let fields =
                    self.optional_many(Tk::BraceL, Self::parse_field_definition, Tk::BraceR)?;
                empty = interfaces.is_none() && directives.is_none() && fields.is_none();
                kind = if keyword == "type" {
                    Kind::ObjectTypeExtension
                } else {
                    Kind::InterfaceTypeExtension
                };
                props = Props::default()
                    .one(Key::Name, name)
                    .many(Key::Interfaces, interfaces)
                    .many(Key::Directives, directives)
                    .many(Key::Fields, fields);
            }
            "union" => {
                let name = self.parse_name()?;
                let directives = self.parse_directives(true)?;
                let types = self.parse_union_member_types()?;
                empty = directives.is_none() && types.is_none();
                kind = Kind::UnionTypeExtension;
                props = Props::default()
                    .one(Key::Name, name)
                    .many(Key::Directives, directives)
                    .many(Key::Types, types);
            }
            "enum" => {
                let name = self.parse_name()?;
                let directives = self.parse_directives(true)?;
                let values = self.parse_enum_values()?;
                empty = directives.is_none() && values.is_none();
                kind = Kind::EnumTypeExtension;
                props = Props::default()
                    .one(Key::Name, name)
                    .many(Key::Directives, directives)
                    .many(Key::Values, values);
            }
            "input" => {
                let name = self.parse_name()?;
                let directives = self.parse_directives(true)?;
                let fields = self.parse_input_fields()?;
                empty = directives.is_none() && fields.is_none();
                kind = Kind::InputObjectTypeExtension;
                props = Props::default()
                    .one(Key::Name, name)
                    .many(Key::Directives, directives)
                    .many(Key::Fields, fields);
            }
            _ => {
                self.expect_token(Tk::At)?;
                let name = self.parse_name()?;
                let directives = self.parse_directives(true)?;
                empty = directives.is_none();
                kind = Kind::DirectiveExtension;
                props = Props::default()
                    .one(Key::Name, name)
                    .many(Key::Directives, directives);
            }
        }
        if empty {
            return self.unexpected(None);
        }
        Ok(self.node(start, kind, "", false, props))
    }
}

type Ast = (Vec<Node>, usize, Vec<Comment>);

fn parse(chars: &[char]) -> Result<Ast, SyntaxError> {
    let mut parser = Parser {
        lexer: Lexer::new(chars),
        nodes: Vec::new(),
    };
    let root = parser.parse_document()?;
    Ok((parser.nodes, root, parser.lexer.comments))
}

// ── Doc printer (Prettier) ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Doc {
    Text(String),
    Concat(Vec<Doc>),
    Group(Box<Doc>, bool),
    Indent(Box<Doc>),
    Line {
        hard: bool,
        soft: bool,
        literal: bool,
    },
    IfBreak(Box<Doc>, Box<Doc>),
    LineSuffix(Box<Doc>),
    BreakParent,
}

fn text(value: impl Into<String>) -> Doc {
    Doc::Text(value.into())
}

fn empty() -> Doc {
    Doc::Text(String::new())
}

fn concat(parts: Vec<Doc>) -> Doc {
    Doc::Concat(parts)
}

fn group(contents: Doc) -> Doc {
    Doc::Group(Box::new(contents), false)
}

fn indent(contents: Doc) -> Doc {
    Doc::Indent(Box::new(contents))
}

fn line() -> Doc {
    Doc::Line {
        hard: false,
        soft: false,
        literal: false,
    }
}

fn softline() -> Doc {
    Doc::Line {
        hard: false,
        soft: true,
        literal: false,
    }
}

fn hardline() -> Doc {
    concat(vec![
        Doc::Line {
            hard: true,
            soft: false,
            literal: false,
        },
        Doc::BreakParent,
    ])
}

fn if_break(broken: Doc, flat: Doc) -> Doc {
    Doc::IfBreak(Box::new(broken), Box::new(flat))
}

fn line_suffix(contents: Doc) -> Doc {
    Doc::LineSuffix(Box::new(contents))
}

fn join(separator: Doc, parts: Vec<Doc>) -> Doc {
    let mut out = Vec::new();
    for (index, part) in parts.into_iter().enumerate() {
        if index > 0 {
            out.push(separator.clone());
        }
        out.push(part);
    }
    concat(out)
}

/// Mark groups that contain a forced break as broken.
fn propagate_breaks(doc: &mut Doc) -> bool {
    match doc {
        Doc::BreakParent => true,
        Doc::Text(_) | Doc::Line { .. } => false,
        Doc::Concat(parts) => {
            let mut any = false;
            for part in parts {
                any |= propagate_breaks(part);
            }
            any
        }
        Doc::Group(contents, broken) => {
            if propagate_breaks(contents) {
                *broken = true;
            }
            *broken
        }
        Doc::Indent(contents) | Doc::LineSuffix(contents) => propagate_breaks(contents),
        Doc::IfBreak(broken, flat) => {
            let a = propagate_breaks(broken);
            let b = propagate_breaks(flat);
            a || b
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Break,
    Flat,
}

/// Prettier's `getStringWidth`: wide characters count 2, combining marks 0.
fn string_width(text: &str) -> usize {
    if text.is_ascii() {
        return text.len();
    }
    text.chars()
        .map(|c| {
            let code = c as u32;
            if code < 0x20
                || (0x7F..=0x9F).contains(&code)
                || (0x300..=0x36F).contains(&code)
                || (0x200B..=0x200F).contains(&code)
                || (0xFE00..=0xFE0F).contains(&code)
            {
                0
            } else if (0x1100..=0x115F).contains(&code)
                || (0x2E80..=0x303E).contains(&code)
                || (0x3041..=0x33FF).contains(&code)
                || (0x3400..=0x4DBF).contains(&code)
                || (0x4E00..=0x9FFF).contains(&code)
                || (0xA000..=0xA4CF).contains(&code)
                || (0xAC00..=0xD7A3).contains(&code)
                || (0xF900..=0xFAFF).contains(&code)
                || (0xFE30..=0xFE4F).contains(&code)
                || (0xFF00..=0xFF60).contains(&code)
                || (0xFFE0..=0xFFE6).contains(&code)
                || (0x1F300..=0x1F64F).contains(&code)
                || (0x1F900..=0x1F9FF).contains(&code)
                || (0x20000..=0x3FFFD).contains(&code)
            {
                2
            } else {
                1
            }
        })
        .sum()
}

/// Trim trailing spaces and tabs from the output; returns the count.
fn trim_output(out: &mut String) -> usize {
    let trimmed = out.trim_end_matches([' ', '\t']).len();
    let count = out.len() - trimmed;
    out.truncate(trimmed);
    count
}

const PRINT_WIDTH: isize = 60;

fn fits(
    next: (Mode, &Doc),
    rest: &[(usize, Mode, &Doc)],
    width: isize,
    mut has_line_suffix: bool,
) -> bool {
    let mut width = width;
    let mut rest_index = rest.len();
    let mut cmds: Vec<(Mode, &Doc)> = vec![next];
    while width >= 0 {
        let Some((mode, doc)) = cmds.pop() else {
            if rest_index == 0 {
                return true;
            }
            rest_index -= 1;
            let (_, mode, doc) = rest[rest_index];
            cmds.push((mode, doc));
            continue;
        };
        match doc {
            Doc::Text(value) => width -= string_width(value) as isize,
            Doc::Concat(parts) => {
                for part in parts.iter().rev() {
                    cmds.push((mode, part));
                }
            }
            Doc::Indent(contents) => cmds.push((mode, contents)),
            Doc::Group(contents, broken) => {
                cmds.push((if *broken { Mode::Break } else { mode }, contents));
            }
            Doc::IfBreak(broken, flat) => {
                cmds.push((mode, if mode == Mode::Break { broken } else { flat }));
            }
            Doc::Line { hard, soft, .. } => {
                if mode == Mode::Break || *hard {
                    return true;
                }
                if !*soft {
                    width -= 1;
                }
            }
            Doc::LineSuffix(_) => has_line_suffix = true,
            Doc::BreakParent => {}
        }
    }
    let _ = has_line_suffix;
    false
}

fn print_doc(mut doc: Doc) -> String {
    propagate_breaks(&mut doc);
    let mut out = String::new();
    let mut pos: isize = 0;
    let mut should_remeasure = false;
    let mut cmds: Vec<(usize, Mode, &Doc)> = vec![(0, Mode::Break, &doc)];
    let mut line_suffixes: Vec<(usize, Mode, &Doc)> = Vec::new();
    while let Some((ind, mode, doc)) = cmds.pop() {
        match doc {
            Doc::Text(value) => {
                out.push_str(value);
                if !cmds.is_empty() {
                    pos += string_width(value) as isize;
                }
            }
            Doc::Concat(parts) => {
                for part in parts.iter().rev() {
                    cmds.push((ind, mode, part));
                }
            }
            Doc::Indent(contents) => cmds.push((ind + 2, mode, contents)),
            Doc::Group(contents, broken) => {
                if mode == Mode::Flat && !should_remeasure {
                    cmds.push((
                        ind,
                        if *broken { Mode::Break } else { Mode::Flat },
                        contents,
                    ));
                } else {
                    should_remeasure = false;
                    let next = (Mode::Flat, &**contents);
                    let remaining = PRINT_WIDTH - pos;
                    if !*broken && fits(next, &cmds, remaining, !line_suffixes.is_empty()) {
                        cmds.push((ind, Mode::Flat, contents));
                    } else {
                        cmds.push((ind, Mode::Break, contents));
                    }
                }
            }
            Doc::IfBreak(broken, flat) => {
                cmds.push((ind, mode, if mode == Mode::Break { broken } else { flat }));
            }
            Doc::LineSuffix(contents) => line_suffixes.push((ind, mode, contents)),
            Doc::BreakParent => {}
            Doc::Line {
                hard,
                soft,
                literal,
            } => {
                if mode == Mode::Flat && !*hard {
                    if !*soft {
                        out.push(' ');
                        pos += 1;
                    }
                } else {
                    if mode == Mode::Flat {
                        should_remeasure = true;
                    }
                    if !line_suffixes.is_empty() {
                        cmds.push((ind, mode, doc));
                        while let Some(suffix) = line_suffixes.pop() {
                            cmds.push(suffix);
                        }
                    } else if *literal {
                        out.push('\n');
                        pos = 0;
                    } else {
                        trim_output(&mut out);
                        out.push('\n');
                        out.push_str(&" ".repeat(ind));
                        pos = ind as isize;
                    }
                }
            }
        }
        if cmds.is_empty() && !line_suffixes.is_empty() {
            while let Some(suffix) = line_suffixes.pop() {
                cmds.push(suffix);
            }
        }
    }
    out
}

// ── Comments and the GraphQL printer (Prettier) ────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    Leading,
    Trailing,
    Dangling,
}

struct AttachedComment {
    start: usize,
    end: usize,
    value: String,
    placement: Placement,
    printed: bool,
}

struct Printer<'a> {
    text: &'a [char],
    nodes: Vec<Node>,
    comments: Vec<AttachedComment>,
    /// Comment indexes attached to each node, in source order.
    attached: Vec<Vec<usize>>,
}

fn is_newline(c: Option<char>) -> bool {
    matches!(c, Some('\n' | '\r' | '\u{2028}' | '\u{2029}'))
}

impl<'a> Printer<'a> {
    fn new(text: &'a [char], nodes: Vec<Node>, comments: Vec<Comment>) -> Self {
        let attached = vec![Vec::new(); nodes.len()];
        Printer {
            text,
            nodes,
            comments: comments
                .into_iter()
                .map(|comment| AttachedComment {
                    start: comment.start,
                    end: comment.end,
                    value: comment.value,
                    placement: Placement::Dangling,
                    printed: false,
                })
                .collect(),
            attached,
        }
    }

    fn at(&self, index: isize) -> Option<char> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.text.get(i).copied())
    }

    /// Skip characters in `set`; `None` when the index runs off both ends.
    fn skip(&self, set: &[char], index: Option<isize>, backwards: bool) -> Option<isize> {
        let mut at = index?;
        let length = self.text.len() as isize;
        while at >= 0 && at < length {
            let c = self.text[at as usize];
            if !set.contains(&c) {
                return Some(at);
            }
            at += if backwards { -1 } else { 1 };
        }
        if at == -1 || at == length {
            Some(at)
        } else {
            None
        }
    }

    fn skip_spaces(&self, index: Option<isize>, backwards: bool) -> Option<isize> {
        self.skip(&[' ', '\t'], index, backwards)
    }

    fn skip_newline(&self, index: Option<isize>, backwards: bool) -> Option<isize> {
        let at = index?;
        let c = self.at(at);
        if backwards {
            if self.at(at - 1) == Some('\r') && c == Some('\n') {
                return Some(at - 2);
            }
            if is_newline(c) {
                return Some(at - 1);
            }
        } else {
            if c == Some('\r') && self.at(at + 1) == Some('\n') {
                return Some(at + 2);
            }
            if is_newline(c) {
                return Some(at + 1);
            }
        }
        Some(at)
    }

    fn has_newline(&self, index: isize, backwards: bool) -> bool {
        let start = self.skip_spaces(Some(if backwards { index - 1 } else { index }), backwards);
        start != self.skip_newline(start, backwards)
    }

    fn is_next_line_empty(&self, index: usize) -> bool {
        let mut old = None;
        let mut at = Some(index as isize);
        while at != old {
            old = at;
            at = self.skip(&[',', ';', ' ', '\t'], at, false);
            at = self.skip_block_comment(at);
            at = self.skip_spaces(at, false);
        }
        at = self.skip_line_comment(at);
        at = self.skip_newline(at, false);
        at.is_some_and(|at| self.has_newline(at, false))
    }

    /// Prettier's `skipInlineComment`: a `/* */` comment.
    fn skip_block_comment(&self, index: Option<isize>) -> Option<isize> {
        let at = index?;
        if self.at(at) == Some('/') && self.at(at + 1) == Some('*') {
            let mut i = at + 2;
            while (i as usize) < self.text.len() {
                if self.at(i) == Some('*') && self.at(i + 1) == Some('/') {
                    return Some(i + 2);
                }
                i += 1;
            }
        }
        Some(at)
    }

    /// Prettier's `skipTrailingComment`: a `//` comment to the line end.
    fn skip_line_comment(&self, index: Option<isize>) -> Option<isize> {
        let at = index?;
        if self.at(at) == Some('/') && self.at(at + 1) == Some('/') {
            let length = self.text.len() as isize;
            let mut i = at;
            while i < length && !matches!(self.at(i), Some('\n' | '\r')) {
                i += 1;
            }
            return Some(i);
        }
        Some(at)
    }

    fn is_previous_line_empty(&self, index: usize) -> bool {
        let at = self.skip_spaces(Some(index as isize - 1), true);
        let at = self.skip_newline(at, true);
        let at = self.skip_spaces(at, true);
        at != self.skip_newline(at, true)
    }

    fn children(&self, id: usize) -> Vec<usize> {
        let mut children: Vec<usize> = self.nodes[id]
            .props
            .iter()
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect();
        children.sort_by_key(|child| (self.nodes[*child].start, self.nodes[*child].end));
        children
    }

    fn attach_comments(&mut self, root: usize) {
        for index in 0..self.comments.len() {
            let (start, end) = (self.comments[index].start, self.comments[index].end);
            let mut enclosing = root;
            let (preceding, following) = loop {
                let children = self.children(enclosing);
                let mut preceding = None;
                let mut following = None;
                let mut inner = None;
                let (mut left, mut right) = (0, children.len());
                while left < right {
                    let middle = (left + right) / 2;
                    let child = &self.nodes[children[middle]];
                    if child.start <= start && end <= child.end {
                        inner = Some(children[middle]);
                        break;
                    }
                    if child.end <= start {
                        preceding = Some(children[middle]);
                        left = middle + 1;
                        continue;
                    }
                    if end <= child.start {
                        following = Some(children[middle]);
                        right = middle;
                        continue;
                    }
                    break;
                }
                match inner {
                    Some(child) => enclosing = child,
                    None => break (preceding, following),
                }
            };
            // An own-line comment leads the next node. End-of-line and other
            // comments trail the previous node.
            let (target, placement) = if self.has_newline(start as isize, true) {
                match (following, preceding) {
                    (Some(node), _) => (node, Placement::Leading),
                    (None, Some(node)) => (node, Placement::Trailing),
                    _ => (enclosing, Placement::Dangling),
                }
            } else {
                match (preceding, following) {
                    (Some(node), _) => (node, Placement::Trailing),
                    (None, Some(node)) => (node, Placement::Leading),
                    _ => (enclosing, Placement::Dangling),
                }
            };
            self.comments[index].placement = placement;
            self.attached[target].push(index);
        }
    }

    fn print_comment(&mut self, index: usize) -> Doc {
        self.comments[index].printed = true;
        text(format!("#{}", self.comments[index].value.trim_end()))
    }

    fn main_print(&mut self, id: usize) -> Doc {
        let ignored = self.attached[id]
            .iter()
            .any(|index| self.comments[*index].value.trim() == "prettier-ignore");
        let doc = if ignored {
            let (start, end) = (self.nodes[id].start, self.nodes[id].end);
            for comment in &mut self.comments {
                if comment.start >= start && comment.end <= end {
                    comment.printed = true;
                }
            }
            text(self.text[start..end].iter().collect::<String>())
        } else {
            self.print(id)
        };
        let mut leading = Vec::new();
        let mut trailing = Vec::new();
        let mut previous_has_suffix = false;
        for index in self.attached[id].clone() {
            let (start, end) = (self.comments[index].start, self.comments[index].end);
            match self.comments[index].placement {
                Placement::Leading => {
                    let printed = self.print_comment(index);
                    leading.push(printed);
                    leading.push(hardline());
                    let at = self.skip_newline(self.skip_spaces(Some(end as isize), false), false);
                    if at.is_some_and(|at| self.has_newline(at, false)) {
                        leading.push(hardline());
                    }
                }
                Placement::Trailing => {
                    let printed = self.print_comment(index);
                    if previous_has_suffix || self.has_newline(start as isize, true) {
                        let blank = if self.is_previous_line_empty(start) {
                            hardline()
                        } else {
                            empty()
                        };
                        trailing.push(line_suffix(concat(vec![hardline(), blank, printed])));
                    } else {
                        trailing.push(concat(vec![
                            line_suffix(concat(vec![text(" "), printed])),
                            Doc::BreakParent,
                        ]));
                    }
                    previous_has_suffix = true;
                }
                Placement::Dangling => {}
            }
        }
        if leading.is_empty() && trailing.is_empty() {
            return doc;
        }
        leading.push(doc);
        leading.extend(trailing);
        concat(leading)
    }

    fn print_dangling(&mut self, id: usize) -> Doc {
        let dangling: Vec<usize> = self.attached[id]
            .iter()
            .copied()
            .filter(|index| self.comments[*index].placement == Placement::Dangling)
            .collect();
        if dangling.is_empty() {
            return empty();
        }
        let parts = dangling
            .into_iter()
            .map(|index| self.print_comment(index))
            .collect();
        indent(concat(vec![hardline(), join(hardline(), parts)]))
    }

    fn node(&self, id: usize) -> &Node {
        &self.nodes[id]
    }

    fn print_key(&mut self, id: usize, key: Key) -> Doc {
        match self.node(id).one(key) {
            Some(child) => self.main_print(child),
            None => empty(),
        }
    }

    fn print_all(&mut self, id: usize, key: Key) -> Vec<Doc> {
        let children = self.node(id).list(key).to_vec();
        children
            .into_iter()
            .map(|child| self.main_print(child))
            .collect()
    }

    /// Children, keeping one blank line where the source had one.
    fn print_sequence(&mut self, id: usize, key: Key) -> Vec<Doc> {
        let children = self.node(id).list(key).to_vec();
        let count = children.len();
        children
            .into_iter()
            .enumerate()
            .map(|(index, child)| {
                let printed = self.main_print(child);
                if index + 1 < count && self.is_next_line_empty(self.nodes[child].end) {
                    concat(vec![printed, hardline()])
                } else {
                    printed
                }
            })
            .collect()
    }

    fn has(&self, id: usize, key: Key) -> bool {
        !self.node(id).list(key).is_empty()
    }

    fn parenthesized(&mut self, parts: Vec<Doc>) -> Doc {
        group(concat(vec![
            text("("),
            indent(concat(vec![
                softline(),
                join(
                    concat(vec![if_break(empty(), text(", ")), softline()]),
                    parts,
                ),
            ])),
            softline(),
            text(")"),
        ]))
    }

    fn print_arguments(&mut self, id: usize) -> Doc {
        if !self.has(id, Key::Arguments) {
            return empty();
        }
        let parts = self.print_sequence(id, Key::Arguments);
        self.parenthesized(parts)
    }

    fn print_description(&mut self, id: usize) -> Doc {
        let Some(description) = self.node(id).one(Key::Description) else {
            return empty();
        };
        let printed = self.main_print(description);
        let inline =
            self.node(id).kind == Kind::InputValueDefinition && !self.nodes[description].flag;
        concat(vec![printed, if inline { line() } else { hardline() }])
    }

    fn print_directives(&mut self, id: usize) -> Doc {
        if !self.has(id, Key::Directives) {
            return empty();
        }
        let printed = join(line(), self.print_all(id, Key::Directives));
        if matches!(
            self.node(id).kind,
            Kind::FragmentDefinition | Kind::OperationDefinition
        ) {
            return group(concat(vec![line(), printed]));
        }
        concat(vec![
            text(" "),
            group(indent(concat(vec![softline(), printed]))),
        ])
    }

    fn print_variable_definitions(&mut self, id: usize) -> Doc {
        if !self.has(id, Key::VariableDefinitions) {
            return empty();
        }
        let parts = self.print_all(id, Key::VariableDefinitions);
        self.parenthesized(parts)
    }

    fn block(&mut self, id: usize, key: Key) -> Doc {
        let parts = self.print_sequence(id, key);
        concat(vec![
            text(" {"),
            indent(concat(vec![hardline(), join(hardline(), parts)])),
            hardline(),
            text("}"),
        ])
    }

    fn print(&mut self, id: usize) -> Doc {
        let node = self.node(id).clone();
        match node.kind {
            Kind::Document => {
                let parts = self.print_sequence(id, Key::Definitions);
                concat(vec![join(hardline(), parts), hardline()])
            }
            Kind::OperationDefinition => {
                let has_operation = self.text.get(node.start) != Some(&'{');
                let has_name = node.one(Key::Name).is_some();
                let description = self.print_description(id);
                let name = if has_operation && has_name {
                    concat(vec![text(" "), self.print_key(id, Key::Name)])
                } else {
                    empty()
                };
                let space = if has_operation && !has_name && self.has(id, Key::VariableDefinitions)
                {
                    text(" ")
                } else {
                    empty()
                };
                let variables = self.print_variable_definitions(id);
                let directives = self.print_directives(id);
                let gap = if !has_operation && !has_name {
                    empty()
                } else {
                    text(" ")
                };
                let selection_set = self.print_key(id, Key::SelectionSet);
                concat(vec![
                    description,
                    if has_operation {
                        text(&node.text)
                    } else {
                        empty()
                    },
                    name,
                    space,
                    variables,
                    directives,
                    gap,
                    selection_set,
                ])
            }
            Kind::FragmentDefinition => concat(vec![
                self.print_description(id),
                text("fragment "),
                self.print_key(id, Key::Name),
                self.print_variable_definitions(id),
                text(" on "),
                self.print_key(id, Key::TypeCondition),
                self.print_directives(id),
                text(" "),
                self.print_key(id, Key::SelectionSet),
            ]),
            Kind::SelectionSet => {
                let parts = self.print_sequence(id, Key::Selections);
                concat(vec![
                    text("{"),
                    indent(concat(vec![hardline(), join(hardline(), parts)])),
                    hardline(),
                    text("}"),
                ])
            }
            Kind::Field => {
                let alias = match node.one(Key::Alias) {
                    Some(alias) => concat(vec![self.main_print(alias), text(": ")]),
                    None => empty(),
                };
                let name = self.print_key(id, Key::Name);
                let arguments = self.print_arguments(id);
                let directives = self.print_directives(id);
                let space = if node.one(Key::SelectionSet).is_some() {
                    text(" ")
                } else {
                    empty()
                };
                let selection_set = self.print_key(id, Key::SelectionSet);
                group(concat(vec![
                    alias,
                    name,
                    arguments,
                    directives,
                    space,
                    selection_set,
                ]))
            }
            Kind::Name => text(&node.text),
            Kind::StringValue => {
                if node.flag {
                    let escaped = node.text.replace("\"\"\"", "\\\"\"\"");
                    let mut lines: Vec<String> = escaped.split('\n').map(String::from).collect();
                    if lines.len() == 1 {
                        lines[0] = crate::curl_import::js_trim(&lines[0]).to_string();
                    }
                    if lines.iter().all(String::is_empty) {
                        lines.clear();
                    }
                    let mut parts = vec![text("\"\"\"")];
                    parts.extend(lines.into_iter().map(text));
                    parts.push(text("\"\"\""));
                    join(hardline(), parts)
                } else {
                    let mut out = String::from("\"");
                    for c in node.text.chars() {
                        match c {
                            '"' | '\\' => {
                                out.push('\\');
                                out.push(c);
                            }
                            '\n' => out.push_str("\\n"),
                            _ => out.push(c),
                        }
                    }
                    out.push('"');
                    text(out)
                }
            }
            Kind::IntValue | Kind::FloatValue | Kind::EnumValue => text(&node.text),
            Kind::BooleanValue => text(if node.flag { "true" } else { "false" }),
            Kind::NullValue => text("null"),
            Kind::Variable => concat(vec![text("$"), self.print_key(id, Key::Name)]),
            Kind::ListValue => {
                let dangling = self.print_dangling(id);
                let values = if self.has(id, Key::Values) {
                    let parts = self.print_all(id, Key::Values);
                    indent(concat(vec![
                        softline(),
                        join(
                            concat(vec![if_break(empty(), text(", ")), softline()]),
                            parts,
                        ),
                    ]))
                } else {
                    empty()
                };
                group(concat(vec![
                    text("["),
                    dangling,
                    values,
                    softline(),
                    text("]"),
                ]))
            }
            Kind::ObjectValue => {
                let has_fields = self.has(id, Key::Fields);
                let space = if has_fields { " " } else { "" };
                let dangling = self.print_dangling(id);
                let fields = if has_fields {
                    let parts = self.print_all(id, Key::Fields);
                    indent(concat(vec![
                        softline(),
                        join(
                            concat(vec![if_break(empty(), text(", ")), softline()]),
                            parts,
                        ),
                    ]))
                } else {
                    empty()
                };
                group(concat(vec![
                    text("{"),
                    text(space),
                    dangling,
                    fields,
                    softline(),
                    if_break(empty(), text(space)),
                    text("}"),
                ]))
            }
            Kind::ObjectField | Kind::Argument | Kind::FragmentArgument => concat(vec![
                self.print_key(id, Key::Name),
                text(": "),
                self.print_key(id, Key::Value),
            ]),
            Kind::Directive => concat(vec![
                text("@"),
                self.print_key(id, Key::Name),
                self.print_arguments(id),
            ]),
            Kind::NamedType => self.print_key(id, Key::Name),
            Kind::VariableDefinition => {
                let description = self.print_description(id);
                let variable = self.print_key(id, Key::Variable);
                let kind = self.print_key(id, Key::Type);
                let default = match node.one(Key::DefaultValue) {
                    Some(value) => concat(vec![text(" = "), self.main_print(value)]),
                    None => empty(),
                };
                let directives = self.print_directives(id);
                concat(vec![
                    description,
                    variable,
                    text(": "),
                    kind,
                    default,
                    directives,
                ])
            }
            Kind::ObjectTypeExtension
            | Kind::ObjectTypeDefinition
            | Kind::InputObjectTypeExtension
            | Kind::InputObjectTypeDefinition
            | Kind::InterfaceTypeExtension
            | Kind::InterfaceTypeDefinition => {
                let mut parts = Vec::new();
                let definition = matches!(
                    node.kind,
                    Kind::ObjectTypeDefinition
                        | Kind::InputObjectTypeDefinition
                        | Kind::InterfaceTypeDefinition
                );
                parts.push(if definition {
                    self.print_description(id)
                } else {
                    text("extend ")
                });
                let input = matches!(
                    node.kind,
                    Kind::InputObjectTypeExtension | Kind::InputObjectTypeDefinition
                );
                parts.push(text(match node.kind {
                    Kind::ObjectTypeExtension | Kind::ObjectTypeDefinition => "type",
                    _ if input => "input",
                    _ => "interface",
                }));
                parts.push(text(" "));
                parts.push(self.print_key(id, Key::Name));
                if !input && self.has(id, Key::Interfaces) {
                    let interfaces = self.print_all(id, Key::Interfaces);
                    parts.push(text(" implements "));
                    parts.push(indent(group(join(
                        concat(vec![text(" &"), line()]),
                        interfaces,
                    ))));
                }
                parts.push(self.print_directives(id));
                if self.has(id, Key::Fields) {
                    parts.push(self.block(id, Key::Fields));
                }
                concat(parts)
            }
            Kind::FieldDefinition => concat(vec![
                self.print_description(id),
                self.print_key(id, Key::Name),
                self.print_arguments(id),
                text(": "),
                self.print_key(id, Key::Type),
                self.print_directives(id),
            ]),
            Kind::DirectiveDefinition => {
                let mut parts = vec![
                    self.print_description(id),
                    text("directive "),
                    text("@"),
                    self.print_key(id, Key::Name),
                    self.print_arguments(id),
                    self.print_directives(id),
                    text(if node.flag { " repeatable" } else { "" }),
                    text(" on "),
                ];
                let locations = self.print_all(id, Key::Locations);
                parts.push(join(text(" | "), locations));
                concat(parts)
            }
            Kind::DirectiveExtension => concat(vec![
                text("extend directive @"),
                self.print_key(id, Key::Name),
                self.print_directives(id),
            ]),
            Kind::EnumTypeExtension | Kind::EnumTypeDefinition => {
                let description = self.print_description(id);
                let extend = text(if node.kind == Kind::EnumTypeExtension {
                    "extend "
                } else {
                    ""
                });
                let name = self.print_key(id, Key::Name);
                let directives = self.print_directives(id);
                let values = if self.has(id, Key::Values) {
                    self.block(id, Key::Values)
                } else {
                    empty()
                };
                concat(vec![
                    description,
                    extend,
                    text("enum "),
                    name,
                    directives,
                    values,
                ])
            }
            Kind::EnumValueDefinition => concat(vec![
                self.print_description(id),
                self.print_key(id, Key::Name),
                self.print_directives(id),
            ]),
            Kind::InputValueDefinition => {
                let description = self.print_description(id);
                let name = self.print_key(id, Key::Name);
                let kind = self.print_key(id, Key::Type);
                let default = match node.one(Key::DefaultValue) {
                    Some(value) => concat(vec![text(" = "), self.main_print(value)]),
                    None => empty(),
                };
                let directives = self.print_directives(id);
                concat(vec![
                    description,
                    name,
                    text(": "),
                    kind,
                    default,
                    directives,
                ])
            }
            Kind::SchemaExtension => {
                let directives = self.print_directives(id);
                let operations = if self.has(id, Key::OperationTypes) {
                    self.block(id, Key::OperationTypes)
                } else {
                    empty()
                };
                concat(vec![text("extend schema"), directives, operations])
            }
            Kind::SchemaDefinition => {
                let description = self.print_description(id);
                let directives = self.print_directives(id);
                let operations = if self.has(id, Key::OperationTypes) {
                    let parts = self.print_sequence(id, Key::OperationTypes);
                    indent(concat(vec![hardline(), join(hardline(), parts)]))
                } else {
                    empty()
                };
                concat(vec![
                    description,
                    text("schema"),
                    directives,
                    text(" {"),
                    operations,
                    hardline(),
                    text("}"),
                ])
            }
            Kind::OperationTypeDefinition => concat(vec![
                text(&node.text),
                text(": "),
                self.print_key(id, Key::Type),
            ]),
            Kind::FragmentSpread => concat(vec![
                text("..."),
                self.print_key(id, Key::Name),
                self.print_arguments(id),
                self.print_directives(id),
            ]),
            Kind::InlineFragment => {
                let condition = match node.one(Key::TypeCondition) {
                    Some(condition) => concat(vec![text(" on "), self.main_print(condition)]),
                    None => empty(),
                };
                concat(vec![
                    text("..."),
                    condition,
                    self.print_directives(id),
                    text(" "),
                    self.print_key(id, Key::SelectionSet),
                ])
            }
            Kind::UnionTypeExtension | Kind::UnionTypeDefinition => {
                let description = self.print_description(id);
                let extend = text(if node.kind == Kind::UnionTypeExtension {
                    "extend "
                } else {
                    ""
                });
                let name = self.print_key(id, Key::Name);
                let directives = self.print_directives(id);
                let types = if self.has(id, Key::Types) {
                    let types = self.print_all(id, Key::Types);
                    concat(vec![
                        text(" ="),
                        if_break(empty(), text(" ")),
                        indent(concat(vec![
                            if_break(concat(vec![line(), text("| ")]), empty()),
                            join(concat(vec![line(), text("| ")]), types),
                        ])),
                    ])
                } else {
                    empty()
                };
                group(concat(vec![
                    description,
                    group(concat(vec![
                        extend,
                        text("union "),
                        name,
                        directives,
                        types,
                    ])),
                ]))
            }
            Kind::ScalarTypeExtension | Kind::ScalarTypeDefinition => concat(vec![
                self.print_description(id),
                text(if node.kind == Kind::ScalarTypeExtension {
                    "extend "
                } else {
                    ""
                }),
                text("scalar "),
                self.print_key(id, Key::Name),
                self.print_directives(id),
            ]),
            Kind::NonNullType => concat(vec![self.print_key(id, Key::Type), text("!")]),
            Kind::ListType => concat(vec![text("["), self.print_key(id, Key::Type), text("]")]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_queries_and_keeps_comments() {
        assert_eq!(
            format_graphql("# fetch\nquery Q($id:ID!){node(id:$id){id ...on User{name}}}").unwrap(),
            [
                "# fetch",
                "query Q($id: ID!) {",
                "  node(id: $id) {",
                "    id",
                "    ... on User {",
                "      name",
                "    }",
                "  }",
                "}",
            ]
            .join("\n")
        );
    }

    #[test]
    fn rejects_invalid_graphql() {
        assert!(format_graphql("query {").is_err());
    }

    #[test]
    fn reports_the_syntax_error_location() {
        let text = "query {";
        let error = format_graphql(text).unwrap_err();
        assert_eq!(
            error.message,
            "Syntax Error: Expected Name, found <EOF>. (1:8)"
        );
        let location = graphql_error_location(text, &error).unwrap();
        assert_eq!((location.line, location.column), (1, 8));
        assert_eq!(location.reason, "Expected Name, found <EOF>.");
    }
}
