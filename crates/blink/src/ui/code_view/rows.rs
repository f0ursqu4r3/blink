//! Virtual rows for long read-only views. One-line rows use a uniform list
//! (fast for a 16 MiB body, with horizontal scrolling); wrapped rows have
//! different heights, so they use a measured list.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::*;

use crate::ui::response_panel::r;

pub type RowRenderer = Rc<dyn Fn(usize, &mut Window, &mut App) -> AnyElement>;

pub struct Rows {
    uniform: UniformListScrollHandle,
    list: ListState,
    count: usize,
    /// One-line row height in CSS pixels at zoom 1.
    row_height: f32,
    estimate: HeightEstimate,
}

impl Rows {
    pub fn new(row_height: f32) -> Self {
        Rows {
            uniform: UniformListScrollHandle::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            count: 0,
            row_height,
            estimate: HeightEstimate::default(),
        }
    }

    /// Match the row count. The measured list forgets its heights.
    pub fn reset(&mut self, count: usize) {
        self.count = count;
        self.list.reset(count);
        self.estimate.invalidate();
    }

    /// Scroll so `row` shows, centered when it was out of view.
    pub fn reveal(&self, row: usize, wrap: bool) {
        if wrap {
            self.list.scroll_to_reveal_item(row);
        } else {
            self.uniform.scroll_to_item(row, ScrollStrategy::Center);
        }
    }

    /// Scroll offset from the top in pixels.
    pub fn offset(&self, wrap: bool, row_height: Pixels) -> f64 {
        if wrap {
            let top = self.list.logical_scroll_top();
            (row_height * top.item_ix as f32 + top.offset_in_item).as_f32() as f64
        } else {
            let offset = self.uniform.0.borrow().base_handle.offset();
            (-offset.y.as_f32()) as f64
        }
    }

    pub fn set_offset(&self, offset: f64, wrap: bool, row_height: Pixels) {
        let offset = px(offset.max(0.0) as f32);
        if wrap {
            let height = row_height.as_f32().max(1.0);
            let item_ix = (offset.as_f32() / height).floor() as usize;
            self.list.scroll_to(ListOffset {
                item_ix: item_ix.min(self.count),
                offset_in_item: offset - row_height * item_ix as f32,
            });
        } else {
            let handle = self.uniform.0.borrow().base_handle.clone();
            let x = handle.offset().x;
            handle.set_offset(point(x, -offset));
        }
    }

    /// The rows as a scroll area filling its parent. `widest` is the row
    /// that sets the width of one-line rows.
    pub fn render(
        &self,
        id: impl Into<ElementId>,
        wrap: bool,
        widest: usize,
        row: RowRenderer,
        padding_y: Rems,
    ) -> AnyElement {
        if wrap {
            let list = list(self.list.clone(), move |ix, window, cx| row(ix, window, cx))
                .size_full()
                .py(padding_y);
            div()
                .relative()
                .size_full()
                .child(list)
                .child(self.estimate.element(&self.list, self.row_height))
                .vertical_scrollbar(&self.list)
                .into_any_element()
        } else {
            let list = uniform_list(id, self.count, move |range, window, cx| {
                range.map(|ix| row(ix, window, cx)).collect::<Vec<_>>()
            })
            .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
            .with_width_from_item(Some(widest))
            .track_scroll(&self.uniform)
            .size_full()
            .py(padding_y);
            div()
                .relative()
                .size_full()
                .child(list)
                .scrollbar(&self.uniform, ScrollbarAxis::Both)
                .into_any_element()
        }
    }
}

/// Height estimates for the rows a measured list has not drawn.
///
/// The list counts those rows as 0 px high, so its scrollbar would end at
/// the last drawn row. This gives each of them one line. The list forgets
/// the estimates when its width changes, and new rows have none, so the
/// check runs after the list lays out, and draws again when it set them.
#[derive(Default)]
pub struct HeightEstimate {
    /// The list width, row height, and row count the estimates are for.
    done: Rc<Cell<Option<(Pixels, Pixels, usize)>>>,
}

impl HeightEstimate {
    /// Estimate again on the next draw, as after `ListState::reset`.
    pub fn invalidate(&self) {
        self.done.set(None);
    }

    /// An empty element to put after the list, in the same parent.
    /// `row_height` is one row in CSS pixels at zoom 1.
    pub fn element(&self, list: &ListState, row_height: f32) -> impl IntoElement + use<> {
        let list = list.clone();
        let done = self.done.clone();
        let row_height = r(row_height);
        canvas(
            move |_, window, _| {
                let width = list.viewport_bounds().size.width;
                let height = row_height.to_pixels(window.rem_size());
                let key = Some((width, height, list.item_count()));
                if done.get() != key {
                    done.set(key);
                    list.with_uniform_item_height(height);
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0()
    }
}

#[cfg(test)]
mod ui_tests {
    use std::rc::Rc;

    // `gpui_kit::*` exports its own `test` attribute; use the standard one.
    use core::prelude::v1::test;
    use gpui_kit::*;

    use super::{HeightEstimate, RowRenderer, Rows};

    const COUNT: usize = 1_000;
    const HEIGHT: f32 = 20.;

    struct View {
        rows: Rows,
        width: Pixels,
    }

    impl Render for View {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let row: RowRenderer =
                Rc::new(|_, _, _| div().h(px(HEIGHT)).child("row").into_any_element());
            div()
                .w(self.width)
                .h(px(400.))
                .child(self.rows.render("rows", true, 0, row, rems(0.)))
        }
    }

    /// The scroll range the scrollbar sees, minus the range of all rows.
    fn scroll_range_error(view: &Entity<View>, cx: &mut VisualTestContext) -> f32 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let state = &view.read(cx).rows.list;
            let viewport = state.viewport_bounds().size.height;
            let max = state.max_offset_for_scrollbar().y;
            (max - (px(HEIGHT * COUNT as f32) - viewport)).as_f32()
        })
    }

    fn view() -> View {
        let mut rows = Rows::new(HEIGHT);
        rows.reset(COUNT);
        View {
            rows,
            width: px(600.),
        }
    }

    #[gpui_kit::test]
    fn wrapped_rows_scroll_over_rows_not_yet_drawn(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| view());
        assert_eq!(scroll_range_error(&view, cx), 0.);

        // A new width remeasures the rows; the range stays whole.
        view.update(cx, |view, cx| {
            view.width = px(500.);
            cx.notify();
        });
        assert_eq!(scroll_range_error(&view, cx), 0.);

        // So does a new row count.
        view.update(cx, |view, cx| {
            view.rows.reset(COUNT);
            cx.notify();
        });
        assert_eq!(scroll_range_error(&view, cx), 0.);
    }

    /// A live log: a measured list that grows at the end.
    struct Log {
        list: ListState,
        estimate: HeightEstimate,
    }

    impl Render for Log {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let log = list(self.list.clone(), |_, _, _| {
                div().h(px(HEIGHT)).child("message").into_any_element()
            })
            .size_full();
            div()
                .relative()
                .w(px(600.))
                .h(px(400.))
                .child(log)
                .child(self.estimate.element(&self.list, HEIGHT))
        }
    }

    fn log() -> Log {
        Log {
            list: ListState::new(COUNT, ListAlignment::Top, px(400.)),
            estimate: HeightEstimate::default(),
        }
    }

    fn log_range_error(view: &Entity<Log>, count: usize, cx: &mut VisualTestContext) -> f32 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let state = &view.read(cx).list;
            let viewport = state.viewport_bounds().size.height;
            let max = state.max_offset_for_scrollbar().y;
            (max - (px(HEIGHT * count as f32) - viewport)).as_f32()
        })
    }

    #[gpui_kit::test]
    fn a_growing_log_scrolls_over_rows_not_yet_drawn(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| log());
        assert_eq!(log_range_error(&view, COUNT, cx), 0.);

        // New messages at the end.
        view.update(cx, |view, cx| {
            view.list.splice(COUNT..COUNT, COUNT);
            cx.notify();
        });
        assert_eq!(log_range_error(&view, 2 * COUNT, cx), 0.);

        // A new log.
        view.update(cx, |view, cx| {
            view.list.reset(COUNT);
            view.estimate.invalidate();
            cx.notify();
        });
        assert_eq!(log_range_error(&view, COUNT, cx), 0.);
    }
}
