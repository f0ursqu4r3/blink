//! Ordered collection runs. Each dataset row gets an isolated workspace;
//! captures remain available to later requests in that row.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::history::now_ms;
use crate::model::Definitions;
use crate::runner::{begin_send, finish_send, prepare};
use crate::workspace_state::Workspace;

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub request_ids: Vec<u64>,
    /// Empty means one run with no dataset overrides.
    pub dataset: Vec<Definitions>,
    pub stop_on_failure: bool,
    pub allow_protected: bool,
    /// Exact (zero-based dataset row, request id) pairs from a prior report.
    pub only: Option<HashSet<(usize, u64)>>,
    /// In-memory state retained for retry. Never included in JSON reports.
    pub resume: Vec<RunResume>,
}

#[derive(Debug, Clone, Default)]
pub struct RunResume {
    captures: Vec<(Option<u64>, Definitions)>,
    cache: crate::response_token_cache::ResponseTokenCache,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    Passed,
    Failed,
    Skipped,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub request_id: u64,
    pub name: String,
    pub iteration: usize,
    pub status: RunStatus,
    pub http_status: Option<u16>,
    pub duration_ms: f64,
    pub assertions_passed: usize,
    pub assertions_failed: usize,
    pub capture_errors: usize,
    pub contract_errors: usize,
    pub contract_unsupported: usize,
    /// Check labels and validation messages. Assertion actual values are omitted.
    pub failures: Vec<String>,
    /// No response body, assertion actual value, or captured secret is exported.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    pub started_at: f64,
    pub duration_ms: f64,
    pub results: Vec<RunResult>,
    #[serde(skip)]
    pub resume: Vec<RunResume>,
}

impl RunReport {
    pub fn passed(&self) -> bool {
        !self.results.is_empty() && self.results.iter().all(|r| r.status == RunStatus::Passed)
    }
    pub fn failed_cases(&self) -> HashSet<(usize, u64)> {
        self.results
            .iter()
            .filter(|r| r.status == RunStatus::Failed)
            .map(|r| (r.iteration, r.request_id))
            .collect()
    }
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
}

#[derive(Clone, Default)]
pub struct RunControl(Arc<Control>);
#[derive(Default)]
struct Control {
    canceled: AtomicBool,
    current: Mutex<Option<(Engine, String)>>,
}
impl RunControl {
    pub fn cancel(&self) {
        self.0.canceled.store(true, Ordering::SeqCst);
        if let Ok(current) = self.0.current.lock()
            && let Some((engine, id)) = current.as_ref()
        {
            engine.cancel_request(id);
        }
    }
    pub fn canceled(&self) -> bool {
        self.0.canceled.load(Ordering::SeqCst)
    }
}

/// Tree order: direct requests first, then child groups, preserving each list's order.
pub fn group_order(workspace: &Workspace, group_id: u64) -> Vec<u64> {
    fn visit(w: &Workspace, id: u64, seen: &mut HashSet<u64>, out: &mut Vec<u64>) {
        if !seen.insert(id) || w.group(id).is_none() {
            return;
        }
        out.extend(
            w.sessions
                .iter()
                .filter(|s| s.group_id == Some(id))
                .map(|s| s.id),
        );
        for group in w.groups.iter().filter(|g| g.parent_id == Some(id)) {
            visit(w, group.id, seen, out);
        }
    }
    let mut ids = Vec::new();
    visit(workspace, group_id, &mut HashSet::new(), &mut ids);
    ids
}

/// Parse a JSON array of token objects or CSV with a header row. Reject malformed
/// rows instead of silently changing iteration counts or overwriting tokens.
pub fn parse_dataset(text: &str, json: bool) -> Result<Vec<Definitions>, String> {
    if text.len() > 8 * 1024 * 1024 {
        return Err("Dataset exceeds 8 MiB.".into());
    }
    let rows = if json {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("Invalid dataset JSON: {e}"))?;
        value
            .as_array()
            .ok_or("Dataset JSON must be an array of objects.")?
            .iter()
            .map(|row| {
                row.as_object()
                    .ok_or_else(|| "Each dataset row must be an object.".to_string())?
                    .iter()
                    .map(|(key, value)| {
                        let value =
                            match value {
                                serde_json::Value::String(s) => s.clone(),
                                serde_json::Value::Null => String::new(),
                                serde_json::Value::Number(_) | serde_json::Value::Bool(_) => {
                                    value.to_string()
                                }
                                _ => return Err(
                                    "Dataset values must be strings, numbers, booleans, or null."
                                        .into(),
                                ),
                            };
                        Ok((key.clone(), value))
                    })
                    .collect::<Result<Definitions, String>>()
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let records = csv_records(text.trim_start_matches('\u{feff}'))?;
        let headers = records
            .first()
            .ok_or("Dataset CSV requires a header row.")?;
        let mut seen = HashSet::new();
        if headers.iter().any(|h| !seen.insert(h)) {
            return Err("Dataset headers must be unique.".into());
        }
        records
            .iter()
            .skip(1)
            .map(|row| {
                if row.len() != headers.len() {
                    return Err("Dataset row has a different column count.".into());
                }
                Ok(headers.iter().cloned().zip(row.iter().cloned()).collect())
            })
            .collect::<Result<Vec<_>, String>>()?
    };
    if rows.is_empty() {
        return Err("Dataset contains no rows.".into());
    }
    if rows.len() > 10_000 {
        return Err("Dataset exceeds 10000 rows.".into());
    }
    if rows
        .iter()
        .any(|row| row.keys().any(|key| !crate::checks::is_capture_name(key)))
    {
        return Err("Dataset headers must be valid token names.".into());
    }
    Ok(rows)
}

fn csv_records(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut closed = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' if field.is_empty() && !closed => quoted = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                    closed = false;
                }
                '\r' | '\n' => {
                    if c == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    closed = false;
                }
                '"' => return Err("Unexpected quote in dataset CSV.".into()),
                _ if closed => return Err("Unexpected text after a quoted CSV field.".into()),
                _ => field.push(c),
            }
        }
    }
    if quoted {
        return Err("Unclosed quote in dataset CSV.".into());
    }
    if !row.is_empty() || !field.is_empty() || closed {
        row.push(field);
        records.push(row);
    }
    Ok(records)
}

pub async fn run(
    engine: Engine,
    workspace: Workspace,
    options: RunOptions,
    control: RunControl,
) -> Result<RunReport, String> {
    if options.request_ids.is_empty() {
        return Err("Select at least one request.".into());
    }
    if options
        .request_ids
        .len()
        .checked_mul(options.dataset.len().max(1))
        .is_none_or(|n| n > 100_000)
    {
        return Err("A collection run cannot exceed 100000 request iterations.".into());
    }
    let mut unique = HashSet::new();
    for id in &options.request_ids {
        if !unique.insert(*id) {
            return Err("Run order contains duplicate requests.".into());
        }
        let session = workspace
            .session(*id)
            .ok_or("A selected request no longer exists.")?;
        if !options.allow_protected
            && crate::environments::request_environment(session.group_id, &workspace.groups)
                .is_some_and(|e| e.protected == Some(true))
        {
            return Err("This run uses a protected environment. Confirm it before running.".into());
        }
    }
    let started_at = now_ms();
    let clock = std::time::Instant::now();
    let mut report = RunReport {
        started_at,
        duration_ms: 0.0,
        results: Vec::new(),
        resume: Vec::new(),
    };
    let rows = if options.dataset.is_empty() {
        vec![Definitions::new()]
    } else {
        options.dataset.clone()
    };
    let mut stopped = false;
    for (iteration, row) in rows.iter().enumerate() {
        // Cookies and response storage belong to this dataset row only.
        let row_storage = (!stopped
            && !control.canceled()
            && options.only.as_ref().is_none_or(|only| {
                options
                    .request_ids
                    .iter()
                    .any(|id| only.contains(&(iteration, *id)))
            }))
        .then(|| engine.for_collection_row())
        .transpose()?;
        let engine = row_storage
            .as_ref()
            .map(|(engine, _)| engine)
            .unwrap_or(&engine);
        let mut work = workspace.clone();
        // Apply overrides to the snapshot itself so response-token source
        // fingerprints use the same values as the requests that were sent.
        work.global_definitions.extend(row.clone());
        for group in &mut work.groups {
            group
                .local_definitions
                .get_or_insert_default()
                .extend(row.clone());
            if let Some(environment) = group.environments.as_mut().and_then(|items| {
                items
                    .iter_mut()
                    .find(|e| Some(e.id) == group.active_environment_id)
            }) {
                environment.values.extend(row.clone());
            }
        }
        for project in &mut work.projects {
            for values in project.private_values.values_mut() {
                values.extend(row.clone());
            }
        }
        let mut resume = options.resume.get(iteration).cloned().unwrap_or_default();
        if options.only.is_some() {
            for (group, values) in &resume.captures {
                work.capture(*group, values);
            }
            work.response_cache = resume.cache.clone();
        }
        // The snapshot must never release response bodies owned by the editor.
        for session in &mut work.sessions {
            session.response = None;
            session.busy = false;
            session.history.clear();
            session.test_results = None;
            session.capture_errors = None;
            session.error.clear();
        }
        for id in &options.request_ids {
            if options
                .only
                .as_ref()
                .is_some_and(|only| !only.contains(&(iteration, *id)))
            {
                continue;
            }
            let session = work.session(*id).expect("validated request");
            let mut result = RunResult {
                request_id: *id,
                name: format!("Request {id}"),
                iteration,
                status: RunStatus::Skipped,
                http_status: None,
                duration_ms: 0.0,
                assertions_passed: 0,
                assertions_failed: 0,
                capture_errors: 0,
                contract_errors: 0,
                contract_unsupported: 0,
                failures: Vec::new(),
                error: None,
            };
            if control.canceled() {
                result.status = RunStatus::Canceled;
            } else if !stopped {
                let group_id = session.group_id;
                let project = work.project_for_group(group_id).map(|p| p.path.clone());
                let ctx = work.token_sources(now_ms()).request_context(session);
                let prepared = prepare(&session.draft, Some(&ctx));
                let contract = session.draft.openapi_contract.clone();
                if crate::websocket_log::is_web_socket_url(&session.draft.url) {
                    result.status = RunStatus::Failed;
                    result.error = Some("Collection runs require HTTP requests.".into());
                } else if prepared.request.is_err() {
                    result.status = RunStatus::Failed;
                    result.error = Some("Request could not be prepared.".into());
                } else {
                    if let Some(contract) = &contract {
                        let validation = contract.validate_request(prepared.request().unwrap());
                        result.contract_errors += validation.errors.len();
                        result.contract_unsupported += validation.unsupported.len();
                        result.failures.extend(
                            validation
                                .errors
                                .into_iter()
                                .map(|_| "Request contract validation failed.".to_string()),
                        );
                        result.failures.extend(
                            validation
                                .unsupported
                                .into_iter()
                                .map(|_| "Request contract uses an unsupported rule.".to_string()),
                        );
                    }
                    let transport = crate::preferences::transport_options(&work.preferences);
                    let ticket =
                        begin_send(work.session_mut(*id).unwrap(), &prepared, &transport).unwrap();
                    let request_id = engine.next_id("collection");
                    // Publish the id and reserve its engine cancellation slot
                    // under the same lock used by cancel().
                    let pending = {
                        let mut current = control.0.current.lock().unwrap();
                        if control.canceled() {
                            None
                        } else {
                            *current = Some((engine.clone(), request_id.clone()));
                            Some(engine.send_request_scoped(
                                ticket.request.clone(),
                                ticket.options.clone(),
                                request_id,
                                None,
                                project,
                            ))
                        }
                    };
                    let response = match pending {
                        Some(pending) => pending.await,
                        None => Err(crate::engine::CANCELED.to_string()),
                    };
                    *control.0.current.lock().unwrap() = None;
                    result.duration_ms = ticket.elapsed_ms();
                    let completed = response.is_ok();
                    let captured =
                        finish_send(work.session_mut(*id).unwrap(), ticket, response).captured;
                    let session = work.session(*id).unwrap();
                    result.http_status = session.response.as_ref().map(|r| r.status);
                    result.assertions_passed = session
                        .test_results
                        .as_ref()
                        .map_or(0, |r| r.iter().filter(|r| r.pass).count());
                    result.assertions_failed = session
                        .test_results
                        .as_ref()
                        .map_or(0, |r| r.iter().filter(|r| !r.pass).count());
                    result.capture_errors = session.capture_errors.as_ref().map_or(0, Vec::len);
                    if let Some(checks) = &session.test_results {
                        result.failures.extend(
                            checks
                                .iter()
                                .filter(|c| !c.pass)
                                .map(|c| format!("Assertion {} failed.", c.id)),
                        );
                    }
                    if let Some(errors) = &session.capture_errors {
                        result
                            .failures
                            .extend(errors.iter().map(|_| "Capture failed.".to_string()));
                    }
                    if let Some(contract) = &contract
                        && let Some(response) = &session.response
                    {
                        let validation = contract.validate_response(response);
                        result.contract_errors += validation.errors.len();
                        result.contract_unsupported += validation.unsupported.len();
                        result.failures.extend(
                            validation
                                .errors
                                .into_iter()
                                .map(|_| "Response contract validation failed.".to_string()),
                        );
                        result.failures.extend(
                            validation
                                .unsupported
                                .into_iter()
                                .map(|_| "Response contract uses an unsupported rule.".to_string()),
                        );
                    }
                    result.error = (!session.error.is_empty()).then(|| "Request failed.".into());
                    result.status = if control.canceled() {
                        RunStatus::Canceled
                    } else if !completed
                        || result.assertions_failed > 0
                        || result.capture_errors > 0
                        || result.contract_errors > 0
                        || result.contract_unsupported > 0
                        || result.http_status.is_some_and(|s| s >= 400)
                    {
                        RunStatus::Failed
                    } else {
                        RunStatus::Passed
                    };
                    let fingerprint = session.sent_fingerprint.clone();
                    let values = session
                        .response
                        .as_ref()
                        .filter(|r| (200..300).contains(&r.status))
                        .map(|r| {
                            crate::response_tokens::values_for(*id, work.all_response_tokens(), r)
                        });
                    if let Some(values) = values {
                        work.response_cache
                            .record(*id, &fingerprint, now_ms() as u64, values);
                    }
                    if let Some(body) = work
                        .session_mut(*id)
                        .unwrap()
                        .response
                        .as_mut()
                        .and_then(|r| r.body_id.take())
                    {
                        engine.release_response(&body);
                    }
                    work.capture(group_id, &captured);
                    if !captured.is_empty() {
                        resume.captures.push((group_id, captured));
                    }
                }
                stopped = options.stop_on_failure && result.status == RunStatus::Failed;
            }
            report.results.push(result);
        }
        resume.cache = work.response_cache;
        report.resume.push(resume);
    }
    report.duration_ms = clock.elapsed().as_secs_f64() * 1000.0;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_supports_quotes_newlines_and_crlf() {
        let rows = parse_dataset(
            "name,note\r\n\"A,B\",\"line1\nline2\"\r\nZ,\"a\"\"b\"\r\n",
            false,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], "A,B");
        assert_eq!(rows[0]["note"], "line1\nline2");
        assert_eq!(rows[1]["note"], "a\"b");
    }
    #[test]
    fn malformed_datasets_fail() {
        for text in [
            "a,a\nx,y",
            "a,b\nx",
            "a\n\"x",
            "a\n\"x\"z",
            "bad name\nx",
            "a\n",
        ] {
            assert!(parse_dataset(text, false).is_err(), "{text}");
        }
        for text in ["[]", "{}", "[4]", "[{\"x\": []}]"] {
            assert!(parse_dataset(text, true).is_err());
        }
    }
    #[test]
    fn json_scalar_values_become_tokens() {
        let rows = parse_dataset(r#"[{"a":1,"b":true,"c":null}]"#, true).unwrap();
        assert_eq!(rows[0]["a"], "1");
        assert_eq!(rows[0]["b"], "true");
        assert_eq!(rows[0]["c"], "");
    }

    fn engine() -> (tempfile::TempDir, Engine) {
        let temp = tempfile::tempdir().unwrap();
        let engine =
            Engine::new(crate::engine::paths::Paths::with_base(temp.path().into())).unwrap();
        (temp, engine)
    }

    fn fixture() -> Workspace {
        let mut workspace = Workspace::new();
        workspace.sessions.clear();
        let root = workspace.add_group("Root", None);
        let mut first = crate::session::create_session(None);
        first.group_id = Some(root);
        let mut second = crate::session::create_session(None);
        second.group_id = Some(root);
        workspace.sessions = vec![first, second];
        workspace
    }

    fn http_server(count: usize) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = std::thread::spawn(move || {
            let mut paths = Vec::new();
            for _ in 0..count {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                    let mut buffer = [0; 1024];
                    let n = stream.read(&mut buffer).unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..n]);
                }
                paths.push(
                    String::from_utf8_lossy(&request)
                        .lines()
                        .next()
                        .unwrap()
                        .to_string(),
                );
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK",
                    )
                    .unwrap();
            }
            paths
        });
        (url, task)
    }

    #[test]
    fn ordered_requests_share_captures_and_dataset_rows_are_isolated() {
        let (_temp, engine) = engine();
        let (url, server) = http_server(4);
        let mut workspace = fixture();
        workspace.sessions[0].draft.url = format!("{url}/{{{{item}}}}");
        workspace.sessions[0].draft.captures = Some(vec![crate::checks::create_capture(
            "code",
            crate::model::CheckSource::Status,
            "",
        )]);
        workspace.sessions[1].draft.url = format!("{url}/{{{{code}}}}/{{{{item}}}}");
        let snapshot = workspace.encode();
        let options = RunOptions {
            request_ids: workspace.sessions.iter().map(|s| s.id).collect(),
            dataset: parse_dataset("item\nfirst\nsecond\n", false).unwrap(),
            ..Default::default()
        };
        let report = futures::executor::block_on(run(
            engine,
            workspace.clone(),
            options,
            RunControl::default(),
        ))
        .unwrap();
        assert!(report.passed());
        assert_eq!(
            server.join().unwrap(),
            [
                "GET /first HTTP/1.1",
                "GET /200/first HTTP/1.1",
                "GET /second HTTP/1.1",
                "GET /200/second HTTP/1.1"
            ]
        );
        assert_eq!(workspace.encode(), snapshot);
        assert!(!report.to_json().unwrap().contains("body"));
    }

    #[test]
    fn assertion_failure_stops_remaining_rows_and_rerun_selects_exact_cases() {
        let (_temp, engine) = engine();
        let (url, server) = http_server(1);
        let mut workspace = fixture();
        workspace.sessions[0].draft.url = url;
        workspace.sessions[0].draft.assertions = Some(vec![crate::checks::create_assertion(
            crate::model::CheckSource::Status,
            crate::model::CheckOperator::Equals,
            "201",
            "",
        )]);
        workspace.sessions[1].draft.url = "http://must-not-send.invalid".into();
        let mut options = RunOptions {
            request_ids: workspace.sessions.iter().map(|s| s.id).collect(),
            dataset: parse_dataset("item\na\nb", false).unwrap(),
            stop_on_failure: true,
            ..Default::default()
        };
        let report = futures::executor::block_on(run(
            engine.clone(),
            workspace.clone(),
            options.clone(),
            RunControl::default(),
        ))
        .unwrap();
        assert!(!report.passed());
        assert_eq!(
            report.results.iter().map(|r| &r.status).collect::<Vec<_>>(),
            [
                &RunStatus::Failed,
                &RunStatus::Skipped,
                &RunStatus::Skipped,
                &RunStatus::Skipped
            ]
        );
        assert_eq!(server.join().unwrap().len(), 1);
        options.only = Some(report.failed_cases());
        let (url, server) = http_server(1);
        workspace.sessions[0].draft.url = url;
        workspace.sessions[0].draft.assertions = None;
        let rerun =
            futures::executor::block_on(run(engine, workspace, options, RunControl::default()))
                .unwrap();
        assert!(rerun.passed());
        assert_eq!(rerun.results.len(), 1);
        assert_eq!(rerun.results[0].iteration, 0);
        server.join().unwrap();
    }

    #[test]
    fn cancellation_and_invalid_selection_do_not_send() {
        let (_temp, engine) = engine();
        let mut workspace = fixture();
        workspace.sessions[0].draft.url = "http://must-not-send.invalid".into();
        let id = workspace.sessions[0].id;
        let options = RunOptions {
            request_ids: vec![id],
            ..Default::default()
        };
        let control = RunControl::default();
        control.cancel();
        let report = futures::executor::block_on(run(
            engine.clone(),
            workspace.clone(),
            options.clone(),
            control,
        ))
        .unwrap();
        assert_eq!(report.results[0].status, RunStatus::Canceled);
        assert!(
            futures::executor::block_on(run(
                engine,
                workspace,
                RunOptions {
                    request_ids: vec![id, id],
                    ..options
                },
                RunControl::default()
            ))
            .is_err()
        );
    }

    #[test]
    fn group_order_uses_tree_order_and_ignores_other_groups() {
        let mut workspace = fixture();
        let root = workspace.groups[0].id;
        let child = workspace.add_group("Child", Some(root));
        let unrelated = workspace.add_group("Other", None);
        workspace.sessions[0].group_id = Some(child);
        let mut other = crate::session::create_session(None);
        other.group_id = Some(unrelated);
        workspace.sessions.push(other);
        assert_eq!(
            group_order(&workspace, root),
            [workspace.sessions[1].id, workspace.sessions[0].id]
        );
        assert!(group_order(&workspace, u64::MAX).is_empty());
    }

    #[test]
    fn protected_environment_is_rejected_before_any_send() {
        let (_temp, engine) = engine();
        let mut workspace = fixture();
        let mut environment = crate::environments::create_environment(
            "Production",
            crate::model::EnvironmentColor::Info,
        );
        environment.protected = Some(true);
        workspace.groups[0].active_environment_id = Some(environment.id);
        workspace.groups[0].environments = Some(vec![environment]);
        let options = RunOptions {
            request_ids: workspace.sessions.iter().map(|s| s.id).collect(),
            ..Default::default()
        };
        let error =
            futures::executor::block_on(run(engine, workspace, options, RunControl::default()))
                .unwrap_err();
        assert!(error.contains("protected"));
    }

    #[test]
    fn retry_uses_prior_captures_without_exporting_values() {
        let (_temp, engine) = engine();
        let (url, server) = http_server(2);
        let mut workspace = fixture();
        workspace.sessions[0].draft.url = url.clone();
        workspace.sessions[0].draft.captures = Some(vec![crate::checks::create_capture(
            "secret",
            crate::model::CheckSource::Body,
            "",
        )]);
        workspace.sessions[1].draft.url = format!("{url}/{{{{secret}}}}");
        workspace.sessions[1].draft.assertions = Some(vec![crate::checks::create_assertion(
            crate::model::CheckSource::Status,
            crate::model::CheckOperator::Equals,
            "201",
            "",
        )]);
        let mut options = RunOptions {
            request_ids: workspace.sessions.iter().map(|s| s.id).collect(),
            ..Default::default()
        };
        let report = futures::executor::block_on(run(
            engine.clone(),
            workspace.clone(),
            options.clone(),
            RunControl::default(),
        ))
        .unwrap();
        server.join().unwrap();
        assert_eq!(report.results[1].status, RunStatus::Failed);
        assert!(!report.to_json().unwrap().contains("OK"));
        options.only = Some(report.failed_cases());
        options.resume = report.resume;
        let (url, server) = http_server(1);
        workspace.sessions[1].draft.url = format!("{url}/{{{{secret}}}}");
        workspace.sessions[1].draft.assertions = None;
        let report =
            futures::executor::block_on(run(engine, workspace, options, RunControl::default()))
                .unwrap();
        assert!(report.passed());
        assert_eq!(server.join().unwrap(), ["GET /OK HTTP/1.1"]);
    }

    fn exchange(responses: Vec<String>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = std::thread::spawn(move || {
            responses
                .into_iter()
                .map(|response| {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                        .unwrap();
                    let mut request = Vec::new();
                    while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                        let mut buffer = [0; 1024];
                        let n = stream.read(&mut buffer).unwrap();
                        if n == 0 {
                            break;
                        }
                        request.extend_from_slice(&buffer[..n]);
                    }
                    stream.write_all(response.as_bytes()).unwrap();
                    String::from_utf8(request).unwrap()
                })
                .collect()
        });
        (url, task)
    }

    fn response(body: &str, headers: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn report_omits_urls_assertion_values_and_jq_error_payloads() {
        let (_temp, engine) = engine();
        let (url, server) = exchange(vec![response(r#"{"secret":"BODY_SECRET"}"#, "")]);
        let mut workspace = fixture();
        workspace.sessions[0].draft.url = format!("{url}/URL_SECRET");
        workspace.sessions[0].draft.captures = Some(vec![crate::checks::create_capture(
            "value",
            crate::model::CheckSource::Json,
            "error(.)",
        )]);
        workspace.sessions[0].draft.assertions = Some(vec![crate::checks::create_assertion(
            crate::model::CheckSource::Body,
            crate::model::CheckOperator::Equals,
            "EXPECTED_SECRET",
            "",
        )]);
        let options = RunOptions {
            request_ids: vec![workspace.sessions[0].id],
            ..Default::default()
        };
        let report =
            futures::executor::block_on(run(engine, workspace, options, RunControl::default()))
                .unwrap();
        server.join().unwrap();
        assert_eq!(report.results[0].capture_errors, 1);
        let json = report.to_json().unwrap();
        for secret in ["URL_SECRET", "BODY_SECRET", "EXPECTED_SECRET"] {
            assert!(!json.contains(secret), "{json}");
        }
    }

    #[test]
    fn cookies_chain_within_rows_but_do_not_cross_rows_or_enter_desktop_jar() {
        let (_temp, engine) = engine();
        let (url, server) = exchange(vec![
            response("OK", "Set-Cookie: session=one; Path=/\r\n"),
            response("OK", ""),
            response("OK", "Set-Cookie: session=two; Path=/\r\n"),
            response("OK", ""),
        ]);
        let mut workspace = fixture();
        for session in &mut workspace.sessions {
            session.draft.url = url.clone();
        }
        let options = RunOptions {
            request_ids: workspace.sessions.iter().map(|s| s.id).collect(),
            dataset: parse_dataset("row\n1\n2", false).unwrap(),
            ..Default::default()
        };
        let report = futures::executor::block_on(run(
            engine.clone(),
            workspace,
            options,
            RunControl::default(),
        ))
        .unwrap();
        assert!(report.passed());
        let requests = server.join().unwrap();
        assert!(!requests[0].to_lowercase().contains("cookie:"));
        assert!(requests[1].contains("session=one"));
        assert!(!requests[2].to_lowercase().contains("cookie:"));
        assert!(requests[3].contains("session=two"));
        assert!(engine.list_cookies().is_empty());
    }

    #[test]
    fn retry_preserves_capture_event_order_across_groups_in_same_root() {
        let (_temp, engine) = engine();
        let (url, server) = exchange(vec![
            response("x1", ""),
            response("x2", ""),
            response("x3", ""),
            response("OK", ""),
        ]);
        let mut workspace = fixture();
        let root = workspace.groups[0].id;
        let child = workspace.add_group("Child", Some(root));
        workspace.sessions[1].group_id = Some(child);
        for _ in 0..2 {
            let mut session = crate::session::create_session(None);
            session.group_id = Some(root);
            workspace.sessions.push(session);
        }
        for session in &mut workspace.sessions[..3] {
            session.draft.url = url.clone();
            session.draft.captures = Some(vec![crate::checks::create_capture(
                "value",
                crate::model::CheckSource::Body,
                "",
            )]);
        }
        workspace.sessions[3].draft.url = format!("{url}/{{{{value}}}}");
        workspace.sessions[3].draft.assertions = Some(vec![crate::checks::create_assertion(
            crate::model::CheckSource::Status,
            crate::model::CheckOperator::Equals,
            "201",
            "",
        )]);
        let mut options = RunOptions {
            request_ids: workspace.sessions.iter().map(|s| s.id).collect(),
            ..Default::default()
        };
        let report = futures::executor::block_on(run(
            engine.clone(),
            workspace.clone(),
            options.clone(),
            RunControl::default(),
        ))
        .unwrap();
        server.join().unwrap();
        options.only = Some(report.failed_cases());
        options.resume = report.resume;
        let (url, server) = http_server(1);
        workspace.sessions[3].draft.url = format!("{url}/{{{{value}}}}");
        workspace.sessions[3].draft.assertions = None;
        let report =
            futures::executor::block_on(run(engine, workspace, options, RunControl::default()))
                .unwrap();
        assert!(report.passed());
        assert_eq!(server.join().unwrap(), ["GET /x3 HTTP/1.1"]);
    }
}
