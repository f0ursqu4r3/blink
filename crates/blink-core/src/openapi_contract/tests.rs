use super::*;
use crate::model::Header;
use serde_json::json;

fn contract() -> OpenApiContract {
    OpenApiContract {
        source: "/tmp/pets.yaml".into(),
        path: "/pets/{id}".into(),
        method: "POST".into(),
        spec: json!({
            "openapi": "3.1.0", "paths": {"/pets/{id}": {
                "parameters": [{"name":"id", "in":"path", "required":true, "schema":{"type":"integer"}}],
                "post": {
                    "parameters":[{"name":"limit", "in":"query", "required":true, "schema":{"type":"integer", "minimum":1}}],
                    "requestBody":{"required":true,"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Pet"}}}},
                    "responses":{"200":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Pet"}}}}}
                }
            }},
            "components":{"schemas":{"Pet":{"type":"object","required":["name"],"properties":{"name":{"type":"string","minLength":1,"example":"Milo"}},"additionalProperties":false}}}
        }),
    }
}
fn request() -> RequestInput {
    RequestInput {
        method: "POST".into(),
        url: "https://example.com/v1/pets/12?limit=2".into(),
        headers: vec![Header {
            key: "Content-Type".into(),
            value: "application/json".into(),
        }],
        body: Some(r#"{"name":"Milo"}"#.into()),
        ..Default::default()
    }
}
fn response(body: &str) -> ApiResponse {
    ApiResponse {
        status: 200,
        status_text: "OK".into(),
        duration_ms: 1.0,
        headers: request().headers,
        body: body.into(),
        size_bytes: body.len() as u64,
        body_id: None,
        truncated: None,
        binary: None,
        final_url: None,
        redirect_count: None,
        timing: None,
    }
}
#[test]
fn import_retains_source_and_operation_through_serialization() {
    let imported =
        crate::import::parse_import(&contract().spec.to_string(), "/tmp/pets.json").unwrap();
    let draft = &imported.root.requests[0];
    let saved = serde_json::to_string(draft).unwrap();
    let restored: Draft = serde_json::from_str(&saved).unwrap();
    let retained = restored.openapi_contract.unwrap();
    assert_eq!(retained.source, "/tmp/pets.json");
    assert_eq!(retained.path, "/pets/{id}");
    assert_eq!(retained.method, "POST");
    assert!(retained.operation().is_ok());
}
#[test]
fn validates_resolved_request_and_required_parameters() {
    let contract = contract();
    assert_eq!(
        contract.validate_request(&request()).summary(),
        "Contract matched"
    );
    let mut request = request();
    request.url = "https://example.com/pets/no?limit=0".into();
    let report = contract.validate_request(&request);
    assert!(report.errors.iter().any(|error| error.contains("path.id")));
    assert!(report.errors.iter().any(|error| error.contains("minimum")));
    request.url = "https://example.com/pets/1".into();
    assert!(
        contract
            .validate_request(&request)
            .errors
            .iter()
            .any(|error| error.contains("query.limit"))
    );
}
#[test]
fn resolves_nested_schema_and_applies_example() {
    let contract = contract();
    let mut draft = crate::request::create_draft();
    contract.apply_example(&mut draft).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&draft.body).unwrap(),
        json!({"name":"Milo"})
    );
    assert_eq!(draft.body_mode, BodyMode::Json);
    assert_eq!(
        contract
            .validate_response(&response(r#"{"name":"Milo"}"#))
            .summary(),
        "Contract matched"
    );
    let invalid = contract.validate_response(&response(r#"{"age":1}"#));
    assert!(
        invalid
            .errors
            .iter()
            .any(|error| error.contains("required property"))
    );
    assert!(
        invalid
            .errors
            .iter()
            .any(|error| error.contains("additional property"))
    );
}
#[test]
fn never_reports_unsupported_or_truncated_data_as_valid() {
    let mut contract = contract();
    contract.spec["components"]["schemas"]["Pet"]["properties"]["name"]["pattern"] = json!("^M");
    assert_eq!(
        contract
            .validate_response(&response(r#"{"name":"Milo"}"#))
            .summary(),
        "Validation incomplete"
    );
    contract.spec["components"]["schemas"]["Pet"] =
        json!({"$ref":"https://example.com/schema.json"});
    assert_eq!(
        contract.validate_response(&response("{}")).summary(),
        "Validation incomplete"
    );
    assert!(contract.request_example().is_err());
    let mut response = response("{}");
    response.truncated = Some(true);
    assert_eq!(
        contract.validate_response(&response).summary(),
        "Validation incomplete"
    );
}
#[test]
fn refresh_detects_changes_and_missing_operations() {
    let mut contract = contract();
    assert!(!contract.refresh(&contract.spec.to_string()).unwrap());
    let old = contract.spec.clone();
    assert!(contract.refresh("garbage").is_err());
    assert_eq!(contract.spec, old);
    assert!(contract.refresh("openapi: 3.1.0\npaths: {}\n").unwrap());
    assert!(contract.operation().unwrap_err().contains("removed"));
}
#[test]
fn cyclic_and_missing_references_are_incomplete() {
    let mut contract = contract();
    for reference in ["#/components/schemas/Pet", "#/components/schemas/Missing"] {
        contract.spec["components"]["schemas"]["Pet"] = json!({"$ref":reference});
        assert_eq!(
            contract.validate_response(&response("{}")).summary(),
            "Validation incomplete"
        );
    }
}
#[test]
fn operation_parameters_override_path_parameters() {
    let mut contract = contract();
    contract.spec["paths"]["/pets/{id}"]["post"]["parameters"] = json!([
        {"name":"id","in":"path","schema":{"type":"string","example":"abc"}}
    ]);
    let suggestions = contract.suggestions().unwrap();
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].example.as_deref(), Some("abc"));
}
#[test]
fn composed_schema_and_numeric_bounds_fail_correctly() {
    let mut report = ValidationReport::default();
    schema::validate(
        &json!({}),
        &json!({"oneOf":[{"type":"integer"},{"minimum":0}]}),
        &json!(2),
        "$",
        0,
        &mut report,
    );
    assert!(!report.errors.is_empty());
    let mut report = ValidationReport::default();
    schema::validate(
        &json!({}),
        &json!({"type":"array","uniqueItems":true,"items":{"type":"number","exclusiveMinimum":0}}),
        &json!([0, 0]),
        "$",
        0,
        &mut report,
    );
    assert_eq!(report.errors.len(), 3);
}

#[test]
fn resolves_path_item_refs_and_yaml_imports() {
    let text = "openapi: 3.0.3\npaths:\n  /pets:\n    $ref: '#/components/pathItems/Pets'\ncomponents:\n  pathItems:\n    Pets:\n      get:\n        responses:\n          '204':\n            description: No content\n";
    let imported = crate::import::parse_import(text, "/tmp/pets.yaml").unwrap();
    let contract = imported.root.requests[0].openapi_contract.as_ref().unwrap();
    assert_eq!(contract.source, "/tmp/pets.yaml");
    assert_eq!(contract.path, "/pets");
    let mut response = response("");
    response.status = 204;
    assert_eq!(
        contract.validate_response(&response).summary(),
        "Contract matched"
    );
}

#[test]
fn validates_siblings_in_a_reference_chain() {
    let spec = json!({"openapi":"3.1.0","components":{"schemas":{
        "A":{"$ref":"#/components/schemas/B","minLength":3},
        "B":{"type":"string"}
    }}});
    let mut report = ValidationReport::default();
    schema::validate(
        &spec,
        &json!({"$ref":"#/components/schemas/A"}),
        &json!("a"),
        "$",
        0,
        &mut report,
    );
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("minLength"))
    );
}

#[test]
fn nullable_does_not_disable_other_constraints() {
    let mut report = ValidationReport::default();
    schema::validate(
        &json!({"openapi":"3.0.3"}),
        &json!({"type":"string","nullable":true,"enum":["a"]}),
        &Value::Null,
        "$",
        0,
        &mut report,
    );
    assert!(report.errors.iter().any(|error| error.contains("enum")));
}

#[test]
fn workspace_and_project_disk_keep_contract_and_new_tabs() {
    use crate::project::{ProjectDisk, ProjectDocument, ProjectRequest};
    use crate::workspace_state::Workspace;
    let imported =
        crate::import::parse_import(&contract().spec.to_string(), "/tmp/pets.json").unwrap();
    let mut workspace = Workspace::new();
    workspace.import(&imported);
    let session = workspace
        .sessions
        .iter_mut()
        .find(|session| session.draft.openapi_contract.is_some())
        .unwrap();
    session.view.request_tab = "contract".into();
    session.view.response_tab = "request".into();
    let id = session.id;
    let expected = session.draft.openapi_contract.clone();
    let restored = Workspace::decode(&workspace.encode()).unwrap();
    let session = restored.session(id).unwrap();
    assert_eq!(session.draft.openapi_contract, expected);
    assert_eq!(session.view.request_tab, "contract");
    assert_eq!(session.view.response_tab, "request");
    let document = ProjectDocument {
        root_id: restored.groups[0].id,
        groups: restored.groups.clone(),
        requests: restored
            .sessions
            .iter()
            .filter_map(|session| {
                Some(ProjectRequest {
                    id: session.id,
                    group_id: session.group_id?,
                    draft: session.draft.clone(),
                })
            })
            .collect(),
    };
    let dir = tempfile::tempdir().unwrap();
    ProjectDisk::create(dir.path(), &document).unwrap();
    let (_, loaded) = ProjectDisk::open(dir.path()).unwrap();
    assert_eq!(
        loaded
            .requests
            .iter()
            .find(|request| request.id == id)
            .unwrap()
            .draft
            .openapi_contract,
        expected
    );
}

#[test]
fn schema_work_limit_bounds_recursive_compositions() {
    let spec = json!({"openapi":"3.1.0","components":{"schemas":{"Loop":{"anyOf":[{"$ref":"#/components/schemas/Loop"},{"$ref":"#/components/schemas/Loop"}]}}}});
    let mut report = ValidationReport::default();
    schema::validate(
        &spec,
        &json!({"$ref":"#/components/schemas/Loop"}),
        &json!(1),
        "$",
        0,
        &mut report,
    );
    assert_eq!(report.summary(), "Validation incomplete");
    assert!(
        report
            .unsupported
            .iter()
            .any(|message| message.contains("work limit"))
    );
}

#[test]
fn full_import_path_does_not_become_http_collection_title() {
    let imported =
        crate::import::parse_import("GET https://example.com", "/tmp/requests.http").unwrap();
    assert_eq!(imported.root.name, "requests");
}

#[test]
fn imported_contract_keeps_only_operation_and_reference_closure() {
    let mut spec = contract().spec;
    spec["paths"]["/other"] = json!({"get":{"responses":{"200":{"description":"Unrelated"}}}});
    spec["paths"]["/pets/{id}"]["delete"] =
        json!({"responses":{"204":{"description":"Unrelated method"}}});
    spec["components"]["schemas"]["Unused"] =
        json!({"type":"string","description":"Unrelated component"});
    let retained = OpenApiContract::new("/tmp/spec.json", &spec, "/pets/{id}", "POST");
    assert!(retained.spec["paths"].get("/other").is_none());
    assert!(retained.spec["paths"]["/pets/{id}"].get("delete").is_none());
    assert!(
        retained.spec["components"]["schemas"]
            .get("Unused")
            .is_none()
    );
    assert_eq!(
        retained
            .validate_response(&response(r#"{"name":"Milo"}"#))
            .summary(),
        "Contract matched"
    );
    assert!(retained.request_example().is_ok());
}

#[test]
fn response_header_content_is_explicitly_incomplete() {
    let mut contract = contract();
    contract.spec["paths"]["/pets/{id}"]["post"]["responses"]["200"]["headers"] =
        json!({"X-Meta":{"content":{"application/json":{"schema":{"type":"integer"}}}}});
    let mut response = response(r#"{"name":"Milo"}"#);
    response.headers.push(Header {
        key: "X-Meta".into(),
        value: "not an integer".into(),
    });
    assert_eq!(
        contract.validate_response(&response).summary(),
        "Validation incomplete"
    );
}

#[test]
fn read_only_and_write_only_required_properties_use_body_direction() {
    let mut contract = contract();
    contract.spec["components"]["schemas"]["Pet"] = json!({"type":"object","required":["id","password"],"properties":{
        "id":{"type":"string","readOnly":true},"password":{"type":"string","writeOnly":true}
    }});
    let mut request = request();
    request.body = Some(r#"{"password":"example"}"#.into());
    assert_eq!(
        contract.validate_request(&request).summary(),
        "Contract matched"
    );
    assert_eq!(
        contract
            .validate_response(&response(r#"{"id":"abc"}"#))
            .summary(),
        "Contract matched"
    );
    assert!(
        contract
            .validate_response(&response("{}"))
            .errors
            .iter()
            .any(|error| error.contains("$.id"))
    );
    request.body = Some("{}".into());
    assert!(
        contract
            .validate_request(&request)
            .errors
            .iter()
            .any(|error| error.contains("$.password"))
    );
}
