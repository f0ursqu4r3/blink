//! The duration with its phase timing on hover. Port of `TimingCard.vue`.

use std::time::Duration;

use blink_core::json::js_number_string;
use blink_core::model::ResponseTiming;
use blink_core::timing::{TimingPhaseId, format_ms, timing_phases};
use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::theme;
use crate::ui::response_panel::r;

/// Bar placement as fractions of the total: `(left, width)`.
pub fn bar(offset_ms: f64, ms: f64, total: f64) -> (f32, f32) {
    ((offset_ms / total) as f32, (ms / total) as f32)
}

pub fn render(timing: Option<&ResponseTiming>, duration_ms: f64, cx: &App) -> AnyElement {
    let colors = theme::colors(cx);
    let phases = timing.map(timing_phases).unwrap_or_default();
    let total = phases
        .iter()
        .map(|phase| phase.ms)
        .sum::<f64>()
        .max(duration_ms)
        .max(1.0);
    let trigger = div()
        .id("response-duration")
        .cursor_default()
        .underline()
        .text_decoration_color(colors.muted_foreground)
        .child(js_number_string(duration_ms))
        .child(
            div()
                .ml(r(3.))
                .text_size(r(9.))
                .text_color(colors.muted_foreground)
                .child("ms"),
        )
        .flex()
        .items_baseline();
    HoverCard::new("timing-card")
        .anchor(Anchor::TopLeft)
        .open_delay(Duration::from_millis(150))
        .close_delay(Duration::from_millis(100))
        .w(r(320.))
        .p(r(12.))
        .rounded(px(4.))
        .trigger(trigger)
        .content(move |_, _, cx| {
            let colors = theme::colors(cx);
            div()
                .flex()
                .flex_col()
                .font_family(theme::MONO)
                .text_size(r(11.))
                .text_color(colors.foreground)
                .child(
                    div()
                        .mb(r(8.))
                        .text_size(r(9.))
                        .text_color(colors.muted_foreground)
                        .child("TIMING"),
                )
                .when(phases.is_empty(), |this| {
                    this.child(
                        div()
                            .text_color(colors.muted_foreground)
                            .child("This response has no phase timing. Send the request again."),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(r(6.))
                        .children(phases.iter().map(|phase| {
                            let (left, width) = bar(phase.offset_ms, phase.ms, total);
                            let fill = match phase.id {
                                TimingPhaseId::Wait => colors.primary,
                                TimingPhaseId::Download => colors.success,
                                _ => colors.info,
                            };
                            div()
                                .flex()
                                .items_center()
                                .gap(r(8.))
                                .child(
                                    div()
                                        .w(r(120.))
                                        .flex_none()
                                        .truncate()
                                        .text_color(colors.muted_foreground)
                                        .child(phase.label),
                                )
                                .child(
                                    div()
                                        .relative()
                                        .flex_1()
                                        .h(r(8.))
                                        .rounded(px(2.))
                                        .bg(colors.muted)
                                        .child(
                                            div()
                                                .absolute()
                                                .top_0()
                                                .bottom_0()
                                                .left(relative(left))
                                                .w(relative(width))
                                                .min_w(px(1.))
                                                .rounded(px(2.))
                                                .bg(fill),
                                        ),
                                )
                                .child(
                                    div()
                                        .w(r(64.))
                                        .flex_none()
                                        .text_right()
                                        .child(format_ms(phase.ms)),
                                )
                        })),
                )
                .child(
                    div()
                        .mt(r(8.))
                        .pt(r(8.))
                        .border_t_1()
                        .border_color(colors.border)
                        .flex()
                        .justify_between()
                        .child(div().text_color(colors.muted_foreground).child("Total"))
                        .child(format_ms(duration_ms)),
                )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` exports its own `test` attribute; use the standard one.
    use core::prelude::v1::test;

    #[test]
    fn places_bars_by_share_of_the_total() {
        assert_eq!(bar(25.0, 50.0, 100.0), (0.25, 0.5));
    }
}
