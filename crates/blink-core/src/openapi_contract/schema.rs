use serde_json::{Map, Value};

use super::{ValidationReport, resolve};

pub(super) fn example(spec: &Value, schema: &Value, depth: usize) -> Result<Value, String> {
    example_inner(spec, schema, depth, &mut 10_000)
}

fn example_inner(
    spec: &Value,
    schema: &Value,
    depth: usize,
    budget: &mut usize,
) -> Result<Value, String> {
    if *budget == 0 {
        return Err("The example exceeds the schema work limit.".into());
    }
    *budget -= 1;
    if depth >= 24 {
        return Err("The example contains a recursive or deeply nested schema.".into());
    }
    let schema = resolve(spec, schema)?;
    for key in ["example", "default", "const"] {
        if let Some(value) = schema.get(key) {
            return Ok(value.clone());
        }
    }
    if let Some(value) = schema
        .get("examples")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
    {
        return Ok(value.clone());
    }
    if let Some(value) = schema
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
    {
        return Ok(value.clone());
    }
    if ["allOf", "anyOf", "oneOf", "not"]
        .iter()
        .any(|key| schema.get(key).is_some())
    {
        return Err("Add an explicit example for a composed schema.".into());
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") | None if schema.get("properties").is_some() => {
            let mut object = Map::new();
            for (key, property) in schema["properties"]
                .as_object()
                .ok_or("Invalid properties schema.")?
            {
                if property["readOnly"] != true {
                    object.insert(
                        key.clone(),
                        example_inner(spec, property, depth + 1, budget)?,
                    );
                }
            }
            Ok(Value::Object(object))
        }
        Some("object") => Ok(serde_json::json!({})),
        Some("array") => Ok(Value::Array(vec![example_inner(
            spec,
            schema.get("items").ok_or("Array schema has no items.")?,
            depth + 1,
            budget,
        )?])),
        Some("string") => Ok(Value::String(String::new())),
        Some("number" | "integer") => Ok(Value::from(0)),
        Some("boolean") => Ok(Value::Bool(false)),
        Some("null") => Ok(Value::Null),
        _ => Err("No example is available for this schema.".into()),
    }
}

/// Validates supported constraints. Unknown constraints make the report
/// incomplete, so a partial implementation never reports full conformance.
pub(super) fn validate(
    spec: &Value,
    schema: &Value,
    value: &Value,
    path: &str,
    depth: usize,
    report: &mut ValidationReport,
) {
    validate_inner(
        spec,
        schema,
        value,
        path,
        depth,
        report,
        &mut ValidationState {
            budget: 10_000,
            request: None,
        },
    );
}

struct ValidationState {
    budget: usize,
    request: Option<bool>,
}

pub(super) fn validate_body(
    spec: &Value,
    schema: &Value,
    value: &Value,
    request: bool,
    report: &mut ValidationReport,
) {
    validate_inner(
        spec,
        schema,
        value,
        "$",
        0,
        report,
        &mut ValidationState {
            budget: 10_000,
            request: Some(request),
        },
    );
}

fn validate_inner(
    spec: &Value,
    schema: &Value,
    value: &Value,
    path: &str,
    depth: usize,
    report: &mut ValidationReport,
    state: &mut ValidationState,
) {
    if state.budget == 0 {
        report.unsupported("Schema validation work limit reached.");
        return;
    }
    state.budget -= 1;
    if depth >= 64 {
        report.unsupported(format!("{path}: schema depth limit reached."));
        return;
    }
    if let Some(allowed) = schema.as_bool() {
        if !allowed {
            report
                .errors
                .push(format!("{path}: value is forbidden by the schema."));
        }
        return;
    }
    if schema.get("$ref").is_some() {
        // OpenAPI 3.1 permits siblings. Validate them as well as the target.
        if schema.as_object().is_some_and(|object| object.len() > 1) {
            let mut siblings = schema.clone();
            siblings.as_object_mut().unwrap().remove("$ref");
            if spec["openapi"]
                .as_str()
                .is_some_and(|version| version.starts_with("3.1."))
            {
                validate_inner(spec, &siblings, value, path, depth + 1, report, state);
            } else if siblings
                .as_object()
                .unwrap()
                .keys()
                .any(|key| !matches!(key.as_str(), "description" | "summary"))
            {
                report.unsupported(format!(
                    "{path}: $ref sibling constraints require OpenAPI 3.1."
                ));
            }
        }
        match schema["$ref"]
            .as_str()
            .and_then(|reference| reference.strip_prefix('#'))
        {
            Some(pointer) => match spec.pointer(pointer) {
                Some(target) => validate_inner(spec, target, value, path, depth + 1, report, state),
                None => report.unsupported(format!(
                    "{path}: reference was not found: {}",
                    schema["$ref"]
                )),
            },
            None => report.unsupported(format!(
                "{path}: external or invalid reference is not supported: {}",
                schema["$ref"]
            )),
        }
        return;
    }
    let Some(object) = schema.as_object() else {
        report.unsupported(format!("{path}: invalid schema."));
        return;
    };
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "type"
                | "properties"
                | "required"
                | "additionalProperties"
                | "items"
                | "enum"
                | "const"
                | "minimum"
                | "maximum"
                | "exclusiveMinimum"
                | "exclusiveMaximum"
                | "minLength"
                | "maxLength"
                | "minItems"
                | "maxItems"
                | "uniqueItems"
                | "minProperties"
                | "maxProperties"
                | "allOf"
                | "anyOf"
                | "oneOf"
                | "not"
                | "nullable"
                | "title"
                | "description"
                | "example"
                | "examples"
                | "default"
                | "deprecated"
                | "externalDocs"
                | "xml"
                | "$comment"
                | "summary"
                | "readOnly"
                | "writeOnly"
        ) && !key.starts_with("x-")
        {
            report.unsupported(format!("{path}: schema keyword {key} is not supported."));
        }
    }
    // Empty/composed schemas are legal. Invalid keyword shapes are not.
    for key in ["enum", "required", "allOf", "anyOf", "oneOf"] {
        if let Some(rule) = schema.get(key)
            && !rule.is_array()
        {
            report.unsupported(format!("{path}: invalid {key} rule."));
        }
    }
    if schema
        .get("properties")
        .is_some_and(|rule| !rule.is_object())
    {
        report.unsupported(format!("{path}: invalid properties rule."));
    }
    let nullable = schema["nullable"] == true
        && spec["openapi"]
            .as_str()
            .is_some_and(|version| version.starts_with("3.0."));
    if schema.get("nullable").is_some()
        && !spec["openapi"]
            .as_str()
            .is_some_and(|version| version.starts_with("3.0."))
    {
        report.unsupported(format!(
            "{path}: nullable is supported only in OpenAPI 3.0; use a null type in 3.1."
        ));
    }
    for key in ["nullable", "uniqueItems"] {
        if schema.get(key).is_some_and(|rule| !rule.is_boolean()) {
            report.unsupported(format!("{path}: invalid {key} rule."));
        }
    }
    for key in ["exclusiveMinimum", "exclusiveMaximum"] {
        if schema
            .get(key)
            .is_some_and(|rule| !rule.is_boolean() && !rule.is_number())
        {
            report.unsupported(format!("{path}: invalid {key} rule."));
        }
    }
    if let Some(types) = schema.get("type") {
        if types
            .as_array()
            .is_some_and(|kinds| kinds.iter().any(|kind| !kind.is_string()))
        {
            report.unsupported(format!("{path}: invalid schema type array."));
        }
        let types: Vec<&str> = match types {
            Value::String(kind) => vec![kind],
            Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if types.is_empty()
            || types.iter().any(|kind| {
                !matches!(
                    *kind,
                    "null" | "object" | "array" | "string" | "number" | "integer" | "boolean"
                )
            })
        {
            report.unsupported(format!("{path}: invalid schema type."));
        } else if !(value.is_null() && nullable)
            && !types.iter().any(|kind| type_matches(kind, value))
        {
            report
                .errors
                .push(format!("{path}: expected {}.", types.join(" or ")));
            return;
        }
    }
    if let Some(choices) = schema.get("enum").and_then(Value::as_array)
        && !choices.contains(value)
    {
        report.errors.push(format!("{path}: value is not in enum."));
    }
    if let Some(expected) = schema.get("const")
        && value != expected
    {
        report
            .errors
            .push(format!("{path}: value does not match const."));
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema.get(key).and_then(Value::as_array) {
            if branches.is_empty() {
                report.unsupported(format!("{path}: {key} has no schemas."));
            }
            let mut outcomes = Vec::new();
            for branch in branches {
                let mut outcome = ValidationReport::default();
                validate_inner(spec, branch, value, path, depth + 1, &mut outcome, state);
                outcomes.push(outcome);
            }
            let complete = outcomes
                .iter()
                .all(|outcome| outcome.unsupported.is_empty());
            let matches = outcomes
                .iter()
                .filter(|outcome| outcome.errors.is_empty())
                .count();
            for outcome in &outcomes {
                for item in &outcome.unsupported {
                    report.unsupported(item.clone());
                }
            }
            if key == "allOf" {
                for outcome in outcomes {
                    report.errors.extend(outcome.errors);
                }
            } else if complete
                && ((key == "anyOf" && matches == 0) || (key == "oneOf" && matches != 1))
            {
                report
                    .errors
                    .push(format!("{path}: {key} matched {matches} schemas."));
            }
        }
    }
    if let Some(rule) = schema.get("not") {
        let mut outcome = ValidationReport::default();
        validate_inner(spec, rule, value, path, depth + 1, &mut outcome, state);
        if outcome.errors.is_empty() && outcome.unsupported.is_empty() {
            report
                .errors
                .push(format!("{path}: value matches a forbidden schema."));
        }
        for item in outcome.unsupported {
            report.unsupported(item);
        }
    }
    if let Some(value) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for name in required {
                let property = name
                    .as_str()
                    .and_then(|name| {
                        schema
                            .get("properties")
                            .and_then(|properties| properties.get(name))
                    })
                    .and_then(|property| resolve(spec, property).ok());
                let exempt = match state.request {
                    Some(true) => property.is_some_and(|property| property["readOnly"] == true),
                    Some(false) => property.is_some_and(|property| property["writeOnly"] == true),
                    None => false,
                };
                if exempt {
                    continue;
                }
                match name.as_str() {
                    Some(name) if !value.contains_key(name) => report
                        .errors
                        .push(format!("{path}.{name}: required property is missing.")),
                    None => {
                        report.unsupported(format!("{path}: required contains a non-string value."))
                    }
                    _ => {}
                }
            }
        }
        for (name, value) in value {
            if let Some(property) = schema
                .get("properties")
                .and_then(|properties| properties.get(name))
            {
                validate_inner(
                    spec,
                    property,
                    value,
                    &format!("{path}.{name}"),
                    depth + 1,
                    report,
                    state,
                );
            } else if let Some(additional) = schema.get("additionalProperties") {
                if additional == false {
                    report
                        .errors
                        .push(format!("{path}.{name}: additional property is forbidden."));
                } else if additional != true {
                    validate_inner(
                        spec,
                        additional,
                        value,
                        &format!("{path}.{name}"),
                        depth + 1,
                        report,
                        state,
                    );
                }
            }
        }
        length(
            schema,
            value.len(),
            "minProperties",
            "maxProperties",
            path,
            report,
        );
    }
    if let Some(value) = value.as_array() {
        length(schema, value.len(), "minItems", "maxItems", path, report);
        if let Some(items) = schema.get("items") {
            for (index, value) in value.iter().enumerate() {
                validate_inner(
                    spec,
                    items,
                    value,
                    &format!("{path}[{index}]"),
                    depth + 1,
                    report,
                    state,
                );
            }
        }
        if schema["uniqueItems"] == true {
            if value.len() > 1000 {
                report.unsupported(format!(
                    "{path}: uniqueItems check is limited to 1000 items."
                ));
            } else {
                for index in 0..value.len() {
                    if value[..index].contains(&value[index]) {
                        report
                            .errors
                            .push(format!("{path}: array items must be unique."));
                        break;
                    }
                }
            }
        }
    }
    if let Some(value) = value.as_str() {
        length(
            schema,
            value.chars().count(),
            "minLength",
            "maxLength",
            path,
            report,
        );
    }
    if let Some(value) = value.as_f64() {
        for (key, minimum) in [("minimum", true), ("maximum", false)] {
            if let Some(bound) = schema.get(key) {
                if let Some(bound) = bound.as_f64() {
                    let exclusive_key = if minimum {
                        "exclusiveMinimum"
                    } else {
                        "exclusiveMaximum"
                    };
                    let exclusive = schema[exclusive_key] == true;
                    if if minimum {
                        value < bound || (exclusive && value == bound)
                    } else {
                        value > bound || (exclusive && value == bound)
                    } {
                        report
                            .errors
                            .push(format!("{path}: value violates {key} {bound}."));
                    }
                } else {
                    report.unsupported(format!("{path}: invalid {key} rule."));
                }
            }
        }
        for (key, minimum) in [("exclusiveMinimum", true), ("exclusiveMaximum", false)] {
            if let Some(bound) = schema.get(key).and_then(Value::as_f64)
                && if minimum {
                    value <= bound
                } else {
                    value >= bound
                }
            {
                report
                    .errors
                    .push(format!("{path}: value violates {key} {bound}."));
            }
        }
    }
}

fn length(
    schema: &Value,
    length: usize,
    minimum: &str,
    maximum: &str,
    path: &str,
    report: &mut ValidationReport,
) {
    for (key, is_minimum) in [(minimum, true), (maximum, false)] {
        if let Some(bound) = schema.get(key) {
            if let Some(bound) = bound.as_u64() {
                if if is_minimum {
                    (length as u64) < bound
                } else {
                    (length as u64) > bound
                } {
                    report
                        .errors
                        .push(format!("{path}: length violates {key} {bound}."));
                }
            } else {
                report.unsupported(format!("{path}: invalid {key} rule."));
            }
        }
    }
}

fn type_matches(kind: &str, value: &Value) -> bool {
    match kind {
        "null" => value.is_null(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "integer" => {
            value.is_i64()
                || value.is_u64()
                || value.as_f64().is_some_and(|number| number.fract() == 0.0)
        }
        _ => false,
    }
}
