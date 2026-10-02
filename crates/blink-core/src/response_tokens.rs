//! Tokens whose values come from another request's response: max age,
//! validation, resolution, and the send plan.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use indexmap::IndexMap;

use crate::authorization::{ResolvedRequestContext, ancestry, resolve_authorization};
use crate::checks::{INVALID_NAME_MESSAGE, is_capture_name, read_source};
use crate::environments::{group_definitions, request_environment};
use crate::interpolation::{InterpolationContext, ResponseTokenInfo, TOKEN_RE};
use crate::model::{
    ApiResponse, AuthorizationConfig, CheckSource, Definitions, RequestGroup, RequestSession,
    ResponseToken,
};
use crate::response_token_cache::{ResponseTokenCache, ValueKey};
use crate::session::{LabelTokens, session_label};

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

/// The problem of a response token whose source request is gone.
pub fn deleted_request_problem() -> String {
    "reads a deleted request.".to_string()
}

static EMPTY_CACHE: LazyLock<ResponseTokenCache> = LazyLock::new(ResponseTokenCache::default);

/// Fingerprints by request id. A missing id reads as no fingerprint.
type Fingerprints = HashMap<u64, String>;

/// Everything token resolution reads: text tokens, response tokens, the
/// requests they read, and the cached values at `now_ms`.
pub struct TokenSources<'a> {
    pub groups: &'a [RequestGroup],
    pub globals: &'a Definitions,
    pub global_response_tokens: &'a [ResponseToken],
    pub sessions: &'a [RequestSession],
    pub cache: &'a ResponseTokenCache,
    pub now_ms: f64,
    /// The fingerprints of every request a response token reads, solved on
    /// first use.
    fingerprints: RefCell<Option<Fingerprints>>,
    /// Fingerprints built: the cost of resolution.
    #[cfg(test)]
    prepares: std::cell::Cell<usize>,
}

impl<'a> TokenSources<'a> {
    /// All sources at time zero. Use `at` to set the time.
    pub fn new(
        groups: &'a [RequestGroup],
        globals: &'a Definitions,
        global_response_tokens: &'a [ResponseToken],
        sessions: &'a [RequestSession],
        cache: &'a ResponseTokenCache,
    ) -> Self {
        TokenSources {
            groups,
            globals,
            global_response_tokens,
            sessions,
            cache,
            now_ms: 0.0,
            fingerprints: RefCell::new(None),
            #[cfg(test)]
            prepares: std::cell::Cell::new(0),
        }
    }

    /// Text tokens only: no response tokens, empty cache.
    pub fn text(groups: &'a [RequestGroup], globals: &'a Definitions) -> Self {
        Self::new(groups, globals, &[], &[], &EMPTY_CACHE)
    }

    /// The same sources at `now_ms`.
    pub fn at(mut self, now_ms: f64) -> Self {
        self.now_ms = now_ms;
        self.fingerprints = RefCell::new(None);
        self
    }

    /// Merged group and global tokens for `group_id`, with response tokens.
    /// The nearest scope wins; in one scope, text tokens win over response
    /// tokens.
    pub fn context(&self, group_id: Option<u64>) -> InterpolationContext {
        self.with_fingerprints(|fingerprints| self.context_with(group_id, fingerprints))
    }

    /// The resolved auth and tokens of `session`.
    pub fn request_context(&self, session: &RequestSession) -> ResolvedRequestContext {
        self.with_fingerprints(|fingerprints| self.request_context_with(session, fingerprints))
    }

    /// The fingerprint `session` would send now.
    ///
    /// All source fingerprints are solved together on first use (see
    /// `solve`) and remembered, so each is built about once, and no result
    /// depends on the order in which tokens are declared or read.
    /// `TokenSources` is short-lived, so the memo cannot go stale.
    pub fn fingerprint(&self, session: &RequestSession) -> String {
        self.with_fingerprints(|fingerprints| match fingerprints.get(&session.id) {
            Some(fingerprint) => fingerprint.clone(),
            None => self.build(session, fingerprints),
        })
    }

    fn with_fingerprints<T>(&self, f: impl FnOnce(&Fingerprints) -> T) -> T {
        if self.fingerprints.borrow().is_none() {
            let solved = self.solve();
            *self.fingerprints.borrow_mut() = Some(solved);
        }
        f(self.fingerprints.borrow().as_ref().expect("solved above"))
    }

    /// The fingerprint of every request a response token reads.
    ///
    /// A request depends on the requests read by the response tokens it may
    /// reference (`used_names`). The strongly connected groups of that graph
    /// are solved dependencies first. A request outside any cycle is built
    /// once, from final fingerprints. A cycle group is iterated to a fixed
    /// point, starting with no fingerprints. A fingerprint is non-empty only
    /// when every token the request sends has a value, and each such value
    /// comes from a non-empty fingerprint that is already final, so every
    /// non-empty result is final and the loop ends within the group size plus
    /// one round. A request in a real use cycle keeps an empty fingerprint,
    /// so the lookup misses and its tokens read no value. A false use (a
    /// disabled row, a body that is not sent) only joins requests into one
    /// group; it does not change their results.
    fn solve(&self) -> Fingerprints {
        let mut nodes: Vec<&RequestSession> = Vec::new();
        let all_tokens = self
            .groups
            .iter()
            .flat_map(|g| g.response_tokens.iter().flatten())
            .chain(self.global_response_tokens);
        for token in all_tokens {
            if let Some(source) = self.sessions.iter().find(|s| s.id == token.request_id)
                && !nodes.iter().any(|n| n.id == source.id)
            {
                nodes.push(source);
            }
        }
        let edges: Vec<Vec<usize>> = nodes
            .iter()
            .map(|session| {
                let used = self.used_names(session);
                let mut targets: Vec<usize> = self
                    .claims(session.group_id)
                    .into_iter()
                    .filter(|token| used.contains(&token.name))
                    .filter_map(|token| nodes.iter().position(|n| n.id == token.request_id))
                    .collect();
                targets.sort_unstable();
                targets.dedup();
                targets
            })
            .collect();

        let mut fingerprints = Fingerprints::new();
        for component in strongly_connected(&edges) {
            let cyclic = component.len() > 1 || edges[component[0]].contains(&component[0]);
            let mut settled = false;
            for _ in 0..=component.len() {
                let mut changed = false;
                for &node in &component {
                    let fingerprint = self.build(nodes[node], &fingerprints);
                    let old = fingerprints.get(&nodes[node].id).map_or("", String::as_str);
                    changed |= old != fingerprint;
                    fingerprints.insert(nodes[node].id, fingerprint);
                }
                if !cyclic || !changed {
                    settled = true;
                    break;
                }
            }
            // A valid cache settles within the group size plus one round (see
            // above). Only corrupt cache contents can keep changing; then the
            // group reads no values.
            if !settled {
                for &node in &component {
                    fingerprints.insert(nodes[node].id, String::new());
                }
            }
        }
        fingerprints
    }

    /// The fingerprint of `session`, with sources read from `fingerprints`.
    fn build(&self, session: &RequestSession, fingerprints: &Fingerprints) -> String {
        #[cfg(test)]
        self.prepares.set(self.prepares.get() + 1);
        let ctx = self.request_context_with(session, fingerprints);
        crate::runner::prepare(&session.draft, Some(&ctx)).fingerprint()
    }

    fn request_context_with(
        &self,
        session: &RequestSession,
        fingerprints: &Fingerprints,
    ) -> ResolvedRequestContext {
        ResolvedRequestContext {
            auth: self.auth(session),
            tokens: self.context_with(session.group_id, fingerprints),
        }
    }

    fn auth(&self, session: &RequestSession) -> AuthorizationConfig {
        resolve_authorization(
            session.draft.local_auth.as_ref(),
            session.group_id,
            self.groups,
        )
    }

    /// The response tokens that own their names for `group_id`, by the same
    /// rules as `context_with`.
    fn claims(&self, group_id: Option<u64>) -> Vec<&'a ResponseToken> {
        let mut texts: HashSet<String> = HashSet::new();
        let mut group = Vec::<&ResponseToken>::new();
        for scope in ancestry(group_id, self.groups) {
            for key in group_definitions(scope).into_keys() {
                if !group.iter().any(|t| t.name == key) {
                    texts.insert(key);
                }
            }
            for token in scope.response_tokens.iter().flatten() {
                if !texts.contains(&token.name) && !group.iter().any(|t| t.name == token.name) {
                    group.push(token);
                }
            }
        }
        let mut global = Vec::<&ResponseToken>::new();
        for token in self.global_response_tokens {
            if !self.globals.contains_key(&token.name)
                && !global.iter().any(|t| t.name == token.name)
            {
                global.push(token);
            }
        }
        group.extend(global);
        group
    }

    fn context_with(
        &self,
        group_id: Option<u64>,
        fingerprints: &Fingerprints,
    ) -> InterpolationContext {
        let mut definitions = Definitions::new();
        let mut response_tokens = IndexMap::new();
        for group in ancestry(group_id, self.groups) {
            for (key, value) in group_definitions(group) {
                // A nearer response token without a value still owns its name.
                if !response_tokens.contains_key(&key) {
                    definitions.entry(key).or_insert(value);
                }
            }
            for token in group.response_tokens.iter().flatten() {
                if definitions.contains_key(&token.name)
                    || response_tokens.contains_key(&token.name)
                {
                    continue;
                }
                let (info, value) = self.resolve(token, fingerprints);
                if let Some(value) = value {
                    definitions.insert(token.name.clone(), value);
                }
                response_tokens.insert(token.name.clone(), info);
            }
        }

        let mut workspace_definitions = self.globals.clone();
        let mut workspace_response_tokens = IndexMap::new();
        for token in self.global_response_tokens {
            if self.globals.contains_key(&token.name)
                || workspace_response_tokens.contains_key(&token.name)
            {
                continue;
            }
            let (info, value) = self.resolve(token, fingerprints);
            if let Some(value) = value {
                workspace_definitions.insert(token.name.clone(), value);
            }
            workspace_response_tokens.insert(token.name.clone(), info);
        }

        InterpolationContext {
            definitions,
            workspace_definitions,
            response_tokens,
            workspace_response_tokens,
        }
    }

    /// Every token name `session` can reach: names in its draft and auth,
    /// and names in the text tokens those names refer to. A superset is
    /// safe: a false name only adds a dependency, which can join requests
    /// into one cycle group, and `solve` iterates a group to the result that
    /// real use gives.
    fn used_names(&self, session: &RequestSession) -> HashSet<String> {
        let texts: Vec<Definitions> = ancestry(session.group_id, self.groups)
            .into_iter()
            .map(group_definitions)
            .chain(std::iter::once(self.globals.clone()))
            .collect();
        let mut pending = vec![
            serde_json::to_string(&session.draft).unwrap_or_default(),
            serde_json::to_string(&self.auth(session)).unwrap_or_default(),
        ];
        let mut names = HashSet::new();
        while let Some(text) = pending.pop() {
            for caps in TOKEN_RE.captures_iter(&text) {
                let name = caps[2].to_string();
                if names.insert(name.clone()) {
                    pending.extend(texts.iter().filter_map(|t| t.get(&name).cloned()));
                }
            }
        }
        names
    }

    /// What is known about `token`, and its value when usable.
    fn resolve(
        &self,
        token: &ResponseToken,
        fingerprints: &Fingerprints,
    ) -> (ResponseTokenInfo, Option<String>) {
        let Some(source) = self.sessions.iter().find(|s| s.id == token.request_id) else {
            let info = ResponseTokenInfo {
                request_label: "a deleted request".to_string(),
                source: token.source,
                path: token.path.clone(),
                fetched_at_ms: None,
                environment: None,
                problem: Some(deleted_request_problem()),
            };
            return (info, None);
        };
        let key = ValueKey {
            source: token.source,
            path: token.path.clone(),
        };
        let value = fingerprints
            .get(&source.id)
            .filter(|fingerprint| !fingerprint.is_empty())
            .and_then(|fingerprint| self.cache.lookup(token.request_id, fingerprint, &key))
            .filter(|(_, at)| match token.max_age_secs {
                None => true,
                Some(secs) => self.now_ms - *at as f64 <= secs as f64 * 1000.0,
            })
            .map(|(value, at)| (value.to_string(), at));
        let info = ResponseTokenInfo {
            request_label: session_label(
                source,
                Some(LabelTokens {
                    groups: self.groups,
                    global_definitions: self.globals,
                }),
            ),
            source: token.source,
            path: token.path.clone(),
            fetched_at_ms: value.as_ref().map(|(_, at)| *at),
            environment: request_environment(source.group_id, self.groups).map(|e| e.name.clone()),
            problem: None,
        };
        (info, value.map(|(value, _)| value))
    }
}

/// A source request to send before the request that needs its token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    /// The source request to send.
    pub request_id: u64,
    /// The token that needs it, for messages. When several tokens read the
    /// same request, the first one found names the step.
    pub token: String,
}

/// Why a source request did not give its token a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyFailure {
    /// The source returned this status, which the send treats as a failure.
    Status(u16),
    /// The transport failed. The engine text never holds token values.
    Error(String),
    /// The response has no value at `path`. Empty for a status or body source.
    Missing {
        path: String,
    },
    Cancelled,
}

/// The values `response` gives for each distinct `(source, path)` that a token
/// reading `request_id` uses. A source that is absent or fails is skipped.
pub fn values_for<'a>(
    request_id: u64,
    tokens: impl Iterator<Item = &'a ResponseToken>,
    response: &ApiResponse,
) -> Vec<(ValueKey, String)> {
    let mut seen = HashSet::new();
    let mut values = Vec::new();
    for token in tokens.filter(|token| token.request_id == request_id) {
        let key = ValueKey {
            source: token.source,
            path: token.path.clone(),
        };
        if !seen.insert(key.clone()) {
            continue;
        }
        if let Ok(Some(value)) = read_source(key.source, &key.path, response) {
            values.push((key, value));
        }
    }
    values
}

/// The message for a source request that did not supply `token`.
pub fn dependency_error(token: &str, label: &str, failure: &DependencyFailure) -> String {
    let what = match failure {
        DependencyFailure::Status(status) => format!("returned {status}."),
        DependencyFailure::Error(error) => format!("failed. {error}"),
        DependencyFailure::Missing { path } if path.is_empty() => "has no value.".to_string(),
        DependencyFailure::Missing { path } => format!("has no value at {path}."),
        DependencyFailure::Cancelled => "was cancelled.".to_string(),
    };
    format!("Could not get \"{token}\": \"{label}\" {what}")
}

/// Source requests to send before `session_id`, in order. Empty when every
/// response token it uses has a usable value. Err for a cycle or a deleted
/// source.
///
/// Only tokens the request sends count: a disabled row, a body that is not
/// sent, or an overridden auth does not make a dependency.
pub fn response_token_plan(
    session_id: u64,
    sources: &TokenSources,
) -> Result<Vec<PlanStep>, String> {
    let mut plan = Vec::new();
    let mut stack = vec![session_id];
    sources.plan_into(session_id, &mut stack, &mut plan)?;
    Ok(plan)
}

/// Encloses each probe marker. Digits only, so a marker stays intact through
/// URL and form encoding, and is valid in a JSON string or number. No proper
/// prefix is also a suffix, so a fence cannot start inside another marker.
const PROBE_FENCE: &str = "7302958";

/// The probe value of the token at `index` in the probe key list.
///
/// GraphQL variables are parsed and written back, so a marker in a number
/// position must stay a u64: 18 digits up to index 9999, 19 up to 99999.
/// A request cannot reach more response tokens than that in practice.
fn probe_marker(index: usize) -> String {
    format!("{PROBE_FENCE}{index:04}{PROBE_FENCE}")
}

impl TokenSources<'_> {
    fn label(&self, session: &RequestSession) -> String {
        session_label(
            session,
            Some(LabelTokens {
                groups: self.groups,
                global_definitions: self.globals,
            }),
        )
    }

    /// The response token that owns `key` for `group_id`. A key that starts
    /// with `_.` names a global response token.
    fn claimed(&self, group_id: Option<u64>, key: &str) -> Option<&ResponseToken> {
        match key.strip_prefix("_.") {
            Some(name) => self
                .global_response_tokens
                .iter()
                .find(|t| t.name == name && !self.globals.contains_key(name)),
            None => self.claims(group_id).into_iter().find(|t| t.name == key),
        }
    }

    /// Append the steps `session_id` needs to `plan`. `stack` holds the
    /// requests being planned, `session_id` last.
    fn plan_into(
        &self,
        session_id: u64,
        stack: &mut Vec<u64>,
        plan: &mut Vec<PlanStep>,
    ) -> Result<(), String> {
        let Some(session) = self.sessions.iter().find(|s| s.id == session_id) else {
            return Ok(());
        };
        let ctx = self.request_context(session);
        for key in self.probe(session, &ctx) {
            let (name, info) = match key.strip_prefix("_.") {
                Some(name) => (name, ctx.tokens.workspace_response_tokens.get(name)),
                None => (key.as_str(), ctx.tokens.response_tokens.get(&key)),
            };
            if let Some(problem) = info.and_then(|info| info.problem.as_ref()) {
                return Err(format!("\"{name}\" {problem}"));
            }
            let Some(token) = self.claimed(session.group_id, &key) else {
                continue;
            };
            if let Some(start) = stack.iter().position(|id| *id == token.request_id) {
                let labels: Vec<String> = stack[start..]
                    .iter()
                    .chain(std::iter::once(&token.request_id))
                    .filter_map(|id| self.sessions.iter().find(|s| s.id == *id))
                    .map(|s| self.label(s))
                    .collect();
                return Err(format!("Response token cycle: {}", labels.join(" → ")));
            }
            if plan.iter().any(|step| step.request_id == token.request_id) {
                continue;
            }
            stack.push(token.request_id);
            self.plan_into(token.request_id, stack, plan)?;
            stack.pop();
            plan.push(PlanStep {
                request_id: token.request_id,
                token: name.to_string(),
            });
        }
        Ok(())
    }

    /// The keys of the response tokens without a usable value that `session`
    /// sends, in order of first appearance. A global key starts with `_.`.
    ///
    /// Each such token gets a marker value and is treated as text; the built
    /// request is then scanned for the markers. A request that does not
    /// build needs nothing: the send reports the real error.
    fn probe(&self, session: &RequestSession, ctx: &ResolvedRequestContext) -> Vec<String> {
        let mut probe = ctx.clone();
        let tokens = &mut probe.tokens;
        let mut keys: Vec<String> = Vec::new();
        let unusable = |map: &IndexMap<String, ResponseTokenInfo>| -> Vec<String> {
            map.iter()
                .filter(|(_, info)| info.fetched_at_ms.is_none())
                .map(|(name, _)| name.clone())
                .collect()
        };
        for name in unusable(&tokens.response_tokens) {
            tokens.response_tokens.shift_remove(&name);
            tokens
                .definitions
                .insert(name.clone(), probe_marker(keys.len()));
            keys.push(name);
        }
        for name in unusable(&tokens.workspace_response_tokens) {
            tokens.workspace_response_tokens.shift_remove(&name);
            tokens
                .workspace_definitions
                .insert(name.clone(), probe_marker(keys.len()));
            keys.push(format!("_.{name}"));
        }
        if keys.is_empty() {
            return Vec::new();
        }

        let mut sent: Vec<String> = Vec::new();
        let headers = if crate::websocket_log::is_web_socket_url(&session.draft.url) {
            let Ok(socket) = crate::runner::build_web_socket_request(&session.draft, Some(&probe))
            else {
                return Vec::new();
            };
            sent.push(socket.url);
            socket.headers
        } else {
            let Ok(request) = crate::runner::prepare(&session.draft, Some(&probe)).request else {
                return Vec::new();
            };
            sent.push(request.url);
            sent.extend(request.body);
            for part in request.multipart.into_iter().flatten() {
                sent.push(part.key);
                sent.push(part.value);
            }
            request.headers
        };
        for header in headers {
            // Basic auth sends its credentials in base64.
            let decoded = header
                .value
                .strip_prefix("Basic ")
                .and_then(|encoded| BASE64.decode(encoded).ok())
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
            sent.push(header.value);
            sent.extend(decoded);
        }
        let text = sent.join("\n");

        let mut found: Vec<(usize, String)> = keys
            .into_iter()
            .enumerate()
            .filter_map(|(index, key)| text.find(&probe_marker(index)).map(|at| (at, key)))
            .collect();
        found.sort_by_key(|(at, _)| *at);
        found.into_iter().map(|(_, key)| key).collect()
    }
}

/// The strongly connected components of `edges` (Tarjan), each listed only
/// after every component it has an edge to.
fn strongly_connected(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct Walk<'e> {
        edges: &'e [Vec<usize>],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        components: Vec<Vec<usize>>,
    }

    impl Walk<'_> {
        fn visit(&mut self, node: usize) {
            self.index[node] = Some(self.next);
            self.low[node] = self.next;
            self.next += 1;
            self.stack.push(node);
            self.on_stack[node] = true;
            for &target in &self.edges[node] {
                match self.index[target] {
                    None => {
                        self.visit(target);
                        self.low[node] = self.low[node].min(self.low[target]);
                    }
                    Some(index) if self.on_stack[target] => {
                        self.low[node] = self.low[node].min(index);
                    }
                    Some(_) => {}
                }
            }
            if Some(self.low[node]) == self.index[node] {
                let mut component = Vec::new();
                while let Some(member) = self.stack.pop() {
                    self.on_stack[member] = false;
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                component.reverse();
                self.components.push(component);
            }
        }
    }

    let mut walk = Walk {
        edges,
        index: vec![None; edges.len()],
        low: vec![0; edges.len()],
        on_stack: vec![false; edges.len()],
        stack: Vec::new(),
        next: 0,
        components: Vec::new(),
    };
    for node in 0..edges.len() {
        if walk.index[node].is_none() {
            walk.visit(node);
        }
    }
    walk.components
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CheckSource, Definitions, RequestGroup, ResponseToken};
    use crate::response_token_cache::{ResponseTokenCache, ValueKey};
    use crate::session::create_session;

    fn group(id: u64, tokens: Vec<ResponseToken>) -> RequestGroup {
        RequestGroup {
            id,
            name: format!("G{id}"),
            parent_id: None,
            collapsed: false,
            local_auth: None,
            local_definitions: None,
            response_tokens: Some(tokens),
            default_method: None,
            default_url: None,
            environments: None,
            active_environment_id: None,
        }
    }

    fn login(group_id: u64) -> RequestSession {
        let mut session = create_session(None);
        session.group_id = Some(group_id);
        session.draft.url = "https://api.test/login".into();
        session
    }

    fn sources<'a>(
        groups: &'a [RequestGroup],
        globals: &'a Definitions,
        sessions: &'a [RequestSession],
        cache: &'a ResponseTokenCache,
        now_ms: f64,
    ) -> TokenSources<'a> {
        TokenSources::new(groups, globals, &[], sessions, cache).at(now_ms)
    }

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

    #[test]
    fn a_cached_value_for_the_current_fingerprint_resolves() {
        let source = login(1);
        let mut token = token(5, "access_token", source.id);
        token.max_age_secs = Some(60);
        let groups = vec![group(1, vec![token])];
        let sessions = vec![source.clone()];
        let globals = Definitions::new();
        let mut cache = ResponseTokenCache::default();
        let fingerprint =
            TokenSources::new(&groups, &globals, &[], &sessions, &cache).fingerprint(&source);
        cache.record(
            source.id,
            &fingerprint,
            1_000,
            vec![(
                ValueKey {
                    source: CheckSource::Json,
                    path: ".access_token".into(),
                },
                "abc".into(),
            )],
        );

        let fresh =
            sources(&groups, &globals, &sessions, &cache, 1_000.0 + 60_000.0).context(Some(1));
        assert_eq!(fresh.definitions["access_token"], "abc");
        assert_eq!(
            fresh.response_tokens["access_token"].fetched_at_ms,
            Some(1_000)
        );

        let old =
            sources(&groups, &globals, &sessions, &cache, 1_000.0 + 60_001.0).context(Some(1));
        assert!(!old.definitions.contains_key("access_token"));
        assert_eq!(old.response_tokens["access_token"].fetched_at_ms, None);
    }

    #[test]
    fn another_fingerprint_does_not_resolve() {
        let source = login(1);
        let groups = vec![group(1, vec![token(5, "access_token", source.id)])];
        let sessions = vec![source.clone()];
        let globals = Definitions::new();
        let mut cache = ResponseTokenCache::default();
        cache.record(
            source.id,
            "some other request",
            0,
            vec![(
                ValueKey {
                    source: CheckSource::Json,
                    path: ".access_token".into(),
                },
                "abc".into(),
            )],
        );
        let ctx = sources(&groups, &globals, &sessions, &cache, 0.0).context(Some(1));
        assert!(!ctx.definitions.contains_key("access_token"));
    }

    #[test]
    fn a_nearer_text_token_wins_over_a_farther_response_token() {
        let source = login(1);
        let mut parent = group(1, vec![token(5, "t", source.id)]);
        parent.local_definitions = None;
        let mut child = group(2, vec![]);
        child.parent_id = Some(1);
        child.local_definitions = Some(
            [("t".to_string(), "text".to_string())]
                .into_iter()
                .collect(),
        );
        let groups = vec![parent, child];
        let sessions = vec![source];
        let globals = Definitions::new();
        let cache = ResponseTokenCache::default();
        let ctx = sources(&groups, &globals, &sessions, &cache, 0.0).context(Some(2));
        assert_eq!(ctx.definitions["t"], "text");
        assert!(ctx.response_info("t").is_none());
    }

    #[test]
    fn a_deleted_source_reports_a_problem() {
        let groups = vec![group(1, vec![token(5, "t", 9_999)])];
        let globals = Definitions::new();
        let cache = ResponseTokenCache::default();
        let ctx = sources(&groups, &globals, &[], &cache, 0.0).context(Some(1));
        assert_eq!(
            ctx.response_tokens["t"].problem.as_deref(),
            Some("reads a deleted request.")
        );
    }

    #[test]
    fn a_source_that_uses_its_own_token_does_not_recurse() {
        let mut source = login(1);
        source
            .draft
            .headers
            .push(crate::request::pair("Authorization", "Bearer {{t}}"));
        let groups = vec![group(1, vec![token(5, "t", source.id)])];
        let sessions = vec![source.clone()];
        let globals = Definitions::new();
        let cache = ResponseTokenCache::default();
        let all = sources(&groups, &globals, &sessions, &cache, 0.0);
        let ctx = all.context(Some(1));
        let _ = all.fingerprint(&source);
        assert!(!ctx.definitions.contains_key("t"));
        assert_eq!(ctx.response_tokens["t"].fetched_at_ms, None);
    }

    /// A request in no group that sends `{{uses}}` in a header.
    fn request(uses: &[&str]) -> RequestSession {
        let mut session = create_session(None);
        session.draft.url = format!("https://api.test/r{}", session.id);
        for name in uses {
            session
                .draft
                .headers
                .push(crate::request::pair("X-Token", format!("{{{{{name}}}}}")));
        }
        session
    }

    fn json_value() -> ValueKey {
        ValueKey {
            source: CheckSource::Json,
            path: ".access_token".into(),
        }
    }

    #[test]
    fn requests_that_read_each_other_do_not_recurse() {
        let mut a = login(1);
        let mut b = login(1);
        a.draft
            .headers
            .push(crate::request::pair("Authorization", "Bearer {{tb}}"));
        b.draft
            .headers
            .push(crate::request::pair("Authorization", "Bearer {{ta}}"));
        let groups = vec![group(1, vec![token(5, "ta", a.id), token(6, "tb", b.id)])];
        let sessions = vec![a.clone(), b.clone()];
        let globals = Definitions::new();
        let cache = ResponseTokenCache::default();
        let all = sources(&groups, &globals, &sessions, &cache, 0.0);
        let ctx = all.context(Some(1));
        let _ = all.fingerprint(&a);
        let _ = all.fingerprint(&b);
        for name in ["ta", "tb"] {
            assert!(!ctx.definitions.contains_key(name));
            assert_eq!(ctx.response_tokens[name].fetched_at_ms, None);
        }
    }

    #[test]
    fn a_nearer_response_token_without_a_value_hides_a_farther_text_token() {
        let source = login(1);
        let mut parent = group(1, vec![]);
        parent.local_definitions =
            Some([("t".to_string(), "far".to_string())].into_iter().collect());
        let mut child = group(2, vec![token(5, "t", source.id)]);
        child.parent_id = Some(1);
        let groups = vec![parent, child];
        let sessions = vec![source];
        let globals = Definitions::new();
        let cache = ResponseTokenCache::default();
        let ctx = sources(&groups, &globals, &sessions, &cache, 0.0).context(Some(2));
        assert!(!ctx.definitions.contains_key("t"));
        let info = ctx.response_info("t").expect("the nearer response token");
        assert_eq!(info, &ctx.response_tokens["t"]);
        assert_eq!(info.fetched_at_ms, None);
        assert_eq!(info.problem, None);
    }

    /// Global token `t{i}` reads request `i`; request `i` sends `uses(i)`.
    fn global_chain(
        uses: impl Fn(usize) -> Vec<String>,
    ) -> (Vec<RequestSession>, Vec<ResponseToken>) {
        let sessions: Vec<RequestSession> = (0..8)
            .map(|i| {
                let names = uses(i);
                request(&names.iter().map(String::as_str).collect::<Vec<_>>())
            })
            .collect();
        let tokens = sessions
            .iter()
            .enumerate()
            .map(|(i, s)| token(100 + i as u64, &format!("t{i}"), s.id))
            .collect();
        (sessions, tokens)
    }

    #[test]
    fn a_chain_of_eight_response_tokens_fingerprints_each_request_once() {
        // Request i sends t{i+1}; the last sends nothing.
        let (sessions, tokens) = global_chain(|i| {
            if i < 7 {
                vec![format!("t{}", i + 1)]
            } else {
                vec![]
            }
        });
        let globals = Definitions::new();
        let mut cache = ResponseTokenCache::default();
        // Send the chain from the end, as a user would.
        for i in (0..8).rev() {
            let fingerprint = TokenSources::new(&[], &globals, &tokens, &sessions, &cache)
                .fingerprint(&sessions[i]);
            cache.record(
                sessions[i].id,
                &fingerprint,
                0,
                vec![(json_value(), format!("v{i}"))],
            );
        }

        let all = TokenSources::new(&[], &globals, &tokens, &sessions, &cache);
        let ctx = all.context(None);
        assert_eq!(all.prepares.get(), 8);
        for i in 0..8 {
            assert_eq!(ctx.workspace_definitions[&format!("t{i}")], format!("v{i}"));
        }
    }

    #[test]
    fn eight_requests_that_use_every_token_fingerprint_each_request_once() {
        let (sessions, tokens) = global_chain(|_| (0..8).map(|j| format!("t{j}")).collect());
        let globals = Definitions::new();
        let cache = ResponseTokenCache::default();
        let all = TokenSources::new(&[], &globals, &tokens, &sessions, &cache);
        let ctx = all.context(None);
        assert_eq!(all.prepares.get(), 8);
        assert!(
            ctx.workspace_response_tokens
                .values()
                .all(|info| info.fetched_at_ms.is_none())
        );
    }

    #[test]
    fn an_unused_token_does_not_change_a_remembered_fingerprint() {
        // A sends tb, B sends tc, C sends nothing. Every request also sees
        // every token, but uses only its own.
        let c = request(&[]);
        let b = request(&["tc"]);
        let a = request(&["tb"]);
        let tokens = vec![
            token(1, "tc", c.id),
            token(2, "ta", a.id),
            token(3, "tb", b.id),
        ];
        let sessions = vec![a.clone(), b.clone(), c.clone()];
        let globals = Definitions::new();
        let mut cache = ResponseTokenCache::default();
        for (session, value) in [(&c, "vc"), (&b, "vb"), (&a, "va")] {
            let fingerprint =
                TokenSources::new(&[], &globals, &tokens, &sessions, &cache).fingerprint(session);
            cache.record(
                session.id,
                &fingerprint,
                0,
                vec![(json_value(), value.into())],
            );
        }

        let ctx = TokenSources::new(&[], &globals, &tokens, &sessions, &cache).context(None);
        assert_eq!(ctx.workspace_definitions["tc"], "vc");
        assert_eq!(ctx.workspace_definitions["tb"], "vb");
        assert_eq!(ctx.workspace_definitions["ta"], "va");
    }

    /// A sends `tb` only in a disabled header, B sends `ta`, C sends `tb`.
    /// The false use A -> B must not make B's fingerprint degenerate.
    fn disabled_use_case(tb_first: bool) {
        let mut a = request(&[]);
        let mut disabled = crate::request::pair("X", "{{tb}}");
        disabled.enabled = false;
        a.draft.headers.push(disabled);
        let mut b = request(&[]);
        b.draft
            .headers
            .push(crate::request::pair("Authorization", "Bearer {{ta}}"));
        let c = request(&["tb"]);
        let mut tokens = vec![token(1, "ta", a.id), token(2, "tb", b.id)];
        if tb_first {
            tokens.reverse();
        }
        let sessions = vec![a.clone(), b.clone(), c.clone()];
        let globals = Definitions::new();
        let mut cache = ResponseTokenCache::default();
        for (session, value) in [(&a, "va"), (&b, "vb")] {
            let fingerprint =
                TokenSources::new(&[], &globals, &tokens, &sessions, &cache).fingerprint(session);
            assert!(!fingerprint.is_empty());
            cache.record(
                session.id,
                &fingerprint,
                0,
                vec![(json_value(), value.into())],
            );
        }

        let all = TokenSources::new(&[], &globals, &tokens, &sessions, &cache);
        let ctx = all.context(None);
        assert_eq!(ctx.workspace_definitions["ta"], "va");
        assert_eq!(ctx.workspace_definitions["tb"], "vb");
        assert!(!all.fingerprint(&c).is_empty());
    }

    #[test]
    fn a_disabled_use_does_not_hide_a_value_when_ta_is_declared_first() {
        disabled_use_case(false);
    }

    #[test]
    fn a_disabled_use_does_not_hide_a_value_when_tb_is_declared_first() {
        disabled_use_case(true);
    }

    /// The fingerprint `session` sends with global token `name` set to `value`.
    fn fingerprint_with(session: &RequestSession, name: &str, value: &str) -> String {
        let ctx = ResolvedRequestContext {
            auth: AuthorizationConfig::None,
            tokens: InterpolationContext::new(
                Definitions::new(),
                [(name.to_string(), value.to_string())]
                    .into_iter()
                    .collect(),
            ),
        };
        crate::runner::prepare(&session.draft, Some(&ctx)).fingerprint()
    }

    #[test]
    fn a_corrupt_cache_entry_under_no_fingerprint_does_not_loop() {
        // S sends tx (reads X); X sends ts (reads S).
        let s = request(&["tx"]);
        let x = request(&["ts"]);
        let tokens = vec![token(1, "ts", s.id), token(2, "tx", x.id)];
        let sessions = vec![s.clone(), x.clone()];
        let globals = Definitions::new();
        let entry = |id: u64, fingerprint: &str, value: &str| {
            serde_json::json!({
                "requestId": id,
                "fingerprint": fingerprint,
                "fetchedAtMs": 0,
                "values": [[{"source": "json", "path": ".access_token"}, value]],
            })
        };
        // S@"" -> ts=a, X@fp(X|ts=a) -> tx=b, S@fp(S|tx=b) -> ts=c.
        let cache: ResponseTokenCache = serde_json::from_value(serde_json::json!({
            "entries": [
                entry(s.id, "", "a"),
                entry(x.id, &fingerprint_with(&x, "ts", "a"), "b"),
                entry(s.id, &fingerprint_with(&s, "tx", "b"), "c"),
            ]
        }))
        .unwrap();

        let ctx = TokenSources::new(&[], &globals, &tokens, &sessions, &cache).context(None);
        for name in ["ts", "tx"] {
            assert!(!ctx.workspace_definitions.contains_key(name));
            assert_eq!(ctx.workspace_response_tokens[name].fetched_at_ms, None);
        }
    }

    fn workspace_with_login() -> (crate::workspace_state::Workspace, u64, u64) {
        let mut workspace = crate::workspace_state::Workspace::new();
        let login = workspace.sessions[0].id;
        let group_id = workspace.add_group("API", None);
        workspace.move_request(login, Some(group_id));
        workspace.sessions[0].draft.url = "https://api.test/login".into();
        workspace.groups[0].response_tokens = Some(vec![token(1, "access_token", login)]);
        let mut me = create_session(None);
        me.group_id = Some(group_id);
        me.draft.url = "https://api.test/me".into();
        me.draft.headers.push(crate::request::pair(
            "Authorization",
            "Bearer {{access_token}}",
        ));
        let me_id = me.id;
        workspace.sessions.push(me);
        (workspace, login, me_id)
    }

    #[test]
    fn plans_the_source_when_no_value_exists() {
        let (workspace, login, me) = workspace_with_login();
        let plan = response_token_plan(me, &workspace.token_sources(0.0)).unwrap();
        assert_eq!(
            plan,
            vec![PlanStep {
                request_id: login,
                token: "access_token".into()
            }]
        );
    }

    #[test]
    fn plans_nothing_when_the_value_is_usable() {
        let (mut workspace, login, me) = workspace_with_login();
        let fingerprint = workspace
            .token_sources(0.0)
            .fingerprint(workspace.session(login).unwrap());
        workspace.response_cache.record(
            login,
            &fingerprint,
            0,
            vec![(
                ValueKey {
                    source: CheckSource::Json,
                    path: ".access_token".into(),
                },
                "abc".into(),
            )],
        );
        assert!(
            response_token_plan(me, &workspace.token_sources(0.0))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn finds_tokens_through_text_tokens_and_ignores_disabled_headers() {
        let (mut workspace, _, me) = workspace_with_login();
        let session = workspace.session_mut(me).unwrap();
        session.draft.headers.clear();
        let mut disabled = crate::request::pair("X-Unused", "{{access_token}}");
        disabled.enabled = false;
        session.draft.headers.push(disabled);
        assert!(
            response_token_plan(me, &workspace.token_sources(0.0))
                .unwrap()
                .is_empty()
        );
        workspace.groups[0].local_definitions = Some(
            [("auth".to_string(), "Bearer {{access_token}}".to_string())]
                .into_iter()
                .collect(),
        );
        workspace
            .session_mut(me)
            .unwrap()
            .draft
            .headers
            .push(crate::request::pair("Authorization", "{{auth}}"));
        assert_eq!(
            response_token_plan(me, &workspace.token_sources(0.0))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn orders_chains_and_reports_cycles() {
        let (mut workspace, login, me) = workspace_with_login();
        let mut refresh = create_session(None);
        refresh.group_id = workspace.sessions[0].group_id;
        refresh.draft.url = "https://api.test/refresh?r={{refresh_token}}".into();
        let refresh_id = refresh.id;
        workspace.sessions.push(refresh);
        let tokens = workspace.groups[0].response_tokens.as_mut().unwrap();
        tokens[0].request_id = refresh_id; // access_token now comes from Refresh
        tokens.push(token(2, "refresh_token", login));
        let plan = response_token_plan(me, &workspace.token_sources(0.0)).unwrap();
        assert_eq!(
            plan.iter().map(|s| s.request_id).collect::<Vec<_>>(),
            vec![login, refresh_id]
        );
        // Login now needs access_token: Login → Refresh → Login.
        workspace.session_mut(login).unwrap().draft.url =
            "https://api.test/login?a={{access_token}}".into();
        let error = response_token_plan(me, &workspace.token_sources(0.0)).unwrap_err();
        assert!(error.starts_with("Response token cycle: "), "{error}");
    }

    #[test]
    fn a_deleted_source_fails_the_plan() {
        let (mut workspace, _, me) = workspace_with_login();
        workspace.groups[0].response_tokens.as_mut().unwrap()[0].request_id = 9_999;
        assert_eq!(
            response_token_plan(me, &workspace.token_sources(0.0)).unwrap_err(),
            "\"access_token\" reads a deleted request."
        );
    }

    #[test]
    fn dependency_errors_name_the_token_and_request_only() {
        assert_eq!(
            dependency_error("access_token", "Login", &DependencyFailure::Status(401)),
            "Could not get \"access_token\": \"Login\" returned 401."
        );
        assert_eq!(
            dependency_error(
                "t",
                "Login",
                &DependencyFailure::Missing { path: ".t".into() }
            ),
            "Could not get \"t\": \"Login\" has no value at .t."
        );
    }

    #[test]
    fn every_dependency_error_has_its_message() {
        assert_eq!(
            dependency_error(
                "access_token",
                "Login",
                &DependencyFailure::Error("Connection refused.".into())
            ),
            "Could not get \"access_token\": \"Login\" failed. Connection refused."
        );
        assert_eq!(
            dependency_error(
                "access_token",
                "Login",
                &DependencyFailure::Missing {
                    path: String::new()
                }
            ),
            "Could not get \"access_token\": \"Login\" has no value."
        );
        assert_eq!(
            dependency_error("access_token", "Login", &DependencyFailure::Cancelled),
            "Could not get \"access_token\": \"Login\" was cancelled."
        );
    }

    #[test]
    fn the_cycle_message_names_only_the_requests_in_the_cycle() {
        let (mut workspace, login, me) = workspace_with_login();
        let mut refresh = create_session(None);
        refresh.group_id = workspace.sessions[0].group_id;
        refresh.draft.url = "https://api.test/refresh?r={{refresh_token}}".into();
        let refresh_id = refresh.id;
        workspace.sessions.push(refresh);
        let tokens = workspace.groups[0].response_tokens.as_mut().unwrap();
        tokens[0].request_id = refresh_id;
        tokens.push(token(2, "refresh_token", login));
        workspace.session_mut(login).unwrap().draft.url =
            "https://api.test/login?a={{access_token}}".into();
        assert_eq!(
            response_token_plan(me, &workspace.token_sources(0.0)).unwrap_err(),
            "Response token cycle: /refresh → /login → /refresh"
        );
    }

    #[test]
    fn a_request_that_reads_its_own_token_is_a_cycle() {
        let (mut workspace, login, _) = workspace_with_login();
        workspace
            .session_mut(login)
            .unwrap()
            .draft
            .headers
            .push(crate::request::pair("X-Token", "{{access_token}}"));
        assert_eq!(
            response_token_plan(login, &workspace.token_sources(0.0)).unwrap_err(),
            "Response token cycle: /login → /login"
        );
    }

    #[test]
    fn finds_tokens_in_query_rows_json_numbers_and_basic_auth() {
        let one = |edit: &dyn Fn(&mut RequestSession)| {
            let (mut workspace, _, me) = workspace_with_login();
            let session = workspace.session_mut(me).unwrap();
            session.draft.headers.clear();
            edit(session);
            response_token_plan(me, &workspace.token_sources(0.0)).unwrap()
        };
        let query = one(&|s| {
            s.draft
                .query
                .push(crate::request::pair("token", "{{access_token}}"))
        });
        assert_eq!(query.len(), 1, "query");
        let json = one(&|s| {
            s.draft.method = "POST".into();
            s.draft.body_mode = crate::model::BodyMode::Json;
            s.draft.body = "{\"id\": {{access_token}}}".into();
        });
        assert_eq!(json.len(), 1, "json");
        let json_string = one(&|s| {
            s.draft.method = "POST".into();
            s.draft.body_mode = crate::model::BodyMode::Json;
            s.draft.body = "{\"token\": \"{{access_token}}\"}".into();
        });
        assert_eq!(json_string.len(), 1, "json string");
        let form = one(&|s| {
            s.draft.method = "POST".into();
            s.draft.body_mode = crate::model::BodyMode::Form;
            s.draft.form = Some(vec![crate::request::pair("token", "{{access_token}}")]);
        });
        assert_eq!(form.len(), 1, "form");
        let graphql = one(&|s| {
            s.draft.method = "POST".into();
            s.draft.body_mode = crate::model::BodyMode::Graphql;
            s.draft.body = "query { me { id } }".into();
            s.draft.variables = Some("{\"id\": {{access_token}}}".into());
        });
        assert_eq!(graphql.len(), 1, "graphql number");
        let basic = one(&|s| {
            s.draft.local_auth = Some(AuthorizationConfig::Basic {
                username: "me".into(),
                password: "{{access_token}}".into(),
            });
        });
        assert_eq!(basic.len(), 1, "basic");
    }

    #[test]
    fn finds_tokens_in_a_web_socket_request_and_global_tokens() {
        let (mut workspace, login, me) = workspace_with_login();
        workspace.groups[0].response_tokens = None;
        workspace.global_response_tokens = vec![token(1, "access_token", login)];
        let session = workspace.session_mut(me).unwrap();
        session.draft.headers.clear();
        session.draft.url = "wss://api.test/socket?t={{_.access_token}}".into();
        let plan = response_token_plan(me, &workspace.token_sources(0.0)).unwrap();
        assert_eq!(
            plan,
            vec![PlanStep {
                request_id: login,
                token: "access_token".into()
            }]
        );
    }

    #[test]
    fn probe_markers_survive_a_json_number_round_trip() {
        for index in [0, 9_999, 99_999] {
            let marker = probe_marker(index);
            let value: serde_json::Value = serde_json::from_str(&marker).unwrap();
            assert_eq!(value.to_string(), marker);
        }
    }

    #[test]
    fn a_request_that_does_not_build_plans_nothing() {
        let (mut workspace, _, me) = workspace_with_login();
        workspace.session_mut(me).unwrap().draft.url = "not a url {{access_token}}".into();
        assert!(
            response_token_plan(me, &workspace.token_sources(0.0))
                .unwrap()
                .is_empty()
        );
    }
}
