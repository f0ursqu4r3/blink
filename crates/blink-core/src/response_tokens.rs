//! Tokens whose values come from another request's response: max age,
//! validation, resolution, and the send plan.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use indexmap::IndexMap;

use crate::authorization::{ResolvedRequestContext, ancestry, resolve_authorization};
use crate::checks::{INVALID_NAME_MESSAGE, is_capture_name};
use crate::environments::{group_definitions, request_environment};
use crate::interpolation::{InterpolationContext, ResponseTokenInfo, TOKEN_RE};
use crate::model::{
    AuthorizationConfig, CheckSource, Definitions, RequestGroup, RequestSession, ResponseToken,
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

/// Everything token resolution reads: text tokens, response tokens, the
/// requests they read, and the cached values at `now_ms`.
pub struct TokenSources<'a> {
    pub groups: &'a [RequestGroup],
    pub globals: &'a Definitions,
    pub global_response_tokens: &'a [ResponseToken],
    pub sessions: &'a [RequestSession],
    pub cache: &'a ResponseTokenCache,
    pub now_ms: f64,
    /// Requests whose fingerprint is being built, to stop recursion.
    visiting: RefCell<Vec<u64>>,
    /// Fingerprints already built, by request id.
    fingerprints: RefCell<HashMap<u64, String>>,
    /// Fingerprints built, not remembered: the cost of resolution.
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
            visiting: RefCell::new(Vec::new()),
            fingerprints: RefCell::new(HashMap::new()),
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
        self
    }

    /// Merged group and global tokens for `group_id`, with response tokens.
    /// The nearest scope wins; in one scope, text tokens win over response
    /// tokens.
    pub fn context(&self, group_id: Option<u64>) -> InterpolationContext {
        self.context_reading(group_id, None)
    }

    /// `context`, but only response tokens named in `used` read a value.
    /// The others keep their names and read nothing.
    fn context_reading(
        &self,
        group_id: Option<u64>,
        used: Option<&HashSet<String>>,
    ) -> InterpolationContext {
        let reads = |token: &ResponseToken| used.is_none_or(|used| used.contains(&token.name));
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
                let (info, value) = self.resolve(token, reads(token));
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
            let (info, value) = self.resolve(token, reads(token));
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

    /// The resolved auth and tokens of `session`.
    pub fn request_context(&self, session: &RequestSession) -> ResolvedRequestContext {
        ResolvedRequestContext {
            auth: resolve_authorization(
                session.draft.local_auth.as_ref(),
                session.group_id,
                self.groups,
            ),
            tokens: self.context(session.group_id),
        }
    }

    /// The fingerprint `session` would send now.
    ///
    /// Empty while `session` is already being fingerprinted, so a token that
    /// reads its own request finds no value.
    ///
    /// Each fingerprint is built once and remembered by request id, at any
    /// depth, so resolution costs about O(k²) for k response tokens instead of
    /// O(k!). The source context reads only the response tokens the request
    /// references, so the recursion follows real use and never reaches an
    /// unrelated request on the `visiting` stack. In an acyclic graph a
    /// fingerprint therefore does not depend on the stack, and the memo is
    /// exact. In a cycle the result is degenerate anyway: the send planner
    /// reports the cycle and the lookup misses. `TokenSources` is
    /// short-lived, so the memo cannot go stale.
    pub fn fingerprint(&self, session: &RequestSession) -> String {
        if let Some(fingerprint) = self.fingerprints.borrow().get(&session.id) {
            return fingerprint.clone();
        }
        if self.visiting.borrow().contains(&session.id) {
            return String::new();
        }
        self.visiting.borrow_mut().push(session.id);
        #[cfg(test)]
        self.prepares.set(self.prepares.get() + 1);
        let auth = resolve_authorization(
            session.draft.local_auth.as_ref(),
            session.group_id,
            self.groups,
        );
        let used = self.used_names(session, &auth);
        let ctx = ResolvedRequestContext {
            auth,
            tokens: self.context_reading(session.group_id, Some(&used)),
        };
        let fingerprint = crate::runner::prepare(&session.draft, Some(&ctx)).fingerprint();
        self.visiting.borrow_mut().pop();
        self.fingerprints
            .borrow_mut()
            .insert(session.id, fingerprint.clone());
        fingerprint
    }

    /// Every token name `session` can reach: names in its draft and auth,
    /// and names in the text tokens those names refer to. A superset is safe.
    fn used_names(&self, session: &RequestSession, auth: &AuthorizationConfig) -> HashSet<String> {
        let texts: Vec<Definitions> = ancestry(session.group_id, self.groups)
            .into_iter()
            .map(group_definitions)
            .chain(std::iter::once(self.globals.clone()))
            .collect();
        let mut pending = vec![
            serde_json::to_string(&session.draft).unwrap_or_default(),
            serde_json::to_string(auth).unwrap_or_default(),
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

    /// What is known about `token`, and its value when usable and `read`.
    fn resolve(&self, token: &ResponseToken, read: bool) -> (ResponseTokenInfo, Option<String>) {
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
        let value = read
            .then(|| self.fingerprint(source))
            .and_then(|fingerprint| {
                self.cache
                    .lookup(token.request_id, &fingerprint, &key)
                    .map(|(value, at)| (value.to_string(), at))
            })
            .filter(|(_, at)| match token.max_age_secs {
                None => true,
                Some(secs) => self.now_ms - *at as f64 <= secs as f64 * 1000.0,
            });
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
}
