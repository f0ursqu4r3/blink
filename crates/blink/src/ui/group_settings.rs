//! Group settings. Port of `GroupSettingsDialog.vue` and `EnvironmentTokensEditor.vue`.

mod environment_tokens;

use std::collections::{HashMap, HashSet};

use blink_core::definitions::{definitions_to_rows, rows_to_definitions};
use blink_core::groups::{can_nest_group, group_subtree};
use blink_core::model::{
    AuthorizationConfig, Definitions, Environment, METHODS, Pair, RequestGroup,
};
use blink_core::preferences::resolve_new_request_defaults;
use blink_core::workspace_state::GroupSettingsChanges;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::select::{Select, SelectEvent};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use self::environment_tokens::{
    EnvironmentTokens, EnvironmentTokensEvent, parse_table, to_rows,
};
use crate::store::Store;
use crate::theme;
use crate::ui::key_value_editor::{KeyValueEditor, KeyValueEvent, KeyValueOptions};
use crate::ui::form::{
    Choice, ChoiceSelect, choice_index, dialog_footer, dialog_header, footer_button, note,
    section_heading, section_heading_with_help, selected,
};

/// Open the dialog.
pub fn open(store: Entity<Store>, group_id: u64, window: &mut Window, cx: &mut App) {
    if store.read(cx).workspace.group(group_id).is_none() {
        return;
    }
    let form = cx.new(|cx| GroupForm::new(store, group_id, window, cx));
    let name = form.read(cx).name.clone();
    window.open_dialog(cx, move |dialog, _, cx| {
        let save = form.clone();
        let confirm = form.clone();
        dialog
            .w(px(760.))
            // `top-1/2 -translate-y-1/2 max-h-[90dvh]`.
            .centered(true)
            .max_h(relative(0.9))
            .p_0()
            .close_button(false)
            .overlay_closable(false)
            .title(dialog_header("Group Settings", cx).w_full())
            .child(div().mt(px(-8.)).child(form.clone()))
            .footer(
                dialog_footer(cx)
                    .mt(px(-8.))
                    .p(px(12.))
                    .child(
                        footer_button("group-cancel", "Cancel", false)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(footer_button("group-save", "Save", true).on_click(
                        move |_, window, cx| {
                            if save.update(cx, |form, cx| form.save(cx)) {
                                window.close_dialog(cx);
                            }
                        },
                    )),
            )
            .on_ok(move |_, _, cx| confirm.update(cx, |form, cx| form.save(cx)))
    });
    // The name takes focus with its text selected, ready to rename.
    window.defer(cx, move |window, cx| {
        name.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthMode {
    Inherit,
    None,
    Bearer,
    Basic,
}

const AUTH_MODES: [(AuthMode, &str); 4] = [
    (AuthMode::Inherit, "Inherit"),
    (AuthMode::None, "No auth"),
    (AuthMode::Bearer, "Bearer"),
    (AuthMode::Basic, "Basic"),
];

fn auth_type_label(auth: &AuthorizationConfig) -> &'static str {
    match auth {
        AuthorizationConfig::None => "No auth",
        AuthorizationConfig::Bearer { .. } => "Bearer",
        AuthorizationConfig::Basic { .. } => "Basic",
    }
}

/// Parent names from the root down, or "(root)" for a root group.
fn parent_breadcrumb(groups: &[RequestGroup], parent_id: Option<u64>) -> String {
    if parent_id.is_none() {
        return "(root)".into();
    }
    let by_id: HashMap<u64, &RequestGroup> = groups.iter().map(|g| (g.id, g)).collect();
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = parent_id;
    while let Some(id) = cursor {
        if !seen.insert(id) {
            break;
        }
        let Some(group) = by_id.get(&id) else { break };
        chain.insert(0, group.name.as_str());
        cursor = group.parent_id;
    }
    chain.join(" > ")
}

/// Where the group's auth comes from, without any credential.
fn effective_auth_label(
    groups: &[RequestGroup],
    parent_id: Option<u64>,
    local: Option<&AuthorizationConfig>,
) -> String {
    if let Some(local) = local {
        return format!("Local · {}", auth_type_label(local));
    }
    let by_id: HashMap<u64, &RequestGroup> = groups.iter().map(|g| (g.id, g)).collect();
    let mut seen = HashSet::new();
    let mut cursor = parent_id;
    while let Some(id) = cursor {
        if !seen.insert(id) {
            break;
        }
        let Some(group) = by_id.get(&id) else { break };
        if let Some(auth) = &group.local_auth {
            return format!("Inherited from {} · {}", group.name, auth_type_label(auth));
        }
        cursor = group.parent_id;
    }
    "Local · No auth".into()
}

/// `(requests, groups)` inside the group and its descendants.
fn descendant_counts(store: &Store, group_id: u64) -> (usize, usize) {
    let workspace = &store.workspace;
    let ids = group_subtree(&workspace.groups, group_id);
    let requests = workspace
        .sessions
        .iter()
        .filter(|s| s.group_id.is_some_and(|id| ids.contains(&id)))
        .count();
    (requests, ids.len().saturating_sub(1))
}

fn js_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The first form error, as the Vue save checks them in order.
fn form_error(name: &str, url: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some("Enter a group name.")
    } else if js_len(name) > 80 {
        Some("Group name must be 80 characters or fewer.")
    } else if js_len(url) > 65536 {
        Some("Initial URL must be 65536 characters or fewer.")
    } else {
        None
    }
}

fn base_definitions(rows: &[Pair]) -> Definitions {
    rows.iter()
        .filter(|row| !row.key.trim().is_empty())
        .map(|row| (row.key.clone(), row.value.clone()))
        .collect()
}

fn text_input<T>(value: &str, window: &mut Window, cx: &mut Context<T>) -> Entity<InputState> {
    let value = value.to_string();
    cx.new(|cx| InputState::new(window, cx).default_value(value))
}

struct GroupForm {
    store: Entity<Store>,
    group_id: u64,
    name: Entity<InputState>,
    parent: Entity<ChoiceSelect>,
    parent_id: Option<u64>,
    auth_mode: AuthMode,
    bearer: Entity<InputState>,
    username: Entity<InputState>,
    password: Entity<InputState>,
    method: Entity<ChoiceSelect>,
    url: Entity<InputState>,
    local_tokens: Entity<KeyValueEditor>,
    local_rows: Vec<Pair>,
    environments: Entity<EnvironmentTokens>,
    token_error: String,
    form_error: String,
    _subscriptions: Vec<Subscription>,
}

impl GroupForm {
    fn new(store: Entity<Store>, group_id: u64, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = &store.read(cx).workspace;
        let group = workspace.group(group_id).cloned().expect("group exists");
        let groups = workspace.groups.clone();
        let preferences = workspace.preferences.clone();

        let name = text_input(&group.name, window, cx);
        let mut parent_choices = vec![Choice::new("", "Root")];
        parent_choices.extend(
            groups
                .iter()
                .filter(|candidate| can_nest_group(&groups, group_id, Some(candidate.id)))
                .map(|candidate| Choice::new(candidate.id.to_string(), candidate.name.clone())),
        );
        let parent_value = group.parent_id.map(|id| id.to_string()).unwrap_or_default();
        let parent_index = choice_index(&parent_choices, &parent_value);
        let parent = cx.new(|cx| {
            gpui_kit::component::select::SelectState::new(parent_choices, parent_index, window, cx)
        });

        let (auth_mode, token, user, pass) = match &group.local_auth {
            None => (AuthMode::Inherit, "", "", ""),
            Some(AuthorizationConfig::None) => (AuthMode::None, "", "", ""),
            Some(AuthorizationConfig::Bearer { token }) => (AuthMode::Bearer, token.as_str(), "", ""),
            Some(AuthorizationConfig::Basic { username, password }) => {
                (AuthMode::Basic, "", username.as_str(), password.as_str())
            }
        };
        let bearer = cx.new(|cx| InputState::new(window, cx).masked(true).default_value(token.to_string()));
        let username = text_input(user, window, cx);
        let password =
            cx.new(|cx| InputState::new(window, cx).masked(true).default_value(pass.to_string()));

        let inherited = resolve_new_request_defaults(&groups, group.parent_id, &preferences);
        let method = {
            let choices = method_choices(&inherited.method);
            let index = choice_index(&choices, group.default_method.as_deref().unwrap_or(""));
            cx.new(|cx| gpui_kit::component::select::SelectState::new(choices, index, window, cx))
        };
        let url_value = group.default_url.clone().unwrap_or_default();
        let url_placeholder = if inherited.url.is_empty() {
            "No initial URL".to_string()
        } else {
            inherited.url.clone()
        };
        let url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(url_placeholder)
                .default_value(url_value)
        });

        let local_definitions = group.local_definitions.clone().unwrap_or_default();
        let local_rows = definitions_to_rows(&local_definitions);
        let local_tokens = cx.new(|cx| {
            KeyValueEditor::new(
                local_rows.clone(),
                KeyValueOptions {
                    key_label: "Name".into(),
                    value_label: "Value".into(),
                    key_placeholder: "Name".into(),
                    value_placeholder: "Value".into(),
                    allow_files: false,
                    toggles: false,
                    id: "local-tokens".into(),
                },
                window,
                cx,
            )
        });
        let group_environments = group.environments.clone().unwrap_or_default();
        let environments = cx.new(|cx| {
            EnvironmentTokens::new(
                to_rows(&local_definitions, &group_environments),
                &group_environments,
                window,
                cx,
            )
        });

        let subscriptions = vec![
            cx.subscribe_in(
                &parent,
                window,
                |this, _, event: &SelectEvent<Vec<Choice>>, window, cx| {
                    let SelectEvent::Confirm(value) = event;
                    let next = value.as_ref().and_then(|v| v.parse::<u64>().ok());
                    this.set_parent(next, window, cx);
                },
            ),
            cx.subscribe(&local_tokens, |this, _, event: &KeyValueEvent, cx| {
                if let KeyValueEvent::Change(rows) = event {
                    this.local_rows = rows.clone();
                    cx.notify();
                }
            }),
            cx.subscribe(&environments, |_, _, _: &EnvironmentTokensEvent, cx| cx.notify()),
            cx.subscribe(&method, |_, _, _: &SelectEvent<Vec<Choice>>, cx| cx.notify()),
        ];
        let watch_name = cx.subscribe(&name, |_, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        });

        GroupForm {
            store,
            group_id,
            name,
            parent,
            parent_id: group.parent_id,
            auth_mode,
            bearer,
            username,
            password,
            method,
            url,
            local_tokens,
            local_rows,
            environments,
            token_error: String::new(),
            form_error: String::new(),
            _subscriptions: subscriptions.into_iter().chain([watch_name]).collect(),
        }
    }

    fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Move the form to another parent. Moving in or out of the root keeps
    /// the tokens typed so far, and the inherited defaults follow.
    fn set_parent(&mut self, next: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        let was_root = self.is_root();
        self.parent_id = next;
        if was_root != self.is_root() {
            if self.is_root() {
                let rows = self.local_tokens.read(cx).rows().to_vec();
                let rows = if self.local_rows.is_empty() { rows } else { self.local_rows.clone() };
                let table = to_rows(&base_definitions(&rows), &[]);
                self.environments
                    .update(cx, |editor, cx| editor.set_rows(table, window, cx));
            } else {
                let rows = self.environments.read(cx).rows(cx);
                let base: Definitions = rows
                    .iter()
                    .filter(|row| !row.key.trim().is_empty())
                    .map(|row| (row.key.clone(), row.base.clone()))
                    .collect();
                let pairs = definitions_to_rows(&base);
                self.local_rows = pairs.clone();
                self.local_tokens
                    .update(cx, |editor, cx| editor.set_rows(pairs, window, cx));
            }
        }
        let workspace = &self.store.read(cx).workspace;
        let inherited =
            resolve_new_request_defaults(&workspace.groups, next, &workspace.preferences);
        let current = SharedString::from(selected(&self.method, cx));
        self.method.update(cx, |select, cx| {
            select.set_items(method_choices(&inherited.method), window, cx);
            select.set_selected_value(&current, window, cx);
        });
        let placeholder = if inherited.url.is_empty() {
            "No initial URL".to_string()
        } else {
            inherited.url
        };
        self.url
            .update(cx, |url, cx| url.set_placeholder(placeholder, window, cx));
        cx.notify();
    }

    fn local_auth(&self, cx: &App) -> Option<AuthorizationConfig> {
        let value = |input: &Entity<InputState>| input.read(cx).value().to_string();
        match self.auth_mode {
            AuthMode::Inherit => None,
            AuthMode::None => Some(AuthorizationConfig::None),
            AuthMode::Bearer => Some(AuthorizationConfig::Bearer {
                token: value(&self.bearer),
            }),
            AuthMode::Basic => Some(AuthorizationConfig::Basic {
                username: value(&self.username),
                password: value(&self.password),
            }),
        }
    }

    /// Save, as the Vue `handleSave`. True when the dialog may close.
    fn save(&mut self, cx: &mut Context<Self>) -> bool {
        cx.notify();
        let name = self.name.read(cx).value().trim().to_string();
        let url = self.url.read(cx).value().to_string();
        if let Some(error) = form_error(&name, &url) {
            self.form_error = error.into();
            return false;
        }
        self.form_error.clear();
        let (local_definitions, environments): (Definitions, Option<Vec<Environment>>) =
            if self.is_root() {
                let editor = self.environments.read(cx);
                match parse_table(&editor.rows(cx), &editor.columns(cx)) {
                    Ok((base, environments)) => (base, Some(environments)),
                    Err(error) => {
                        self.token_error = error;
                        return false;
                    }
                }
            } else {
                let rows = if self.local_rows.is_empty() {
                    self.local_tokens.read(cx).rows().to_vec()
                } else {
                    self.local_rows.clone()
                };
                match rows_to_definitions(&rows) {
                    // Environments stay stored but apply only to a root group.
                    Ok(definitions) => {
                        let stored = self
                            .store
                            .read(cx)
                            .workspace
                            .group(self.group_id)
                            .and_then(|group| group.environments.clone());
                        (definitions, stored)
                    }
                    Err(error) => {
                        self.token_error = error;
                        return false;
                    }
                }
            };
        self.token_error.clear();
        let method = selected(&self.method, cx);
        let changes = GroupSettingsChanges {
            name: Some(name),
            local_auth: Some(self.local_auth(cx)),
            local_definitions: Some(local_definitions),
            parent_id: Some(self.parent_id),
            new_request_defaults: Some((
                (!method.is_empty()).then_some(method),
                (!url.is_empty()).then_some(url),
            )),
            environments: Some(environments),
        };
        let group_id = self.group_id;
        self.store.update(cx, |store, cx| {
            store.update_workspace(cx, |workspace| workspace.save_group_settings(group_id, changes))
        });
        true
    }
}

fn method_choices(inherited: &str) -> Vec<Choice> {
    let mut choices = vec![Choice::new("", format!("Inherit ({inherited})"))];
    choices.extend(METHODS.iter().map(|m| Choice::same(*m)));
    choices
}

/// A `w-20` label and its control on one row.
fn field_row(label: &'static str, control: impl IntoElement, cx: &App) -> Div {
    h_flex()
        .gap(px(10.))
        .text_size(px(12.))
        .child(
            div()
                .w(px(80.))
                .flex_none()
                .text_color(theme::colors(cx).muted_foreground)
                .child(label),
        )
        .child(div().flex_1().min_w_0().font_family(theme::MONO).child(control))
}

fn small_input(state: &Entity<InputState>) -> Input {
    Input::new(state).small().h(px(28.))
}

impl Render for GroupForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let store = self.store.read(cx);
        let workspace = &store.workspace;
        let groups = &workspace.groups;
        let breadcrumb = parent_breadcrumb(groups, self.parent_id);
        let auth_label = effective_auth_label(groups, self.parent_id, self.local_auth(cx).as_ref());
        let (request_count, group_count) = descendant_counts(store, self.group_id);
        let stored_environments = workspace
            .group(self.group_id)
            .and_then(|group| group.environments.as_ref())
            .is_some_and(|list| !list.is_empty());
        let mono_note = |text: String| {
            div()
                .font_family(theme::MONO)
                .text_size(px(11.))
                .text_color(colors.muted_foreground)
                .child(text)
        };
        let section = || {
            v_flex()
                .gap(px(6.))
                .border_b_1()
                .border_color(colors.border)
                .pb(px(16.))
        };

        let general = section()
            .child(section_heading("General", cx))
            .child(field_row("Name", small_input(&self.name), cx))
            .child(mono_note(breadcrumb))
            .child(field_row(
                "Parent",
                Select::new(&self.parent).small().h(px(28.)),
                cx,
            ))
            .child(mono_note(auth_label))
            .child(mono_note(format!(
                "{request_count} requests · {group_count} groups"
            )));

        let defaults = section()
            .child(section_heading_with_help(
                "group-defaults-help",
                "New request defaults",
                "These values apply only to new requests in this group. Unset values inherit from parent groups, then application defaults. Existing requests and duplicates are unchanged.",
                cx,
            ))
            .child(field_row(
                "Method",
                Select::new(&self.method).small().h(px(28.)),
                cx,
            ))
            .child(field_row("Initial URL", small_input(&self.url), cx));

        let selected_mode = AUTH_MODES
            .iter()
            .position(|(mode, _)| *mode == self.auth_mode);
        let authorization = section()
            .child(section_heading("Authorization", cx))
            .child(
                RadioGroup::horizontal("auth-mode")
                    .gap(px(14.))
                    .text_size(px(12.))
                    .selected_index(selected_mode)
                    .children(AUTH_MODES.iter().map(|(mode, label)| {
                        Radio::new(SharedString::from(format!("auth-option-{mode:?}")))
                            .label(*label)
                            .small()
                    }))
                    .on_change(cx.listener(|this, index: &usize, _, cx| {
                        this.auth_mode = AUTH_MODES[*index].0;
                        cx.notify();
                    })),
            )
            .when(self.auth_mode == AuthMode::Bearer, |this| {
                this.child(field_row("Token", small_input(&self.bearer), cx))
            })
            .when(self.auth_mode == AuthMode::Basic, |this| {
                this.child(field_row("Username", small_input(&self.username), cx))
                    .child(field_row("Password", small_input(&self.password), cx))
            });

        let table = if self.is_root() {
            self.environments.clone().into_any_element()
        } else {
            self.local_tokens.clone().into_any_element()
        };
        let tokens = section()
            .child(div().mb(px(8.)).child(section_heading_with_help(
                "token-help-local",
                "Tokens",
                "Local tokens inherit through parent groups. Use {{name}} to interpolate tokens. Use {{!NAME}} to read NAME from Blink's process environment when a request is sent.",
                cx,
            )))
            .child(
                div()
                    .overflow_hidden()
                    .border_1()
                    .border_color(colors.input)
                    .rounded(px(2.))
                    .bg(colors.background)
                    .child(table),
            )
            .map(|this| {
                if self.is_root() {
                    this.child(note(
                        "Add an environment column to switch values from the Browser. An empty cell uses the base value. Nested groups follow this group's environment.",
                        colors.muted_foreground,
                    ))
                } else if stored_environments {
                    this.child(note(
                        "This group's environments apply only while it is a root group.",
                        colors.warning,
                    ))
                } else {
                    this
                }
            })
            .when(!self.token_error.is_empty(), |this| {
                this.child(note(self.token_error.clone(), colors.destructive).text_size(px(10.)))
            });

        v_flex()
            .p(px(16.))
            .gap(px(12.))
            .text_color(colors.foreground)
            .child(general)
            .child(defaults)
            .child(authorization)
            .child(tokens)
            .when(!self.form_error.is_empty(), |this| {
                this.child(
                    div()
                        .text_size(px(12.))
                        .text_color(colors.destructive)
                        .child(self.form_error.clone()),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use blink_core::groups::create_group;

    fn group(id: u64, name: &str, parent_id: Option<u64>) -> RequestGroup {
        RequestGroup {
            id,
            parent_id,
            ..create_group(name, parent_id)
        }
    }

    #[test]
    fn writes_the_parent_breadcrumb() {
        let groups = vec![group(1, "API", None), group(2, "Users", Some(1))];
        assert_eq!(parent_breadcrumb(&groups, None), "(root)");
        assert_eq!(parent_breadcrumb(&groups, Some(2)), "API > Users");
    }

    #[test]
    fn labels_local_and_inherited_auth_without_credentials() {
        let mut parent = group(1, "API", None);
        parent.local_auth = Some(AuthorizationConfig::Bearer {
            token: "secret".into(),
        });
        let groups = vec![parent, group(2, "Child", Some(1))];
        let label = effective_auth_label(&groups, Some(1), None);
        assert_eq!(label, "Inherited from API · Bearer");
        assert!(!label.contains("secret"));
        assert_eq!(
            effective_auth_label(&groups, Some(1), Some(&AuthorizationConfig::None)),
            "Local · No auth"
        );
        assert_eq!(effective_auth_label(&[], None, None), "Local · No auth");
    }

    #[test]
    fn checks_the_name_and_url_lengths() {
        assert_eq!(form_error("", ""), Some("Enter a group name."));
        assert_eq!(
            form_error(&"a".repeat(81), ""),
            Some("Group name must be 80 characters or fewer.")
        );
        assert_eq!(
            form_error("ok", &"u".repeat(65537)),
            Some("Initial URL must be 65536 characters or fewer.")
        );
        assert_eq!(form_error("ok", ""), None);
    }

    #[test]
    fn offers_inherit_with_the_inherited_method() {
        let choices = method_choices("POST");
        assert_eq!(choices[0].label.as_ref(), "Inherit (POST)");
        assert_eq!(choices.len(), 8);
    }

    #[test]
    fn keeps_named_rows_as_base_definitions() {
        let rows = vec![
            blink_core::request::pair("host", "x"),
            blink_core::request::pair(" ", "y"),
        ];
        let base = base_definitions(&rows);
        assert_eq!(base.len(), 1);
    }
}
