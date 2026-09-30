//! An editable table of key/value rows: query, headers, form fields,
//! multipart parts, and token tables. Port of `KeyValueEditor.vue`.
//!
//! The owner sets the rows; user edits arrive as `KeyValueEvent::Change`
//! and are never echoed back by `set_rows`.

use blink_core::interpolation::InterpolationContext;
use blink_core::model::Pair;
use gpui_kit::*;

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
    /// The user asked to pick a file for the row with this id.
    PickFile(u64),
}

pub struct KeyValueEditor {
    options: KeyValueOptions,
    rows: Vec<Pair>,
    context: Option<InterpolationContext>,
}

impl EventEmitter<KeyValueEvent> for KeyValueEditor {}

impl KeyValueEditor {
    pub fn new(
        rows: Vec<Pair>,
        options: KeyValueOptions,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        KeyValueEditor {
            options,
            rows,
            context: None,
        }
    }

    /// Replace the rows without emitting `Change`.
    pub fn set_rows(&mut self, rows: Vec<Pair>, _window: &mut Window, cx: &mut Context<Self>) {
        self.rows = rows;
        cx.notify();
    }

    pub fn rows(&self) -> &[Pair] {
        &self.rows
    }

    /// Tokens for resolving and suggesting `{{name}}` references in values.
    pub fn set_context(&mut self, context: Option<InterpolationContext>, cx: &mut Context<Self>) {
        self.context = context;
        cx.notify();
    }
}

impl Render for KeyValueEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let _ = (&self.options, &self.context);
        div()
    }
}
