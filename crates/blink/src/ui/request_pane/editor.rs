//! The request side: tabs for query, headers, body, auth, and tests.
//! Port of `RequestEditor.vue`.

use std::rc::Rc;
use std::time::Duration;

use blink_core::engine::PickedFile;
use blink_core::graphql::{format_graphql, graphql_error_location};
use blink_core::graphql_schema::{fetch_schema, format_schema_age, get_cached_schema, schema_key};
use blink_core::history::now_ms;
use blink_core::interpolation::InterpolationContext;
use blink_core::json::{format_json, json_error_location};
use blink_core::model::{AuthorizationConfig, BodyMode, Draft, RequestGroup};
use blink_core::preferences::transport_options;
use blink_core::request::{RequestContext, active_pairs, file_name, format_bytes, pair, supports_body};
use blink_core::text_location::{TextLocation, describe_location};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{
    Editor, EditorState, Enter, Input, InputEvent, InputState, Textarea, TextareaState,
};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::resizable::{resizable_panel, v_resizable};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::actions::SendRequest;
use crate::store::{Store, StoreEvent};
use crate::theme;
use crate::ui::key_value_editor::{KeyValueEditor, KeyValueEvent, KeyValueOptions};
use crate::ui::request_pane::checks::ChecksEditor;
use crate::ui::request_pane::code_language::{BodyLanguage, apply_highlight_theme, register_graphql};
use crate::ui::request_pane::common::{
    edit_draft, fill_select, help_link, replace_draft, select_button, session_context,
};
use crate::ui::token_input::{TokenInput, TokenInputEvent};

pub const BODY_MODES: [(BodyMode, &str); 7] = [
    (BodyMode::None, "None"),
    (BodyMode::Json, "JSON"),
    (BodyMode::Text, "Text"),
    (BodyMode::Graphql, "GraphQL"),
    (BodyMode::Form, "Form URL-encoded"),
    (BodyMode::Multipart, "Multipart form"),
    (BodyMode::File, "File"),
];

/// The auth select: inherit, or a local override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthChoice {
    Inherit,
    None,
    Bearer,
    Basic,
}

const AUTH_CHOICES: [(AuthChoice, &str); 4] = [
    (AuthChoice::Inherit, "Inherit"),
    (AuthChoice::None, "No auth"),
    (AuthChoice::Bearer, "Bearer token"),
    (AuthChoice::Basic, "Basic auth"),
];

pub fn auth_choice(draft: &Draft) -> AuthChoice {
    match &draft.local_auth {
        None => AuthChoice::Inherit,
        Some(AuthorizationConfig::None) => AuthChoice::None,
        Some(AuthorizationConfig::Bearer { .. }) => AuthChoice::Bearer,
        Some(AuthorizationConfig::Basic { .. }) => AuthChoice::Basic,
    }
}

/// The local auth for a choice. A new bearer or basic override starts from
/// the draft's flat fields.
pub fn local_auth_for(choice: AuthChoice, draft: &Draft) -> Option<AuthorizationConfig> {
    match choice {
        AuthChoice::Inherit => None,
        AuthChoice::None => Some(AuthorizationConfig::None),
        AuthChoice::Bearer => Some(AuthorizationConfig::Bearer {
            token: draft.token.clone(),
        }),
        AuthChoice::Basic => Some(AuthorizationConfig::Basic {
            username: draft.username.clone(),
            password: draft.password.clone(),
        }),
    }
}

fn auth_type_id(auth: &AuthorizationConfig) -> &'static str {
    match auth {
        AuthorizationConfig::None => "none",
        AuthorizationConfig::Bearer { .. } => "bearer",
        AuthorizationConfig::Basic { .. } => "basic",
    }
}

/// "Group · Bearer": the nearest group whose auth a request inherits.
pub fn inherited_source(draft: &Draft, group_id: Option<u64>, groups: &[RequestGroup]) -> Option<String> {
    if draft.local_auth.is_some() {
        return None;
    }
    let mut cursor = group_id;
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = cursor {
        if !seen.insert(id) {
            break;
        }
        let group = groups.iter().find(|group| group.id == id)?;
        if let Some(auth) = &group.local_auth {
            let label = match auth {
                AuthorizationConfig::Bearer { .. } => "Bearer",
                AuthorizationConfig::Basic { .. } => "Basic",
                AuthorizationConfig::None => "None",
            };
            return Some(format!("{} · {label}", group.name));
        }
        cursor = group.parent_id;
    }
    None
}

/// Tab ids and labels, and each tab's count badge.
pub fn tab_counts(draft: &Draft, effective: &AuthorizationConfig) -> [(&'static str, &'static str, usize); 5] {
    let body = usize::from(supports_body(&draft.method) && draft.body_mode != BodyMode::None);
    let auth = draft.local_auth.as_ref().unwrap_or(effective);
    let tests = draft.assertions.iter().flatten().filter(|row| row.enabled).count()
        + draft.captures.iter().flatten().filter(|row| row.enabled).count();
    [
        ("query", "Query", active_pairs(&draft.query).count()),
        ("headers", "Headers", active_pairs(&draft.headers).count()),
        ("body", "Body", body),
        ("auth", "Auth", usize::from(*auth != AuthorizationConfig::None)),
        ("tests", "Tests", tests),
    ]
}

/// The footer: "0 QUERY · 1 HEADERS" and "AUTH / NONE".
pub fn footer_text(draft: &Draft) -> (String, String) {
    let auth = match &draft.local_auth {
        Some(auth) => auth_type_id(auth),
        None => match draft.auth {
            blink_core::model::AuthKind::None => "none",
            blink_core::model::AuthKind::Bearer => "bearer",
            blink_core::model::AuthKind::Basic => "basic",
        },
    };
    (
        format!(
            "{} QUERY · {} HEADERS",
            active_pairs(&draft.query).count(),
            active_pairs(&draft.headers).count()
        ),
        format!("AUTH / {}", auth.to_uppercase()),
    )
}

/// "Invalid JSON at line 2, column 3: Unexpected token. The body was not changed."
pub fn format_error_text(label: &str, outcome: &str, location: Option<&TextLocation>) -> String {
    match location {
        None => format!("{label}. {outcome}"),
        Some(location) => {
            let reason = location.reason.trim_end_matches('.');
            format!("{label} at {}: {reason}. {outcome}", describe_location(location))
        }
    }
}

/// The GraphQL variables pane: one size and collapsed state for all tabs,
/// reset when the app restarts.
struct VariablesPane {
    height: Pixels,
    collapsed: bool,
}

impl Default for VariablesPane {
    fn default() -> Self {
        VariablesPane {
            height: px(128.),
            collapsed: false,
        }
    }
}

impl Global for VariablesPane {}

const MIN_VARIABLES_HEIGHT: f32 = 64.;
const MIN_QUERY_HEIGHT: f32 = 96.;

/// Which editor a format error points into.
#[derive(Clone, Copy)]
enum ErrorEditor {
    Body,
    Variables,
}

pub struct RequestEditor {
    store: Entity<Store>,
    session_id: u64,
    query: Entity<KeyValueEditor>,
    headers: Entity<KeyValueEditor>,
    form: Entity<KeyValueEditor>,
    body: Entity<EditorState>,
    body_language: &'static str,
    /// Tokens and schema for the body editor's providers.
    body_features: Rc<BodyLanguage>,
    /// Tokens for the variables editor's providers.
    variables_features: Rc<BodyLanguage>,
    text_body: Entity<TextareaState>,
    variables: Entity<EditorState>,
    checks: Entity<ChecksEditor>,
    token: Entity<InputState>,
    username: Entity<TokenInput>,
    password: Entity<InputState>,
    format_error: String,
    schema_loading: bool,
    schema_error: String,
    schema_key: Option<String>,
    body_file_error: String,
    picked_file: Option<PickedFile>,
    context: Option<InterpolationContext>,
    _clock: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl RequestEditor {
    pub fn new(store: Entity<Store>, session_id: u64, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let draft = store
            .read(cx)
            .workspace
            .session(session_id)
            .map(|session| session.draft.clone())
            .unwrap_or_else(blink_core::request::create_draft);
        let table = |_label: &str, key: &str, files: bool| KeyValueOptions {
            key_label: "Name".into(),
            value_label: "Value".into(),
            key_placeholder: "Name".into(),
            value_placeholder: "Value".into(),
            allow_files: files,
            toggles: true,
            id: format!("{key}-{session_id}").into(),
        };
        let labeled = |editor: KeyValueEditor, label: &'static str, cx: &mut Context<KeyValueEditor>| {
            let mut editor = editor;
            editor.set_row_label(label, cx);
            editor
        };
        let query = cx.new(|cx| {
            labeled(KeyValueEditor::new(draft.query.clone(), table("Query", "query", false), window, cx), "Query", cx)
        });
        let headers = cx.new(|cx| {
            labeled(KeyValueEditor::new(draft.headers.clone(), table("Header", "headers", false), window, cx), "Header", cx)
        });
        let form = cx.new(|cx| {
            let editor = KeyValueEditor::new(
                draft.form.clone().unwrap_or_default(),
                table("Form", "form", draft.body_mode == BodyMode::Multipart),
                window,
                cx,
            );
            labeled(editor, "Form", cx)
        });
        register_graphql();
        apply_highlight_theme(cx);
        let body_features = BodyLanguage::new();
        let variables_features = BodyLanguage::new();
        let body_language = body_language(draft.body_mode);
        let body = cx.new(|cx| {
            with_features(EditorState::new(window, cx), &body_features)
                .language(body_language)
                .line_number(false)
                .folding(false)
                .soft_wrap(false)
                .indent_guides(false)
                .placeholder(body_placeholder(draft.body_mode))
        });
        let text_body = cx.new(|cx| TextareaState::new(window, cx).placeholder("Request body"));
        let variables = cx.new(|cx| {
            with_features(EditorState::new(window, cx), &variables_features)
                .language("json")
                .line_number(false)
                .folding(false)
                .soft_wrap(false)
                .indent_guides(false)
                .placeholder("{\n  \"id\": \"1\"\n}")
        });
        let checks = cx.new(|cx| ChecksEditor::new(store.clone(), session_id, window, cx));
        let token = cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("Bearer token"));
        let username = cx.new(|cx| {
            let mut input = TokenInput::new("", window, cx);
            input.set_size(gpui_kit::component::Size::Small, cx);
            input
        });
        let password = cx.new(|cx| InputState::new(window, cx).masked(true));

        let id = session_id;
        let mut subscriptions = vec![
            // A theme change rebuilds the GPUI Kit theme; keep the editor colors.
            cx.observe_global::<gpui_kit::component::Theme>(|_, cx| apply_highlight_theme(cx)),
            cx.observe_in(&store, window, |this, _, _, cx| {
                this.refresh_context(cx);
                cx.notify();
            }),
            cx.subscribe_in(&store, window, move |this, _, event, window, cx| match event {
                StoreEvent::DraftReplaced(replaced) if *replaced == id => this.reload(window, cx),
                StoreEvent::Restored => this.reload(window, cx),
                _ => {}
            }),
            cx.subscribe_in(&body, window, |this, input, event, _, cx| {
                if let InputEvent::Change = event {
                    let text = input.read(cx).value().to_string();
                    this.format_error.clear();
                    edit_draft(&this.store, this.session_id, cx, |draft| draft.body = text);
                }
            }),
            cx.subscribe_in(&text_body, window, |this, input, event, _, cx| {
                if let InputEvent::Change = event {
                    let text = input.read(cx).value().to_string();
                    this.format_error.clear();
                    edit_draft(&this.store, this.session_id, cx, |draft| draft.body = text);
                }
            }),
            cx.subscribe_in(&variables, window, |this, input, event, _, cx| {
                if let InputEvent::Change = event {
                    let text = input.read(cx).value().to_string();
                    this.format_error.clear();
                    edit_draft(&this.store, this.session_id, cx, |draft| {
                        draft.variables = Some(text)
                    });
                }
            }),
            cx.subscribe_in(&token, window, |this, input, event, _, cx| {
                if let InputEvent::Change = event {
                    let token = input.read(cx).value().to_string();
                    edit_draft(&this.store, this.session_id, cx, |draft| {
                        if let Some(AuthorizationConfig::Bearer { token: current }) = &mut draft.local_auth {
                            *current = token;
                        }
                    });
                }
            }),
            cx.subscribe_in(&username, window, |this, _, event: &TokenInputEvent, _, cx| {
                if let TokenInputEvent::Change(text) = event {
                    let text = text.clone();
                    edit_draft(&this.store, this.session_id, cx, |draft| {
                        if let Some(AuthorizationConfig::Basic { username, .. }) = &mut draft.local_auth {
                            *username = text;
                        }
                    });
                }
            }),
            cx.subscribe_in(&password, window, |this, input, event, _, cx| {
                if let InputEvent::Change = event {
                    let text = input.read(cx).value().to_string();
                    edit_draft(&this.store, this.session_id, cx, |draft| {
                        if let Some(AuthorizationConfig::Basic { password, .. }) = &mut draft.local_auth {
                            *password = text;
                        }
                    });
                }
            }),
        ];
        for (table, field) in [(&query, 0u8), (&headers, 1), (&form, 2)] {
            subscriptions.push(cx.subscribe_in(table, window, move |this, editor, event, window, cx| {
                match event {
                    KeyValueEvent::Change(rows) => {
                        let rows = rows.clone();
                        edit_draft(&this.store, this.session_id, cx, |draft| match field {
                            0 => draft.query = rows,
                            1 => draft.headers = rows,
                            _ => draft.form = Some(rows),
                        });
                    }
                    KeyValueEvent::PickFile(row) => this.pick_form_file(editor.clone(), *row, window, cx),
                }
            }));
        }

        let clock = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(60)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });

        let mut editor = RequestEditor {
            store,
            session_id,
            query,
            headers,
            form,
            body,
            body_language,
            body_features,
            variables_features,
            text_body,
            variables,
            checks,
            token,
            username,
            password,
            format_error: String::new(),
            schema_loading: false,
            schema_error: String::new(),
            schema_key: None,
            body_file_error: String::new(),
            picked_file: None,
            context: None,
            _clock: clock,
            _subscriptions: subscriptions,
        };
        editor.load_text(&draft, window, cx);
        editor.refresh_context(cx);
        editor
    }

    fn draft(&self, cx: &App) -> Option<Draft> {
        self.store
            .read(cx)
            .workspace
            .session(self.session_id)
            .map(|session| session.draft.clone())
    }

    /// Reload every input from the draft after it was replaced.
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.draft(cx) else {
            return;
        };
        self.query
            .update(cx, |editor, cx| editor.set_rows(draft.query.clone(), window, cx));
        self.headers
            .update(cx, |editor, cx| editor.set_rows(draft.headers.clone(), window, cx));
        self.form.update(cx, |editor, cx| {
            editor.set_rows(draft.form.clone().unwrap_or_default(), window, cx)
        });
        self.checks.update(cx, |checks, cx| checks.reload(window, cx));
        self.load_text(&draft, window, cx);
        self.ensure_form_rows(&draft, cx);
        cx.notify();
    }

    /// Put the draft's text into the body, variables, and auth fields.
    fn load_text(&mut self, draft: &Draft, window: &mut Window, cx: &mut Context<Self>) {
        let body = draft.body.clone();
        if self.body.read(cx).value().as_ref() != body {
            self.body
                .update(cx, |input, cx| input.set_value(body.clone(), window, cx));
        }
        if self.text_body.read(cx).value().as_ref() != body {
            self.text_body
                .update(cx, |input, cx| input.set_value(body.clone(), window, cx));
        }
        let variables = draft.variables.clone().unwrap_or_default();
        if self.variables.read(cx).value().as_ref() != variables {
            self.variables
                .update(cx, |input, cx| input.set_value(variables, window, cx));
        }
        self.load_auth(draft, window, cx);
        self.set_body_language(draft.body_mode, window, cx);
    }

    fn load_auth(&mut self, draft: &Draft, window: &mut Window, cx: &mut Context<Self>) {
        let (token, username, password) = match &draft.local_auth {
            Some(AuthorizationConfig::Bearer { token }) => (token.clone(), String::new(), String::new()),
            Some(AuthorizationConfig::Basic { username, password }) => {
                (String::new(), username.clone(), password.clone())
            }
            _ => (String::new(), String::new(), String::new()),
        };
        self.token.update(cx, |input, cx| input.set_value(token, window, cx));
        self.username
            .update(cx, |input, cx| input.set_value(&username, window, cx));
        self.password
            .update(cx, |input, cx| input.set_value(password, window, cx));
    }

    fn set_body_language(&mut self, mode: BodyMode, window: &mut Window, cx: &mut Context<Self>) {
        let language = body_language(mode);
        let placeholder = body_placeholder(mode);
        self.body.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx);
        });
        if language != self.body_language {
            self.body_language = language;
            self.body
                .update(cx, |input, cx| input.set_highlighter(language, cx));
        }
    }

    /// Push the resolved tokens to the token-aware fields.
    fn refresh_context(&mut self, cx: &mut Context<Self>) {
        let Some((_, ctx)) = session_context(self.store.read(cx), self.session_id) else {
            return;
        };
        let tokens = Some(ctx.tokens.clone());
        if self.context == tokens {
            return;
        }
        self.context = tokens.clone();
        for table in [&self.query, &self.headers, &self.form] {
            let tokens = tokens.clone();
            table.update(cx, |table, cx| table.set_context(tokens, cx));
        }
        self.username
            .update(cx, |input, cx| input.set_context(tokens.clone(), cx));
        for (features, editor) in [
            (&self.body_features, &self.body),
            (&self.variables_features, &self.variables),
        ] {
            if features.set_tokens(tokens.clone()) {
                editor.update(cx, |editor, cx| editor.refresh(cx));
            }
        }
    }

    /// Form rows start with one blank row, like query and header rows.
    fn ensure_form_rows(&mut self, draft: &Draft, cx: &mut Context<Self>) {
        if matches!(draft.body_mode, BodyMode::Form | BodyMode::Multipart) && draft.form.is_none() {
            let rows = vec![pair("", "")];
            replace_draft(&self.store, self.session_id, cx, |draft| draft.form = Some(rows));
        }
    }

    fn set_tab(&mut self, tab: &str, cx: &mut Context<Self>) {
        let id = self.session_id;
        let tab = tab.to_string();
        self.store.update(cx, |store, cx| {
            store.update_workspace(cx, |workspace| {
                if let Some(session) = workspace.session_mut(id) {
                    session.view.request_tab = tab;
                }
            })
        });
    }

    pub fn set_body_mode(&mut self, mode: BodyMode, window: &mut Window, cx: &mut Context<Self>) {
        edit_draft(&self.store, self.session_id, cx, |draft| draft.body_mode = mode);
        self.form
            .update(cx, |form, cx| form.set_allow_files(mode == BodyMode::Multipart, cx));
        self.set_body_language(mode, window, cx);
        if let Some(draft) = self.draft(cx) {
            self.ensure_form_rows(&draft, cx);
        }
    }

    pub fn set_auth(&mut self, choice: AuthChoice, window: &mut Window, cx: &mut Context<Self>) {
        edit_draft(&self.store, self.session_id, cx, |draft| {
            draft.local_auth = local_auth_for(choice, draft);
        });
        if let Some(draft) = self.draft(cx) {
            self.load_auth(&draft, window, cx);
        }
        cx.notify();
    }

    fn clear_body(&mut self, cx: &mut Context<Self>) {
        self.format_error.clear();
        replace_draft(&self.store, self.session_id, cx, |draft| draft.body.clear());
    }

    /// Format the JSON body, or the GraphQL query and its variables. An
    /// invalid document is left as it is and its error line is shown.
    pub fn format_body(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.draft(cx) else {
            return;
        };
        if draft.body_mode == BodyMode::Graphql {
            return self.format_graphql_body(&draft, window, cx);
        }
        match format_json(&draft.body) {
            Ok(body) => {
                self.format_error.clear();
                replace_draft(&self.store, self.session_id, cx, |draft| draft.body = body);
            }
            Err(error) => {
                let location = json_error_location(&draft.body, &error);
                self.show_format_error("Invalid JSON", "The body was not changed.", Some(location), ErrorEditor::Body, window, cx);
            }
        }
    }

    fn format_graphql_body(&mut self, draft: &Draft, window: &mut Window, cx: &mut Context<Self>) {
        let variables = draft.variables.clone().unwrap_or_default();
        let mut formatted_variables = variables.clone();
        if !variables.trim().is_empty() {
            match format_json(&variables) {
                Ok(text) => formatted_variables = text,
                Err(error) => {
                    let location = json_error_location(&variables, &error);
                    return self.show_format_error(
                        "Invalid JSON variables",
                        "The variables were not changed.",
                        Some(location),
                        ErrorEditor::Variables,
                        window,
                        cx,
                    );
                }
            }
        }
        match format_graphql(&draft.body) {
            Ok(body) => {
                self.format_error.clear();
                replace_draft(&self.store, self.session_id, cx, |draft| {
                    draft.body = body;
                    draft.variables = Some(formatted_variables);
                });
            }
            Err(error) => {
                let location = graphql_error_location(&draft.body, &error);
                self.show_format_error(
                    "Invalid GraphQL",
                    "The body was not changed.",
                    location.as_ref().cloned(),
                    ErrorEditor::Body,
                    window,
                    cx,
                );
            }
        }
    }

    /// Show where formatting failed and move the cursor to that place.
    fn show_format_error(
        &mut self,
        label: &str,
        outcome: &str,
        location: Option<TextLocation>,
        editor: ErrorEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.format_error = format_error_text(label, outcome, location.as_ref());
        cx.notify();
        let Some(location) = location else {
            return;
        };
        let editor = match editor {
            ErrorEditor::Body => self.body.clone(),
            ErrorEditor::Variables => {
                cx.default_global::<VariablesPane>().collapsed = false;
                self.variables.clone()
            }
        };
        let offset = location.offset;
        editor.update(cx, |input, cx| {
            input.set_selected_range(offset..offset, cx);
            input.focus(window, cx);
        });
    }

    fn load_schema(&mut self, cx: &mut Context<Self>) {
        if self.schema_loading {
            return;
        }
        let store = self.store.read(cx);
        let Some((session, ctx)) = session_context(store, self.session_id) else {
            return;
        };
        let engine = store.engine.clone();
        let options = transport_options(&store.workspace.preferences);
        self.schema_loading = true;
        self.schema_error.clear();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = fetch_schema(
                &engine,
                &session.draft,
                Some(RequestContext::Resolved(&ctx)),
                &options,
            )
            .await;
            this.update(cx, |this, cx| {
                this.schema_loading = false;
                if let Err(error) = result {
                    this.schema_error = error;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn choose_body_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.body_file_error.clear();
        let picking = pick_request_file(&self.store, cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = picking.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Some(picked)) => {
                        let path = picked.path.clone();
                        this.picked_file = Some(picked);
                        edit_draft(&this.store, this.session_id, cx, |draft| {
                            draft.body_file = Some(path)
                        });
                    }
                    Ok(None) => {}
                    Err(error) => this.body_file_error = error,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn pick_form_file(
        &mut self,
        form: Entity<KeyValueEditor>,
        row: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let picking = pick_request_file(&self.store, cx);
        cx.spawn_in(window, async move |_, cx| {
            let result = picking.await;
            form.update_in(cx, |form, window, cx| match result {
                Ok(Some(picked)) if row == 0 => form.add_file_row(picked.path, &picked.name, window, cx),
                Ok(Some(picked)) => form.set_row_file(row, picked.path, window, cx),
                Ok(None) => {}
                Err(error) => form.set_file_error(error, cx),
            })
            .ok();
        })
        .detach();
    }

    // ── Rendering ───────────────────────────────────────────────────────────

    fn render_header(&self, busy: bool, cx: &App) -> impl IntoElement {
        let colors = theme::colors(cx);
        div()
            .h(px(36.))
            .flex_none()
            .px_4()
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.muted)
            .font_family(theme::MONO)
            .text_size(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(11.))
                    .child(div().text_color(colors.muted_foreground).mr(px(10.)).child("01"))
                    .child("REQUEST"),
            )
            .child(
                div()
                    .text_color(colors.muted_foreground)
                    .child(if busy { "SENDING" } else { "COMPOSE" }),
            )
    }

    fn render_tabs(&self, active: &str, draft: &Draft, effective: &AuthorizationConfig, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let tabs = tab_counts(draft, effective).into_iter().map(|(id, label, count)| {
            let selected = id == active;
            div()
                .id(SharedString::from(format!("request-tab-{id}")))
                .h(px(38.))
                .px_3()
                .flex()
                .items_center()
                .gap(px(7.))
                .border_b_1()
                .border_color(if selected { colors.primary } else { gpui_kit::transparent_black() })
                .text_size(px(12.))
                .text_color(if selected { colors.foreground } else { colors.muted_foreground })
                .hover(|this| this.text_color(colors.foreground).bg(colors.muted))
                .cursor_pointer()
                .child(label)
                .when(count > 0, |this| {
                    this.child(
                        div()
                            .font_family(theme::MONO)
                            .text_size(px(10.))
                            .text_color(colors.muted_foreground)
                            .child(count.to_string()),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| this.set_tab(id, cx)))
        });
        div()
            .flex()
            .flex_none()
            .px_2()
            .border_b_1()
            .border_color(colors.border)
            .font_family(theme::SANS)
            .children(tabs)
    }

    fn render_body(&mut self, draft: &Draft, ctx: &blink_core::authorization::ResolvedRequestContext, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::colors(cx);
        let mode = draft.body_mode;
        let entity = cx.entity().downgrade();
        let formattable = matches!(mode, BodyMode::Json | BodyMode::Graphql);
        let format_label = if mode == BodyMode::Graphql { "Format GraphQL" } else { "Format JSON" };
        let has_body = !draft.body.is_empty();

        // The schema of the current URL, and its age.
        let key = (mode == BodyMode::Graphql)
            .then(|| schema_key(draft, Some(RequestContext::Resolved(ctx))))
            .flatten();
        if key != self.schema_key {
            self.schema_key = key.clone();
            self.schema_error.clear();
        }
        let cached = key.as_deref().and_then(get_cached_schema);
        self.body_features
            .set_schema(cached.as_ref().map(|schema| schema.schema.clone()));
        let schema_status = cached
            .as_ref()
            .map(|schema| format!("Schema loaded · {}", format_schema_age(schema.fetched_at, now_ms())));

        let mode_entity = entity.clone();
        let actions = div()
            .id("body-actions")
            .flex()
            .flex_none()
            .items_center()
            .px_3()
            .py_2()
            .gap(px(10.))
            .border_b_1()
            .border_color(colors.border)
            .text_size(px(12.))
            .text_color(colors.muted_foreground)
            .child("Body")
            .child(select_button(
                "body-mode",
                BODY_MODES.iter().map(|(mode, label)| (*mode, SharedString::from(*label))).collect(),
                mode,
                false,
                move |mode, window, cx| {
                    mode_entity
                        .update(cx, |this, cx| this.set_body_mode(mode, window, cx))
                        .ok();
                },
                cx,
            ))
            .child(
                div()
                    .ml_auto()
                    .flex()
                    .items_center()
                    .gap_1()
                    .when(mode == BodyMode::Graphql, |this| {
                        let tooltip = match &schema_status {
                            Some(status) => format!(
                                "{status}. Fetch again with this request's URL, headers, and auth"
                            ),
                            None => "Fetch schema with this request's URL, headers, and auth".to_string(),
                        };
                        this.child(
                            div()
                                .relative()
                                .child(
                                    Button::new("fetch-schema")
                                        .ghost()
                                        .small()
                                        .icon(if self.schema_loading {
                                            Icon::new(IconName::LoaderCircle).size(px(14.))
                                        } else {
                                            Icon::new(IconName::Network).size(px(14.))
                                        })
                                        .loading(self.schema_loading)
                                        .disabled(self.schema_loading || draft.url.trim().is_empty())
                                        .tooltip(tooltip)
                                        .on_click(cx.listener(|this, _, _, cx| this.load_schema(cx))),
                                )
                                .when(cached.is_some() && !self.schema_loading, |this| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .top(px(2.))
                                            .right(px(2.))
                                            .size(px(10.))
                                            .rounded_full()
                                            .bg(colors.success)
                                            .border_2()
                                            .border_color(colors.background)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                Icon::new(IconName::Check)
                                                    .size(px(8.))
                                                    .text_color(colors.background),
                                            ),
                                    )
                                }),
                        )
                    })
                    .when(formattable, |this| {
                        this.child(
                            Button::new("format-body")
                                .ghost()
                                .small()
                                .icon(Icon::new(IconName::Braces).size(px(14.)))
                                .disabled(!has_body)
                                .tooltip(format_label)
                                .on_click(cx.listener(|this, _, window, cx| this.format_body(window, cx))),
                        )
                    }),
            )
            .context_menu({
                let entity = entity.clone();
                move |menu, window, cx| {
                    let radio = entity.clone();
                    let format = entity.clone();
                    let clear = entity.clone();
                    menu.submenu("Body type", window, cx, move |menu, _, _| {
                        BODY_MODES.iter().fold(menu, |menu, (value, label)| {
                            let radio = radio.clone();
                            let value = *value;
                            menu.item(PopupMenuItem::new(*label).checked(value == mode).on_click(
                                move |_, window, cx| {
                                    radio
                                        .update(cx, |this, cx| this.set_body_mode(value, window, cx))
                                        .ok();
                                },
                            ))
                        })
                    })
                    .separator()
                    .item(
                        PopupMenuItem::new(format_label)
                            .disabled(!formattable || !has_body)
                            .on_click(move |_, window, cx| {
                                format.update(cx, |this, cx| this.format_body(window, cx)).ok();
                            }),
                    )
                    .item(PopupMenuItem::new("Clear body").disabled(!has_body).on_click(
                        move |_, _, cx| {
                            clear.update(cx, |this, cx| this.clear_body(cx)).ok();
                        },
                    ))
                }
            });

        let note = |text: String| {
            div()
                .px_4()
                .py_3()
                .text_size(px(11.))
                .line_height(relative(1.7))
                .text_color(colors.muted_foreground)
                .child(text)
        };
        let error = |text: String| {
            div()
                .px_4()
                .py_3()
                .text_size(px(11.))
                .line_height(relative(1.7))
                .text_color(colors.destructive)
                .child(text)
        };

        let content: AnyElement = match mode {
            BodyMode::Form | BodyMode::Multipart => self.form.clone().into_any_element(),
            BodyMode::File => self.render_body_file(draft, cx).into_any_element(),
            BodyMode::Text => div()
                .flex_1()
                .min_h(px(180.))
                .flex()
                .flex_col()
                .font_family(theme::MONO)
                .text_size(px(13.))
                .line_height(relative(1.75))
                .p_4()
                .child(
                    Textarea::new(&self.text_body)
                        .appearance(false)
                        .h_full()
                        .font_family(theme::MONO)
                        .text_size(px(13.)),
                )
                .into_any_element(),
            BodyMode::Json => code_editor(&self.body, px(180.)).into_any_element(),
            BodyMode::Graphql => self.render_graphql(window, cx),
            BodyMode::None => note("No request body.".into()).into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(actions)
            .when(!supports_body(&draft.method), |this| {
                this.child(
                    note(format!("{} sends no body. Your draft is retained.", draft.method))
                        .border_b_1()
                        .border_color(colors.border),
                )
            })
            .child(content)
            .when(!self.format_error.is_empty(), |this| this.child(error(self.format_error.clone())))
            .when(!self.schema_error.is_empty() && mode == BodyMode::Graphql, |this| {
                this.child(error(self.schema_error.clone()))
            })
            .into_any_element()
    }

    fn render_graphql(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::colors(cx);
        let pane = cx.default_global::<VariablesPane>();
        let collapsed = pane.collapsed;
        let height = pane.height;
        let toggle = div()
            .id("variables-toggle")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .px_3()
            .py_2()
            .border_y_1()
            .border_color(colors.border)
            .text_size(px(12.))
            .text_color(colors.muted_foreground)
            .hover(|this| this.text_color(colors.foreground))
            .cursor_pointer()
            .child(Icon::new(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown }).size(px(12.)))
            .child("Variables")
            .on_click(cx.listener(|_, _, _, cx| {
                let pane = cx.default_global::<VariablesPane>();
                pane.collapsed = !pane.collapsed;
                cx.notify();
            }));
        if collapsed {
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(code_editor(&self.body, px(MIN_QUERY_HEIGHT)))
                .child(toggle)
                .into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(MIN_QUERY_HEIGHT + MIN_VARIABLES_HEIGHT + 40.))
            .child(
                v_resizable(SharedString::from(format!("variables-split-{}", self.session_id)))
                    .on_resize(|state, _, cx| {
                        if let Some(size) = state.read(cx).sizes().get(1).copied() {
                            cx.default_global::<VariablesPane>().height = size;
                        }
                    })
                    .child(
                        resizable_panel()
                            .size_range(px(MIN_QUERY_HEIGHT)..Pixels::MAX)
                            .child(code_editor(&self.body, px(MIN_QUERY_HEIGHT))),
                    )
                    .child(
                        resizable_panel()
                            .size(height)
                            .size_range(px(MIN_VARIABLES_HEIGHT)..Pixels::MAX)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .size_full()
                                    .child(toggle)
                                    .child(code_editor(&self.variables, px(0.))),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_body_file(&self, draft: &Draft, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let size = self
            .picked_file
            .as_ref()
            .filter(|picked| Some(&picked.path) == draft.body_file.as_ref())
            .map(|picked| format!(" · {}", format_bytes(picked.size_bytes)));
        div()
            .flex()
            .flex_col()
            .gap_2()
            .px_4()
            .py_3()
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .min_w_0()
                    .child(
                        Button::new("choose-body-file")
                            .small()
                            .icon(Icon::new(IconName::FileUp).size(px(13.)))
                            .label(if draft.body_file.is_some() { "Change file" } else { "Choose file" })
                            .on_click(cx.listener(|this, _, window, cx| this.choose_body_file(window, cx))),
                    )
                    .child(match &draft.body_file {
                        Some(path) => div()
                            .id("body-file-name")
                            .flex()
                            .min_w_0()
                            .truncate()
                            .font_family(theme::MONO)
                            .child(file_name(path).to_string())
                            .when_some(size, |this, size| {
                                this.child(div().text_color(colors.muted_foreground).child(size))
                            })
                            .tooltip({
                                let path = path.clone();
                                move |window, cx| gpui_kit::component::tooltip::Tooltip::new(path.clone()).build(window, cx)
                            })
                            .into_any_element(),
                        None => div()
                            .text_color(colors.muted_foreground)
                            .child("No file chosen.")
                            .into_any_element(),
                    }),
            )
            .when(!self.body_file_error.is_empty(), |this| {
                this.child(div().text_color(colors.destructive).child(self.body_file_error.clone()))
            })
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(colors.muted_foreground)
                    .child("Blink reads the file when you send. Content-Type defaults to application/octet-stream."),
            )
    }

    fn render_auth(&self, draft: &Draft, effective: &AuthorizationConfig, source: Option<String>, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let choice = auth_choice(draft);
        let entity = cx.entity().downgrade();
        let label = |text: &'static str| {
            div()
                .w(px(100.))
                .flex_none()
                .text_color(colors.muted_foreground)
                .child(text)
        };
        let small = |text: String| {
            div()
                .text_size(px(11.))
                .text_color(colors.muted_foreground)
                .child(text)
        };
        let row = || div().flex().items_center().gap_3().min_h(px(28.));
        let select_entity = entity.clone();
        let grid = div()
            .id("auth-actions")
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .text_size(px(12.))
            .child(
                row().child(label("Authorization")).child(fill_select(
                    "auth-type",
                    AUTH_CHOICES.iter().map(|(choice, label)| (*choice, SharedString::from(*label))).collect(),
                    choice,
                    false,
                    move |choice, window, cx| {
                        select_entity
                            .update(cx, |this, cx| this.set_auth(choice, window, cx))
                            .ok();
                    },
                    cx,
                )),
            )
            .when(choice == AuthChoice::Inherit, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(small("Effective".into()).w(px(100.)).flex_none())
                        .child(small(format!("Effective: {}", auth_type_id(effective))).font_family(theme::MONO)),
                )
                .when_some(source, |this, source| {
                    this.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(small("Source".into()).w(px(100.)).flex_none())
                            .child(small(source).font_family(theme::MONO)),
                    )
                })
            })
            .when(choice == AuthChoice::Bearer, |this| {
                this.child(
                    row().child(label("Token")).child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(theme::MONO)
                            .child(Input::new(&self.token).small().font_family(theme::MONO).text_size(px(12.))),
                    ),
                )
            })
            .when(choice == AuthChoice::Basic, |this| {
                this.child(
                    row().child(label("Username")).child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(theme::MONO)
                            .child(self.username.clone()),
                    ),
                )
                .child(
                    row().child(label("Password")).child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(theme::MONO)
                            .child(Input::new(&self.password).small().font_family(theme::MONO).text_size(px(12.))),
                    ),
                )
            })
            .context_menu(move |menu, _, _| {
                let menu = AUTH_CHOICES.iter().fold(menu, |menu, (value, label)| {
                    let entity = entity.clone();
                    let value = *value;
                    menu.item(PopupMenuItem::new(*label).checked(value == choice).on_click(
                        move |_, window, cx| {
                            entity.update(cx, |this, cx| this.set_auth(value, window, cx)).ok();
                        },
                    ))
                });
                let clear = entity.clone();
                menu.separator().item(
                    PopupMenuItem::new("Clear local override")
                        .disabled(choice == AuthChoice::Inherit)
                        .on_click(move |_, window, cx| {
                            clear
                                .update(cx, |this, cx| this.set_auth(AuthChoice::Inherit, window, cx))
                                .ok();
                        }),
                )
            });
        div()
            .flex()
            .flex_col()
            .child(grid)
            .child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(11.))
                    .line_height(relative(1.7))
                    .text_color(colors.muted_foreground)
                    .child(Icon::new(IconName::KeyRound).size(px(13.)))
                    .child("Credentials are saved locally in plaintext. Copy cURL includes them."),
            )
    }
}

/// Install the token and schema providers on a body editor.
fn with_features(mut state: EditorState, features: &Rc<BodyLanguage>) -> EditorState {
    let lsp = state.lsp_mut();
    lsp.completion_provider = Some(features.clone());
    lsp.semantic_tokens_provider = Some(features.clone());
    lsp.hover_provider = Some(features.clone());
    state
}

/// A body code editor that leaves `Cmd/Ctrl+Enter` to the app.
fn code_editor(state: &Entity<EditorState>, min_height: Pixels) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(min_height)
        .size_full()
        // CodeMirror: 16 px around the text; the editor adds 6 px left and
        // 8 px above and below.
        .pl(px(10.))
        .pr(px(6.))
        .py(px(8.))
        .font_family(theme::MONO)
        .text_size(px(13.))
        .capture_action(|action: &Enter, window, cx| {
            if action.secondary {
                cx.stop_propagation();
                window.dispatch_action(Box::new(SendRequest), cx);
            }
        })
        .child(
            Editor::new(state)
                .appearance(false)
                .bordered(false)
                .h_full()
                .font_family(theme::MONO)
                .text_size(px(13.))
                .line_height(relative(1.75)),
        )
}

fn body_language(mode: BodyMode) -> &'static str {
    if mode == BodyMode::Graphql { "graphql" } else { "json" }
}

fn body_placeholder(mode: BodyMode) -> &'static str {
    match mode {
        BodyMode::Json => "{\n  \"key\": \"value\"\n}",
        BodyMode::Graphql => "query {\n  viewer {\n    id\n  }\n}",
        _ => "Request body",
    }
}

/// Ask for a file, then allow requests to read it. None when canceled.
fn pick_request_file(store: &Entity<Store>, cx: &mut App) -> Task<Result<Option<PickedFile>, String>> {
    let engine = store.read(cx).engine.clone();
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: None,
    });
    cx.spawn(async move |_| {
        let path = match paths.await {
            Ok(Ok(Some(paths))) => paths.into_iter().next(),
            Ok(Ok(None)) | Err(_) => None,
            Ok(Err(error)) => return Err(error.to_string()),
        };
        match path {
            Some(path) => engine.grant_file(path).await.map(Some),
            None => Ok(None),
        }
    })
}

impl Render for RequestEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let store = self.store.read(cx);
        let Some(session) = store.workspace.session(self.session_id).cloned() else {
            return div().into_any_element();
        };
        let groups = store.workspace.groups.clone();
        let Some((_, ctx)) = session_context(store, self.session_id) else {
            return div().into_any_element();
        };
        let draft = &session.draft;
        let effective = ctx.auth.clone();
        let source = inherited_source(draft, session.group_id, &groups);
        let tab = match session.view.request_tab.as_str() {
            tab @ ("query" | "headers" | "body" | "auth" | "tests") => tab.to_string(),
            _ => "query".to_string(),
        };
        let content: AnyElement = match tab.as_str() {
            "headers" => div()
                .flex()
                .flex_col()
                .child(self.headers.clone())
                .child(div().px_4().py_2().child(help_link(
                    "header-help",
                    "Header help",
                    "Body mode sets Content-Type unless a header overrides it.",
                    cx,
                )))
                .into_any_element(),
            "body" => self.render_body(draft, &ctx, window, cx),
            "auth" => self.render_auth(draft, &effective, source, cx).into_any_element(),
            "tests" => self.checks.clone().into_any_element(),
            _ => div()
                .flex()
                .flex_col()
                .child(self.query.clone())
                .child(div().px_4().py_2().child(help_link(
                    "query-help",
                    "Query help",
                    "Enabled rows are appended to the URL. Duplicate keys are preserved.",
                    cx,
                )))
                .into_any_element(),
        };
        let (counts, auth) = footer_text(draft);
        let body_tab = tab == "body";
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(self.render_header(session.busy, cx))
            .child(self.render_tabs(&tab, draft, &effective, cx))
            .child(
                div()
                    .id("request-tab-content")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when(!body_tab, |this| this.overflow_y_scroll())
                    .child(content),
            )
            .child(
                div()
                    .h(px(28.))
                    .flex_none()
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(colors.border)
                    .font_family(theme::MONO)
                    .text_size(px(9.))
                    .text_color(colors.muted_foreground)
                    .child(counts)
                    .child(auth),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod ui_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use blink_core::model::{AuthKind, CheckOperator, CheckSource};
    use blink_core::request::create_draft;

    #[test]
    fn offers_form_multipart_and_file_bodies() {
        let labels: Vec<_> = BODY_MODES.iter().map(|(_, label)| *label).collect();
        assert_eq!(
            labels,
            ["None", "JSON", "Text", "GraphQL", "Form URL-encoded", "Multipart form", "File"]
        );
    }

    #[test]
    fn shows_inherit_as_selected_without_a_local_auth() {
        let mut draft = create_draft();
        assert_eq!(auth_choice(&draft), AuthChoice::Inherit);
        draft.local_auth = Some(AuthorizationConfig::Bearer { token: "t".into() });
        assert_eq!(auth_choice(&draft), AuthChoice::Bearer);
    }

    #[test]
    fn setting_bearer_from_inherit_starts_from_the_flat_token() {
        let mut draft = create_draft();
        draft.token = "abc".into();
        assert_eq!(
            local_auth_for(AuthChoice::Bearer, &draft),
            Some(AuthorizationConfig::Bearer { token: "abc".into() })
        );
        assert_eq!(local_auth_for(AuthChoice::Inherit, &draft), None);
    }

    #[test]
    fn auth_badge_counts_inherited_active_auth() {
        let draft = create_draft();
        let bearer = AuthorizationConfig::Bearer { token: "x".into() };
        assert_eq!(tab_counts(&draft, &bearer)[3].2, 1);
        assert_eq!(tab_counts(&draft, &AuthorizationConfig::None)[3].2, 0);
    }

    #[test]
    fn counts_active_rows_body_and_checks() {
        let mut draft = create_draft();
        draft.method = "POST".into();
        draft.body_mode = BodyMode::Json;
        let mut disabled = blink_core::checks::create_assertion(
            CheckSource::Status,
            CheckOperator::Equals,
            "200",
            "",
        );
        disabled.enabled = false;
        draft.assertions = Some(vec![
            blink_core::checks::create_assertion(CheckSource::Status, CheckOperator::Equals, "200", ""),
            disabled,
        ]);
        let counts = tab_counts(&draft, &AuthorizationConfig::None);
        assert_eq!(counts.map(|(_, _, count)| count), [0, 1, 1, 0, 1]);
        draft.method = "GET".into();
        assert_eq!(tab_counts(&draft, &AuthorizationConfig::None)[2].2, 0);
    }

    #[test]
    fn writes_the_footer() {
        let mut draft = create_draft();
        assert_eq!(
            footer_text(&draft),
            ("0 QUERY · 1 HEADERS".to_string(), "AUTH / NONE".to_string())
        );
        draft.auth = AuthKind::Basic;
        assert_eq!(footer_text(&draft).1, "AUTH / BASIC");
        draft.local_auth = Some(AuthorizationConfig::Bearer { token: String::new() });
        assert_eq!(footer_text(&draft).1, "AUTH / BEARER");
    }

    #[test]
    fn names_the_group_an_auth_is_inherited_from() {
        let draft = create_draft();
        let group = |id: u64, parent: Option<u64>, auth: Option<AuthorizationConfig>| RequestGroup {
            id,
            name: format!("G{id}"),
            parent_id: parent,
            collapsed: false,
            local_auth: auth,
            local_definitions: None,
            default_method: None,
            default_url: None,
            environments: None,
            active_environment_id: None,
        };
        let groups = vec![
            group(1, None, Some(AuthorizationConfig::Bearer { token: "t".into() })),
            group(2, Some(1), None),
        ];
        assert_eq!(inherited_source(&draft, Some(2), &groups).as_deref(), Some("G1 · Bearer"));
        assert_eq!(inherited_source(&draft, None, &groups), None);
    }

    #[test]
    fn describes_format_errors() {
        let location = TextLocation {
            line: 2,
            column: 3,
            offset: 5,
            reason: "Unexpected token".into(),
        };
        assert_eq!(
            format_error_text("Invalid JSON", "The body was not changed.", Some(&location)),
            "Invalid JSON at line 2, column 3: Unexpected token. The body was not changed."
        );
        assert_eq!(
            format_error_text("Invalid JSON", "The body was not changed.", None),
            "Invalid JSON. The body was not changed."
        );
    }
}
