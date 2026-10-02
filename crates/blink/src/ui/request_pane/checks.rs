//! Assertions and captures of the Tests tab. Port of `ChecksEditor.vue`.

use std::collections::HashMap;

use blink_core::checks::{create_assertion, create_capture, is_capture_name};
use blink_core::model::{Assertion, Capture, CheckOperator, CheckSource};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::Store;
use crate::theme;
use crate::ui::key_value_editor::{check_box, ghost_button};
use crate::ui::request_pane::common::{cell_select, edit_draft, help_link};
use crate::ui::widgets::{WIDEST, tracked};

pub(crate) const ROW_HEIGHT: f32 = 34.;
pub(crate) const SIDE_COLUMN: f32 = 34.;

/// Header name for header checks, a jq path for JSON checks.
fn path_placeholder(source: CheckSource) -> &'static str {
    if source == CheckSource::Header {
        "Header name"
    } else {
        ".path.to.value"
    }
}

/// Sources a capture can read: not time or size.
fn capture_sources() -> Vec<CheckSource> {
    CheckSource::ALL
        .into_iter()
        .filter(|source| !matches!(source, CheckSource::Time | CheckSource::Size))
        .collect()
}

enum Field {
    AssertionPath,
    AssertionExpected,
    CaptureName,
    CapturePath,
}

pub struct ChecksEditor {
    store: Entity<Store>,
    session_id: u64,
    assertions: Vec<Assertion>,
    captures: Vec<Capture>,
    inputs: HashMap<(u64, u8), (Entity<InputState>, Subscription)>,
}

impl ChecksEditor {
    pub fn new(
        store: Entity<Store>,
        session_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut editor = ChecksEditor {
            store,
            session_id,
            assertions: Vec::new(),
            captures: Vec::new(),
            inputs: HashMap::new(),
        };
        editor.reload(window, cx);
        editor
    }

    /// Load the rows from the draft, replacing the inputs' text.
    pub fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.store.read(cx).workspace.session(self.session_id) else {
            return;
        };
        self.assertions = session.draft.assertions.clone().unwrap_or_default();
        self.captures = session.draft.captures.clone().unwrap_or_default();
        self.inputs.clear();
        self.sync_inputs(window, cx);
        cx.notify();
    }

    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut wanted = Vec::new();
        for row in &self.assertions {
            wanted.push((
                row.id,
                Field::AssertionPath,
                row.path.clone(),
                path_placeholder(row.source),
            ));
            wanted.push((
                row.id,
                Field::AssertionExpected,
                row.expected.clone(),
                "Value",
            ));
        }
        for row in &self.captures {
            wanted.push((row.id, Field::CaptureName, row.name.clone(), "name"));
            wanted.push((
                row.id,
                Field::CapturePath,
                row.path.clone(),
                path_placeholder(row.source),
            ));
        }
        let keys: Vec<(u64, u8)> = wanted
            .iter()
            .map(|(id, field, _, _)| (*id, field_key(field)))
            .collect();
        self.inputs.retain(|key, _| keys.contains(key));
        for (id, field, text, placeholder) in wanted {
            let key = (id, field_key(&field));
            if let Some((input, _)) = self.inputs.get(&key) {
                input.update(cx, |input, cx| {
                    input.set_placeholder(placeholder, window, cx)
                });
                continue;
            }
            let input = cx.new(|cx| {
                let mut input = InputState::new(window, cx).placeholder(placeholder);
                input.set_value(text, window, cx);
                input
            });
            let subscription = cx.subscribe_in(&input, window, move |this, input, event, _, cx| {
                if let InputEvent::Change = event {
                    let text = input.read(cx).value().to_string();
                    this.set_text(id, &field, text, cx);
                }
            });
            self.inputs.insert(key, (input, subscription));
        }
    }

    fn set_text(&mut self, id: u64, field: &Field, text: String, cx: &mut Context<Self>) {
        match field {
            Field::AssertionPath | Field::AssertionExpected => {
                if let Some(row) = self.assertions.iter_mut().find(|row| row.id == id) {
                    if matches!(field, Field::AssertionPath) {
                        row.path = text;
                    } else {
                        row.expected = text;
                    }
                }
            }
            Field::CaptureName | Field::CapturePath => {
                if let Some(row) = self.captures.iter_mut().find(|row| row.id == id) {
                    if matches!(field, Field::CaptureName) {
                        row.name = text;
                    } else {
                        row.path = text;
                    }
                }
            }
        }
        self.save(cx);
    }

    /// Write the rows to the draft.
    fn save(&mut self, cx: &mut Context<Self>) {
        let assertions = self.assertions.clone();
        let captures = self.captures.clone();
        edit_draft(&self.store, self.session_id, cx, |draft| {
            draft.assertions = Some(assertions);
            draft.captures = Some(captures);
        });
        cx.notify();
    }

    fn update_assertion(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Assertion),
    ) {
        if let Some(row) = self.assertions.iter_mut().find(|row| row.id == id) {
            change(row);
        }
        self.sync_inputs(window, cx);
        self.save(cx);
    }

    fn update_capture(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Capture),
    ) {
        if let Some(row) = self.captures.iter_mut().find(|row| row.id == id) {
            change(row);
        }
        self.sync_inputs(window, cx);
        self.save(cx);
    }

    fn input(&self, id: u64, field: Field) -> Option<Entity<InputState>> {
        self.inputs
            .get(&(id, field_key(&field)))
            .map(|(input, _)| input.clone())
    }

    fn render_assertions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let entity = cx.entity().downgrade();
        let rows: Vec<AnyElement> = self
            .assertions
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let id = row.id;
                let path = self.input(id, Field::AssertionPath);
                let expected = self.input(id, Field::AssertionExpected);
                let source_entity = entity.clone();
                let operator_entity = entity.clone();
                table_row(cx)
                    .child(enabled_cell(
                        SharedString::from(format!("assertion-enabled-{id}")),
                        row.enabled,
                        format!("Enable assertion {}", index + 1),
                        cx.listener(move |this, _, window, cx| {
                            this.update_assertion(id, window, cx, |row| row.enabled = !row.enabled)
                        }),
                        cx,
                    ))
                    .child(
                        cell(cx).w(relative(0.28)).flex_none().child(cell_select(
                            SharedString::from(format!("assertion-source-{id}")),
                            CheckSource::ALL
                                .into_iter()
                                .map(|source| (source, source.label().into()))
                                .collect(),
                            row.source,
                            false,
                            move |source, window, cx| {
                                source_entity
                                    .update(cx, |this, cx| {
                                        this.update_assertion(id, window, cx, |row| {
                                            row.source = source
                                        })
                                    })
                                    .ok();
                            },
                            cx,
                        )),
                    )
                    .child(
                        cell(cx)
                            .flex_1()
                            .min_w_0()
                            .when(row.source.takes_path(), |this| {
                                this.when_some(path, |this, input| this.child(text_input(&input)))
                            }),
                    )
                    .child(
                        cell(cx).w(relative(0.22)).flex_none().child(cell_select(
                            SharedString::from(format!("assertion-operator-{id}")),
                            CheckOperator::ALL
                                .into_iter()
                                .map(|operator| (operator, operator.label().into()))
                                .collect(),
                            row.operator,
                            false,
                            move |operator, window, cx| {
                                operator_entity
                                    .update(cx, |this, cx| {
                                        this.update_assertion(id, window, cx, |row| {
                                            row.operator = operator
                                        })
                                    })
                                    .ok();
                            },
                            cx,
                        )),
                    )
                    .child(
                        cell(cx)
                            .flex_1()
                            .min_w_0()
                            .when(!row.operator.is_unary(), |this| {
                                this.when_some(expected, |this, input| {
                                    this.child(text_input(&input))
                                })
                            }),
                    )
                    .child(remove_cell(
                        SharedString::from(format!("assertion-remove-{id}")),
                        format!("Remove assertion {}", index + 1),
                        cx.listener(move |this, _, window, cx| {
                            this.assertions.retain(|row| row.id != id);
                            this.sync_inputs(window, cx);
                            this.save(cx);
                        }),
                        cx,
                    ))
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .child(section_heading(
                "ASSERTIONS",
                "assertions-help",
                "Checked after each send. Results show in the response Tests tab. JSON sources take a jq expression.",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .child(head(None, cx).w(px(SIDE_COLUMN)).flex_none())
                    .child(head(Some("Source"), cx).w(relative(0.28)).flex_none().border_l_1())
                    .child(head(Some("Path"), cx).flex_1().border_l_1())
                    .child(head(Some("Operator"), cx).w(relative(0.22)).flex_none().border_l_1())
                    .child(head(Some("Expected"), cx).flex_1().border_l_1())
                    .child(head(None, cx).w(px(SIDE_COLUMN)).flex_none().border_l_1())
                    .border_color(colors.border),
            )
            .children(rows)
            .child(
                div().flex().m(px(6.)).child(
                    ghost_button("add-assertion", IconName::Plus, "Add assertion", cx)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.assertions.push(create_assertion(
                                CheckSource::Status,
                                CheckOperator::Equals,
                                "200",
                                "",
                            ));
                            this.sync_inputs(window, cx);
                            this.save(cx);
                        })),
                ),
            )
    }

    fn render_captures(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let entity = cx.entity().downgrade();
        let rows: Vec<AnyElement> = self
            .captures
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let id = row.id;
                let name = self.input(id, Field::CaptureName);
                let path = self.input(id, Field::CapturePath);
                let invalid = !row.name.is_empty() && !is_capture_name(&row.name);
                let source_entity = entity.clone();
                table_row(cx)
                    .child(enabled_cell(
                        SharedString::from(format!("capture-enabled-{id}")),
                        row.enabled,
                        format!("Enable capture {}", index + 1),
                        cx.listener(move |this, _, window, cx| {
                            this.update_capture(id, window, cx, |row| row.enabled = !row.enabled)
                        }),
                        cx,
                    ))
                    .child(
                        cell(cx)
                            .id(SharedString::from(format!("capture-name-{id}")))
                            .flex_1()
                            .min_w_0()
                            .when(invalid, |this| {
                                this.text_color(colors.destructive).tooltip(|window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(
                                        "Start with a letter. Use letters, digits, _, . or -.",
                                    )
                                    .build(window, cx)
                                })
                            })
                            .when_some(name, |this, input| this.child(text_input(&input))),
                    )
                    .child(
                        cell(cx).w(relative(0.28)).flex_none().child(cell_select(
                            SharedString::from(format!("capture-source-{id}")),
                            capture_sources()
                                .into_iter()
                                .map(|source| (source, source.label().into()))
                                .collect(),
                            row.source,
                            false,
                            move |source, window, cx| {
                                source_entity
                                    .update(cx, |this, cx| {
                                        this.update_capture(id, window, cx, |row| {
                                            row.source = source
                                        })
                                    })
                                    .ok();
                            },
                            cx,
                        )),
                    )
                    .child(
                        cell(cx)
                            .flex_1()
                            .min_w_0()
                            .when(row.source.takes_path(), |this| {
                                this.when_some(path, |this, input| this.child(text_input(&input)))
                            }),
                    )
                    .child(remove_cell(
                        SharedString::from(format!("capture-remove-{id}")),
                        format!("Remove capture {}", index + 1),
                        cx.listener(move |this, _, window, cx| {
                            this.captures.retain(|row| row.id != id);
                            this.sync_inputs(window, cx);
                            this.save(cx);
                        }),
                        cx,
                    ))
                    .into_any_element()
            })
            .collect();
        div()
            .mt_2()
            .flex()
            .flex_col()
            .child(section_heading(
                "CAPTURES",
                "captures-help",
                "After each successful send, saves a response value as a workspace token. Use it in other requests as {{name}}.",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .child(head(None, cx).w(px(SIDE_COLUMN)).flex_none())
                    .child(head(Some("Token"), cx).flex_1().border_l_1())
                    .child(head(Some("Source"), cx).w(relative(0.28)).flex_none().border_l_1())
                    .child(head(Some("Path"), cx).flex_1().border_l_1())
                    .child(head(None, cx).w(px(SIDE_COLUMN)).flex_none().border_l_1())
                    .border_color(colors.border),
            )
            .children(rows)
            .child(
                div().flex().m(px(6.)).child(
                    ghost_button("add-capture", IconName::Plus, "Add capture", cx)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.captures.push(create_capture("", CheckSource::Json, "."));
                            this.sync_inputs(window, cx);
                            this.save(cx);
                        })),
                ),
            )
    }
}

fn field_key(field: &Field) -> u8 {
    match field {
        Field::AssertionPath => 0,
        Field::AssertionExpected => 1,
        Field::CaptureName => 2,
        Field::CapturePath => 3,
    }
}

fn section_heading(
    title: &'static str,
    id: &'static str,
    help: &'static str,
    cx: &App,
) -> impl IntoElement {
    let colors = theme::colors(cx);
    div()
        .h(px(32.))
        .px_3()
        .flex()
        .items_center()
        .justify_between()
        .font_family(theme::MONO)
        .text_size(px(10.))
        .text_color(colors.muted_foreground)
        .child(tracked(title, 0.12))
        .child(help_link(id, "Help", help, cx).font_family(theme::SANS))
}

pub(crate) fn head(text: Option<&'static str>, cx: &App) -> Div {
    let colors = theme::colors(cx);
    div()
        .h(px(32.))
        .px_2()
        .flex()
        .items_center()
        .bg(colors.muted)
        .border_color(colors.border)
        .text_size(px(10.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.muted_foreground)
        .when_some(text, |this, text| {
            this.child(tracked(text.to_uppercase(), WIDEST))
        })
}

pub(crate) fn table_row(cx: &App) -> Div {
    let _ = cx;
    div().flex().w_full()
}

pub(crate) fn cell(cx: &App) -> Div {
    let colors = theme::colors(cx);
    div()
        .h(px(ROW_HEIGHT))
        .flex()
        .items_center()
        .border_b_1()
        .border_l_1()
        .border_color(colors.border)
        .font_family(theme::MONO)
        .text_size(px(12.))
}

pub(crate) fn text_input(input: &Entity<InputState>) -> impl IntoElement {
    Input::new(input)
        .appearance(false)
        .small()
        .px_2()
        .font_family(theme::MONO)
        .text_size(px(12.))
}

fn enabled_cell(
    id: SharedString,
    checked: bool,
    label: String,
    on_click: impl Fn(&bool, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let colors = theme::colors(cx);
    div()
        .w(px(SIDE_COLUMN))
        .h(px(ROW_HEIGHT))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .border_b_1()
        .border_color(colors.border)
        .child(check_box(
            id,
            checked,
            false,
            label,
            move |_, window, cx| on_click(&!checked, window, cx),
            cx,
        ))
}

pub(crate) fn remove_cell(
    id: SharedString,
    label: String,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    cell(cx)
        .w(px(SIDE_COLUMN))
        .flex_none()
        .justify_center()
        .child(
            Button::new(id)
                .ghost()
                .small()
                .icon(Icon::new(IconName::X).size(px(13.)))
                .tooltip(label)
                .on_click(on_click),
        )
}

impl Render for ChecksEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(self.render_assertions(cx))
            .child(self.render_captures(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn captures_never_read_time_or_size() {
        assert_eq!(
            capture_sources(),
            [
                CheckSource::Status,
                CheckSource::Header,
                CheckSource::Json,
                CheckSource::Body
            ]
        );
    }

    #[test]
    fn suggests_a_header_name_or_a_jq_path() {
        assert_eq!(path_placeholder(CheckSource::Header), "Header name");
        assert_eq!(path_placeholder(CheckSource::Json), ".path.to.value");
    }
}
