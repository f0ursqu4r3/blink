//! Tokens whose values come from another request's response: max age,
//! validation, resolution, and the send plan.

use std::cell::RefCell;
use std::sync::LazyLock;

use indexmap::IndexMap;

use crate::authorization::{ResolvedRequestContext, ancestry, resolve_authorization};
use crate::checks::{INVALID_NAME_MESSAGE, is_capture_name};
use crate::environments::{group_definitions, request_environment};
use crate::interpolation::{InterpolationContext, ResponseTokenInfo};
use crate::model::{CheckSource, Definitions, RequestGroup, RequestSession, ResponseToken};
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
                let (info, value) = self.resolve(token);
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
            let (info, value) = self.resolve(token);
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

    /// The fingerprint `session` would send now. Empty while `session` is
    /// already being fingerprinted, so a token that reads its own request
    /// finds no value.
    pub fn fingerprint(&self, session: &RequestSession) -> String {
        if self.visiting.borrow().contains(&session.id) {
            return String::new();
        }
        self.visiting.borrow_mut().push(session.id);
        let ctx = self.request_context(session);
        let fingerprint = crate::runner::prepare(&session.draft, Some(&ctx)).fingerprint();
        self.visiting.borrow_mut().pop();
        fingerprint
    }

    /// What is known about `token`, and its value when usable.
    fn resolve(&self, token: &ResponseToken) -> (ResponseTokenInfo, Option<String>) {
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
        let fingerprint = self.fingerprint(source);
        let value = self
            .cache
            .lookup(token.request_id, &fingerprint, &key)
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
        let _ = all.context(Some(1));
        let _ = all.fingerprint(&source);
    }
}
