//! Port of `src/lib/jq.ts`.
//!
//! The TS runs jq 1.8 (jq-wasm). This runs the same programs with jaq and
//! writes output as jq does: two-space indent, or one line with `-c`; raw
//! strings with `-r`. Errors read like jq's stderr.

use std::fmt::Write as _;

use jaq_core::load::{self, Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data, unwrap_valr};
use jaq_json::{Num, Val};

/// Run `query` over each JSON value in `input`. `flags` may hold "-c"
/// (compact) and "-r" (raw strings). Each output ends a line; trailing
/// space is trimmed.
pub fn run_jq(input: &str, query: &str, flags: &[&str]) -> Result<String, String> {
    let compact = flags.contains(&"-c");
    let raw = flags.contains(&"-r");

    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
    let arena = Arena::default();
    let program = File {
        code: query,
        path: (),
    };
    let modules = Loader::new(defs)
        .load(&arena, program)
        .map_err(|errors| compile_error(load_messages(errors)))?;
    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errors| {
            compile_error(
                errors
                    .into_iter()
                    .flat_map(|(_, list)| list)
                    .map(|(name, undefined)| match undefined {
                        jaq_core::compile::Undefined::Filter(arity) => {
                            format!("{name}/{arity} is not defined")
                        }
                        other => format!("{} {name} is not defined", other.as_str()),
                    })
                    .collect(),
            )
        })?;

    let mut out = String::new();
    for (index, value) in jaq_json::read::parse_many(input.as_bytes()).enumerate() {
        let value = value.map_err(|error| {
            format!("jq: error (at <stdin>:{index}): Cannot parse input: {error}")
        })?;
        let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
        for output in filter.id.run((ctx, value)).map(unwrap_valr) {
            match output {
                Ok(value) => {
                    write_output(&mut out, &value, compact, raw);
                    out.push('\n');
                }
                Err(error) => {
                    let line = input.matches('\n').count();
                    return Err(format!("jq: error (at <stdin>:{line}): {error}"));
                }
            }
        }
    }
    Ok(out.trim_end().to_string())
}

fn compile_error(messages: Vec<String>) -> String {
    let count = messages.len().max(1);
    let plural = if count == 1 { "" } else { "s" };
    let mut text: String = messages
        .iter()
        .map(|message| format!("jq: error: {message}\n"))
        .collect();
    let _ = write!(text, "jq: {count} compile error{plural}");
    text
}

fn found(text: &str) -> String {
    let token: String = text.lines().next().unwrap_or("").chars().take(24).collect();
    if token.is_empty() {
        "end of file".into()
    } else {
        format!("'{token}'")
    }
}

fn load_messages(errors: load::Errors<&str, ()>) -> Vec<String> {
    let mut messages = Vec::new();
    for (_, error) in errors {
        match error {
            load::Error::Io(list) => messages.extend(
                list.into_iter()
                    .map(|(path, error)| format!("{path}: {error}")),
            ),
            load::Error::Lex(list) => messages.extend(list.into_iter().map(|(expect, rest)| {
                format!(
                    "syntax error, expected {}, found {}",
                    expect.as_str(),
                    found(rest)
                )
            })),
            load::Error::Parse(list) => messages.extend(list.into_iter().map(|(expect, rest)| {
                format!(
                    "syntax error, expected {}, found {}",
                    expect.as_str(),
                    found(rest)
                )
            })),
        }
    }
    messages
}

fn write_output(out: &mut String, value: &Val, compact: bool, raw: bool) {
    if raw && let Val::TStr(bytes) = value {
        out.push_str(&String::from_utf8_lossy(bytes));
        return;
    }
    write_value(out, value, if compact { None } else { Some("  ") }, 0);
}

fn write_value(out: &mut String, value: &Val, indent: Option<&str>, level: usize) {
    let newline = |out: &mut String, level: usize| {
        if let Some(indent) = indent {
            out.push('\n');
            out.push_str(&indent.repeat(level));
        }
    };
    match value {
        Val::Null => out.push_str("null"),
        Val::Bool(true) => out.push_str("true"),
        Val::Bool(false) => out.push_str("false"),
        Val::Num(number) => write_number(out, number),
        Val::TStr(bytes) | Val::BStr(bytes) => write_string(out, &String::from_utf8_lossy(bytes)),
        Val::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                newline(out, level + 1);
                write_value(out, item, indent, level + 1);
            }
            newline(out, level);
            out.push(']');
        }
        Val::Obj(entries) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (index, (key, item)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                newline(out, level + 1);
                match key {
                    Val::TStr(bytes) | Val::BStr(bytes) => {
                        write_string(out, &String::from_utf8_lossy(bytes))
                    }
                    other => write_string(out, &other.to_string()),
                }
                out.push_str(if indent.is_some() { ": " } else { ":" });
                write_value(out, item, indent, level + 1);
            }
            newline(out, level);
            out.push('}');
        }
    }
}

/// jq writes computed numbers with the shortest round-trip digits, integers
/// without a fraction, and NaN as null. Number literals keep their text.
fn write_number(out: &mut String, number: &Num) {
    match number {
        Num::Float(value) if value.is_nan() => out.push_str("null"),
        Num::Float(value) if value.is_infinite() => {
            let max = if *value > 0.0 { f64::MAX } else { f64::MIN };
            write_float(out, max);
        }
        Num::Float(value) => write_float(out, *value),
        other => {
            let _ = write!(out, "{other}");
        }
    }
}

fn write_float(out: &mut String, value: f64) {
    if value == 0.0 {
        out.push_str(if value.is_sign_negative() { "-0" } else { "0" });
        return;
    }
    let exp = format!("{value:e}");
    let (mantissa, exponent) = exp.split_once('e').unwrap_or((&exp, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exponent + 1;
    out.push_str(sign);
    // As jq's dtoa formatting: exponent notation for small values and for
    // values with many trailing zeros.
    if !(n <= -4 || n > k + 15) {
        if k <= n {
            out.push_str(&digits);
            out.push_str(&"0".repeat((n - k) as usize));
        } else if n > 0 {
            out.push_str(&digits[..n as usize]);
            out.push('.');
            out.push_str(&digits[n as usize..]);
        } else {
            out.push_str("0.");
            out.push_str(&"0".repeat((-n) as usize));
            out.push_str(&digits);
        }
    } else {
        out.push_str(&digits[..1]);
        if k > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let _ = write!(
            out,
            "e{}{:02}",
            if exponent < 0 { '-' } else { '+' },
            exponent.abs()
        );
    }
}

/// jq escapes quotes, backslashes, control characters, and DEL.
fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{"id":7,"token":"abc","tags":["a","b"],"none":null,"big":12345678901234567890,"dec":0.10000000000000000001}"#;

    #[test]
    fn prints_pretty_output_by_default() {
        assert_eq!(
            run_jq(BODY, "{id, tags}", &[]).unwrap(),
            "{\n  \"id\": 7,\n  \"tags\": [\n    \"a\",\n    \"b\"\n  ]\n}"
        );
    }

    #[test]
    fn prints_compact_raw_output() {
        assert_eq!(run_jq(BODY, ".token", &["-c", "-r"]).unwrap(), "abc");
        assert_eq!(
            run_jq(BODY, ".tags", &["-c", "-r"]).unwrap(),
            r#"["a","b"]"#
        );
        assert_eq!(run_jq(BODY, ".tags[]", &["-c"]).unwrap(), "\"a\"\n\"b\"");
        assert_eq!(run_jq(BODY, ".none", &["-c", "-r"]).unwrap(), "null");
        assert_eq!(run_jq(BODY, "empty", &["-c", "-r"]).unwrap(), "");
    }

    #[test]
    fn keeps_number_literals_exact() {
        assert_eq!(
            run_jq(BODY, ".big", &["-c"]).unwrap(),
            "12345678901234567890"
        );
        assert_eq!(
            run_jq(BODY, ".dec", &["-c"]).unwrap(),
            "0.10000000000000000001"
        );
    }

    #[test]
    fn writes_computed_numbers_as_jq_does() {
        assert_eq!(run_jq("6", ". / 2", &[]).unwrap(), "3");
        assert_eq!(run_jq("1", ". / 3", &[]).unwrap(), "0.3333333333333333");
        assert_eq!(
            run_jq("1", ". * 1e300 * 1e300", &[]).unwrap(),
            "1.7976931348623157e+308"
        );
        assert_eq!(
            run_jq("null", "[nan, 1 / 10000000, 3 / 2, 1e15 * 100]", &["-c"]).unwrap(),
            "[null,1e-07,1.5,1e+17]"
        );
    }

    #[test]
    fn reports_errors() {
        let syntax = run_jq(BODY, ".[", &[]).unwrap_err();
        assert!(syntax.starts_with("jq: error: "), "{syntax}");
        assert!(syntax.ends_with("jq: 1 compile error"), "{syntax}");
        let undefined = run_jq(BODY, "nope", &[]).unwrap_err();
        assert!(undefined.contains("nope/0 is not defined"), "{undefined}");
        let runtime = run_jq(BODY, ".id.x", &[]).unwrap_err();
        assert!(
            runtime.starts_with("jq: error (at <stdin>:0): "),
            "{runtime}"
        );
        assert!(run_jq("{", ".", &[]).unwrap_err().contains("Cannot parse"));
    }

    #[test]
    fn runs_over_each_input_value() {
        assert_eq!(run_jq("1 2", ". + 1", &["-c"]).unwrap(), "2\n3");
        assert_eq!(run_jq("", ".", &[]).unwrap(), "");
    }
}
