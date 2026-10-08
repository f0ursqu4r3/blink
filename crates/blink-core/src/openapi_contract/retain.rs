//! Keep one operation and the local-reference closure it needs.
use super::resolve;
use serde_json::{Map, Value};
use std::collections::HashSet;

pub(super) fn operation_document(spec: &Value, path: &str, method: &str) -> Value {
    let mut root = Map::new();
    for key in ["openapi", "jsonSchemaDialect", "servers", "security"] {
        if let Some(value) = spec.get(key) {
            root.insert(key.into(), value.clone());
        }
    }
    let mut paths = Map::new();
    if let Some(original) = spec.get("paths").and_then(|paths| paths.get(path)) {
        if let Ok(item) = resolve(spec, original) {
            let mut retained = Map::new();
            for key in [
                "parameters",
                "servers",
                "summary",
                "description",
                method.to_ascii_lowercase().as_str(),
            ] {
                if let Some(value) = item.get(key) {
                    retained.insert(key.into(), value.clone());
                }
            }
            paths.insert(path.into(), Value::Object(retained));
        } else {
            paths.insert(path.into(), original.clone());
        }
    }
    root.insert("paths".into(), Value::Object(paths));
    let mut result = Value::Object(root);
    let mut pending = Vec::new();
    references(&result, &mut pending);
    let mut seen = HashSet::new();
    while let Some(reference) = pending.pop() {
        if !seen.insert(reference.clone()) {
            continue;
        }
        let Some(pointer) = reference.strip_prefix('#') else {
            continue;
        };
        if pointer.is_empty() {
            return spec.clone();
        }
        let Some(target) = spec.pointer(pointer) else {
            continue;
        };
        references(target, &mut pending);
        let tokens: Vec<_> = pointer
            .trim_start_matches('/')
            .split('/')
            .map(|token| token.replace("~1", "/").replace("~0", "~"))
            .collect();
        insert(&mut result, spec, &tokens, target);
    }
    result
}

fn references(value: &Value, references: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                references.push(reference.into());
            }
            for value in object.values() {
                self::references(value, references);
            }
        }
        Value::Array(items) => {
            for value in items {
                self::references(value, references);
            }
        }
        _ => {}
    }
}

fn insert(target: &mut Value, source: &Value, tokens: &[String], value: &Value) {
    let Some((key, rest)) = tokens.split_first() else {
        *target = value.clone();
        return;
    };
    match source {
        Value::Object(source) => {
            let Some(child) = source.get(key) else {
                return;
            };
            if !target.is_object() {
                *target = Value::Object(Map::new());
            }
            let target = target
                .as_object_mut()
                .unwrap()
                .entry(key.clone())
                .or_insert(Value::Null);
            insert(target, child, rest, value);
        }
        Value::Array(source) => {
            let Ok(index) = key.parse::<usize>() else {
                return;
            };
            let Some(child) = source.get(index) else {
                return;
            };
            if !target.is_array() {
                *target = Value::Array(Vec::new());
            }
            let target = target.as_array_mut().unwrap();
            if target.len() <= index {
                target.resize(index + 1, Value::Null);
            }
            insert(&mut target[index], child, rest, value);
        }
        _ => {}
    }
}
