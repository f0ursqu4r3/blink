//! Retained OpenAPI contracts. Validation is explicit about unsupported rules.
//! Only references inside the retained document are resolved; no network IO.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{ApiResponse, BodyMode, Draft, RequestInput};
use crate::request::pair;

mod retain;
mod schema;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiContract {
    pub source: String,
    pub spec: Value,
    pub path: String,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ValidationReport {
    pub errors: Vec<String>,
    pub unsupported: Vec<String>,
}

impl ValidationReport {
    pub fn summary(&self) -> &'static str {
        if !self.errors.is_empty() {
            "Contract mismatch"
        } else if !self.unsupported.is_empty() {
            "Validation incomplete"
        } else {
            "Contract matched"
        }
    }

    fn unsupported(&mut self, message: impl Into<String>) {
        let message = message.into();
        if !self.unsupported.contains(&message) {
            self.unsupported.push(message);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParameterSuggestion {
    pub name: String,
    pub location: String,
    pub required: bool,
    pub example: Option<String>,
    pub description: String,
}

pub(super) fn resolve<'a>(spec: &'a Value, value: &'a Value) -> Result<&'a Value, String> {
    let mut current = value;
    let mut seen = Vec::new();
    while let Some(reference) = current.get("$ref") {
        let reference = reference.as_str().ok_or("Invalid $ref value.")?;
        if seen.contains(&reference) || seen.len() >= 64 {
            return Err(format!("Cyclic reference: {reference}"));
        }
        seen.push(reference);
        let pointer = reference
            .strip_prefix('#')
            .ok_or_else(|| format!("External reference is not supported: {reference}"))?;
        current = spec
            .pointer(pointer)
            .ok_or_else(|| format!("Reference was not found: {reference}"))?;
    }
    Ok(current)
}

impl OpenApiContract {
    pub fn new(source: &str, spec: &Value, path: &str, method: &str) -> Self {
        Self {
            source: source.into(),
            spec: retain::operation_document(spec, path, method),
            path: path.into(),
            method: method.to_uppercase(),
        }
    }

    pub fn operation(&self) -> Result<&Value, String> {
        let version = self
            .spec
            .get("openapi")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !version.starts_with("3.0.") && !version.starts_with("3.1.") {
            return Err(format!(
                "Contract validation supports OpenAPI 3.0 and 3.1; found {version}."
            ));
        }
        let path = self
            .spec
            .get("paths")
            .and_then(|paths| paths.get(&self.path))
            .ok_or_else(|| format!("Operation path was removed: {}", self.path))?;
        let path = resolve(&self.spec, path)?;
        path.get(self.method.to_ascii_lowercase()).ok_or_else(|| {
            format!(
                "Operation was removed: {} {}",
                self.method.to_uppercase(),
                self.path
            )
        })
    }

    /// Replace the retained document only after parsing succeeds. The return
    /// value reports a semantic document change, not whitespace changes.
    pub fn refresh(&mut self, text: &str) -> Result<bool, String> {
        let next: Value = if text.trim_start().starts_with('{') {
            serde_json::from_str(text).map_err(|error| format!("Invalid JSON: {error}"))?
        } else {
            serde_yaml_ng::from_str(text).map_err(|error| format!("Invalid YAML: {error}"))?
        };
        let version = next
            .get("openapi")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !version.starts_with("3.") || !next.get("paths").is_some_and(Value::is_object) {
            return Err("The source must contain an OpenAPI 3.x document with paths.".into());
        }
        let next = retain::operation_document(&next, &self.path, &self.method);
        let changed = self.spec != next;
        self.spec = next;
        Ok(changed)
    }

    fn parameters(&self) -> Result<Vec<&Value>, String> {
        let operation = self.operation()?;
        let path = resolve(&self.spec, &self.spec["paths"][&self.path])?;
        let mut parameters: Vec<&Value> = Vec::new();
        for owner in [path, operation] {
            if let Some(list) = owner.get("parameters") {
                for parameter in list.as_array().ok_or("Invalid parameter list.")? {
                    let parameter = resolve(&self.spec, parameter)?;
                    let name = parameter
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or("Parameter has no name.")?;
                    let location = parameter
                        .get("in")
                        .and_then(Value::as_str)
                        .ok_or("Parameter has no location.")?;
                    parameters.retain(|old| old["name"] != name || old["in"] != location);
                    parameters.push(parameter);
                }
            }
        }
        Ok(parameters)
    }

    pub fn suggestions(&self) -> Result<Vec<ParameterSuggestion>, String> {
        self.parameters()?
            .into_iter()
            .map(|parameter| {
                let example = parameter.get("example").cloned().or_else(|| {
                    parameter
                        .get("schema")
                        .and_then(|schema| schema::example(&self.spec, schema, 0).ok())
                });
                Ok(ParameterSuggestion {
                    name: parameter["name"].as_str().unwrap_or_default().into(),
                    location: parameter["in"].as_str().unwrap_or_default().into(),
                    required: parameter["required"] == true || parameter["in"] == "path",
                    example: example.map(|value| match value {
                        Value::String(s) => s,
                        other => other.to_string(),
                    }),
                    description: parameter["description"].as_str().unwrap_or_default().into(),
                })
            })
            .collect()
    }

    pub fn request_example(&self) -> Result<(String, String), String> {
        let operation = self.operation()?;
        let body = operation
            .get("requestBody")
            .ok_or("This operation has no request body.")?;
        let body = resolve(&self.spec, body)?;
        let content = body["content"]
            .as_object()
            .ok_or("No request media types are declared.")?;
        let (kind, media) = content
            .iter()
            .find(|(kind, _)| is_json(kind))
            .ok_or("Example application supports JSON request bodies only.")?;
        let value = if let Some(value) = media.get("example") {
            value.clone()
        } else if let Some(example) = media
            .get("examples")
            .and_then(Value::as_object)
            .and_then(|examples| examples.values().next())
        {
            resolve(&self.spec, example)?
                .get("value")
                .cloned()
                .ok_or("External examples are not supported.")?
        } else {
            schema::example(
                &self.spec,
                media
                    .get("schema")
                    .ok_or("No schema or example is declared.")?,
                0,
            )?
        };
        Ok((
            kind.clone(),
            serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?,
        ))
    }

    pub fn apply_example(&self, draft: &mut Draft) -> Result<(), String> {
        let (kind, body) = self.request_example()?;
        draft.body_mode = BodyMode::Json;
        draft.body = body;
        if let Some(header) = draft
            .headers
            .iter_mut()
            .find(|header| header.key.eq_ignore_ascii_case("content-type"))
        {
            header.value = kind;
            header.enabled = true;
        } else {
            draft.headers.push(pair("Content-Type", kind));
        }
        Ok(())
    }

    pub fn validate_request(&self, request: &RequestInput) -> ValidationReport {
        let mut report = ValidationReport::default();
        let operation = match self.operation() {
            Ok(operation) => operation,
            Err(error) => {
                report.unsupported(error);
                return report;
            }
        };
        if !request.method.eq_ignore_ascii_case(&self.method) {
            report
                .errors
                .push(format!("Method must be {}.", self.method.to_uppercase()));
        }
        let url = match url::Url::parse(&request.url) {
            Ok(url) => url,
            Err(error) => {
                report.errors.push(format!("Invalid URL: {error}"));
                return report;
            }
        };
        let parameters = match self.parameters() {
            Ok(parameters) => parameters,
            Err(error) => {
                report.unsupported(error);
                return report;
            }
        };
        let template: Vec<_> = self.path.trim_start_matches('/').split('/').collect();
        let segments: Vec<_> = url.path().trim_start_matches('/').split('/').collect();
        let path_segments = segments
            .get(segments.len().saturating_sub(template.len())..)
            .unwrap_or_default();
        if template.len() != path_segments.len()
            || template.iter().zip(path_segments).any(|(pattern, actual)| {
                !(pattern.starts_with('{') && pattern.ends_with('}')) && pattern != actual
            })
        {
            report
                .errors
                .push(format!("URL path does not match {}.", self.path));
        }
        for parameter in parameters {
            let name = parameter["name"].as_str().unwrap_or_default();
            let location = parameter["in"].as_str().unwrap_or_default();
            let value = match location {
                "query" => url
                    .query_pairs()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.into_owned()),
                "header" => request
                    .headers
                    .iter()
                    .find(|header| header.key.eq_ignore_ascii_case(name))
                    .map(|header| header.value.clone()),
                "path" => template
                    .iter()
                    .position(|segment| *segment == format!("{{{name}}}"))
                    .and_then(|index| path_segments.get(index))
                    .map(|value| decode_segment(value)),
                "cookie" => request
                    .headers
                    .iter()
                    .filter(|header| header.key.eq_ignore_ascii_case("cookie"))
                    .flat_map(|header| header.value.split(';'))
                    .filter_map(|part| part.trim().split_once('='))
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| value.to_string()),
                _ => {
                    report.unsupported(format!("Unsupported parameter location: {location}"));
                    None
                }
            };
            let label = format!("{location}.{name}");
            if (parameter["required"] == true || location == "path")
                && value.as_ref().is_none_or(|value| value.is_empty())
            {
                report
                    .errors
                    .push(format!("{label}: required parameter is missing."));
                continue;
            }
            if let Some(value) = value {
                if let Some(schema) = parameter.get("schema") {
                    match resolve(&self.spec, schema) {
                        Ok(resolved) => {
                            let kind = resolved
                                .get("type")
                                .and_then(Value::as_str)
                                .unwrap_or("string");
                            let scalar = match kind {
                                "integer" | "number" | "boolean" => {
                                    serde_json::from_str(&value).unwrap_or(Value::String(value))
                                }
                                "string" => Value::String(value),
                                _ => {
                                    report.unsupported(format!(
                                        "{label}: structured parameters are not supported."
                                    ));
                                    continue;
                                }
                            };
                            schema::validate(&self.spec, schema, &scalar, &label, 0, &mut report);
                        }
                        Err(error) => report.unsupported(error),
                    }
                } else {
                    report.unsupported(format!(
                        "{label}: parameter content without a schema is not supported."
                    ));
                }
            }
        }
        if let Some(body) = operation.get("requestBody") {
            match resolve(&self.spec, body) {
                Err(error) => report.unsupported(error),
                Ok(body) => {
                    if request.body.is_none()
                        && request.body_file.is_none()
                        && request.multipart.is_none()
                    {
                        if body["required"] == true {
                            report
                                .errors
                                .push("Required request body is missing.".into());
                        }
                    } else if request.body_file.is_some() || request.multipart.is_some() {
                        report.unsupported("File and multipart body validation is not supported.");
                    } else {
                        let kind = request
                            .headers
                            .iter()
                            .find(|header| header.key.eq_ignore_ascii_case("content-type"))
                            .map(|header| header.value.as_str())
                            .unwrap_or_default();
                        validate_content(
                            &self.spec,
                            body,
                            kind,
                            request.body.as_deref().unwrap_or_default(),
                            true,
                            &mut report,
                        );
                    }
                }
            }
        }
        report
    }

    pub fn validate_response(&self, response: &ApiResponse) -> ValidationReport {
        let mut report = ValidationReport::default();
        let operation = match self.operation() {
            Ok(operation) => operation,
            Err(error) => {
                report.unsupported(error);
                return report;
            }
        };
        if response.is_binary() || response.is_truncated() {
            report.unsupported("A binary or truncated response cannot be validated.");
            return report;
        }
        let responses = &operation["responses"];
        let status = response.status.to_string();
        let range = format!("{}XX", response.status / 100);
        let declared = responses
            .get(&status)
            .or_else(|| responses.get(&range))
            .or_else(|| responses.get("default"));
        let Some(declared) = declared else {
            report.errors.push(format!(
                "Response status {} is not declared.",
                response.status
            ));
            return report;
        };
        match resolve(&self.spec, declared) {
            Err(error) => report.unsupported(error),
            Ok(declared) => {
                if let Some(headers) = declared.get("headers").and_then(Value::as_object) {
                    for (name, definition) in headers {
                        match resolve(&self.spec, definition) {
                            Ok(definition) => {
                                let header = response
                                    .headers
                                    .iter()
                                    .find(|header| header.key.eq_ignore_ascii_case(name));
                                if definition["required"] == true && header.is_none() {
                                    report.errors.push(format!(
                                        "Required response header {name} is missing."
                                    ));
                                }
                                if header.is_some()
                                    && (definition.get("schema").is_some()
                                        || definition.get("content").is_some())
                                {
                                    report.unsupported(format!(
                                        "Response header schema validation is not supported: {name}"
                                    ));
                                }
                            }
                            Err(error) => report.unsupported(error),
                        }
                    }
                }
                if declared.get("content").is_none() {
                    if !response.body.is_empty() {
                        report.errors.push("Response body is not declared.".into());
                    }
                } else {
                    let kind = response
                        .headers
                        .iter()
                        .find(|header| header.key.eq_ignore_ascii_case("content-type"))
                        .map(|header| header.value.as_str())
                        .unwrap_or_default();
                    validate_content(
                        &self.spec,
                        declared,
                        kind,
                        &response.body,
                        false,
                        &mut report,
                    );
                }
            }
        }
        report
    }
}

fn decode_segment(value: &str) -> String {
    url::form_urlencoded::parse(format!("v={}", value.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}

fn is_json(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("application/json") || kind.to_ascii_lowercase().ends_with("+json")
}

fn validate_content(
    spec: &Value,
    owner: &Value,
    kind: &str,
    body: &str,
    request: bool,
    report: &mut ValidationReport,
) {
    let kind = kind
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let Some(content) = owner.get("content").and_then(Value::as_object) else {
        report.unsupported("Invalid content declaration.");
        return;
    };
    let media = content
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(&kind))
        .map(|(_, media)| media)
        .or_else(|| {
            kind.split_once('/')
                .and_then(|(major, _)| content.get(&format!("{major}/*")))
        })
        .or_else(|| content.get("*/*"));
    let Some(media) = media else {
        report
            .errors
            .push(format!("Content-Type {kind:?} is not declared."));
        return;
    };
    let Some(schema) = media.get("schema") else {
        report.unsupported("No body schema is declared.");
        return;
    };
    if !is_json(&kind) {
        report.unsupported(format!("Body validation supports JSON only; found {kind}."));
        return;
    }
    match serde_json::from_str::<Value>(body) {
        Ok(value) => schema::validate_body(spec, schema, &value, request, report),
        Err(error) => report.errors.push(format!("Invalid JSON body: {error}")),
    }
}
