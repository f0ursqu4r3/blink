//! Read-only highlighted code with find and filter. Port of `CodeView.vue`.
//!
//! Rows are virtual, so a 16 MiB body stays responsive: only visible lines
//! are highlighted and laid out.

pub mod rows;
pub mod syntax;

use std::ops::Range;
use std::rc::Rc;

use blink_core::find::{Finder, find_needle};
use blink_core::response_content::ResponseLanguage;
use gpui_kit::*;

use crate::theme;
use crate::ui::response_panel::r;
use rows::Rows;

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

/// A snapshot for the row renderer.
struct Snapshot {
    text: SharedString,
    language: ResponseLanguage,
    lines: Rc<Vec<Line>>,
    finder: Rc<Finder>,
    wrap: bool,
}

pub struct CodeView {
    content: CodeContent,
    lines: Rc<Vec<Line>>,
    widest: usize,
    finder: Rc<Finder>,
    rows: Rows,
}

impl CodeView {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        CodeView {
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
            rows: Rows::new(),
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
    let highlights = row_highlights(text, snapshot.language, &snapshot.finder, ix, cx);
    let source = StyledText::new(SharedString::from(text.to_string())).with_highlights(highlights);
    if snapshot.wrap {
        div()
            .id(("code-line", ix))
            .w_full()
            .min_h(r(LINE_HEIGHT))
            .px(r(16.))
            .whitespace_normal()
            .child(source)
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
            .child(div().px(r(16.)).child(source))
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
        });
        let row: rows::RowRenderer = Rc::new(move |ix, _, cx| render_line(&snapshot, ix, cx));
        div()
            .flex_1()
            .min_h_0()
            .size_full()
            .font_family(theme::MONO)
            .text_size(r(13.))
            .line_height(r(LINE_HEIGHT))
            .text_color(colors.foreground)
            .child(
                self.rows
                    .render("response-body", self.content.wrap, self.widest, row, r(16.)),
            )
            .into_any_element()
    }
}
