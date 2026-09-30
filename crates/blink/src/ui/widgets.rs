//! Small shared pieces: method labels and the icon button style of the title
//! bar and Browser header.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::*;

use crate::theme;

/// A method name in its method color, such as `GET` in success green.
pub fn method_label(method: &str, size: f32, cx: &App) -> Div {
    div()
        .font_family(theme::MONO)
        .text_size(px(size))
        .font_weight(FontWeight::BOLD)
        .text_color(theme::method_color(method, cx))
        .child(SharedString::from(method.to_string()))
}

/// A ghost icon button with a tooltip, as the Vue `variant="ghost"` icon buttons.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: impl Into<Icon>,
    tooltip: impl Into<SharedString>,
) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .icon(icon.into())
        .tooltip(tooltip)
}
