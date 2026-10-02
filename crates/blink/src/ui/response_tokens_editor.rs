//! The response tokens table of Group Settings and Application Settings:
//! tokens whose values come from another request's last 2xx response.

use std::collections::HashMap;

use blink_core::command_center::group_path;
use blink_core::ids;
use blink_core::model::{CheckSource, RequestGroup, RequestSession, ResponseToken};
use blink_core::response_tokens::{
    RESPONSE_TOKEN_SOURCES, format_max_age, parse_max_age, validate_response_tokens,
};
use blink_core::session::{LabelTokens, session_label};
use blink_core::workspace_state::Workspace;
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::theme;
use crate::ui::form::{Choice, ChoiceSelect, choice_index, note, section_heading, selected};
use crate::ui::key_value_editor::ghost_button;
use crate::ui::request_pane::checks::{
    SIDE_COLUMN, cell, head, remove_cell, table_row, text_input,
};
use crate::ui::request_pane::common::cell_select;

const HELP: &str = "Read a value from another request's last 2xx response. Blink sends that request first when the value is missing, too old, or from other settings.";

/// A jq path for JSON, a header name for Header. Body text and Status take
/// no path.
fn path_placeholder(source: CheckSource) -> &'static str {
    match source {
        CheckSource::Json => ".access_token",
        CheckSource::Header => "Header name",
        _ => "",
    }
}

/// Each request as `(id, "Group / Sub / label")`, in Browser order:
/// ungrouped requests first, then each group depth-first with its requests
/// before its child groups.
pub fn request_choices(workspace: &Workspace) -> Vec<(u64, String)> {
    let order = group_order(&workspace.groups);
    let mut sessions: Vec<&RequestSession> = workspace.sessions.iter().collect();
    sessions.sort_by_key(|session| match session.group_id {
        None => 0,
        Some(id) => order.get(&id).map_or(usize::MAX, |index| index + 1),
    });
    let tokens = LabelTokens {
        groups: &workspace.groups,
        global_definitions: &workspace.global_definitions,
    };
    sessions
        .into_iter()
        .map(|session| {
            let label = session_label(session, Some(tokens));
            let path = group_path(&workspace.groups, session.group_id);
            let text = if path.is_empty() {
                label
            } else {
                format!("{path} / {label}")
            };
            (session.id, text)
        })
        .collect()
}

/// The depth-first index of each group, as the Browser lists them.
fn group_order(groups: &[RequestGroup]) -> HashMap<u64, usize> {
    let mut children: HashMap<Option<u64>, Vec<u64>> = HashMap::new();
    for group in groups {
        children.entry(group.parent_id).or_default().push(group.id);
    }
    let mut order = HashMap::new();
    let mut stack: Vec<u64> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .copied()
        .collect();
    while let Some(id) = stack.pop() {
        if order.contains_key(&id) {
            continue;
        }
        order.insert(id, order.len());
        stack.extend(children.get(&Some(id)).into_iter().flatten().rev());
    }
    order
}

struct Row {
    id: u64,
    name: Entity<InputState>,
    request: Entity<ChoiceSelect>,
    source: CheckSource,
    path: Entity<InputState>,
    max_age: Entity<InputState>,
}

pub struct ResponseTokensEditor {
    requests: Vec<(u64, String)>,
    rows: Vec<Row>,
    /// The error of each row, by token id, from the last save.
    errors: HashMap<u64, String>,
}

impl ResponseTokensEditor {
    pub fn new(
        tokens: Vec<ResponseToken>,
        requests: Vec<(u64, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut editor = ResponseTokensEditor {
            requests,
            rows: Vec::new(),
            errors: HashMap::new(),
        };
        for token in tokens {
            editor.push_row(token, window, cx);
        }
        editor
    }

    fn push_row(&mut self, token: ResponseToken, window: &mut Window, cx: &mut Context<Self>) {
        let input = |text: String,
                     placeholder: &'static str,
                     window: &mut Window,
                     cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut input = InputState::new(window, cx).placeholder(placeholder);
                input.set_value(text, window, cx);
                input
            })
        };
        let name = input(token.name, "name", window, cx);
        let path = input(token.path, path_placeholder(token.source), window, cx);
        let max_age = input(format_max_age(token.max_age_secs), "None", window, cx);
        let choices: Vec<Choice> = self
            .requests
            .iter()
            .map(|(id, label)| Choice::new(id.to_string(), label.clone()))
            .collect();
        let index = choice_index(&choices, &token.request_id.to_string());
        let request = cx.new(|cx| SelectState::new(choices, index, window, cx).searchable(true));
        self.rows.push(Row {
            id: token.id,
            name,
            request,
            source: token.source,
            path,
            max_age,
        });
    }

    /// Add an empty JSON row. Returns its id.
    pub fn add_row(&mut self, window: &mut Window, cx: &mut Context<Self>) -> u64 {
        let id = ids::RESPONSE_TOKENS.next();
        self.push_row(
            ResponseToken {
                id,
                name: String::new(),
                request_id: 0,
                source: CheckSource::Json,
                path: String::new(),
                max_age_secs: None,
            },
            window,
            cx,
        );
        cx.notify();
        id
    }

    pub fn set_source(
        &mut self,
        id: u64,
        source: CheckSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.rows.iter_mut().find(|row| row.id == id) {
            row.source = source;
            row.path.update(cx, |input, cx| {
                input.set_placeholder(path_placeholder(source), window, cx)
            });
        }
        cx.notify();
    }

    fn remove_row(&mut self, id: u64, cx: &mut Context<Self>) {
        self.rows.retain(|row| row.id != id);
        self.errors.remove(&id);
        cx.notify();
    }

    /// The rows as tokens. Returns the first max-age error.
    pub fn tokens(&self, cx: &App) -> Result<Vec<ResponseToken>, String> {
        self.rows
            .iter()
            .map(|row| {
                let max_age_secs = parse_max_age(&row.max_age.read(cx).value())?;
                let path = if row.source.takes_path() {
                    row.path.read(cx).value().trim().to_string()
                } else {
                    String::new()
                };
                Ok(ResponseToken {
                    id: row.id,
                    name: row.name.read(cx).value().trim().to_string(),
                    request_id: selected(&row.request, cx).parse().unwrap_or(0),
                    source: row.source,
                    path,
                    max_age_secs,
                })
            })
            .collect()
    }

    /// The id of the first row whose max age does not parse.
    fn bad_max_age_row(&self, cx: &App) -> Option<u64> {
        self.rows
            .iter()
            .find(|row| parse_max_age(&row.max_age.read(cx).value()).is_err())
            .map(|row| row.id)
    }

    /// Show each error under its row. Replaces the errors shown before.
    pub fn show_errors(&mut self, errors: Vec<(u64, String)>, cx: &mut Context<Self>) {
        self.errors = errors.into_iter().collect();
        cx.notify();
    }

    /// The tokens to save, or None after it shows the errors.
    /// `text_names` are the text tokens of the same scope.
    pub fn validated(
        &mut self,
        text_names: &[&str],
        sessions: &[RequestSession],
        cx: &mut Context<Self>,
    ) -> Option<Vec<ResponseToken>> {
        let tokens = match self.tokens(cx) {
            Ok(tokens) => tokens,
            Err(error) => {
                let errors = self.bad_max_age_row(cx).map(|id| (id, error));
                self.show_errors(errors.into_iter().collect(), cx);
                return None;
            }
        };
        let errors = validate_response_tokens(&tokens, text_names, sessions);
        let valid = errors.is_empty();
        self.show_errors(errors, cx);
        valid.then_some(tokens)
    }

    fn render_row(&self, index: usize, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::colors(cx);
        let id = row.id;
        let entity = cx.entity().downgrade();
        let error = self.errors.get(&id).cloned();
        let takes_path = row.source.takes_path();
        div()
            .flex()
            .flex_col()
            .child(
                table_row(cx)
                    .child(
                        cell(cx)
                            .flex_1()
                            .min_w_0()
                            .border_l_0()
                            .when(error.is_some(), |this| this.text_color(colors.destructive))
                            .child(text_input(&row.name)),
                    )
                    .child(
                        cell(cx).w(relative(0.3)).flex_none().min_w_0().child(
                            Select::new(&row.request)
                                .appearance(false)
                                .small()
                                .w_full()
                                .font_family(theme::MONO)
                                .text_size(px(12.))
                                .placeholder("Choose a request")
                                .search_placeholder("Search requests")
                                .menu_width(px(360.)),
                        ),
                    )
                    .child(
                        cell(cx).w(relative(0.16)).flex_none().child(cell_select(
                            SharedString::from(format!("response-token-source-{id}")),
                            RESPONSE_TOKEN_SOURCES
                                .into_iter()
                                .map(|source| (source, source.label().into()))
                                .collect(),
                            row.source,
                            false,
                            move |source, window, cx| {
                                entity
                                    .update(cx, |this, cx| this.set_source(id, source, window, cx))
                                    .ok();
                            },
                            cx,
                        )),
                    )
                    .child(
                        cell(cx).flex_1().min_w_0().child(
                            Input::new(&row.path)
                                .appearance(false)
                                .small()
                                .px_2()
                                .font_family(theme::MONO)
                                .text_size(px(12.))
                                .disabled(!takes_path),
                        ),
                    )
                    .child(
                        cell(cx)
                            .w(px(72.))
                            .flex_none()
                            .child(text_input(&row.max_age)),
                    )
                    .child(remove_cell(
                        SharedString::from(format!("response-token-remove-{id}")),
                        format!("Remove response token {}", index + 1),
                        cx.listener(move |this, _, _, cx| this.remove_row(id, cx)),
                        cx,
                    )),
            )
            .when_some(error, |this, error| {
                this.child(
                    note(error, colors.destructive)
                        .px_2()
                        .py(px(4.))
                        .border_b_1()
                        .border_color(colors.border),
                )
            })
            .into_any_element()
    }
}

impl Render for ResponseTokensEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let rows: Vec<AnyElement> = self
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| self.render_row(index, row, cx))
            .collect();
        v_flex()
            .gap(px(6.))
            .child(section_heading("Response tokens", cx))
            .child(note(HELP, colors.muted_foreground))
            .child(
                div()
                    .debug_selector(|| "response-tokens-table".into())
                    .overflow_hidden()
                    .border_1()
                    .border_color(colors.input)
                    .rounded(px(2.))
                    .bg(colors.background)
                    .child(
                        div()
                            .flex()
                            .border_b_1()
                            .border_color(colors.border)
                            .child(head(Some("Name"), cx).flex_1())
                            .child(
                                head(Some("Request"), cx)
                                    .w(relative(0.3))
                                    .flex_none()
                                    .border_l_1(),
                            )
                            .child(
                                head(Some("Source"), cx)
                                    .w(relative(0.16))
                                    .flex_none()
                                    .border_l_1(),
                            )
                            .child(head(Some("Path"), cx).flex_1().border_l_1())
                            .child(
                                head(Some("Max age"), cx)
                                    .w(px(72.))
                                    .flex_none()
                                    .border_l_1(),
                            )
                            .child(head(None, cx).w(px(SIDE_COLUMN)).flex_none().border_l_1()),
                    )
                    .children(rows)
                    .child(
                        div().flex().m(px(6.)).child(
                            ghost_button(
                                "add-response-token",
                                IconName::Plus,
                                "Add response token",
                                cx,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.add_row(window, cx);
                                },
                            )),
                        ),
                    ),
            )
    }
}

#[cfg(test)]
mod ui_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn suggests_a_path_for_json_and_header_only() {
        assert_eq!(path_placeholder(CheckSource::Json), ".access_token");
        assert_eq!(path_placeholder(CheckSource::Header), "Header name");
        assert_eq!(path_placeholder(CheckSource::Body), "");
        assert_eq!(path_placeholder(CheckSource::Status), "");
    }

    #[test]
    fn orders_groups_depth_first() {
        let group = |id, parent_id| RequestGroup {
            id,
            parent_id,
            ..blink_core::groups::create_group("g", parent_id)
        };
        let groups = vec![group(1, None), group(2, None), group(3, Some(1))];
        let order = group_order(&groups);
        assert!(order[&1] < order[&3] && order[&3] < order[&2]);
    }
}
