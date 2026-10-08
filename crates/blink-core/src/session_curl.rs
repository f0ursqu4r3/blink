//! Port of `src/lib/session-curl.ts`.

use crate::codegen::generate_redacted_code;
use crate::model::{CodeTarget, RequestSession, TransportOptions};
use crate::request::build_request;
use crate::response_tokens::TokenSources;

/// The cURL command for a session, resolved as a send resolves it. Empty when
/// the draft does not build. Literal credentials are masked for sharing.
pub fn session_curl(
    session: &RequestSession,
    sources: &TokenSources,
    options: &TransportOptions,
) -> String {
    let ctx = sources.request_context(session);
    build_request(&session.draft, Some((&ctx).into()))
        .map(|request| generate_redacted_code(CodeTarget::Curl, &request, options))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AuthorizationConfig, Definitions, RequestGroup};
    use crate::session::create_session;

    #[test]
    fn builds_a_curl_command_with_inherited_authorization() {
        let mut session = create_session(None);
        session.draft.url = "https://api.example.test/users".into();
        session.group_id = Some(1);
        let groups = vec![RequestGroup {
            id: 1,
            name: "API".into(),
            parent_id: None,
            collapsed: false,
            local_auth: Some(AuthorizationConfig::Bearer {
                token: "abc".into(),
            }),
            local_definitions: None,
            response_tokens: None,
            default_method: None,
            default_url: None,
            environments: None,
            active_environment_id: None,
        }];
        let command = session_curl(
            &session,
            &TokenSources::text(&groups, &Definitions::new()),
            &TransportOptions::default(),
        );
        assert!(command.starts_with("curl"));
        assert!(command.contains("https://api.example.test/users"));
        assert!(!command.contains("Bearer abc"));
        assert!(command.contains("Bearer [redacted]"));
    }

    #[test]
    fn returns_an_empty_string_when_the_draft_does_not_build() {
        let mut session = create_session(None);
        session.draft.url = "not a url".into();
        assert_eq!(
            session_curl(
                &session,
                &TokenSources::text(&[], &Definitions::new()),
                &TransportOptions::default()
            ),
            ""
        );
    }
}
