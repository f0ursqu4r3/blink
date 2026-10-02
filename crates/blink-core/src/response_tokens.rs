//! Tokens whose values come from another request's response: max age,
//! validation, resolution, and the send plan.

use crate::checks::{INVALID_NAME_MESSAGE, is_capture_name};
use crate::model::{CheckSource, RequestSession, ResponseToken};

/// Sources a response token can read.
pub const RESPONSE_TOKEN_SOURCES: [CheckSource; 4] = [
    CheckSource::Json,
    CheckSource::Header,
    CheckSource::Body,
    CheckSource::Status,
];

const UNITS: [(char, u64); 4] = [('d', 86_400), ('h', 3_600), ('m', 60), ('s', 1)];

/// `15m` to 900 seconds. Empty means no max age.
pub fn parse_max_age(text: &str) -> Result<Option<u64>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let error = || "Enter a max age such as 30s, 15m, or 1h.".to_string();
    let unit = text.chars().last().ok_or_else(error)?;
    let scale = UNITS
        .iter()
        .find(|(name, _)| *name == unit)
        .map(|(_, scale)| *scale)
        .ok_or_else(error)?;
    let count: u64 = text[..text.len() - 1].parse().map_err(|_| error())?;
    if count == 0 {
        return Err(error());
    }
    count.checked_mul(scale).map(Some).ok_or_else(error)
}

/// 900 seconds to `15m`, in the largest exact unit.
pub fn format_max_age(secs: Option<u64>) -> String {
    let Some(secs) = secs else {
        return String::new();
    };
    let (unit, scale) = UNITS
        .iter()
        .find(|(_, scale)| secs % scale == 0)
        .copied()
        .unwrap_or(('s', 1));
    format!("{}{unit}", secs / scale)
}

/// The first problem of each row, by token id. `text_names` are the text
/// tokens of the same scope.
pub fn validate_response_tokens(
    tokens: &[ResponseToken],
    text_names: &[&str],
    sessions: &[RequestSession],
) -> Vec<(u64, String)> {
    let mut seen: Vec<&str> = text_names.to_vec();
    let mut errors = Vec::new();
    for token in tokens {
        let name = token.name.trim();
        let error = if name.is_empty() {
            Some("Enter a token name.".to_string())
        } else if !is_capture_name(name) {
            Some(INVALID_NAME_MESSAGE.to_string())
        } else if seen.contains(&name) {
            Some(format!("Another token is named \"{name}\"."))
        } else if !sessions.iter().any(|s| s.id == token.request_id) {
            Some("Choose a request.".to_string())
        } else if token.source.takes_path() && token.path.trim().is_empty() {
            Some("Enter a path.".to_string())
        } else {
            None
        };
        seen.push(name);
        if let Some(error) = error {
            errors.push((token.id, error));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CheckSource, ResponseToken};
    use crate::session::create_session;

    fn token(id: u64, name: &str, request_id: u64) -> ResponseToken {
        ResponseToken {
            id,
            name: name.into(),
            request_id,
            source: CheckSource::Json,
            path: ".access_token".into(),
            max_age_secs: None,
        }
    }

    #[test]
    fn parses_and_formats_max_age() {
        assert_eq!(parse_max_age(""), Ok(None));
        assert_eq!(parse_max_age(" 15m "), Ok(Some(900)));
        assert_eq!(parse_max_age("1h"), Ok(Some(3600)));
        assert_eq!(parse_max_age("30s"), Ok(Some(30)));
        assert_eq!(parse_max_age("2d"), Ok(Some(172_800)));
        assert!(parse_max_age("15").is_err());
        assert!(parse_max_age("0m").is_err());
        assert!(parse_max_age("m").is_err());
        assert_eq!(format_max_age(Some(900)), "15m");
        assert_eq!(format_max_age(Some(3600)), "1h");
        assert_eq!(format_max_age(Some(90)), "90s");
        assert_eq!(format_max_age(None), "");
    }

    #[test]
    fn validation_names_the_first_problem_of_each_row() {
        let login = create_session(None);
        let sessions = vec![login.clone()];
        let tokens = vec![
            token(1, "access_token", login.id),
            token(2, "access_token", login.id),
            token(3, "host", login.id),
            token(4, "orphan", 9_999),
            token(5, "", login.id),
        ];
        let errors = validate_response_tokens(&tokens, &["host"], &sessions);
        assert_eq!(
            errors,
            vec![
                (2, "Another token is named \"access_token\".".to_string()),
                (3, "Another token is named \"host\".".to_string()),
                (4, "Choose a request.".to_string()),
                (5, "Enter a token name.".to_string()),
            ]
        );
    }

    #[test]
    fn a_path_is_required_for_json_and_header() {
        let login = create_session(None);
        let mut row = token(1, "t", login.id);
        row.path = " ".into();
        let errors = validate_response_tokens(&[row.clone()], &[], std::slice::from_ref(&login));
        assert_eq!(errors, vec![(1, "Enter a path.".to_string())]);
        row.source = CheckSource::Status;
        assert!(validate_response_tokens(&[row], &[], std::slice::from_ref(&login)).is_empty());
    }

    #[test]
    fn invalid_name_format_is_rejected() {
        let login = create_session(None);
        let mut row = token(1, "1a", login.id);
        let errors = validate_response_tokens(&[row.clone()], &[], std::slice::from_ref(&login));
        assert_eq!(
            errors,
            vec![(
                1,
                "Start with a letter. Use letters, digits, _, . or -.".to_string()
            )]
        );

        row.name = "bad name".into();
        let errors = validate_response_tokens(&[row], &[], std::slice::from_ref(&login));
        assert_eq!(
            errors,
            vec![(
                1,
                "Start with a letter. Use letters, digits, _, . or -.".to_string()
            )]
        );
    }
}
