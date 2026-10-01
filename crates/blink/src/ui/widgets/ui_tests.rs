//! Headless layout: tracked text is as wide as CSS `letter-spacing` makes
//! it, at every zoom, and an ellipsis parent truncates it.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px, rems,
};

use super::tracked;
use crate::theme;

const LABEL: &str = "REQUESTS";
const EM: f32 = 0.08;
const SIZE: f32 = 11.;

struct Labels {
    narrow: bool,
}

impl Render for Labels {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let label = |name: &'static str| {
            div()
                .flex_none()
                .font_family(theme::MONO)
                .text_size(rems(SIZE / theme::REM))
                .debug_selector(move || name.into())
        };
        div()
            .size_full()
            .child(
                h_flex()
                    .child(label("plain").child(LABEL))
                    .child(label("tracked").child(tracked(LABEL, EM)))
                    // The style on the element itself wins over the parent.
                    .child(
                        label("own-size")
                            .text_size(px(30.))
                            .child(tracked(LABEL, EM).text_size(rems(SIZE / theme::REM))),
                    ),
            )
            .when(self.narrow, |this| {
                this.child(
                    div()
                        .w(px(20.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .debug_selector(|| "box".into())
                        .child(
                            div()
                                .debug_selector(|| "truncated".into())
                                .child(tracked(LABEL, EM)),
                        ),
                )
            })
    }
}

fn width(cx: &mut VisualTestContext, selector: &'static str) -> Pixels {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not painted"))
        .size
        .width
}

#[gpui_kit::test]
fn tracked_width_is_css_letter_spacing(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| Labels { narrow: true });
    for zoom in [1., 1.5, 2.] {
        // The untracked width before layout rounds it to whole pixels.
        let shaped = cx.update(|window, cx| {
            window.set_rem_size(px(theme::REM * zoom));
            view.update(cx, |_, cx| cx.notify());
            window.draw(cx).clear(cx);
            let mut style = window.text_style();
            style.font_family = theme::MONO.into();
            let size = px(SIZE * zoom);
            let run = style.to_run(LABEL.len());
            window
                .text_system()
                .shape_line(LABEL.into(), size, &[run], None)
                .width()
        });
        let plain = width(cx, "plain");
        let tracked = width(cx, "tracked");
        let expected = shaped + px(LABEL.len() as f32 * EM * SIZE * zoom);
        assert!(
            plain > px(0.) && (plain - shaped).abs() <= px(1.),
            "{plain:?} {shaped:?}"
        );
        assert!(
            (tracked - expected).abs() <= px(0.5),
            "zoom {zoom}: tracked {tracked:?}, untracked {shaped:?}, expected {expected:?}"
        );
        assert!((width(cx, "own-size") - expected).abs() <= px(0.5));
        // An ellipsis parent shrinks the label to its box.
        assert!(width(cx, "truncated") <= width(cx, "box") + px(0.5));
    }
}
