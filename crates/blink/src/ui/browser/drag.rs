//! Browser drag payloads and the drag preview. Port of `DragPreview.vue` and
//! the Browser parts of `useDragDrop.ts`.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::theme;

use super::css;

/// A group being dragged inside the Browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DraggedGroup {
    pub id: u64,
}

/// The chip that follows the pointer: a folder, a method, or a count.
pub struct DragPreview {
    pub label: String,
    pub method: Option<String>,
    pub folder: bool,
    /// The pointer position inside the dragged row. GPUI places the preview
    /// at the row origin; the chip shows 8 px below and right of the pointer.
    pub offset: Point<Pixels>,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        div()
            .pl(self.offset.x + px(8.))
            .pt(self.offset.y + px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .max_w(css(240.))
                    .gap(css(6.))
                    .rounded(css(6.))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.secondary)
                    .px(css(8.))
                    .py(css(4.))
                    .font_family(theme::MONO)
                    .text_size(css(10.))
                    .text_color(colors.foreground)
                    .shadow_md()
                    .when(self.folder, |this| {
                        this.child(
                            Icon::new(IconName::Folder)
                                .size(css(12.))
                                .flex_shrink_0(),
                        )
                    })
                    .when_some(self.method.clone().filter(|_| !self.folder), |this, method| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_size(css(8.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme::method_color(&method, cx))
                                .child(method),
                        )
                    })
                    .child(div().min_w_0().truncate().child(self.label.clone())),
            )
    }
}

const EDGE: f32 = 24.;
const MAX_SCROLL: f32 = 12.;

/// Scroll speed near an edge of the list: negative scrolls up. `edgeSpeed`.
pub fn edge_speed(start: f32, end: f32, value: f32) -> f32 {
    if value < start + EDGE {
        -(((start + EDGE - value) / EDGE) * MAX_SCROLL).ceil()
    } else if value > end - EDGE {
        (((value - end + EDGE) / EDGE) * MAX_SCROLL).ceil()
    } else {
        0.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn scrolls_faster_closer_to_an_edge() {
        assert_eq!(edge_speed(0., 400., 200.), 0.);
        assert_eq!(edge_speed(0., 400., 0.), -12.);
        assert_eq!(edge_speed(0., 400., 12.), -6.);
        assert_eq!(edge_speed(0., 400., 400.), 12.);
        assert_eq!(edge_speed(0., 400., 390.), 7.);
    }
}
