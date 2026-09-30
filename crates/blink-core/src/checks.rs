//! Port of `src/lib/checks.ts`. The source and operator lists live on
//! `CheckSource` and `CheckOperator` in `model`.

use fancy_regex::Regex;

use crate::ids::CHECKS;
use crate::jq::run_jq;
use crate::json::{canonical_json, js_number_string};
use crate::model::{
    ApiResponse, Assertion, AssertionResult, Capture, CheckOperator, CheckSource, Definitions,
};

/// `^[A-Za-z][\w.-]{0,63}$`: a name usable as a {{token}}.
pub fn is_capture_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && name.len() <= 64
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

pub fn reserve_check_id(id: u64) {
    CHECKS.reserve(id);
}

/// The TS defaults are `Status`, `Equals`, "200", and an empty path.
pub fn create_assertion(
    source: CheckSource,
    operator: CheckOperator,
    expected: &str,
    path: &str,
) -> Assertion {
    Assertion {
        id: CHECKS.next(),
        enabled: true,
        source,
        path: path.into(),
        operator,
        expected: expected.into(),
    }
}

/// The TS defaults are an empty name, `Json`, and an empty path.
pub fn create_capture(name: &str, source: CheckSource, path: &str) -> Capture {
    Capture {
        id: CHECKS.next(),
        enabled: true,
        name: name.into(),
        source,
        path: path.into(),
    }
}

/// The value a source reads, as text; None when it is absent.
pub fn read_source(
    source: CheckSource,
    path: &str,
    response: &ApiResponse,
) -> Result<Option<String>, String> {
    match source {
        CheckSource::Status => return Ok(Some(response.status.to_string())),
        CheckSource::Time => return Ok(Some(js_number_string(response.duration_ms))),
        CheckSource::Size => return Ok(Some(response.size_bytes.to_string())),
        CheckSource::Header => {
            let name = path.trim().to_lowercase();
            return Ok(response
                .headers
                .iter()
                .find(|header| header.key.to_lowercase() == name)
                .map(|header| header.value.clone()));
        }
        CheckSource::Json | CheckSource::Body => {}
    }
    if response.is_truncated() || response.is_binary() {
        return Err("The body is truncated or binary.".into());
    }
    if source == CheckSource::Body {
        return Ok(Some(response.body.clone()));
    }
    // -r prints strings without quotes; -c prints other values on one line.
    let query = match path.trim() {
        "" => ".",
        query => query,
    };
    let output = run_jq(&response.body, query, &["-c", "-r"])?;
    Ok(if output == "null" || output.is_empty() {
        None
    } else {
        Some(output)
    })
}

/// JavaScript whitespace, as `String.prototype.trim` removes it.
fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// `Number(value)` in JavaScript: NaN when the text is not a number.
fn js_number(value: &str) -> f64 {
    let text = js_trim(value);
    if text.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = text.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return digits.chars().fold(0.0, |total, c| {
                total * radix as f64 + c.to_digit(radix).unwrap_or(0) as f64
            });
        }
    }
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    if unsigned == "Infinity" {
        return if text.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(at) => (&unsigned[..at], Some(&unsigned[at + 1..])),
        None => (unsigned, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    let valid_mantissa =
        digits(whole) && digits(fraction) && !(whole.is_empty() && fraction.is_empty());
    let valid_exponent = exponent.is_none_or(|exponent| {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        !exponent.is_empty() && digits(exponent)
    });
    if !valid_mantissa || !valid_exponent {
        return f64::NAN;
    }
    text.parse().unwrap_or(f64::NAN)
}

fn numeric(value: &str) -> bool {
    !js_trim(value).is_empty() && js_number(value).is_finite()
}

/// Parse JSON text so `{"a": 1}` equals `{"a":1}`; plain text stays text.
fn canonical(value: &str) -> String {
    canonical_json(value).unwrap_or_else(|| value.to_string())
}

/// Fails only for an invalid `matches` pattern.
pub fn compare(
    actual: Option<&str>,
    operator: CheckOperator,
    expected: &str,
) -> Result<bool, String> {
    use CheckOperator::*;
    if operator == Exists {
        return Ok(actual.is_some());
    }
    if operator == NotExists {
        return Ok(actual.is_none());
    }
    let Some(actual) = actual else {
        return Ok(matches!(operator, NotEquals | NotContains));
    };
    Ok(match operator {
        Lt | Lte | Gt | Gte => {
            if !numeric(actual) || !numeric(expected) {
                return Ok(false);
            }
            let a = js_number(actual);
            let b = js_number(expected);
            match operator {
                Lt => a < b,
                Lte => a <= b,
                Gt => a > b,
                _ => a >= b,
            }
        }
        Equals | NotEquals => {
            let equal = actual == expected
                || (numeric(actual)
                    && numeric(expected)
                    && js_number(actual) == js_number(expected))
                || canonical(actual) == canonical(expected);
            if operator == Equals { equal } else { !equal }
        }
        Contains => actual.contains(expected),
        NotContains => !actual.contains(expected),
        _ => Regex::new(expected)
            .map_err(|error| format!("Invalid regular expression: /{expected}/: {error}"))?
            .is_match(actual)
            .map_err(|error| error.to_string())?,
    })
}

pub fn describe_assertion(assertion: &Assertion) -> String {
    let source = assertion.source.label();
    let operator = assertion.operator.label();
    let path = if assertion.source.takes_path() {
        let path = match assertion.path.trim() {
            "" if assertion.source == CheckSource::Json => ".",
            path => path,
        };
        format!(" {path}")
    } else {
        String::new()
    };
    let expected = if assertion.operator.is_unary() {
        String::new()
    } else {
        format!(" {}", assertion.expected)
    };
    format!("{source}{path} {operator}{expected}")
}

const MAX_SHOWN: usize = 200;

fn shorten(value: &str) -> String {
    match value.char_indices().nth(MAX_SHOWN) {
        Some((at, _)) => format!("{}…", &value[..at]),
        None => value.to_string(),
    }
}

pub fn run_assertions(assertions: &[Assertion], response: &ApiResponse) -> Vec<AssertionResult> {
    let mut results = Vec::new();
    for assertion in assertions {
        if !assertion.enabled {
            continue;
        }
        let description = describe_assertion(assertion);
        let outcome = read_source(assertion.source, &assertion.path, response).and_then(|actual| {
            let pass = compare(actual.as_deref(), assertion.operator, &assertion.expected)?;
            Ok((pass, actual))
        });
        results.push(match outcome {
            Ok((pass, actual)) => AssertionResult {
                id: assertion.id,
                description,
                pass,
                actual: actual.map_or_else(|| "(absent)".into(), |actual| shorten(&actual)),
            },
            Err(error) => AssertionResult {
                id: assertion.id,
                description,
                pass: false,
                actual: error,
            },
        });
    }
    results
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CaptureOutcome {
    /// Token values from the enabled captures that found a value.
    pub values: Definitions,
    pub errors: Vec<String>,
}

/// Token values from the enabled captures that found a value.
pub fn run_captures(captures: &[Capture], response: &ApiResponse) -> CaptureOutcome {
    let mut outcome = CaptureOutcome::default();
    for capture in captures {
        if !capture.enabled || !is_capture_name(&capture.name) {
            continue;
        }
        match read_source(capture.source, &capture.path, response) {
            Ok(Some(value)) => {
                outcome.values.insert(capture.name.clone(), value);
            }
            Ok(None) => outcome.errors.push(format!("{}: no value", capture.name)),
            Err(error) => outcome.errors.push(format!("{}: {error}", capture.name)),
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Header;
    use CheckOperator::*;
    use CheckSource::*;

    fn response() -> ApiResponse {
        ApiResponse {
            status: 201,
            status_text: "Created".into(),
            duration_ms: 120.0,
            size_bytes: 64,
            headers: vec![Header {
                key: "Content-Type".into(),
                value: "application/json; charset=utf-8".into(),
            }],
            body: r#"{"id":7,"token":"abc","tags":["a","b"],"none":null}"#.into(),
            body_id: None,
            truncated: None,
            binary: None,
            final_url: None,
            redirect_count: None,
            timing: None,
        }
    }

    #[test]
    fn compares_numbers_json_and_text() {
        let check = |actual, operator, expected| compare(actual, operator, expected).unwrap();
        assert!(check(Some("201"), Equals, "201.0"));
        assert!(check(Some(r#"{"a": 1}"#), Equals, r#"{"a":1}"#));
        assert!(check(Some("120"), Lt, "200"));
        assert!(!check(Some("abc"), Gt, "1"));
        assert!(check(Some("hello"), Contains, "ell"));
        assert!(check(Some("hello"), Matches, "^h.*o$"));
        assert!(!check(None, Exists, ""));
        assert!(check(None, NotExists, ""));
        assert!(!check(None, Equals, "x"));
        assert!(check(None, NotEquals, "x"));
    }

    #[test]
    fn reads_numbers_as_javascript_does() {
        assert!(compare(Some("0x10"), Equals, "16").unwrap());
        assert!(compare(Some(" 5 "), Gte, "5e0").unwrap());
        assert!(!compare(Some("1_000"), Gt, "1").unwrap());
        assert!(!compare(Some("inf"), Gt, "1").unwrap());
        assert!(compare(Some("x"), Matches, "(").is_err());
    }

    #[test]
    fn reads_status_time_headers_and_jq_values() {
        let mut disabled = create_assertion(Status, Equals, "500", "");
        disabled.enabled = false;
        let results = run_assertions(
            &[
                create_assertion(Status, Equals, "201", ""),
                create_assertion(Time, Lt, "100", ""),
                create_assertion(Header, Contains, "json", "content-type"),
                create_assertion(Json, Equals, "abc", ".token"),
                create_assertion(Json, Equals, r#"["a","b"]"#, ".tags"),
                create_assertion(Json, NotExists, "", ".none"),
                create_assertion(Json, Equals, "1", ".["),
                disabled,
            ],
            &response(),
        );
        assert_eq!(
            results.iter().map(|r| r.pass).collect::<Vec<_>>(),
            [true, false, true, true, true, true, false]
        );
        assert_eq!(results[1].actual, "120");
        assert_ne!(results[6].actual, "");
    }

    #[test]
    fn fails_body_checks_on_a_truncated_body() {
        let truncated = ApiResponse {
            truncated: Some(true),
            ..response()
        };
        let results = run_assertions(&[create_assertion(Body, Contains, "x", "")], &truncated);
        assert!(!results[0].pass);
        assert!(results[0].actual.contains("truncated"));
    }

    #[test]
    fn describes_an_assertion() {
        assert_eq!(
            describe_assertion(&create_assertion(Json, Gte, "2", ".n")),
            "JSON (jq) .n ≥ 2"
        );
        assert_eq!(
            describe_assertion(&create_assertion(Status, Exists, "", "")),
            "Status exists"
        );
        assert_eq!(
            describe_assertion(&create_assertion(Json, Exists, "", " ")),
            "JSON (jq) . exists"
        );
    }

    #[test]
    fn shortens_long_values() {
        let long = "x".repeat(250);
        let results = run_assertions(
            &[create_assertion(Body, Equals, "", "")],
            &ApiResponse {
                body: long,
                ..response()
            },
        );
        assert_eq!(results[0].actual, format!("{}…", "x".repeat(200)));
    }

    #[test]
    fn captures_named_values_and_reports_misses() {
        let outcome = run_captures(
            &[
                create_capture("token", Json, ".token"),
                create_capture("type", Header, "content-type"),
                create_capture("missing", Json, ".nothing"),
                create_capture("bad name", Json, ".id"),
            ],
            &response(),
        );
        assert_eq!(
            outcome.values,
            Definitions::from([
                ("token".into(), "abc".into()),
                ("type".into(), "application/json; charset=utf-8".into()),
            ])
        );
        assert_eq!(outcome.errors, ["missing: no value"]);
    }

    #[test]
    fn checks_capture_names() {
        assert!(is_capture_name("a"));
        assert!(is_capture_name("user.id-2_x"));
        assert!(!is_capture_name("1a"));
        assert!(!is_capture_name("bad name"));
        assert!(!is_capture_name(&format!("a{}", "b".repeat(64))));
        assert!(is_capture_name(&format!("a{}", "b".repeat(63))));
    }
}
