//! Server-sent events, live or from a saved response. Port of `EventList.vue`.

use std::rc::Rc;

use blink_core::json::canonical_json;
use blink_core::model::SseEvent;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::theme;
use crate::ui::code_view::rows::HeightEstimate;
use crate::ui::response_panel::r;

/// A one-line row: the line, `py-1.5`, and the bottom border.
pub const ROW_HEIGHT: f32 = 19.2 + 12. + 1.;

/// Rendered rows. The stream keeps more; the newest show.
pub const RENDER_LIMIT: usize = 1000;

/// JSON data on one line stays readable; other data as sent.
pub fn display(data: &str) -> String {
    canonical_json(data).unwrap_or_else(|| data.to_string())
}

/// `seconds` in `EventList.vue`.
pub fn seconds(ms: Option<f64>) -> String {
    ms.map_or(String::new(), |ms| format!("{:.2} s", ms / 1000.0))
}

struct Row {
    at: SharedString,
    event: SharedString,
    id: Option<SharedString>,
    data: SharedString,
}

/// The list state of one event list. The owner keeps it across renders.
pub struct EventList {
    list: ListState,
    estimate: HeightEstimate,
    /// Events and hidden count at the last render.
    shown: (usize, usize),
    live: bool,
}

impl EventList {
    pub fn new() -> Self {
        EventList {
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            estimate: HeightEstimate::default(),
            shown: (0, 0),
            live: false,
        }
    }

    /// Rows in the list after the last render, for the UI tests.
    #[cfg(test)]
    pub fn rows(&self) -> usize {
        self.list.item_count()
    }

    pub fn render(&mut self, events: &[SseEvent], live: bool, cx: &App) -> AnyElement {
        let colors = theme::colors(cx);
        let hidden = events.len().saturating_sub(RENDER_LIMIT);
        let shown = &events[hidden..];
        if live != self.live {
            self.live = live;
            // Follow new events while the view is at the bottom.
            self.list.set_follow_mode(if live {
                FollowMode::Tail
            } else {
                FollowMode::Normal
            });
        }
        let (count, was_hidden) = self.shown;
        if hidden == was_hidden && shown.len() > count {
            self.list.splice(count..count, shown.len() - count);
        } else if (shown.len(), hidden) != self.shown {
            let following = self.list.is_following_tail();
            self.list.reset(shown.len());
            self.estimate.invalidate();
            if following {
                self.list.set_follow_mode(FollowMode::Tail);
            }
        }
        self.shown = (shown.len(), hidden);

        let empty = if events.is_empty() {
            Some(if live {
                "Connected. Waiting for events…"
            } else {
                "No events in this response."
            })
        } else {
            None
        };
        let rows: Rc<Vec<Row>> = Rc::new(
            shown
                .iter()
                .enumerate()
                .map(|(index, event)| {
                    let at = seconds(event.at);
                    Row {
                        at: if at.is_empty() {
                            format!("#{}", hidden + index + 1).into()
                        } else {
                            at.into()
                        },
                        event: event.event.clone().into(),
                        id: event.id.clone().map(Into::into),
                        data: display(&event.data).into(),
                    }
                })
                .collect(),
        );
        let list = list(self.list.clone(), move |ix, _, cx| {
            let colors = theme::colors(cx);
            let row = &rows[ix];
            let tip = row.id.clone().map(|id| format!("id {id}"));
            div()
                .flex()
                .items_start()
                .gap(r(12.))
                .border_b_1()
                .border_color(colors.border)
                .px(r(16.))
                .py(r(6.))
                .line_height(r(19.2))
                .child(
                    div()
                        .flex_none()
                        .w(r(72.))
                        .text_color(colors.muted_foreground)
                        .child(row.at.clone()),
                )
                .child(
                    div()
                        .id(("event-name", ix))
                        .flex_none()
                        .w(r(112.))
                        .truncate()
                        .text_color(colors.info)
                        .child(row.event.clone())
                        .when_some(tip, |this, tip| {
                            this.tooltip(move |window, cx| {
                                Tooltip::new(tip.clone()).build(window, cx)
                            })
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_normal()
                        .child(row.data.clone()),
                )
                .into_any_element()
        })
        .size_full();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .font_family(theme::MONO)
            .text_size(r(12.))
            .text_color(colors.foreground)
            .when(hidden > 0, |this| {
                this.child(
                    div()
                        .border_b_1()
                        .border_color(colors.border)
                        .px(r(16.))
                        .py(r(4.))
                        .text_size(r(10.))
                        .text_color(colors.muted_foreground)
                        .child(format!("{hidden} earlier events not shown")),
                )
            })
            .map(|this| match empty {
                Some(text) => this.child(
                    div()
                        .p(r(16.))
                        .font_family(theme::SANS)
                        .text_size(r(12.))
                        .text_color(colors.muted_foreground)
                        .child(text),
                ),
                None => this.child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .child(list)
                        .child(self.estimate.element(&self.list, ROW_HEIGHT))
                        .vertical_scrollbar(&self.list),
                ),
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` exports its own `test` attribute; use the standard one.
    use core::prelude::v1::test;

    #[test]
    fn shows_json_on_one_line_and_other_data_as_sent() {
        assert_eq!(display("{ \"a\": 1 }"), r#"{"a":1}"#);
        assert_eq!(display("plain text"), "plain text");
        assert_eq!(seconds(Some(1234.0)), "1.23 s");
        assert_eq!(seconds(None), "");
    }
}
