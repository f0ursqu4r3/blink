//! Saved workspace snapshots. Port of `src/lib/workspace.ts`.
//!
//! Reads versions 1 to 4 and writes version 4. Validation runs on the raw
//! JSON with the same rules as the TS app, then the typed model is read.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value, json};

use crate::authorization::build_resolved_request_context;
use crate::checks::reserve_check_id;
use crate::environments::{MAX_ENVIRONMENT_NAME, MAX_ENVIRONMENTS, is_environment_color};
use crate::history::HISTORY_LIMIT;
use crate::ids;
use crate::model::{
    ApiResponse, AuthKind, AuthorizationConfig, BodyMode, CheckOperator, CheckSource, Definitions,
    Draft, HistoryEntry, RequestGroup, RequestSession, RequestView, WorkspacePreferences,
};
use crate::preferences::{default_preferences, normalize_preferences};
use crate::request::{build_request, is_method, js_trim};
use crate::session::{request_fingerprint, reserve_session_id};

pub const WORKSPACE_KEY: &str = "blink.workspace.v1";
pub const MAX_STATE_BYTES: usize = 64 * 1024 * 1024;
/// Sanity bounds for a snapshot. The byte limit is the real cap.
pub const MAX_REQUESTS: usize = 10_000;
pub const MAX_GROUPS: usize = 10_000;

const INVALID: &str =
    "Saved workspace is invalid or from an unsupported version. It has not been changed.";
const TOO_LARGE: &str =
    "Workspace exceeds the 64 MiB save limit. Delete unused requests before saving.";
const INTERRUPTED: &str = "Request interrupted when Blink closed. Send again to retry.";

type Checked<T = ()> = Result<T, String>;

fn invalid() -> String {
    INVALID.into()
}

fn record(value: &Value) -> Checked<&Map<String, Value>> {
    value.as_object().ok_or_else(invalid)
}

fn check(ok: bool) -> Checked {
    if ok { Ok(()) } else { Err(invalid()) }
}

fn text(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::String(_)))
}

fn boolean(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Bool(_)))
}

/// A finite number, zero or more.
fn numeric(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_f64)
        .is_some_and(|n| n.is_finite() && n >= 0.0)
}

fn integer(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && n.fract() == 0.0)
}

/// A positive safe integer below one billion.
fn id(value: Option<&Value>) -> Option<u64> {
    integer(value)
        .filter(|n| *n > 0.0 && *n < 1_000_000_000.0)
        .map(|n| n as u64)
}

fn is_id(value: Option<&Value>) -> bool {
    id(value).is_some()
}

fn status(value: Option<&Value>) -> bool {
    integer(value).is_some_and(|n| (100.0..=599.0).contains(&n))
}

/// Absent, or passes `test`. A present `null` is not absent.
fn optional(value: Option<&Value>, test: impl Fn(Option<&Value>) -> bool) -> bool {
    value.is_none() || test(value)
}

fn array(value: Option<&Value>, max: usize) -> Checked<&Vec<Value>> {
    match value {
        Some(Value::Array(items)) if items.len() <= max => Ok(items),
        _ => Err(invalid()),
    }
}

/// String length as JavaScript counts it, in UTF-16 units.
fn js_len(value: &str) -> usize {
    value.encode_utf16().count()
}

fn str_of(value: Option<&Value>) -> &str {
    value.and_then(Value::as_str).unwrap_or_default()
}

fn validate_authorization_config(value: &Value) -> bool {
    let Value::Object(obj) = value else {
        return false;
    };
    match obj.get("type").and_then(Value::as_str) {
        Some("none") => true,
        Some("bearer") => text(obj.get("token")),
        Some("basic") => text(obj.get("username")) && text(obj.get("password")),
        _ => false,
    }
}

/// Max entries in a definitions map (global or local).
const MAX_DEFINITIONS_ENTRIES: usize = 500;
/// Max length for a definition name or value.
const MAX_DEFINITION_KEY_LEN: usize = 256;
const MAX_DEFINITION_VAL_LEN: usize = 65536;

/// Valid definition name: non-empty, no leading `_` for local names.
fn validate_definition_name(name: &str, allow_leading_underscore: bool) -> bool {
    if name.is_empty() || js_len(name) > MAX_DEFINITION_KEY_LEN {
        return false;
    }
    allow_leading_underscore || !name.starts_with('_')
}

fn validate_definitions(value: Option<&Value>, allow_leading_underscore: bool) -> bool {
    let Some(Value::Object(entries)) = value else {
        return false;
    };
    entries.len() <= MAX_DEFINITIONS_ENTRIES
        && entries.iter().all(|(k, v)| {
            validate_definition_name(k, allow_leading_underscore)
                && v.as_str()
                    .is_some_and(|v| js_len(v) <= MAX_DEFINITION_VAL_LEN)
        })
}

fn is_check_source(value: Option<&Value>) -> bool {
    value.is_some_and(|v| serde_json::from_value::<CheckSource>(v.clone()).is_ok())
}

fn is_check_operator(value: Option<&Value>) -> bool {
    value.is_some_and(|v| serde_json::from_value::<CheckOperator>(v.clone()).is_ok())
}

fn validate_draft(input: Option<&Value>, version: u64) -> Checked {
    let draft = record(input.ok_or_else(invalid)?)?;
    check(
        draft
            .get("method")
            .and_then(Value::as_str)
            .is_some_and(is_method),
    )?;
    for key in ["url", "body", "token", "username", "password"] {
        check(text(draft.get(key)))?;
    }
    check(
        draft
            .get("bodyMode")
            .and_then(Value::as_str)
            .and_then(BodyMode::from_id)
            .is_some(),
    )?;
    check(optional(draft.get("variables"), text))?;
    check(optional(draft.get("bodyFile"), text))?;
    check(matches!(
        draft.get("auth").and_then(Value::as_str),
        Some("none" | "bearer" | "basic")
    ))?;
    // v3: validate localAuth if present
    if version >= 3
        && let Some(local_auth) = draft.get("localAuth")
    {
        check(validate_authorization_config(local_auth))?;
    }
    if let Some(assertions) = draft.get("assertions") {
        for row in array(Some(assertions), 500)? {
            let entry = record(row)?;
            check(
                is_id(entry.get("id"))
                    && boolean(entry.get("enabled"))
                    && is_check_source(entry.get("source"))
                    && is_check_operator(entry.get("operator"))
                    && text(entry.get("path"))
                    && text(entry.get("expected")),
            )?;
        }
    }
    if let Some(captures) = draft.get("captures") {
        for row in array(Some(captures), 500)? {
            let entry = record(row)?;
            check(
                is_id(entry.get("id"))
                    && boolean(entry.get("enabled"))
                    && is_check_source(entry.get("source"))
                    && !matches!(str_of(entry.get("source")), "time" | "size")
                    && text(entry.get("name"))
                    && text(entry.get("path")),
            )?;
        }
    }
    for key in ["query", "headers", "form"] {
        if key == "form" && draft.get("form").is_none() {
            continue;
        }
        let mut ids = HashSet::new();
        for row in array(draft.get(key), 10_000)? {
            let entry = record(row)?;
            let row_id = id(entry.get("id"));
            check(
                row_id.is_some_and(|row_id| !ids.contains(&row_id))
                    && text(entry.get("key"))
                    && text(entry.get("value"))
                    && boolean(entry.get("enabled"))
                    && optional(entry.get("file"), boolean),
            )?;
            ids.insert(row_id.unwrap_or_default());
        }
    }
    Ok(())
}

fn validate_response(input: Option<&Value>) -> Checked {
    let input = input.ok_or_else(invalid)?;
    if input.is_null() {
        return Ok(());
    }
    let response = record(input)?;
    check(status(response.get("status")))?;
    check(
        text(response.get("statusText"))
            && text(response.get("body"))
            && numeric(response.get("durationMs"))
            && numeric(response.get("sizeBytes")),
    )?;
    check(
        optional(response.get("bodyId"), text)
            && optional(response.get("truncated"), boolean)
            && optional(response.get("binary"), boolean)
            && optional(response.get("finalUrl"), text)
            && optional(response.get("redirectCount"), numeric),
    )?;
    if let Some(timing) = response.get("timing") {
        validate_timing(timing)?;
    }
    for item in array(response.get("headers"), 10_000)? {
        let header = record(item)?;
        check(text(header.get("key")) && text(header.get("value")))?;
    }
    Ok(())
}

fn validate_history(input: &Value) -> Checked {
    let mut ids = HashSet::new();
    for item in array(Some(input), HISTORY_LIMIT)? {
        let entry = record(item)?;
        let entry_id = id(entry.get("id"));
        check(entry_id.is_some_and(|entry_id| !ids.contains(&entry_id)))?;
        ids.insert(entry_id.unwrap_or_default());
        check(
            numeric(entry.get("sentAt"))
                && entry
                    .get("method")
                    .and_then(Value::as_str)
                    .is_some_and(is_method)
                && text(entry.get("url"))
                && numeric(entry.get("durationMs"))
                && numeric(entry.get("sizeBytes"))
                && text(entry.get("body"))
                && optional(entry.get("error"), text)
                && optional(entry.get("status"), status)
                && optional(entry.get("statusText"), text)
                && optional(entry.get("bodyOmitted"), boolean),
        )?;
        for header in array(entry.get("headers"), 10_000)? {
            let pair = record(header)?;
            check(text(pair.get("key")) && text(pair.get("value")))?;
        }
        if let Some(timing) = entry.get("timing") {
            validate_timing(timing)?;
        }
    }
    Ok(())
}

fn validate_timing(input: &Value) -> Checked {
    let timing = record(input)?;
    check(numeric(timing.get("waitMs")) && numeric(timing.get("downloadMs")))?;
    for key in ["dnsMs", "connectMs", "tlsMs"] {
        check(optional(timing.get(key), numeric))?;
    }
    Ok(())
}

fn validate_group(value: &Value, version: u64) -> Checked<(u64, Option<u64>)> {
    let group = record(value)?;
    let name = group.get("name").and_then(Value::as_str);
    let parent = group.get("parentId");
    check(
        is_id(group.get("id"))
            && name.is_some_and(|name| !js_trim(name).is_empty() && js_len(name) <= 80)
            && (parent == Some(&Value::Null) || is_id(parent))
            && boolean(group.get("collapsed")),
    )?;
    // v3: validate optional localAuth and localDefinitions
    if version >= 3 {
        if let Some(local_auth) = group.get("localAuth") {
            check(validate_authorization_config(local_auth))?;
        }
        if let Some(definitions) = group.get("localDefinitions") {
            check(validate_definitions(Some(definitions), false))?;
        }
        let active = group.get("activeEnvironmentId");
        let no_active = active.is_none() || active == Some(&Value::Null);
        if let Some(environments) = group.get("environments") {
            let mut env_ids = HashSet::new();
            for item in array(Some(environments), MAX_ENVIRONMENTS)? {
                let environment = record(item)?;
                let env_id = id(environment.get("id"));
                let name = environment.get("name").and_then(Value::as_str);
                check(
                    env_id.is_some_and(|env_id| !env_ids.contains(&env_id))
                        && name.is_some_and(|name| {
                            !js_trim(name).is_empty() && js_len(name) <= MAX_ENVIRONMENT_NAME
                        })
                        && environment
                            .get("color")
                            .and_then(Value::as_str)
                            .is_some_and(is_environment_color)
                        && optional(environment.get("protected"), boolean)
                        && validate_definitions(environment.get("values"), false),
                )?;
                env_ids.insert(env_id.unwrap_or_default());
            }
            check(no_active || id(active).is_some_and(|active| env_ids.contains(&active)))?;
        } else {
            check(no_active)?;
        }
        if let Some(method) = group.get("defaultMethod") {
            check(method.as_str().is_some_and(is_method))?;
        }
        if let Some(url) = group.get("defaultUrl") {
            check(url.as_str().is_some_and(|url| js_len(url) <= 65536))?;
        }
    }
    Ok((id(group.get("id")).unwrap_or_default(), id(parent)))
}

fn validate_groups(input: Option<&Value>, version: u64) -> Checked<HashSet<u64>> {
    let groups = array(input, MAX_GROUPS)?
        .iter()
        .map(|value| validate_group(value, version))
        .collect::<Checked<Vec<_>>>()?;
    let by_id: HashMap<u64, Option<u64>> = groups.iter().copied().collect();
    check(by_id.len() == groups.len())?;
    for (group_id, parent_id) in &groups {
        let mut visited = HashSet::from([*group_id]);
        let mut parent_id = *parent_id;
        while let Some(parent) = parent_id {
            check(visited.insert(parent))?;
            parent_id = *by_id.get(&parent).ok_or_else(invalid)?;
        }
    }
    Ok(by_id.into_keys().collect())
}

/// Check a snapshot and return its JSON and version.
fn parse_snapshot(content: &str) -> Checked<(Value, u64)> {
    if content.len() > MAX_STATE_BYTES {
        return Err(TOO_LARGE.into());
    }
    let raw: Value = serde_json::from_str(content).map_err(|_| invalid())?;
    let data = record(&raw)?;
    let version = integer(data.get("version"))
        .filter(|v| (1.0..=4.0).contains(v))
        .ok_or_else(invalid)? as u64;
    let active_id = data.get("activeId");
    check(is_id(active_id) || version >= 4 && active_id == Some(&Value::Null))?;
    let tabs = array(data.get("tabs"), MAX_REQUESTS)?;
    check(!tabs.is_empty())?;
    let mut ids = HashSet::new();
    for item in tabs {
        let tab = record(item)?;
        let tab_id = id(tab.get("id"));
        check(tab_id.is_some_and(|tab_id| !ids.contains(&tab_id)))?;
        ids.insert(tab_id.unwrap_or_default());
        validate_draft(tab.get("draft"), version)?;
        validate_response(tab.get("response"))?;
        if let Some(history) = tab.get("history") {
            validate_history(history)?;
        }
        let group_id = tab.get("groupId");
        check(group_id.is_none() || group_id == Some(&Value::Null) || is_id(group_id))?;
        check(
            text(tab.get("error"))
                && text(tab.get("sentFingerprint"))
                && boolean(tab.get("interrupted")),
        )?;
        let view = record(tab.get("view").ok_or_else(invalid)?)?;
        check(matches!(
            str_of(view.get("requestTab")),
            "query" | "headers" | "body" | "auth" | "tests"
        ))?;
        check(matches!(
            str_of(view.get("responseTab")),
            "body" | "headers" | "tests" | "events"
        ))?;
        check(matches!(
            view.get("jsonView").map(|value| str_of(Some(value))),
            None | Some("tree" | "formatted")
        ))?;
        check(
            boolean(view.get("pretty"))
                && boolean(view.get("wrap"))
                && numeric(view.get("responseScroll")),
        )?;
    }
    // Set membership as JavaScript sees it: only numbers can match an id.
    let member = |value: &Value| integer(Some(value)).is_some_and(|n| ids.contains(&(n as u64)));
    if version >= 4 {
        let open_ids = array(data.get("openIds"), MAX_REQUESTS)?;
        let unique: HashSet<u64> = open_ids.iter().filter_map(|v| id(Some(v))).collect();
        check(open_ids.iter().all(member) && unique.len() == open_ids.len())?;
        check(match active_id {
            Some(Value::Null) => open_ids.is_empty(),
            Some(active) => open_ids
                .iter()
                .any(|open| integer(Some(open)) == integer(Some(active))),
            None => false,
        })?;
    } else {
        check(active_id.is_some_and(member))?;
    }
    if version >= 2 {
        let group_ids = validate_groups(data.get("groups"), version)?;
        for item in tabs {
            let group_id = item.get("groupId");
            check(
                group_id == Some(&Value::Null)
                    || id(group_id).is_some_and(|group_id| group_ids.contains(&group_id)),
            )?;
        }
    }
    if version >= 3 {
        // globalDefinitions is required in v3 and must be a flat string map
        check(validate_definitions(data.get("globalDefinitions"), true))?;
        if let Some(preferences) = data.get("preferences") {
            check(normalize_preferences(preferences).is_some())?;
        }
    }
    Ok((raw, version))
}

/// Migrate flat draft auth fields (v1/v2) to structured localAuth.
/// V1/V2 auth was always explicit (there was no inheritance), so we always
/// produce an explicit localAuth.
fn migrate_draft_auth(draft: &Draft) -> AuthorizationConfig {
    match draft.auth {
        AuthKind::Bearer => AuthorizationConfig::Bearer {
            token: draft.token.clone(),
        },
        AuthKind::Basic => AuthorizationConfig::Basic {
            username: draft.username.clone(),
            password: draft.password.clone(),
        },
        AuthKind::None => AuthorizationConfig::None,
    }
}

/// Write integral floats as integers, as `JSON.stringify` does.
fn js_numbers(value: &mut Value) {
    match value {
        Value::Number(number) => {
            if let Some(n) = number.as_f64()
                && number.is_f64()
                && n.fract() == 0.0
                && n.abs() < 9_007_199_254_740_992.0
            {
                *value = json!(n as i64);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(js_numbers),
        Value::Object(fields) => fields.values_mut().for_each(js_numbers),
        _ => {}
    }
}

fn to_value(value: &impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

pub fn encode_workspace(
    sessions: &[RequestSession],
    active_id: Option<u64>,
    groups: &[RequestGroup],
    global_definitions: &Definitions,
    preferences: &WorkspacePreferences,
    open_ids: &[u64],
) -> String {
    let tabs: Vec<Value> = sessions
        .iter()
        .map(|session| {
            // The body file does not survive a restart. A truncated or binary
            // preview is dropped too: keeping every preview up to the
            // inspection limit could blow past the workspace save limit.
            let response = session.response.as_ref().map(|response| {
                let mut value = to_value(response);
                if response.is_truncated() || response.is_binary() {
                    value["body"] = "".into();
                }
                if let Value::Object(fields) = &mut value {
                    fields.remove("bodyId");
                }
                value
            });
            let mut tab = Map::new();
            tab.insert("id".into(), json!(session.id));
            tab.insert("groupId".into(), json!(session.group_id));
            tab.insert("draft".into(), to_value(&session.draft));
            tab.insert("response".into(), response.unwrap_or(Value::Null));
            tab.insert("error".into(), json!(session.error));
            tab.insert("sentFingerprint".into(), json!(session.sent_fingerprint));
            tab.insert("view".into(), to_value(&session.view));
            if !session.history.is_empty() {
                tab.insert("history".into(), to_value(&session.history));
            }
            tab.insert("interrupted".into(), json!(session.busy));
            Value::Object(tab)
        })
        .collect();
    let mut snapshot = json!({
        "version": 4,
        "activeId": active_id,
        "openIds": open_ids,
        "groups": to_value(&groups),
        "globalDefinitions": to_value(global_definitions),
        "preferences": to_value(preferences),
        "tabs": tabs,
    });
    js_numbers(&mut snapshot);
    snapshot.to_string()
}

/// Check a snapshot before it is written.
pub fn validate_workspace(content: &str) -> Result<(), String> {
    parse_snapshot(content).map(|_| ())
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedWorkspace {
    pub sessions: Vec<RequestSession>,
    pub active_id: Option<u64>,
    pub open_ids: Vec<u64>,
    pub groups: Vec<RequestGroup>,
    pub global_definitions: Definitions,
    pub preferences: WorkspacePreferences,
}

fn read<T: serde::de::DeserializeOwned>(value: Value) -> Checked<T> {
    serde_json::from_value(value).map_err(|_| invalid())
}

fn read_session(tab: &Value, version: u64) -> Checked<RequestSession> {
    let tab = record(tab)?;
    let mut draft_value = tab["draft"].clone();
    if version < 3
        && let Value::Object(fields) = &mut draft_value
    {
        // Replaced by the migrated flat auth below.
        fields.remove("localAuth");
    }
    let mut draft: Draft = read(draft_value)?;
    // Migrate v1/v2 flat auth to localAuth
    if version < 3 {
        draft.local_auth = Some(migrate_draft_auth(&draft));
    }
    let mut response: Option<ApiResponse> = read(tab["response"].clone())?;
    if let Some(response) = &mut response {
        response.body_id = None;
    }
    let history: Vec<HistoryEntry> = match tab.get("history") {
        Some(history) => read(history.clone())?,
        None => Vec::new(),
    };
    let interrupted = tab["interrupted"].as_bool().unwrap_or_default();
    Ok(RequestSession {
        id: id(tab.get("id")).ok_or_else(invalid)?,
        group_id: id(tab.get("groupId")),
        draft,
        response,
        busy: false,
        error: if interrupted {
            INTERRUPTED.into()
        } else {
            str_of(tab.get("error")).into()
        },
        elapsed: 0.0,
        sent_fingerprint: str_of(tab.get("sentFingerprint")).into(),
        view: read::<RequestView>(tab["view"].clone())?,
        history,
        test_results: None,
        capture_errors: None,
        stream: None,
        socket: None,
        stale: false,
    })
}

pub fn decode_workspace(content: &str) -> Result<DecodedWorkspace, String> {
    let (mut data, version) = parse_snapshot(content)?;
    js_numbers(&mut data);
    let sessions = data["tabs"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(|tab| read_session(tab, version))
        .collect::<Checked<Vec<_>>>()?;
    let groups: Vec<RequestGroup> = if version >= 2 {
        read(data["groups"].clone())?
    } else {
        Vec::new()
    };
    let global_definitions: Definitions = if version >= 3 {
        read(data["globalDefinitions"].clone())?
    } else {
        Definitions::new()
    };
    let mut sessions = sessions;
    for session in &sessions {
        reserve_session_id(session.id);
        let draft = &session.draft;
        for row in draft
            .query
            .iter()
            .chain(&draft.headers)
            .chain(draft.form.iter().flatten())
        {
            ids::PAIRS.reserve(row.id);
        }
        let assertions = draft.assertions.iter().flatten().map(|row| row.id);
        let captures = draft.captures.iter().flatten().map(|row| row.id);
        assertions.chain(captures).for_each(reserve_check_id);
    }
    for group in &groups {
        ids::GROUPS.reserve(group.id);
        for environment in group.environments.iter().flatten() {
            ids::ENVIRONMENTS.reserve(environment.id);
        }
    }
    if version < 3 {
        for session in sessions.iter_mut().filter(|s| s.response.is_some()) {
            let context = build_resolved_request_context(
                &session.draft,
                session.group_id,
                &groups,
                &global_definitions,
            );
            session.sent_fingerprint = match build_request(&session.draft, Some((&context).into()))
            {
                Ok(request) => request_fingerprint(Some(&request), Some(auth_type(&context.auth))),
                Err(_) => String::new(),
            };
        }
    }
    let preferences = match data.get("preferences") {
        Some(saved) if version >= 3 => {
            normalize_preferences(saved).unwrap_or_else(default_preferences)
        }
        _ => default_preferences(),
    };
    // Before v4 every request was an open tab.
    let open_ids = if version >= 4 {
        data["openIds"]
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .map(|value| id(Some(value)).ok_or_else(invalid))
            .collect::<Checked<Vec<_>>>()?
    } else {
        sessions.iter().map(|session| session.id).collect()
    };
    Ok(DecodedWorkspace {
        sessions,
        active_id: id(data.get("activeId")),
        open_ids,
        groups,
        global_definitions,
        preferences,
    })
}

/// The `type` tag of an auth config.
pub fn auth_type(auth: &AuthorizationConfig) -> &'static str {
    match auth {
        AuthorizationConfig::None => "none",
        AuthorizationConfig::Bearer { .. } => "bearer",
        AuthorizationConfig::Basic { .. } => "basic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Header, HistoryEntry, ResponseTiming};
    use crate::request::build_request;
    use crate::session::{create_session, draft_fingerprint};

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

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

    /// `encodeWorkspace` with its TS defaults after `activeId`.
    fn encode(sessions: &[RequestSession], active_id: Option<u64>) -> String {
        encode_with(sessions, active_id, &[], &Definitions::new())
    }

    fn encode_with(
        sessions: &[RequestSession],
        active_id: Option<u64>,
        groups: &[RequestGroup],
        globals: &Definitions,
    ) -> String {
        let open: Vec<u64> = sessions.iter().map(|session| session.id).collect();
        encode_workspace(
            sessions,
            active_id,
            groups,
            globals,
            &default_preferences(),
            &open,
        )
    }

    fn encode_open(
        sessions: &[RequestSession],
        active_id: Option<u64>,
        open_ids: &[u64],
    ) -> String {
        encode_workspace(
            sessions,
            active_id,
            &[],
            &Definitions::new(),
            &default_preferences(),
            open_ids,
        )
    }

    fn one(session: &RequestSession) -> String {
        encode(std::slice::from_ref(session), Some(session.id))
    }

    fn parse(content: &str) -> Value {
        serde_json::from_str(content).unwrap()
    }

    fn decode(value: &Value) -> Result<DecodedWorkspace, String> {
        decode_workspace(&value.to_string())
    }

    fn remove(value: &mut Value, key: &str) {
        value.as_object_mut().unwrap().remove(key);
    }

    fn response(body: &str) -> ApiResponse {
        ApiResponse {
            status: 200,
            status_text: "OK".into(),
            duration_ms: 1.0,
            headers: vec![],
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

    // workspace.test.ts

    #[test]
    fn round_trips_complete_tabs_selection_response_and_view_preferences() {
        let mut first = create_session(None);
        let second = create_session(None);
        first.draft.url = "https://example.test".into();
        first.draft.auth = AuthKind::Bearer;
        first.draft.token = "synthetic".into();
        first.draft.body = "{\"id\":9223372036854775807}".into();
        first.view.request_tab = "body".into();
        first.view.pretty = false;
        first.view.json_view = crate::model::JsonView::Formatted;
        first.view.wrap = true;
        first.view.response_scroll = 250.0;
        first.sent_fingerprint = draft_fingerprint(&first.draft);
        first.response = Some(ApiResponse {
            duration_ms: 8.0,
            size_bytes: 2,
            headers: vec![Header {
                key: "X-Test".into(),
                value: "yes".into(),
            }],
            ..response("{}")
        });
        let encoded = encode(&[first.clone(), second.clone()], Some(second.id));
        let result = decode_workspace(&encoded).unwrap();
        assert_eq!(result.active_id, Some(second.id));
        assert_eq!(result.sessions[0].draft, first.draft);
        assert_eq!(result.sessions[0].view, first.view);
        assert_eq!(result.sessions[0].response, first.response);
        assert_eq!(result.sessions[0].sent_fingerprint, first.sent_fingerprint);
        assert!(create_session(None).id > second.id);
    }

    #[test]
    fn round_trips_nested_groups_and_request_membership() {
        let first = create_session(None);
        let second = create_session(None);
        let mut workspace = parse(&encode(&[first.clone(), second], Some(first.id)));
        workspace["groups"] = serde_json::json!([
            { "id": 1, "name": "Platform", "parentId": null, "collapsed": false },
            { "id": 2, "name": "Identity", "parentId": 1, "collapsed": true },
        ]);
        workspace["tabs"][0]["groupId"] = 2.into();
        workspace["version"] = 2.into();
        let result = decode(&workspace).unwrap();
        assert_eq!(
            result.groups,
            [
                group(1, "Platform", None),
                RequestGroup {
                    collapsed: true,
                    ..group(2, "Identity", Some(1))
                }
            ]
        );
        assert_eq!(result.sessions[0].group_id, Some(2));
    }

    #[test]
    fn migrates_version_1_snapshots_into_the_ungrouped_browser_section() {
        let session = create_session(None);
        let mut legacy = parse(&one(&session));
        legacy["version"] = 1.into();
        remove(&mut legacy, "groups");
        remove(&mut legacy["tabs"][0], "groupId");
        let result = decode(&legacy).unwrap();
        assert!(result.groups.is_empty());
        assert_eq!(result.sessions[0].group_id, None);
    }

    #[test]
    fn migrates_old_response_fingerprints_to_the_resolved_request_format() {
        let mut session = create_session(None);
        session.draft.url = "https://example.test/users".into();
        session.response = Some(ApiResponse {
            duration_ms: 8.0,
            ..response("{}")
        });
        session.sent_fingerprint = draft_fingerprint(&session.draft);
        let mut legacy = parse(&one(&session));
        legacy["version"] = 2.into();
        remove(&mut legacy, "globalDefinitions");
        let result = decode(&legacy).unwrap();
        let draft = &result.sessions[0].draft;
        assert_eq!(
            result.sessions[0].sent_fingerprint,
            request_fingerprint(Some(&build_request(draft, None).unwrap()), Some("none"))
        );
    }

    #[test]
    fn restores_interrupted_requests_as_idle_errors_without_replaying_them() {
        let mut session = create_session(None);
        session.busy = true;
        let result = decode_workspace(&one(&session)).unwrap();
        assert!(!result.sessions[0].busy);
        assert!(result.sessions[0].error.contains("interrupted"));
    }

    #[test]
    fn round_trips_graphql_drafts_and_rejects_non_string_variables() {
        let mut session = create_session(None);
        session.draft.method = "POST".into();
        session.draft.body_mode = BodyMode::Graphql;
        session.draft.body = "{ viewer { id } }".into();
        session.draft.variables = Some("{\"first\":10}".into());
        let encoded = one(&session);
        let draft = decode_workspace(&encoded).unwrap().sessions.remove(0).draft;
        assert_eq!(draft.body_mode, BodyMode::Graphql);
        assert_eq!(draft.body, session.draft.body);
        assert_eq!(draft.variables, session.draft.variables);
        let mut data = parse(&encoded);
        data["tabs"][0]["draft"]["variables"] = serde_json::json!({ "first": 10 });
        assert!(decode(&data).is_err());
    }

    #[test]
    fn rejects_corrupt_future_and_malformed_snapshots_instead_of_partially_resetting_them() {
        for content in [
            "{",
            "{\"version\":99}",
            "{\"version\":1,\"tabs\":[],\"activeId\":1}",
        ] {
            assert_eq!(decode_workspace(content).unwrap_err(), INVALID);
        }
        let session = create_session(None);
        let mut data = parse(&one(&session));
        data["tabs"][0]["draft"]["auth"] = "invented".into();
        assert!(decode(&data).is_err());
        data["tabs"][0]["draft"]["auth"] = "none".into();
        let tab = data["tabs"][0].clone();
        data["tabs"].as_array_mut().unwrap().push(tab);
        assert!(decode(&data).is_err());
    }

    #[test]
    fn reads_the_json_view_and_defaults_it_to_the_tree() {
        let session = create_session(None);
        let mut data = parse(&one(&session));
        data["tabs"][0]["view"]["jsonView"] = "formatted".into();
        let view = &decode(&data).unwrap().sessions[0].view;
        assert_eq!(view.json_view, crate::model::JsonView::Formatted);
        // Workspaces from before the setting.
        remove(&mut data["tabs"][0]["view"], "jsonView");
        let view = &decode(&data).unwrap().sessions[0].view;
        assert_eq!(view.json_view, crate::model::JsonView::Tree);
        data["tabs"][0]["view"]["jsonView"] = "table".into();
        assert!(decode(&data).is_err());
        data["tabs"][0]["view"]["jsonView"] = true.into();
        assert!(decode(&data).is_err());
    }

    #[test]
    fn rejects_a_snapshot_over_the_save_limit() {
        let content = " ".repeat(MAX_STATE_BYTES + 1);
        assert_eq!(validate_workspace(&content).unwrap_err(), TOO_LARGE);
    }

    // workspace-history.test.ts

    fn entry(id: u64) -> HistoryEntry {
        HistoryEntry {
            id,
            sent_at: 1_700_000_000_000.0 + id as f64,
            method: "GET".into(),
            url: "https://example.test".into(),
            status: Some(200),
            status_text: Some("OK".into()),
            error: None,
            duration_ms: 12.0,
            size_bytes: 2,
            headers: vec![Header {
                key: "a".into(),
                value: "1".into(),
            }],
            body: "{}".into(),
            body_omitted: None,
            timing: Some(ResponseTiming {
                wait_ms: 10.0,
                download_ms: 2.0,
                ..ResponseTiming::default()
            }),
        }
    }

    #[test]
    fn round_trips_request_history() {
        let mut session = create_session(None);
        session.history = vec![entry(2), entry(1)];
        let restored = decode_workspace(&one(&session)).unwrap().sessions.remove(0);
        assert_eq!(restored.history, session.history);
    }

    #[test]
    fn omits_empty_history() {
        let session = create_session(None);
        assert!(!one(&session).contains("history"));
    }

    #[test]
    fn rejects_invalid_history() {
        let mut session = create_session(None);
        session.history = vec![entry(1), entry(1)];
        assert!(decode_workspace(&one(&session)).is_err());
        session.history = vec![HistoryEntry {
            status: Some(42),
            ..entry(1)
        }];
        assert!(decode_workspace(&one(&session)).is_err());
    }

    // workspace-v3.test.ts

    #[test]
    fn round_trips_v3_with_group_local_auth_and_local_definitions() {
        let mut session = create_session(None);
        let groups = [RequestGroup {
            local_auth: Some(AuthorizationConfig::Bearer {
                token: "g-tok".into(),
            }),
            local_definitions: Some(defs(&[("host", "api.example.com")])),
            ..group(1, "API", None)
        }];
        session.group_id = Some(1);
        let encoded = encode_with(
            std::slice::from_ref(&session),
            Some(session.id),
            &groups,
            &defs(&[]),
        );
        assert_eq!(parse(&encoded)["version"], 4);
        let result = decode_workspace(&encoded).unwrap();
        assert_eq!(result.groups[0].local_auth, groups[0].local_auth);
        assert_eq!(
            result.groups[0].local_definitions,
            groups[0].local_definitions
        );
    }

    #[test]
    fn round_trips_v3_with_workspace_global_definitions() {
        let session = create_session(None);
        let globals = defs(&[("apiVersion", "v2")]);
        let encoded = encode_with(
            std::slice::from_ref(&session),
            Some(session.id),
            &[],
            &globals,
        );
        assert_eq!(
            decode_workspace(&encoded).unwrap().global_definitions,
            globals
        );
    }

    fn v2_with_auth(auth: &str, token: &str, username: &str, password: &str) -> Draft {
        let mut session = create_session(None);
        session.draft.url = "https://example.test".into();
        let mut v2 = parse(&one(&session));
        v2["version"] = 2.into();
        let draft = &mut v2["tabs"][0]["draft"];
        draft["auth"] = auth.into();
        draft["token"] = token.into();
        draft["username"] = username.into();
        draft["password"] = password.into();
        decode(&v2).unwrap().sessions.remove(0).draft
    }

    #[test]
    fn migrates_v2_flat_auth_bearer_to_explicit_local_auth_on_draft() {
        let draft = v2_with_auth("bearer", "tok123", "", "");
        // Flat fields should still exist for backward compat
        assert_eq!(draft.auth, AuthKind::Bearer);
        assert_eq!(draft.token, "tok123");
        // localAuth should be populated as an explicit local config
        assert_eq!(
            draft.local_auth,
            Some(AuthorizationConfig::Bearer {
                token: "tok123".into()
            })
        );
    }

    #[test]
    fn migrates_v2_flat_auth_basic_to_explicit_local_auth_on_draft() {
        let draft = v2_with_auth("basic", "", "alice", "secret");
        assert_eq!(
            draft.local_auth,
            Some(AuthorizationConfig::Basic {
                username: "alice".into(),
                password: "secret".into()
            })
        );
    }

    #[test]
    fn migrates_v2_flat_auth_none_to_explicit_local_auth_none() {
        // Migrated: since v2 was always explicit, localAuth should be explicit none
        let draft = v2_with_auth("none", "", "", "");
        assert_eq!(draft.local_auth, Some(AuthorizationConfig::None));
    }

    #[test]
    fn new_sessions_created_without_local_auth_inherit_by_default() {
        assert_eq!(create_session(None).draft.local_auth, None);
    }

    #[test]
    fn v3_encodes_local_auth_on_draft_not_flat_fields() {
        let mut session = create_session(None);
        session.draft.local_auth = Some(AuthorizationConfig::Bearer {
            token: "d-tok".into(),
        });
        let raw = parse(&one(&session));
        assert_eq!(raw["version"], 4);
        assert_eq!(
            raw["tabs"][0]["draft"]["localAuth"],
            serde_json::json!({ "type": "bearer", "token": "d-tok" })
        );
    }

    #[test]
    fn v3_draft_local_auth_undefined_means_inherit_not_stored_in_json() {
        let raw = parse(&one(&create_session(None)));
        assert!(raw["tabs"][0]["draft"].get("localAuth").is_none());
    }

    #[test]
    fn v3_strict_validation_rejects_unknown_auth_type_in_local_auth() {
        let mut raw = parse(&one(&create_session(None)));
        raw["tabs"][0]["draft"]["localAuth"] = serde_json::json!({ "type": "oauth" });
        assert!(decode(&raw).is_err());
    }

    #[test]
    fn migrates_v1_snapshots_with_no_groups_and_flat_auth() {
        let mut v1 = parse(&one(&create_session(None)));
        v1["version"] = 1.into();
        remove(&mut v1, "groups");
        remove(&mut v1["tabs"][0], "groupId");
        v1["tabs"][0]["draft"]["auth"] = "bearer".into();
        v1["tabs"][0]["draft"]["token"] = "v1tok".into();
        let result = decode(&v1).unwrap();
        assert!(result.groups.is_empty());
        assert_eq!(
            result.sessions[0].draft.local_auth,
            Some(AuthorizationConfig::Bearer {
                token: "v1tok".into()
            })
        );
    }

    #[test]
    fn rejects_v3_snapshot_with_invalid_global_definitions() {
        let mut raw = parse(&one(&create_session(None)));
        raw["globalDefinitions"] = "not-an-object".into();
        assert!(decode(&raw).is_err());
    }

    // workspace-v3-strict.test.ts

    fn minimal_v3() -> Value {
        parse(&one(&create_session(None)))
    }

    fn globals(value: Value) -> Result<DecodedWorkspace, String> {
        let mut base = minimal_v3();
        base["globalDefinitions"] = value;
        decode(&base)
    }

    #[test]
    fn accepts_a_valid_global_definitions_object() {
        let session = create_session(None);
        let globals = defs(&[("apiKey", "abc"), ("host", "example.com")]);
        let encoded = encode_with(std::slice::from_ref(&session), Some(1), &[], &globals);
        let mut raw = parse(&encoded);
        // The TS test passes id 1 as the active id; use this session's id.
        raw["activeId"] = session.id.into();
        assert_eq!(decode(&raw).unwrap().global_definitions, globals);
    }

    #[test]
    fn requires_global_definitions_in_v3_rejects_absent_field() {
        let mut raw = minimal_v3();
        remove(&mut raw, "globalDefinitions");
        assert!(decode(&raw).is_err());
    }

    #[test]
    fn rejects_global_definitions_with_non_string_values() {
        assert!(globals(serde_json::json!({ "key": 123 })).is_err());
    }

    #[test]
    fn rejects_global_definitions_that_is_not_an_object_string() {
        assert!(globals("flat".into()).is_err());
    }

    #[test]
    fn rejects_global_definitions_that_is_not_an_object_array() {
        assert!(globals(serde_json::json!(["a", "b"])).is_err());
    }

    fn many(count: usize) -> Value {
        Value::Object(
            (0..count)
                .map(|i| (format!("key{i}"), "v".into()))
                .collect(),
        )
    }

    #[test]
    fn rejects_global_definitions_with_more_than_500_entries() {
        assert!(globals(many(501)).is_err());
    }

    #[test]
    fn accepts_global_definitions_with_exactly_500_entries() {
        assert!(globals(many(500)).is_ok());
    }

    #[test]
    fn rejects_global_definitions_with_a_key_longer_than_256_chars() {
        let mut map = Map::new();
        map.insert("a".repeat(257), "v".into());
        assert!(globals(Value::Object(map)).is_err());
    }

    #[test]
    fn rejects_global_definitions_with_a_value_longer_than_65536_chars() {
        assert!(globals(serde_json::json!({ "key": "x".repeat(65537) })).is_err());
    }

    #[test]
    fn rejects_group_local_definitions_with_key_starting_with_underscore() {
        let groups = [RequestGroup {
            local_definitions: Some(defs(&[("_secret", "oops")])),
            ..group(1, "Test", None)
        }];
        let mut session = create_session(None);
        session.group_id = Some(1);
        let encoded = encode_with(
            std::slice::from_ref(&session),
            Some(session.id),
            &groups,
            &defs(&[]),
        );
        assert!(decode_workspace(&encoded).is_err());
    }

    #[test]
    fn accepts_group_local_definitions_without_leading_underscore_keys() {
        let definitions = defs(&[("host", "api.example.com"), ("version", "v2")]);
        let groups = [RequestGroup {
            local_definitions: Some(definitions.clone()),
            ..group(1, "Test", None)
        }];
        let mut session = create_session(None);
        session.group_id = Some(1);
        let encoded = encode_with(
            std::slice::from_ref(&session),
            Some(session.id),
            &groups,
            &defs(&[]),
        );
        let decoded = decode_workspace(&encoded).unwrap();
        assert_eq!(decoded.groups[0].local_definitions, Some(definitions));
    }

    #[test]
    fn rejects_group_local_definitions_with_non_string_values() {
        let mut raw = minimal_v3();
        raw["groups"] = serde_json::json!([{
            "id": 1, "name": "G", "parentId": null, "collapsed": false,
            "localDefinitions": { "key": 99 },
        }]);
        raw["tabs"][0]["groupId"] = 1.into();
        assert!(decode(&raw).is_err());
    }

    #[test]
    fn encode_workspace_always_writes_global_definitions_required_field() {
        assert!(minimal_v3()["globalDefinitions"].is_object());
    }

    #[test]
    fn v3_round_trip_preserves_empty_global_definitions_as_empty_object() {
        let decoded = decode(&minimal_v3()).unwrap();
        assert!(decoded.global_definitions.is_empty());
    }

    // workspace-v4.test.ts

    #[test]
    fn round_trips_open_tabs_separately_from_requests() {
        let [a, b, c] = [
            create_session(None),
            create_session(None),
            create_session(None),
        ];
        let all = [a.clone(), b.clone(), c.clone()];
        let result = decode_workspace(&encode_open(&all, Some(c.id), &[c.id, a.id])).unwrap();
        let ids: Vec<u64> = result.sessions.iter().map(|session| session.id).collect();
        assert_eq!(ids, [a.id, b.id, c.id]);
        assert_eq!(result.open_ids, [c.id, a.id]);
        assert_eq!(result.active_id, Some(c.id));
    }

    #[test]
    fn restores_more_than_128_requests() {
        let sessions: Vec<RequestSession> = (0..300).map(|_| create_session(None)).collect();
        let ids: Vec<u64> = sessions.iter().map(|session| session.id).collect();
        let result = decode_workspace(&encode_open(&sessions, Some(ids[0]), &ids)).unwrap();
        assert_eq!(result.sessions.len(), 300);
        assert_eq!(result.open_ids.len(), 300);
    }

    #[test]
    fn allows_no_open_tabs_with_a_null_active_id() {
        let result = decode_workspace(&encode_open(&[create_session(None)], None, &[])).unwrap();
        assert!(result.open_ids.is_empty());
        assert_eq!(result.active_id, None);
    }

    #[test]
    fn opens_every_request_when_migrating_from_v3() {
        let [a, b] = [create_session(None), create_session(None)];
        let mut raw = parse(&encode_open(&[a.clone(), b.clone()], Some(a.id), &[a.id]));
        raw["version"] = 3.into();
        remove(&mut raw, "openIds");
        assert_eq!(decode(&raw).unwrap().open_ids, [a.id, b.id]);
    }

    #[test]
    fn rejects_an_active_id_that_is_not_open() {
        let [a, b] = [create_session(None), create_session(None)];
        let encoded = encode_open(&[a.clone(), b.clone()], Some(b.id), &[a.id]);
        assert!(decode_workspace(&encoded).is_err());
    }

    #[test]
    fn rejects_open_ids_for_unknown_or_duplicate_requests() {
        let session = create_session(None);
        let only = std::slice::from_ref(&session);
        let id = session.id;
        assert!(decode_workspace(&encode_open(only, Some(id), &[id, 999])).is_err());
        assert!(decode_workspace(&encode_open(only, Some(id), &[id, id])).is_err());
    }

    #[test]
    fn rejects_a_null_active_id_while_tabs_are_open() {
        let session = create_session(None);
        let encoded = encode_open(std::slice::from_ref(&session), None, &[session.id]);
        assert!(decode_workspace(&encoded).is_err());
    }

    #[test]
    fn keeps_optional_response_fields_but_never_saves_the_body_id() {
        let mut session = create_session(None);
        session.response = Some(ApiResponse {
            size_bytes: 9,
            body_id: Some("body-1".into()),
            truncated: Some(true),
            binary: Some(true),
            final_url: Some("https://final.test/".into()),
            redirect_count: Some(2),
            ..response("")
        });
        let encoded = one(&session);
        assert!(!encoded.contains("body-1"));
        let restored = decode_workspace(&encoded)
            .unwrap()
            .sessions
            .remove(0)
            .response
            .unwrap();
        assert_eq!(restored.truncated, Some(true));
        assert_eq!(restored.binary, Some(true));
        assert_eq!(restored.final_url.as_deref(), Some("https://final.test/"));
        assert_eq!(restored.redirect_count, Some(2));
        assert_eq!(restored.body_id, None);
    }

    #[test]
    fn drops_a_body_id_found_in_a_saved_workspace() {
        let mut session = create_session(None);
        session.response = Some(response(""));
        let mut data = parse(&one(&session));
        data["tabs"][0]["response"]["bodyId"] = "stale".into();
        let restored = decode(&data).unwrap().sessions.remove(0).response.unwrap();
        assert_eq!(restored.body_id, None);
    }

    #[test]
    fn empties_the_body_of_a_truncated_preview_so_it_does_not_blow_past_the_save_limit() {
        let mut session = create_session(None);
        session.response = Some(ApiResponse {
            size_bytes: 5 * 1024 * 1024,
            truncated: Some(true),
            binary: Some(false),
            ..response(&"x".repeat(4 * 1024 * 1024))
        });
        let encoded = parse(&one(&session));
        assert_eq!(encoded["tabs"][0]["response"]["body"], "");
        let restored = decode(&encoded)
            .unwrap()
            .sessions
            .remove(0)
            .response
            .unwrap();
        assert_eq!(restored.body, "");
        assert_eq!(restored.truncated, Some(true));
        assert_eq!(restored.binary, Some(false));
        assert_eq!(restored.size_bytes, 5 * 1024 * 1024);
    }

    #[test]
    fn empties_the_body_of_a_binary_response() {
        let mut session = create_session(None);
        session.response = Some(ApiResponse {
            size_bytes: 2048,
            binary: Some(true),
            truncated: Some(true),
            ..response("")
        });
        let encoded = parse(&one(&session));
        let saved = &encoded["tabs"][0]["response"];
        assert_eq!(saved["body"], "");
        assert_eq!(saved["binary"], true);
        assert_eq!(saved["truncated"], true);
    }

    #[test]
    fn keeps_the_body_of_a_complete_text_response() {
        let mut session = create_session(None);
        session.response = Some(response("hello"));
        assert_eq!(
            parse(&one(&session))["tabs"][0]["response"]["body"],
            "hello"
        );
    }

    #[test]
    fn writes_whole_numbers_as_javascript_does() {
        let encoded = one(&create_session(None));
        assert!(encoded.contains("\"zoom\":1,"));
        assert!(encoded.contains("\"responseScroll\":0}"));
    }

    // Snapshots written by the TS app (tests/fixtures/workspace).

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/workspace/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
    }

    /// Rust writes an unset active environment as absent; TS may write null.
    /// Both read the same.
    fn normalized(mut value: Value) -> Value {
        if let Some(groups) = value["groups"].as_array_mut() {
            for group in groups {
                if group.get("activeEnvironmentId") == Some(&Value::Null) {
                    remove(group, "activeEnvironmentId");
                }
            }
        }
        value
    }

    fn rust_round_trip(content: &str) -> String {
        let decoded = decode_workspace(content).unwrap();
        encode_workspace(
            &decoded.sessions,
            decoded.active_id,
            &decoded.groups,
            &decoded.global_definitions,
            &decoded.preferences,
            &decoded.open_ids,
        )
    }

    fn assert_matches_ts(name: &str) {
        let input = fixture(name);
        let expected = normalized(parse(&fixture(&format!("{name}.roundtrip"))));
        validate_workspace(&input).unwrap();
        assert_eq!(
            normalized(parse(&rust_round_trip(&input))),
            expected,
            "{name}"
        );
        // Writing again changes nothing.
        let again = rust_round_trip(&expected.to_string());
        assert_eq!(normalized(parse(&again)), expected, "{name} again");
    }

    #[test]
    fn reads_and_writes_a_full_v4_snapshot_like_the_ts_app() {
        assert_matches_ts("v4-full");
        let decoded = decode_workspace(&fixture("v4-full")).unwrap();
        assert_eq!(decoded.open_ids.len(), 2);
        assert_eq!(decoded.preferences.zoom, 1.25);
        let form = &decoded.sessions[1];
        assert!(form.error.contains("interrupted"));
        assert_eq!(form.response.as_ref().unwrap().body_id, None);
        let max_environment = decoded
            .groups
            .iter()
            .flat_map(|group| group.environments.iter().flatten())
            .map(|environment| environment.id)
            .max()
            .unwrap();
        assert!(ids::ENVIRONMENTS.next() > max_environment);
    }

    #[test]
    fn reads_and_writes_a_v4_snapshot_without_open_tabs_like_the_ts_app() {
        assert_matches_ts("v4-no-tabs");
    }

    #[test]
    fn migrates_a_v3_snapshot_like_the_ts_app() {
        assert_matches_ts("v3");
    }

    #[test]
    fn migrates_a_v2_snapshot_like_the_ts_app() {
        assert_matches_ts("v2");
    }

    #[test]
    fn migrates_a_v1_snapshot_like_the_ts_app() {
        assert_matches_ts("v1");
    }
}
