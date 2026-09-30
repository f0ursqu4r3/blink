//! An editable table of key/value rows: query, headers, form fields,
//! multipart parts, and token tables. Port of `KeyValueEditor.vue`.
//!
//! The owner sets the rows; user edits arrive as `KeyValueEvent::Change`
//! and are never echoed back by `set_rows`. Duplicate keys are kept.

use std::collections::HashMap;

use blink_core::interpolation::InterpolationContext;
use blink_core::model::Pair;
use blink_core::request::{file_name, pair};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::theme;
use crate::ui::token_input::{TokenInput, TokenInputEvent};

#[derive(Debug, Clone, Default)]
pub struct KeyValueOptions {
    /// Column headers, such as "Name" and "Value".
    pub key_label: SharedString,
    pub value_label: SharedString,
    pub key_placeholder: SharedString,
    pub value_placeholder: SharedString,
    /// Multipart: a row can hold a picked file instead of text.
    pub allow_files: bool,
    /// Show the enabled checkbox column.
    pub toggles: bool,
    /// Id prefix for element ids, unique per editor instance.
    pub id: SharedString,
}

#[derive(Debug, Clone)]
pub enum KeyValueEvent {
    /// The user changed the rows.
    Change(Vec<Pair>),
    /// The user asked to pick a file for the row with this id. Id 0 asks
    /// for a new file row ("Add file"): answer with `add_file_row`, or
    /// `set_row_file` for an existing row.
    PickFile(u64),
}

/// The inputs of one row.
struct RowInputs {
    name: Entity<InputState>,
    value: Entity<TokenInput>,
    _subscriptions: [Subscription; 2],
}

pub struct KeyValueEditor {
    options: KeyValueOptions,
    rows: Vec<Pair>,
    context: Option<InterpolationContext>,
    inputs: HashMap<u64, RowInputs>,
    disabled: bool,
    file_error: String,
    /// Row to focus after the next render creates its inputs.
    focus_row: Option<usize>,
    /// The row noun in tooltips, such as "Header" in "Remove Header row 1".
    row_label: SharedString,
}

impl EventEmitter<KeyValueEvent> for KeyValueEditor {}

/// Row height, `h-8.5`.
const ROW_HEIGHT: f32 = 34.;
/// The enabled and remove columns, `w-8.5`.
const SIDE_COLUMN: f32 = 34.;

impl KeyValueEditor {
    pub fn new(
        rows: Vec<Pair>,
        options: KeyValueOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut editor = KeyValueEditor {
            options,
            rows,
            context: None,
            inputs: HashMap::new(),
            disabled: false,
            file_error: String::new(),
            focus_row: None,
            row_label: "Row".into(),
        };
        editor.sync_inputs(window, cx);
        editor
    }

    /// Replace the rows without emitting `Change`.
    pub fn set_rows(&mut self, rows: Vec<Pair>, window: &mut Window, cx: &mut Context<Self>) {
        self.rows = rows;
        self.sync_inputs(window, cx);
        cx.notify();
    }

    pub fn rows(&self) -> &[Pair] {
        &self.rows
    }

    /// Tokens for resolving and suggesting `{{name}}` references in values.
    pub fn set_context(&mut self, context: Option<InterpolationContext>, cx: &mut Context<Self>) {
        if self.context == context {
            return;
        }
        self.context = context;
        for inputs in self.inputs.values() {
            let context = self.context.clone();
            inputs
                .value
                .update(cx, |value, cx| value.set_context(context, cx));
        }
        cx.notify();
    }

    /// Answer `PickFile(0)`: add a row that sends the picked file, named
    /// after the file without its extension.
    pub fn add_file_row(&mut self, path: String, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let key = match name.rfind('.') {
            Some(dot) => &name[..dot],
            None => name,
        };
        let mut row = pair(key, path);
        row.file = Some(true);
        self.rows.push(row);
        self.focus_row = Some(self.rows.len() - 1);
        self.changed(window, cx);
    }

    /// Answer `PickFile(id)`: the row sends the picked file.
    pub fn set_row_file(&mut self, id: u64, path: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.iter_mut().find(|row| row.id == id) {
            row.value = path;
            row.file = Some(true);
            self.changed(window, cx);
        }
    }

    /// Show why a file could not be picked; empty clears it.
    pub fn set_file_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.file_error = error;
        cx.notify();
    }

    /// Create inputs for new rows, drop inputs of removed rows, and load
    /// values that changed from outside.
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self.rows.iter().map(|row| row.id).collect();
        self.inputs.retain(|id, _| ids.contains(id));
        for index in 0..self.rows.len() {
            let row = self.rows[index].clone();
            if let Some(inputs) = self.inputs.get(&row.id) {
                if inputs.name.read(cx).value().as_ref() != row.key {
                    inputs
                        .name
                        .update(cx, |input, cx| input.set_value(row.key.clone(), window, cx));
                }
                if inputs.value.read(cx).value(cx) != row.value {
                    inputs
                        .value
                        .update(cx, |input, cx| input.set_value(&row.value, window, cx));
                }
                let muted = !row.enabled;
                inputs.value.update(cx, |input, cx| input.set_muted(muted, cx));
                continue;
            }
            let inputs = self.create_inputs(&row, window, cx);
            self.inputs.insert(row.id, inputs);
        }
    }

    fn create_inputs(&mut self, row: &Pair, window: &mut Window, cx: &mut Context<Self>) -> RowInputs {
        let id = row.id;
        let key_placeholder = placeholder(&self.options.key_placeholder, "Name");
        let value_placeholder = placeholder(&self.options.value_placeholder, "Value");
        let name = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(key_placeholder);
            input.set_value(row.key.clone(), window, cx);
            input
        });
        let context = self.context.clone();
        let disabled = self.disabled;
        let value_text = row.value.clone();
        let muted = !row.enabled;
        let value = cx.new(|cx| {
            let mut input = TokenInput::new(value_placeholder, window, cx);
            input.set_context(context, cx);
            input.set_value(&value_text, window, cx);
            input.set_appearance(false, cx);
            input.set_size(gpui_kit::component::Size::Small, cx);
            input.set_disabled(disabled, cx);
            input.set_muted(muted, cx);
            input
        });
        let name_subscription =
            cx.subscribe_in(&name, window, move |this, input, event: &InputEvent, window, cx| {
                if let InputEvent::Change = event {
                    let key = input.read(cx).value().to_string();
                    this.update_row(id, window, cx, |row| row.key = key);
                }
            });
        let value_subscription =
            cx.subscribe_in(&value, window, move |this, _, event: &TokenInputEvent, window, cx| {
                if let TokenInputEvent::Change(text) = event {
                    let text = text.clone();
                    this.update_row(id, window, cx, |row| row.value = text);
                }
            });
        RowInputs {
            name,
            value,
            _subscriptions: [name_subscription, value_subscription],
        }
    }

    fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_inputs(window, cx);
        cx.emit(KeyValueEvent::Change(self.rows.clone()));
        cx.notify();
    }

    fn update_row(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Pair),
    ) {
        if self.disabled {
            return;
        }
        if let Some(row) = self.rows.iter_mut().find(|row| row.id == id) {
            change(row);
            self.changed(window, cx);
        }
    }

    fn toggle_row(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.update_row(id, window, cx, |row| row.enabled = !row.enabled);
    }

    /// Switch a row between a text value and a file. The value is cleared.
    fn toggle_file(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.update_row(id, window, cx, |row| {
            row.value.clear();
            row.file = (!row.file.unwrap_or(false)).then_some(true);
        });
    }

    fn duplicate_row(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        let Some(index) = self.rows.iter().position(|row| row.id == id) else {
            return;
        };
        let source = &self.rows[index];
        let mut copy = pair(source.key.clone(), source.value.clone());
        copy.enabled = source.enabled;
        copy.file = source.file.filter(|file| *file);
        self.rows.insert(index + 1, copy);
        self.focus_row = Some(index + 1);
        self.changed(window, cx);
    }

    fn remove_row(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        let Some(index) = self.rows.iter().position(|row| row.id == id) else {
            return;
        };
        self.rows.remove(index);
        self.focus_row = Some(index);
        self.changed(window, cx);
    }

    fn set_all(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        for row in &mut self.rows {
            row.enabled = enabled;
        }
        self.changed(window, cx);
    }

    fn add_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.rows.push(pair("", ""));
        self.focus_row = Some(self.rows.len() - 1);
        self.changed(window, cx);
    }

    fn pick_file(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.file_error.clear();
        cx.emit(KeyValueEvent::PickFile(id));
    }

    /// Focus the name of the row at `index`, or the last row when it is gone.
    fn apply_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.focus_row.take() else {
            return;
        };
        let Some(row) = self.rows.get(index.min(self.rows.len().saturating_sub(1))) else {
            return;
        };
        if let Some(inputs) = self.inputs.get(&row.id) {
            inputs.name.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    fn label(&self) -> &str {
        self.row_label.as_ref()
    }

    /// The row noun in tooltips, such as "Header" in "Remove Header row 1".
    pub fn set_row_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.row_label = label.into();
        cx.notify();
    }

    /// Let rows send a picked file (multipart bodies).
    pub fn set_allow_files(&mut self, allow: bool, cx: &mut Context<Self>) {
        self.options.allow_files = allow;
        cx.notify();
    }

    fn row_menu(
        entity: WeakEntity<Self>,
        row: &Pair,
        toggles: bool,
        files: bool,
        disabled: bool,
        menu: PopupMenu,
    ) -> PopupMenu {
        let id = row.id;
        let key = row.key.clone();
        let value = row.value.clone();
        let on = |entity: &WeakEntity<Self>, f: fn(&mut Self, u64, &mut Window, &mut Context<Self>)| {
            let entity = entity.clone();
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                entity.update(cx, |this, cx| f(this, id, window, cx)).ok();
            }
        };
        menu.when(toggles, |menu| {
            menu.item(
                PopupMenuItem::new("Enabled")
                    .checked(row.enabled)
                    .disabled(disabled)
                    .on_click(on(&entity, Self::toggle_row)),
            )
        })
        .when(files, |menu| {
            menu.item(
                PopupMenuItem::new("File value")
                    .checked(row.file.unwrap_or(false))
                    .disabled(disabled)
                    .on_click(on(&entity, Self::toggle_file)),
            )
        })
        .item(
            PopupMenuItem::new("Duplicate row")
                .disabled(disabled)
                .on_click(on(&entity, Self::duplicate_row)),
        )
        .separator()
        .item(
            PopupMenuItem::new("Copy name")
                .disabled(key.is_empty())
                .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(key.clone()))),
        )
        .item(
            PopupMenuItem::new("Copy value")
                .disabled(value.is_empty())
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(value.clone()))
                }),
        )
        .separator()
        .item(
            PopupMenuItem::new("Delete row")
                .disabled(disabled)
                .on_click(on(&entity, Self::remove_row)),
        )
    }

    fn render_row(&self, index: usize, row: &Pair, cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::colors(cx);
        let prefix = self.options.id.clone();
        let label = self.label().to_string();
        let id = row.id;
        let toggles = self.options.toggles;
        let files = self.options.allow_files;
        let disabled = self.disabled;
        let entity = cx.entity().downgrade();
        let menu_row = row.clone();
        let Some(inputs) = self.inputs.get(&row.id) else {
            return div().into_any_element();
        };
        let cell = || {
            div()
                .h(px(ROW_HEIGHT))
                .border_b_1()
                .border_color(colors.border)
                .flex()
                .items_center()
        };
        let value_cell = if row.file.unwrap_or(false) {
            let text = if row.value.is_empty() {
                "Choose file…".to_string()
            } else {
                file_name(&row.value).to_string()
            };
            let muted = !row.enabled || row.value.is_empty();
            div()
                .id(SharedString::from(format!("{prefix}-file-{id}")))
                .size_full()
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(10.))
                .font_family(theme::MONO)
                .text_size(px(12.))
                .text_color(if muted {
                    colors.muted_foreground
                } else {
                    colors.foreground
                })
                .hover(|this| this.bg(colors.muted))
                .cursor_pointer()
                .child(Icon::new(IconName::FileUp).size(px(12.)).flex_none())
                .child(div().min_w_0().truncate().child(text))
                .when(!row.value.is_empty(), |this| {
                    let path = row.value.clone();
                    this.tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(path.clone()).build(window, cx)
                    })
                })
                .on_click(cx.listener(move |this, _, _, cx| this.pick_file(id, cx)))
                .into_any_element()
        } else {
            div()
                .size_full()
                .flex()
                .items_center()
                .font_family(theme::MONO)
                .child(inputs.value.clone())
                .into_any_element()
        };
        div()
            .id(SharedString::from(format!("{prefix}-row-{id}")))
            .flex()
            .w_full()
            .when(toggles, |this| {
                this.child(
                    cell().w(px(SIDE_COLUMN)).flex_none().justify_center().child(
                        check_box(
                            SharedString::from(format!("{prefix}-enabled-{id}")),
                            row.enabled,
                            disabled,
                            format!("Enable {label} row {}", index + 1),
                            cx.listener(move |this, _, window, cx| this.toggle_row(id, window, cx)),
                            cx,
                        ),
                    ),
                )
            })
            .child(
                cell()
                    .flex_1()
                    .min_w_0()
                    .when(toggles, |this| this.border_l_1())
                    .font_family(theme::MONO)
                    .text_color(if row.enabled {
                        colors.foreground
                    } else {
                        colors.muted_foreground
                    })
                    .child(
                        Input::new(&inputs.name)
                            .appearance(false)
                            .small()
                            .px(px(10.))
                            .font_family(theme::MONO)
                            .text_size(px(12.))
                            .disabled(disabled),
                    ),
            )
            .child(cell().flex_1().min_w_0().border_l_1().child(value_cell))
            .child(
                cell()
                    .w(px(SIDE_COLUMN))
                    .flex_none()
                    .border_l_1()
                    .justify_center()
                    .child(
                        Button::new(SharedString::from(format!("{prefix}-remove-{id}")))
                            .ghost()
                            .small()
                            .icon(Icon::new(IconName::X).size(px(13.)))
                            .disabled(disabled)
                            .tooltip(format!("Remove {label} row {}", index + 1))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.remove_row(id, window, cx)
                            })),
                    ),
            )
            .context_menu(move |menu, _, _| {
                Self::row_menu(entity.clone(), &menu_row, toggles, files, disabled, menu)
            })
            .into_any_element()
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let toggles = self.options.toggles;
        let disabled = self.disabled;
        let entity = cx.entity().downgrade();
        let head = |text: Option<SharedString>| {
            div()
                .h(px(32.))
                .bg(colors.muted)
                .flex()
                .items_center()
                .px(px(10.))
                .text_size(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.muted_foreground)
                .when_some(text, |this, text| this.child(text.to_uppercase()))
        };
        let key = placeholder(&self.options.key_label, "Name");
        let value = placeholder(&self.options.value_label, "Value");
        div()
            .id(SharedString::from(format!("{}-head", self.options.id)))
            .flex()
            .w_full()
            .font_family(theme::SANS)
            .when(toggles, |this| this.child(head(None).w(px(SIDE_COLUMN)).flex_none()))
            .child(
                head(Some(key))
                    .flex_1()
                    .min_w_0()
                    .when(toggles, |this| this.border_l_1().border_color(colors.border)),
            )
            .child(
                head(Some(value))
                    .flex_1()
                    .min_w_0()
                    .border_l_1()
                    .border_color(colors.border),
            )
            .child(
                head(None)
                    .w(px(SIDE_COLUMN))
                    .flex_none()
                    .border_l_1()
                    .border_color(colors.border),
            )
            .context_menu(move |menu, _, _| {
                let add = entity.clone();
                let enable = entity.clone();
                let disable = entity.clone();
                menu.item(
                    PopupMenuItem::new("Add row")
                        .disabled(disabled)
                        .on_click(move |_, window, cx| {
                            add.update(cx, |this, cx| this.add_row(window, cx)).ok();
                        }),
                )
                .when(toggles, |menu| {
                    menu.separator()
                        .item(PopupMenuItem::new("Enable all").disabled(disabled).on_click(
                            move |_, window, cx| {
                                enable
                                    .update(cx, |this, cx| this.set_all(true, window, cx))
                                    .ok();
                            },
                        ))
                        .item(PopupMenuItem::new("Disable all").disabled(disabled).on_click(
                            move |_, window, cx| {
                                disable
                                    .update(cx, |this, cx| this.set_all(false, window, cx))
                                    .ok();
                            },
                        ))
                })
            })
    }
}

/// Tailwind sizes are rem based: they scale with the app zoom.
fn css(value: f32) -> Rems {
    rems(value / 16.)
}

/// A native checkbox (`accent-primary w-3 h-3`): a 12 px square with a
/// small radius, filled with the primary color and a dark check mark.
pub fn check_box(
    id: impl Into<ElementId>,
    checked: bool,
    disabled: bool,
    tooltip: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let colors = theme::colors(cx);
    let tooltip: SharedString = tooltip.into();
    div()
        .id(id)
        .size(css(12.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(2.5))
        .border_1()
        .when(checked, |this| {
            this.bg(colors.primary)
                .border_color(colors.primary)
                .child(
                    Icon::new(IconName::Check)
                        .size(css(11.))
                        .text_color(colors.primary_foreground),
                )
        })
        .when(!checked, |this| this.bg(colors.muted).border_color(colors.muted_foreground.opacity(0.7)))
        .when(disabled, |this| this.opacity(0.5))
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .when(!disabled, |this| {
            this.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .on_click(on_click)
        })
}

/// A Vue `variant="ghost"` text button (`h-7 px-2.5 font-mono text-xs`,
/// muted until hovered) with a 13 px icon.
pub fn ghost_button(
    id: impl Into<ElementId>,
    icon: IconName,
    label: impl Into<SharedString>,
    cx: &App,
) -> Button {
    let colors = theme::colors(cx);
    Button::new(id)
        .ghost()
        .xsmall()
        .h(css(28.))
        .px(css(10.))
        .rounded(px(4.))
        .font_family(theme::MONO)
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.muted_foreground)
        .icon(Icon::new(icon).size(css(13.)))
        .child(div().ml(css(2.)).child(label.into()))
}

fn placeholder(text: &SharedString, fallback: &'static str) -> SharedString {
    if text.is_empty() {
        fallback.into()
    } else {
        text.clone()
    }
}

impl Render for KeyValueEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_focus(window, cx);
        let colors = theme::colors(cx);
        let prefix = self.options.id.clone();
        let rows: Vec<AnyElement> = self
            .rows
            .clone()
            .iter()
            .enumerate()
            .map(|(index, row)| self.render_row(index, row, cx))
            .collect();
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(self.render_header(cx))
            .children(rows)
            .when(!self.file_error.is_empty(), |this| {
                this.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_size(px(12.))
                        .text_color(colors.destructive)
                        .child(self.file_error.clone()),
                )
            })
            .child(
                div()
                    .flex()
                    .gap_1()
                    .m_2()
                    .child(
                        ghost_button(SharedString::from(format!("{prefix}-add-row")), IconName::Plus, "Add row", cx)
                            .disabled(self.disabled)
                            .on_click(cx.listener(|this, _, window, cx| this.add_row(window, cx))),
                    )
                    .when(self.options.allow_files, |this| {
                        this.child(
                            ghost_button(SharedString::from(format!("{prefix}-add-file")), IconName::FileUp, "Add file", cx)
                                .disabled(self.disabled)
                                .on_click(cx.listener(|this, _, _, cx| this.pick_file(0, cx))),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn falls_back_to_the_default_column_names() {
        assert_eq!(placeholder(&SharedString::default(), "Name").as_ref(), "Name");
        assert_eq!(placeholder(&"Token".into(), "Name").as_ref(), "Token");
    }
}
