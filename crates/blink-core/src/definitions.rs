//! Token definitions as editable rows. Port of `src/lib/definitions.ts`.

use crate::model::{Definitions, Pair};
use crate::request::{js_trim, pair};

/// Show token definitions as key-value rows, with one blank row to type in.
pub fn definitions_to_rows(definitions: &Definitions) -> Vec<Pair> {
    let rows: Vec<Pair> = definitions
        .iter()
        .map(|(key, value)| pair(key.clone(), value.clone()))
        .collect();
    if rows.is_empty() {
        vec![pair("", "")]
    } else {
        rows
    }
}

/// Collect key-value rows into token definitions. Blank rows are skipped.
/// Returns an error message when a row is not a valid token.
pub fn rows_to_definitions(rows: &[Pair]) -> Result<Definitions, String> {
    let mut definitions = Definitions::new();
    for row in rows {
        let name = js_trim(&row.key);
        if name.is_empty() && row.value.is_empty() {
            continue;
        }
        if name.is_empty() {
            return Err("Enter a name for each token.".into());
        }
        if name.starts_with('_') {
            return Err("Token names must not start with _.".into());
        }
        if name.starts_with('!') {
            return Err("Token names must not start with !.".into());
        }
        if name.contains(['{', '}']) {
            return Err("Token names must not contain { or }.".into());
        }
        if definitions.contains_key(name) {
            return Err(format!("Token \"{name}\" is defined more than once."));
        }
        definitions.insert(name.to_string(), row.value.clone());
    }
    Ok(definitions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn gives_one_blank_row_when_there_are_no_definitions() {
        let rows = definitions_to_rows(&Definitions::new());
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].key.as_str(), rows[0].value.as_str()), ("", ""));
    }

    #[test]
    fn gives_one_row_for_each_definition() {
        let rows = definitions_to_rows(&defs(&[("a", "1"), ("b", "2")]));
        let pairs: Vec<(&str, &str)> = rows
            .iter()
            .map(|row| (row.key.as_str(), row.value.as_str()))
            .collect();
        assert_eq!(pairs, [("a", "1"), ("b", "2")]);
    }

    #[test]
    fn skips_blank_rows_and_trims_names() {
        assert_eq!(
            rows_to_definitions(&[pair(" host ", "x"), pair("", "")]),
            Ok(defs(&[("host", "x")]))
        );
    }

    #[test]
    fn keeps_an_empty_value_when_the_name_is_set() {
        assert_eq!(
            rows_to_definitions(&[pair("empty", "")]),
            Ok(defs(&[("empty", "")]))
        );
    }

    #[test]
    fn rejects_invalid_rows() {
        for (rows, message) in [
            (vec![pair("", "x")], "Enter a name"),
            (vec![pair("_x", "1")], "must not start with _"),
            (vec![pair("!x", "1")], "must not start with !"),
            (vec![pair("a{b", "1")], "must not contain"),
            (vec![pair("a", "1"), pair("a", "2")], "more than once"),
        ] {
            assert!(rows_to_definitions(&rows).unwrap_err().contains(message));
        }
    }
}
