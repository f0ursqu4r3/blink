//! Form pieces the settings, group settings, and cookies dialogs share:
//! the dialog frame, section headings, help triggers, and select options.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::select::{SelectItem, SelectState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{IndexPath, Sizable as _, h_flex};
use gpui_kit::*;

use crate::theme;

/// A size in CSS pixels at zoom 1. It scales with the root rem size, as
/// the webview zoom scaled the Vue dialogs.
pub fn u(value: f32) -> Rems {
    rems(value / 16.)
}

/// One select option: a saved value and the label the user reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub value: SharedString,
    pub label: SharedString,
}

impl Choice {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Choice {
            value: value.into(),
            label: label.into(),
        }
    }

    /// An option whose label is its value, such as `GET`.
    pub fn same(value: impl Into<SharedString>) -> Self {
        let value = value.into();
        Choice {
            label: value.clone(),
            value,
        }
    }
}

impl SelectItem for Choice {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &SharedString {
        &self.value
    }
}

pub type ChoiceSelect = SelectState<Vec<Choice>>;

/// Index of `value` in `choices`, as a select index.
pub fn choice_index(choices: &[Choice], value: &str) -> Option<IndexPath> {
    choices
        .iter()
        .position(|choice| choice.value.as_ref() == value)
        .map(|row| IndexPath::default().row(row))
}

/// A select over `choices` with `value` selected.
pub fn choice_select<T>(
    choices: Vec<Choice>,
    value: &str,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<ChoiceSelect> {
    let index = choice_index(&choices, value);
    cx.new(|cx| SelectState::new(choices, index, window, cx))
}

/// The selected value of a choice select, or "".
pub fn selected(select: &Entity<ChoiceSelect>, cx: &App) -> String {
    select
        .read(cx)
        .selected_value()
        .map(|value| value.to_string())
        .unwrap_or_default()
}

/// The dialog title bar: `border-b px-4 py-3`, bold 14 px title.
pub fn dialog_header(title: &'static str, cx: &App) -> Div {
    div()
        .flex_shrink_0()
        .border_b_1()
        .border_color(theme::colors(cx).border)
        .px(u(16.))
        .py(u(12.))
        .child(
            div()
                .text_size(u(14.))
                .font_weight(FontWeight::BOLD)
                .text_color(theme::colors(cx).foreground)
                .child(title),
        )
}

/// The dialog footer: `border-t px-4 py-3`, buttons at the end.
pub fn dialog_footer(cx: &App) -> Div {
    h_flex()
        .flex_shrink_0()
        .justify_end()
        .gap(u(8.))
        .border_t_1()
        .border_color(theme::colors(cx).border)
        .px(u(16.))
        .py(u(12.))
}

/// A footer button: `h-7.5 px-3.5 font-mono text-xs`.
pub fn footer_button(id: &'static str, label: &'static str, primary: bool) -> Button {
    // XSmall sets the label to `text-xs`; the label ignores `text_size`.
    let button = Button::new(id)
        .label(label)
        .xsmall()
        .h(u(30.))
        .px(u(14.))
        .font_family(theme::MONO);
    if primary { button.primary() } else { button.outline() }
}

/// Uppercase section heading: `text-xs font-semibold uppercase`.
pub fn section_heading(text: &'static str, cx: &App) -> Div {
    div()
        .text_size(u(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::colors(cx).muted_foreground)
        .child(text.to_uppercase())
}

/// A heading with a `?` help trigger, as `HelpTooltip.vue` around a button.
pub fn section_heading_with_help(
    id: &'static str,
    text: &'static str,
    help: &'static str,
    cx: &App,
) -> Div {
    h_flex()
        .gap(u(6.))
        .child(section_heading(text, cx))
        .child(help_trigger(id, help, cx))
}

/// The round `?` help trigger. Hover or focus shows `text`.
pub fn help_trigger(id: &'static str, text: &'static str, cx: &App) -> Stateful<Div> {
    let colors = theme::colors(cx);
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(u(20.))
        .rounded_full()
        .border_1()
        .border_color(colors.input)
        .text_size(u(10.))
        .text_color(colors.muted_foreground)
        .cursor_pointer()
        .hover(move |style| style.bg(colors.accent).text_color(colors.foreground))
        .child("?")
        .tooltip(move |window, cx| help_tooltip(text, window, cx))
}

/// Tooltip content: `max-w-72 font-mono text-[11px]`.
pub fn help_tooltip(text: &'static str, window: &mut Window, cx: &mut App) -> AnyView {
    Tooltip::element(move |_, _| {
        div()
            .max_w(u(288.))
            .font_family(theme::MONO)
            .text_size(u(11.))
            .child(text)
    })
    .build(window, cx)
}

/// A field label: muted 12 px text.
pub fn field_label(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_size(u(12.))
        .text_color(theme::colors(cx).muted_foreground)
        .child(text.into())
}

/// Small help or status text under a field: `text-[0.6875rem]`.
pub fn note(text: impl Into<SharedString>, color: Hsla) -> Div {
    div().text_size(u(11.)).text_color(color).child(text.into())
}

/// A labeled checkbox at 12 px.
pub fn check(id: impl Into<ElementId>, label: &'static str, checked: bool) -> Checkbox {
    Checkbox::new(id)
        .label(label)
        .checked(checked)
        .small()
        .text_size(u(12.))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn finds_the_index_of_a_value() {
        let choices = vec![Choice::same("GET"), Choice::same("POST")];
        assert_eq!(
            choice_index(&choices, "POST"),
            Some(IndexPath::default().row(1))
        );
        assert_eq!(choice_index(&choices, "PUT"), None);
    }
}
