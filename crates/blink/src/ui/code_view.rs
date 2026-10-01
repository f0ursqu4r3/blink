//! Read-only highlighted code with find and filter. Port of `CodeView.vue`.
//!
//! Rows are virtual, so a 16 MiB body stays responsive: only visible lines
//! are highlighted and laid out. The pointer selects text over the drawn
//! rows; Select All selects every shown line.

pub mod rows;
pub mod selection;
pub mod syntax;

use std::cell::RefCell;
use std::cmp::Ordering;
use std::ops::Range;
use std::rc::Rc;

use blink_core::find::{Finder, find_needle};
use blink_core::response_content::ResponseLanguage;
use gpui_kit::component::input;
use gpui_kit::*;

use crate::actions::CODE_VIEW_CONTEXT;
use crate::theme;
use crate::ui::response_panel::r;
use rows::Rows;
use selection::{Selection, Spot, word_at};

/// `h-5.75`: 13 px text at 1.75 line height.
pub const LINE_HEIGHT: f32 = 23.0;

/// What the view shows. The response panel owns these values.
#[derive(Clone, PartialEq)]
pub struct CodeContent {
    pub text: SharedString,
    pub language: ResponseLanguage,
    /// Hide lines without this text.
    pub filter: String,
    /// Highlight this text without hiding lines.
    pub find: String,
    pub wrap: bool,
}

struct Line {
    range: Range<usize>,
    /// 1-based line number in the whole text.
    number: usize,
}

/// The text layout of each row drawn in the last frame, to find the text
/// under the pointer.
type Drawn = Rc<RefCell<Vec<(usize, TextLayout)>>>;

/// A snapshot for the row renderer.
struct Snapshot {
    text: SharedString,
    language: ResponseLanguage,
    lines: Rc<Vec<Line>>,
    finder: Rc<Finder>,
    wrap: bool,
    selection: Option<Selection>,
    drawn: Drawn,
}

pub struct CodeView {
    focus_handle: FocusHandle,
    content: CodeContent,
    lines: Rc<Vec<Line>>,
    widest: usize,
    finder: Rc<Finder>,
    rows: Rows,
    selection: Option<Selection>,
    /// True while the pointer drags a selection.
    selecting: bool,
    drawn: Drawn,
}

impl CodeView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        CodeView {
            focus_handle: cx.focus_handle(),
            content: CodeContent {
                text: SharedString::default(),
                language: ResponseLanguage::Plaintext,
                filter: String::new(),
                find: String::new(),
                wrap: false,
            },
            lines: Rc::new(Vec::new()),
            widest: 0,
            finder: Rc::new(Finder::new()),
            rows: Rows::new(LINE_HEIGHT),
            selection: None,
            selecting: false,
            drawn: Drawn::default(),
        }
    }

    /// Show `content`. Returns true when the text changed, so the owner can
    /// restore the saved scroll position.
    pub fn set_content(&mut self, content: CodeContent, cx: &mut Context<Self>) -> bool {
        if content == self.content {
            return false;
        }
        let text_changed = content.text != self.content.text;
        let filter_changed = find_needle(&content.filter) != find_needle(&self.content.filter);
        let wrap_changed = content.wrap != self.content.wrap;
        let find_changed = content.find != self.content.find;
        self.content = content;
        if text_changed || filter_changed {
            self.rebuild_lines();
            self.selection = None;
            self.selecting = false;
        }
        if text_changed || filter_changed || wrap_changed {
            self.rows.reset(self.lines.len());
        }
        if filter_changed {
            self.rows
                .set_offset(0.0, self.content.wrap, px(LINE_HEIGHT));
        }
        if text_changed || filter_changed || find_changed {
            self.refind();
        }
        cx.notify();
        text_changed
    }

    fn rebuild_lines(&mut self) {
        let text = self.content.text.as_ref();
        let needle = find_needle(&self.content.filter);
        let mut lines = Vec::new();
        let mut start = 0;
        let mut widest = (0, 0);
        for (index, line) in text.split('\n').enumerate() {
            let range = start..start + line.len();
            start = range.end + 1;
            if !needle.is_empty() && !line.to_lowercase().contains(&needle) {
                continue;
            }
            if line.len() > widest.1 {
                widest = (lines.len(), line.len());
            }
            lines.push(Line {
                range,
                number: index + 1,
            });
        }
        self.widest = widest.0;
        self.lines = Rc::new(lines);
    }

    fn refind(&mut self) {
        let text = self.content.text.as_ref();
        let texts: Vec<&str> = self
            .lines
            .iter()
            .map(|line| &text[line.range.clone()])
            .collect();
        let mut finder = (*self.finder).clone();
        let target = finder.update(&texts, &self.content.find);
        self.finder = Rc::new(finder);
        if let Some(row) = target {
            self.rows.reveal(row, self.content.wrap);
        }
    }

    pub fn finder(&self) -> &Finder {
        &self.finder
    }

    /// Move to the next (`1`) or previous (`-1`) match.
    pub fn find_step(&mut self, direction: isize, cx: &mut Context<Self>) {
        let mut finder = (*self.finder).clone();
        let target = finder.step(direction);
        self.finder = Rc::new(finder);
        if let Some(row) = target {
            self.rows.reveal(row, self.content.wrap);
        }
        cx.notify();
    }

    fn row_text(&self, row: usize) -> &str {
        self.lines
            .get(row)
            .map_or("", |line| &self.content.text[line.range.clone()])
    }

    /// The text place under `position`, a window position. Above or below
    /// the drawn rows, the start or end of the nearest one.
    fn spot_at(&self, position: Point<Pixels>) -> Option<Spot> {
        let drawn = self.drawn.borrow();
        let distance = |layout: &TextLayout| {
            let bounds = layout.bounds();
            if position.y < bounds.top() {
                bounds.top() - position.y
            } else {
                (position.y - bounds.bottom()).max(Pixels::ZERO)
            }
        };
        let (row, layout) = drawn.iter().min_by(|(_, a), (_, b)| {
            distance(a)
                .partial_cmp(&distance(b))
                .unwrap_or(Ordering::Equal)
        })?;
        // A row is one line of text, maybe wrapped. The nearest character
        // boundary, with the pointer moved into the row.
        let bounds = layout.bounds();
        let inside = point(
            position.x,
            position.y.clamp(bounds.top(), bounds.bottom() - px(1.)),
        );
        let offset = layout
            .line_layout_for_index(0)?
            .closest_index_for_position(inside - bounds.origin, layout.line_height())
            .unwrap_or_else(|closest| closest)
            .min(self.row_text(*row).len());
        Some(Spot::new(*row, offset))
    }

    /// Start a selection: a click places it, a double click selects a word,
    /// a triple click selects the line, and Shift extends the selection.
    fn select_from(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        let Some(spot) = self.spot_at(event.position) else {
            return;
        };
        let row = |range: Range<usize>| Selection {
            anchor: Spot::new(spot.row, range.start),
            head: Spot::new(spot.row, range.end),
        };
        self.selection = Some(match event.click_count {
            1 if event.modifiers.shift => Selection {
                anchor: self.selection.map_or(spot, |selection| selection.anchor),
                head: spot,
            },
            1 => Selection::at(spot),
            2 => row(word_at(self.row_text(spot.row), spot.offset)),
            _ => row(0..self.row_text(spot.row).len()),
        });
        self.selecting = event.click_count == 1;
        cx.notify();
    }

    fn extend_selection(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(head) = self.spot_at(position) else {
            return;
        };
        if let Some(selection) = &mut self.selection
            && selection.head != head
        {
            selection.head = head;
            cx.notify();
        }
    }

    fn end_selection(&mut self, cx: &mut Context<Self>) {
        self.selecting = false;
        cx.notify();
    }

    /// The text shown, for the UI tests.
    #[cfg(test)]
    pub fn text(&self) -> &SharedString {
        &self.content.text
    }

    /// The window position of `offset` in drawn row `row`, for the UI tests.
    #[cfg(test)]
    pub fn position_of(&self, row: usize, offset: usize) -> Option<Point<Pixels>> {
        let drawn = self.drawn.borrow();
        let (_, layout) = drawn.iter().find(|(drawn, _)| *drawn == row)?;
        let line = layout.line_height();
        layout
            .position_for_index(offset)
            .map(|position| point(position.x, position.y + line / 2.))
    }

    pub fn selected_text(&self) -> Option<String> {
        let selection = self.selection.filter(|selection| !selection.is_empty())?;
        Some(selection.text(|row| self.row_text(row)))
    }

    fn copy(&mut self, _: &input::Copy, _: &mut Window, cx: &mut Context<Self>) {
        match self.selected_text() {
            Some(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            None => cx.propagate(),
        }
    }

    fn select_all(&mut self, _: &input::SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let Some(last) = self.lines.len().checked_sub(1) else {
            return;
        };
        self.selection = Some(Selection {
            anchor: Spot::new(0, 0),
            head: Spot::new(last, self.row_text(last).len()),
        });
        cx.notify();
    }

    /// An empty element before the rows. It forgets the drawn rows before
    /// they draw again, and while the pointer drags a selection it follows
    /// the pointer outside the view too.
    fn pointer_tracker(&self, cx: &Context<Self>) -> impl IntoElement {
        let drawn = self.drawn.clone();
        let selecting = self.selecting;
        let view = cx.entity().downgrade();
        canvas(
            move |_, _, _| drawn.borrow_mut().clear(),
            move |_, _, window, _| {
                if !selecting {
                    return;
                }
                let moved = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    moved
                        .update(cx, |this, cx| {
                            if event.pressed_button == Some(MouseButton::Left) {
                                this.extend_selection(event.position, cx);
                            } else {
                                this.end_selection(cx);
                            }
                        })
                        .ok();
                });
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                        view.update(cx, |this, cx| this.end_selection(cx)).ok();
                    }
                });
            },
        )
        .absolute()
        .size_0()
    }

    pub fn scroll_offset(&self) -> f64 {
        self.rows.offset(self.content.wrap, px(LINE_HEIGHT))
    }

    pub fn set_scroll_offset(&mut self, offset: f64) {
        self.rows
            .set_offset(offset, self.content.wrap, px(LINE_HEIGHT));
    }
}

/// Syntax colors and find marks for one row.
pub fn row_highlights(
    text: &str,
    language: ResponseLanguage,
    finder: &Finder,
    row: usize,
    cx: &App,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let syntax = syntax::highlight_line(text, language)
        .into_iter()
        .map(|(range, role)| {
            (
                range,
                HighlightStyle {
                    color: Some(theme::syntax_color(role, cx)),
                    ..Default::default()
                },
            )
        })
        .collect::<Vec<_>>();
    let marks = find_marks(text, finder, row, cx);
    if marks.is_empty() {
        return syntax;
    }
    combine_highlights(syntax, marks).collect()
}

/// Find marks only, for rows without syntax colors.
pub fn find_marks(
    text: &str,
    finder: &Finder,
    row: usize,
    cx: &App,
) -> Vec<(Range<usize>, HighlightStyle)> {
    if finder.needle().is_empty() {
        return vec![];
    }
    let colors = theme::colors(cx);
    finder
        .highlights(row, text)
        .into_iter()
        .map(|(range, current)| {
            (
                range,
                if current {
                    HighlightStyle {
                        background_color: Some(colors.find_current),
                        color: Some(colors.find_current_foreground),
                        ..Default::default()
                    }
                } else {
                    HighlightStyle {
                        background_color: Some(colors.find_match),
                        ..Default::default()
                    }
                },
            )
        })
        .collect()
}

fn render_line(snapshot: &Snapshot, ix: usize, cx: &App) -> AnyElement {
    let colors = theme::colors(cx);
    let Some(line) = snapshot.lines.get(ix) else {
        return div().into_any_element();
    };
    let text = &snapshot.text[line.range.clone()];
    let mut highlights = row_highlights(text, snapshot.language, &snapshot.finder, ix, cx);
    if let Some(range) = snapshot
        .selection
        .and_then(|selection| selection.row_range(ix, text.len()))
    {
        let selected = HighlightStyle {
            background_color: Some(colors.selection),
            ..Default::default()
        };
        highlights = combine_highlights(highlights, [(range, selected)]).collect();
    }
    let source = StyledText::new(SharedString::from(text.to_string())).with_highlights(highlights);
    // After the text lays out, keep its layout for the pointer.
    let layout = source.layout().clone();
    let drawn = snapshot.drawn.clone();
    let recorder = canvas(
        move |_, _, _| drawn.borrow_mut().push((ix, layout)),
        |_, _, _, _| {},
    )
    .absolute()
    .size_0();
    if snapshot.wrap {
        div()
            .id(("code-line", ix))
            .w_full()
            .min_h(r(LINE_HEIGHT))
            .px(r(16.))
            .whitespace_normal()
            .child(source)
            .child(recorder)
            .into_any_element()
    } else {
        div()
            .id(("code-line", ix))
            .flex()
            .h(r(LINE_HEIGHT))
            .min_w_full()
            .whitespace_nowrap()
            .child(
                div()
                    .flex_none()
                    .w(r(54.))
                    .pr(r(12.))
                    .border_r_1()
                    .border_color(colors.border)
                    .text_color(colors.muted_foreground)
                    .opacity(0.7)
                    .text_right()
                    .child(SharedString::from(line.number.to_string())),
            )
            .child(div().px(r(16.)).child(source).child(recorder))
            .into_any_element()
    }
}

impl Render for CodeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        if self.lines.is_empty() {
            return div()
                .flex_1()
                .min_h_0()
                .py(r(16.))
                .child(
                    div()
                        .px(r(16.))
                        .font_family(theme::SANS)
                        .text_size(r(12.))
                        .text_color(colors.muted_foreground)
                        .child("No response lines match this filter."),
                )
                .into_any_element();
        }
        let snapshot = Rc::new(Snapshot {
            text: self.content.text.clone(),
            language: self.content.language,
            lines: self.lines.clone(),
            finder: self.finder.clone(),
            wrap: self.content.wrap,
            selection: self.selection,
            drawn: self.drawn.clone(),
        });
        let row: rows::RowRenderer = Rc::new(move |ix, _, cx| render_line(&snapshot, ix, cx));
        div()
            .id("code-view")
            .track_focus(&self.focus_handle)
            .key_context(CODE_VIEW_CONTEXT)
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::select_all))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, window, cx| this.select_from(event, window, cx)),
            )
            .cursor_text()
            .relative()
            .flex_1()
            .min_h_0()
            .size_full()
            .font_family(theme::MONO)
            .text_size(r(13.))
            .line_height(r(LINE_HEIGHT))
            .text_color(colors.foreground)
            .child(self.pointer_tracker(cx))
            .child(
                self.rows
                    .render("response-body", self.content.wrap, self.widest, row, r(16.)),
            )
            .into_any_element()
    }
}
