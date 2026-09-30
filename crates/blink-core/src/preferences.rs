//! Workspace preferences and new request defaults. Port of `src/lib/preferences.ts`.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::codegen::{code_target_from_id, is_code_target};
use crate::model::{
    BodyMode, PaneLayout, RequestGroup, RequestSession, TransportOptions, WorkspacePreferences,
};
use crate::request::is_method;
use crate::transport_options::{
    TransportField, default_transport_options, proxy_url_error, range_errors,
};

/// Side by side, or request above response.
pub const PANE_LAYOUTS: [PaneLayout; 2] = [PaneLayout::Horizontal, PaneLayout::Vertical];

pub fn pane_layout_id(layout: PaneLayout) -> &'static str {
    match layout {
        PaneLayout::Horizontal => "horizontal",
        PaneLayout::Vertical => "vertical",
    }
}

fn pane_layout_from_id(id: &str) -> Option<PaneLayout> {
    PANE_LAYOUTS
        .into_iter()
        .find(|layout| pane_layout_id(*layout) == id)
}

/// Interface zoom steps, as VS Code uses.
pub const ZOOM_LEVELS: [f64; 11] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

pub fn is_zoom(value: f64) -> bool {
    ZOOM_LEVELS.contains(&value)
}

/// The next zoom step up (`1`) or down (`-1`).
pub fn step_zoom(zoom: f64, direction: i32) -> f64 {
    let one = ZOOM_LEVELS
        .iter()
        .position(|level| *level == 1.0)
        .unwrap_or(0);
    let index = ZOOM_LEVELS
        .iter()
        .position(|level| *level == zoom)
        .unwrap_or(one);
    let next = (index as i64 + direction as i64).clamp(0, ZOOM_LEVELS.len() as i64 - 1);
    ZOOM_LEVELS[next as usize]
}

pub fn default_preferences() -> WorkspacePreferences {
    WorkspacePreferences::default()
}

/// The saved keys of the transport fields, in `TransportOptions` order.
const TRANSPORT_KEYS: [&str; 8] = [
    "timeoutSeconds",
    "connectTimeoutSeconds",
    "followRedirects",
    "maxRedirects",
    "inspectionLimitMiB",
    "verifyTls",
    "proxyUrl",
    "storeCookies",
];

fn is_body_mode(value: &Value) -> bool {
    value.as_str().and_then(BodyMode::from_id).is_some()
}

fn is_bool(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Bool(_)))
}

/// Strict check: every field present, of the right type, and in range.
pub fn valid_preferences(value: &Value) -> bool {
    let Value::Object(p) = value else {
        return false;
    };
    let text = |key: &str| p.get(key).and_then(Value::as_str);
    text("defaultMethod").is_some_and(is_method)
        && p.get("defaultBodyMode").is_some_and(is_body_mode)
        && is_bool(p.get("pretty"))
        && is_bool(p.get("wrap"))
        && is_bool(p.get("confirmCloseDrafts"))
        && text("paneLayout").and_then(pane_layout_from_id).is_some()
        && text("codeTarget").is_some_and(is_code_target)
        && p.get("zoom").and_then(Value::as_f64).is_some_and(is_zoom)
        && is_bool(p.get("followRedirects"))
        && is_bool(p.get("verifyTls"))
        && is_bool(p.get("storeCookies"))
        && text("proxyUrl").is_some_and(|url| proxy_url_error(url).is_empty())
        && range_errors(|field| {
            p.get(field.key())
                .and_then(Value::as_f64)
                .unwrap_or(f64::NAN)
        })
        .is_empty()
}

/// Fill fields added after a workspace was saved with their defaults. Returns
/// None for a wrong type or an out-of-range value. Unknown keys are dropped.
pub fn normalize_preferences(value: &Value) -> Option<WorkspacePreferences> {
    let Value::Object(input) = value else {
        return None;
    };

    // Check that all non-transport fields are present and valid
    let text = |key: &str| input.get(key).and_then(Value::as_str);
    if !text("defaultMethod").is_some_and(is_method)
        || !input.get("defaultBodyMode").is_some_and(is_body_mode)
        || !is_bool(input.get("pretty"))
        || !is_bool(input.get("wrap"))
        || !is_bool(input.get("confirmCloseDrafts"))
    {
        return None;
    }

    // `??`: absent or null takes the default.
    let or = |key: &str, default: Value| match input.get(key) {
        None | Some(Value::Null) => default,
        Some(value) => value.clone(),
    };
    let mut merged = Map::new();
    for key in [
        "defaultMethod",
        "defaultBodyMode",
        "pretty",
        "wrap",
        "confirmCloseDrafts",
    ] {
        merged.insert(key.into(), input[key].clone());
    }
    // Added after v4 shipped: older workspaces open side by side.
    merged.insert("paneLayout".into(), or("paneLayout", "horizontal".into()));
    merged.insert("codeTarget".into(), or("codeTarget", "curl".into()));
    merged.insert("zoom".into(), or("zoom", 1.into()));

    // Add transport fields (use provided values, or defaults)
    let defaults = serde_json::to_value(default_transport_options()).ok()?;
    for key in TRANSPORT_KEYS {
        let value = input.get(key).unwrap_or(&defaults[key]).clone();
        merged.insert(key.into(), value);
    }

    // Validate the complete object
    let merged = Value::Object(merged);
    if !valid_preferences(&merged) {
        return None;
    }
    let number = |key: &str| merged[key].as_f64().unwrap_or_default() as u64;
    let flag = |key: &str| merged[key].as_bool().unwrap_or_default();
    Some(WorkspacePreferences {
        default_method: merged["defaultMethod"].as_str()?.to_string(),
        default_body_mode: BodyMode::from_id(merged["defaultBodyMode"].as_str()?)?,
        pretty: flag("pretty"),
        wrap: flag("wrap"),
        confirm_close_drafts: flag("confirmCloseDrafts"),
        pane_layout: pane_layout_from_id(merged["paneLayout"].as_str()?)?,
        code_target: code_target_from_id(merged["codeTarget"].as_str()?)?,
        zoom: merged["zoom"].as_f64()?,
        transport: TransportOptions {
            timeout_seconds: number(TransportField::TimeoutSeconds.key()),
            connect_timeout_seconds: number(TransportField::ConnectTimeoutSeconds.key()),
            follow_redirects: flag("followRedirects"),
            max_redirects: number(TransportField::MaxRedirects.key()),
            inspection_limit_mi_b: number(TransportField::InspectionLimitMiB.key()),
            verify_tls: flag("verifyTls"),
            proxy_url: merged["proxyUrl"].as_str()?.to_string(),
            store_cookies: flag("storeCookies"),
        },
    })
}

pub fn transport_options(preferences: &WorkspacePreferences) -> TransportOptions {
    preferences.transport.clone()
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewRequestDefaults {
    pub method: String,
    pub body_mode: BodyMode,
    pub url: String,
    pub pretty: bool,
    pub wrap: bool,
}

pub fn resolve_new_request_defaults(
    groups: &[RequestGroup],
    group_id: Option<u64>,
    preferences: &WorkspacePreferences,
) -> NewRequestDefaults {
    let by_id: HashMap<u64, &RequestGroup> = groups.iter().map(|group| (group.id, group)).collect();
    let mut group = group_id.and_then(|id| by_id.get(&id).copied());
    let mut method: Option<&String> = None;
    let mut url: Option<&String> = None;
    let mut seen = HashSet::new();
    while let Some(current) = group {
        if !seen.insert(current.id) {
            break;
        }
        if method.is_none() {
            method = current.default_method.as_ref();
        }
        if url.is_none() {
            url = current.default_url.as_ref();
        }
        group = current.parent_id.and_then(|id| by_id.get(&id).copied());
    }
    NewRequestDefaults {
        method: method.unwrap_or(&preferences.default_method).clone(),
        body_mode: preferences.default_body_mode,
        url: url.cloned().unwrap_or_default(),
        pretty: preferences.pretty,
        wrap: preferences.wrap,
    }
}

pub fn apply_new_request_defaults(session: &mut RequestSession, defaults: &NewRequestDefaults) {
    session.draft.method = defaults.method.clone();
    session.draft.body_mode = defaults.body_mode;
    session.draft.url = defaults.url.clone();
    session.view.pretty = defaults.pretty;
    session.view.wrap = defaults.wrap;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Definitions;
    use crate::request::create_draft;
    use crate::session::create_session;
    use crate::workspace::{decode_workspace, encode_workspace};
    use serde_json::json;

    fn group(id: u64, name: &str, parent_id: Option<u64>) -> RequestGroup {
        RequestGroup {
            id,
            name: name.into(),
            parent_id,
            collapsed: false,
            local_auth: None,
            local_definitions: None,
            default_method: None,
            default_url: None,
            environments: None,
            active_environment_id: None,
        }
    }

    fn groups() -> Vec<RequestGroup> {
        vec![
            RequestGroup {
                default_method: Some("POST".into()),
                default_url: Some("{{host}}/v1".into()),
                ..group(1, "Root", None)
            },
            RequestGroup {
                default_method: Some("PATCH".into()),
                ..group(2, "Child", Some(1))
            },
            group(3, "Leaf", Some(2)),
        ]
    }

    fn defaults_json() -> Value {
        serde_json::to_value(default_preferences()).unwrap()
    }

    fn with(changes: Value) -> Value {
        let mut value = defaults_json();
        for (key, item) in changes.as_object().unwrap() {
            value[key] = item.clone();
        }
        value
    }

    fn encode_one(session: &RequestSession, groups: &[RequestGroup]) -> String {
        encode_workspace(
            std::slice::from_ref(session),
            Some(session.id),
            groups,
            &Definitions::new(),
            &default_preferences(),
            &[session.id],
        )
    }

    // new request preferences

    #[test]
    fn resolves_each_field_from_the_nearest_group_independently() {
        let preferences = WorkspacePreferences {
            default_method: "PUT".into(),
            wrap: true,
            ..default_preferences()
        };
        assert_eq!(
            resolve_new_request_defaults(&groups(), Some(3), &preferences),
            NewRequestDefaults {
                method: "PATCH".into(),
                url: "{{host}}/v1".into(),
                body_mode: BodyMode::None,
                pretty: true,
                wrap: true,
            }
        );
    }

    #[test]
    fn falls_back_to_app_defaults_and_preserves_an_explicit_empty_group_url() {
        let preferences = WorkspacePreferences {
            default_method: "DELETE".into(),
            default_body_mode: BodyMode::Text,
            pretty: false,
            ..default_preferences()
        };
        assert_eq!(
            resolve_new_request_defaults(&[], None, &preferences),
            NewRequestDefaults {
                method: "DELETE".into(),
                body_mode: BodyMode::Text,
                url: String::new(),
                pretty: false,
                wrap: false,
            }
        );
        let mut all = groups();
        all.push(RequestGroup {
            default_url: Some(String::new()),
            ..group(4, "Blank", Some(3))
        });
        assert_eq!(
            resolve_new_request_defaults(&all, Some(4), &preferences).url,
            ""
        );
    }

    #[test]
    fn terminates_malformed_ancestry_without_overwriting_the_nearest_defaults() {
        let base = groups();
        let malformed = [
            RequestGroup {
                parent_id: Some(2),
                ..base[0].clone()
            },
            base[1].clone(),
        ];
        assert_eq!(
            resolve_new_request_defaults(&malformed, Some(2), &default_preferences()).method,
            "PATCH"
        );
    }

    #[test]
    fn round_trips_preferences_and_group_defaults_without_touching_existing_requests() {
        let mut session = create_session(None);
        session.group_id = Some(3);
        session.draft.url = "https://example.test/keep".into();
        let preferences = WorkspacePreferences {
            default_method: "PUT".into(),
            default_body_mode: BodyMode::Json,
            pretty: false,
            wrap: true,
            confirm_close_drafts: false,
            ..default_preferences()
        };
        let globals: Definitions =
            [("host".to_string(), "https://example.test".to_string())].into();
        let decoded = decode_workspace(&encode_workspace(
            std::slice::from_ref(&session),
            Some(session.id),
            &groups(),
            &globals,
            &preferences,
            &[session.id],
        ))
        .unwrap();
        assert_eq!(decoded.preferences, preferences);
        assert_eq!(decoded.groups, groups());
        assert_eq!(decoded.sessions[0].draft, session.draft);
        assert_eq!(decoded.sessions[0].view, session.view);
    }

    #[test]
    fn loads_version_workspaces_without_preferences() {
        for version in [1, 2, 3] {
            let session = create_session(None);
            let mut raw: Value = serde_json::from_str(&encode_one(&session, &[])).unwrap();
            raw["version"] = version.into();
            raw.as_object_mut().unwrap().remove("preferences");
            let decoded = decode_workspace(&raw.to_string()).unwrap();
            assert_eq!(decoded.preferences, default_preferences());
        }
    }

    #[test]
    fn rejects_malformed_preferences() {
        for preferences in [
            Value::Null,
            json!([]),
            json!({}),
            with(json!({ "defaultMethod": "BAD METHOD" })),
            with(json!({ "defaultBodyMode": "xml" })),
            with(json!({ "pretty": "true" })),
            with(json!({ "wrap": 1 })),
            with(json!({ "confirmCloseDrafts": null })),
        ] {
            assert!(!valid_preferences(&preferences), "{preferences}");
            let session = create_session(None);
            let mut raw: Value = serde_json::from_str(&encode_one(&session, &[])).unwrap();
            raw["preferences"] = preferences;
            assert!(decode_workspace(&raw.to_string()).is_err());
        }
    }

    #[test]
    fn rejects_malformed_group_defaults() {
        for changes in [
            json!({ "defaultMethod": "BAD METHOD" }),
            json!({ "defaultMethod": null }),
            json!({ "defaultUrl": false }),
            json!({ "defaultUrl": null }),
            json!({ "defaultUrl": "x".repeat(65537) }),
        ] {
            let session = create_session(None);
            let mut raw: Value = serde_json::from_str(&encode_one(&session, &groups())).unwrap();
            for (key, value) in changes.as_object().unwrap() {
                raw["groups"][0][key] = value.clone();
            }
            assert!(decode_workspace(&raw.to_string()).is_err(), "{changes}");
        }
    }

    #[test]
    fn uses_the_nearest_group_defaults_without_changing_an_existing_draft() {
        let draft = create_draft();
        let groups = [RequestGroup {
            default_method: Some("POST".into()),
            default_url: Some("https://api.test".into()),
            ..group(1, "API", None)
        }];
        let defaults = resolve_new_request_defaults(&groups, Some(1), &default_preferences());
        let mut session = create_session(Some(&draft));
        apply_new_request_defaults(&mut session, &defaults);
        assert_eq!(session.draft.method, "POST");
        assert_eq!(session.draft.url, "https://api.test");
        assert_eq!((draft.method.as_str(), draft.url.as_str()), ("GET", ""));
    }

    // transport preferences

    #[test]
    fn fills_missing_transport_fields_from_defaults() {
        assert_eq!(
            normalize_preferences(&json!({
                "defaultMethod": "POST",
                "defaultBodyMode": "json",
                "pretty": false,
                "wrap": true,
                "confirmCloseDrafts": false,
            })),
            Some(WorkspacePreferences {
                default_method: "POST".into(),
                default_body_mode: BodyMode::Json,
                pretty: false,
                wrap: true,
                confirm_close_drafts: false,
                ..default_preferences()
            })
        );
    }

    #[test]
    fn rejects_wrong_types_and_out_of_range_values() {
        assert_eq!(normalize_preferences(&Value::Null), None);
        assert_eq!(normalize_preferences(&json!([])), None);
        assert_eq!(
            normalize_preferences(&with(json!({ "timeoutSeconds": "30" }))),
            None
        );
        assert_eq!(
            normalize_preferences(&with(json!({ "maxRedirects": 21 }))),
            None
        );
        assert_eq!(
            normalize_preferences(&with(json!({ "followRedirects": 1 }))),
            None
        );
    }

    #[test]
    fn drops_unknown_keys() {
        assert_eq!(
            normalize_preferences(&with(json!({ "extra": true }))),
            Some(default_preferences())
        );
    }

    #[test]
    fn strict_validation_needs_every_field() {
        let mut partial = defaults_json();
        partial.as_object_mut().unwrap().remove("timeoutSeconds");
        assert!(!valid_preferences(&partial));
        assert!(valid_preferences(&defaults_json()));
    }

    #[test]
    fn extracts_transport_options() {
        assert_eq!(
            transport_options(&default_preferences()),
            TransportOptions {
                timeout_seconds: 30,
                connect_timeout_seconds: 10,
                follow_redirects: false,
                max_redirects: 10,
                inspection_limit_mi_b: 4,
                verify_tls: true,
                proxy_url: String::new(),
                store_cookies: true,
            }
        );
    }

    #[test]
    fn opens_older_workspaces_side_by_side_and_rejects_unknown_layouts() {
        let mut saved = defaults_json();
        saved.as_object_mut().unwrap().remove("paneLayout");
        assert_eq!(
            normalize_preferences(&saved).map(|p| p.pane_layout),
            Some(PaneLayout::Horizontal)
        );
        assert_eq!(
            normalize_preferences(&with(json!({ "paneLayout": "vertical" })))
                .map(|p| p.pane_layout),
            Some(PaneLayout::Vertical)
        );
        assert_eq!(
            normalize_preferences(&with(json!({ "paneLayout": "diagonal" }))),
            None
        );
    }

    #[test]
    fn rejects_a_proxy_url_the_desktop_transport_cannot_use() {
        assert!(!valid_preferences(&with(
            json!({ "proxyUrl": "ftp://proxy" })
        )));
        assert!(valid_preferences(&with(
            json!({ "proxyUrl": "socks5://127.0.0.1:1080" })
        )));
    }

    #[test]
    fn loads_a_v4_workspace_saved_before_the_transport_fields() {
        let session = create_session(Some(&create_draft()));
        let mut encoded: Value = serde_json::from_str(&encode_one(&session, &[])).unwrap();
        for key in [
            "timeoutSeconds",
            "connectTimeoutSeconds",
            "followRedirects",
            "maxRedirects",
            "inspectionLimitMiB",
            "verifyTls",
            "proxyUrl",
        ] {
            encoded["preferences"].as_object_mut().unwrap().remove(key);
        }
        assert_eq!(
            decode_workspace(&encoded.to_string()).unwrap().preferences,
            default_preferences()
        );
    }

    #[test]
    fn steps_zoom_levels() {
        assert_eq!(step_zoom(1.0, 1), 1.1);
        assert_eq!(step_zoom(0.5, -1), 0.5);
        assert_eq!(step_zoom(3.0, -1), 0.9);
        assert_eq!(step_zoom(2.0, 1), 2.0);
    }
}
