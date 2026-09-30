//! Virtual rows for long read-only views. One-line rows use a uniform list
//! (fast for a 16 MiB body, with horizontal scrolling); wrapped rows have
//! different heights, so they use a measured list.

use std::rc::Rc;

use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::*;

pub type RowRenderer = Rc<dyn Fn(usize, &mut Window, &mut App) -> AnyElement>;

pub struct Rows {
    uniform: UniformListScrollHandle,
    list: ListState,
    count: usize,
}

impl Rows {
    pub fn new() -> Self {
        Rows {
            uniform: UniformListScrollHandle::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            count: 0,
        }
    }

    /// Match the row count. The measured list forgets its heights.
    pub fn reset(&mut self, count: usize) {
        self.count = count;
        self.list.reset(count);
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
