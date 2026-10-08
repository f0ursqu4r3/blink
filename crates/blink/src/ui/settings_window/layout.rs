//! The settings page structure: a page header, titled groups, and rows with
//! the label at the left and the control at the right.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::theme;
use crate::ui::form::u;
use crate::ui::widgets::tracked;

/// The width of a text field or select at the right of a row.
pub(super) const CONTROL_WIDTH: f32 = 220.;

const HEADER_TRACKING: f32 = 0.12;

/// The page title bar, as the request panel header.
pub(super) fn page_header(title: &'static str, cx: &App) -> Div {
    let colors = theme::colors(cx);
    h_flex()
        .flex_none()
        .h(px(36.))
        .px(u(16.))
        .border_b_1()
        .border_color(colors.border)
        .bg(colors.muted)
        .font_family(theme::MONO)
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(11.))
        .child(tracked(title.to_uppercase(), HEADER_TRACKING))
}

/// A titled group. Dividers separate the rows.
pub(super) fn group(heading: impl IntoElement, rows: Vec<AnyElement>, cx: &App) -> AnyElement {
    let border = theme::colors(cx).border;
    let count = rows.len();
    v_flex()
        .gap(u(4.))
        // Compact uppercase labels use monospace, as in the main window.
        .child(div().font_family(theme::MONO).child(heading))
        .child(
            v_flex().children(rows.into_iter().enumerate().map(move |(index, row)| {
                div()
                    .when(index + 1 < count, |this| {
                        this.border_b_1().border_color(border)
                    })
                    .child(row)
            })),
        )
        .into_any_element()
}

/// The title and description of a row or a stacked item.
fn label(title: &'static str, description: Option<Div>, cx: &App) -> Div {
    v_flex()
        .gap(u(2.))
        .child(
            div()
                .text_size(u(13.))
                .text_color(theme::colors(cx).foreground)
                .child(title),
        )
        .children(description)
}

/// A setting with its control at the right.
pub(super) fn row(
    title: &'static str,
    description: Option<Div>,
    control: impl IntoElement,
    cx: &App,
) -> AnyElement {
    h_flex()
        .py(u(10.))
        .gap(u(16.))
        .justify_between()
        .child(label(title, description, cx).flex_1().min_w_0())
        .child(div().flex_none().child(control))
        .into_any_element()
}

/// A setting whose control needs the full width, such as a list or editor.
pub(super) fn stacked(
    title: &'static str,
    description: Option<Div>,
    content: impl IntoElement,
    cx: &App,
) -> AnyElement {
    v_flex()
        .py(u(10.))
        .gap(u(8.))
        .child(label(title, description, cx))
        .child(content)
        .into_any_element()
}

/// A text field or select at the width of the control column.
pub(super) fn field(control: impl IntoElement) -> Div {
    div().w(u(CONTROL_WIDTH)).child(control)
}
