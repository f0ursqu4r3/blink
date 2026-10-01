//! The collapsible JSON tree. Port of `JsonTreeView.vue`.

use std::collections::HashSet;
use std::rc::Rc;

use blink_core::find::{Finder, find_needle};
use blink_core::json::{JsonValue, parse};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::Store;
use crate::theme;
use crate::ui::code_view::find_marks;
use crate::ui::code_view::rows::{RowRenderer, Rows};
use crate::ui::response_panel::r;

/// `min-h-6.25`.
pub const ROW_HEIGHT: f32 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Null,
    String,
    Bool,
    Number,
    Container,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsonRow {
    /// Path such as `$/items/0`.
    pub id: SharedString,
    pub key: Option<SharedString>,
    pub depth: usize,
    pub container: bool,
    pub value: SharedString,
    pub kind: ValueKind,
}

impl JsonRow {
    /// The text the row shows: `"key": value`.
    pub fn find_text(&self) -> String {
        match &self.key {
            None => self.value.to_string(),
            Some(key) => format!("\"{key}\": {}", self.value),
        }
    }
}

/// `valueLabel` in `JsonTreeView.vue`.
pub fn value_label(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".into(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(text) => text.clone(),
        JsonValue::String(text) => format!("\"{text}\""),
        JsonValue::Array(items) => format!("[{}]", items.len()),
        JsonValue::Object(entries) => format!("{{{}}}", entries.len()),
    }
}

fn kind(value: &JsonValue) -> ValueKind {
    match value {
        JsonValue::Null => ValueKind::Null,
        JsonValue::Bool(_) => ValueKind::Bool,
        JsonValue::Number(_) => ValueKind::Number,
        JsonValue::String(_) => ValueKind::String,
        JsonValue::Array(_) | JsonValue::Object(_) => ValueKind::Container,
    }
}

fn children(value: &JsonValue) -> Vec<(String, &JsonValue)> {
    match value {
        JsonValue::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, child)| (index.to_string(), child))
            .collect(),
        JsonValue::Object(entries) => entries
            .iter()
            .map(|(key, child)| (key.clone(), child))
            .collect(),
        _ => vec![],
    }
}

/// The visible rows: a filter keeps rows that match or hold a match and
/// opens every container; otherwise collapsed containers hide children.
pub fn tree_rows(root: &JsonValue, filter: &str, collapsed: &HashSet<String>) -> Vec<JsonRow> {
    let query = find_needle(filter);
    add_rows(root, None, "$".into(), 0, &query, collapsed).1
}

fn add_rows(
    value: &JsonValue,
    key: Option<String>,
    id: String,
    depth: usize,
    query: &str,
    collapsed: &HashSet<String>,
) -> (bool, Vec<JsonRow>) {
    let label = value_label(value);
    let container = matches!(value, JsonValue::Array(_) | JsonValue::Object(_));
    let open = container && (!query.is_empty() || !collapsed.contains(&id));
    // Without a filter, collapsed children are not needed at all.
    let child_rows: Vec<(bool, Vec<JsonRow>)> = if container && (open || !query.is_empty()) {
        children(value)
            .into_iter()
            .map(|(child_key, child)| {
                let child_id = format!("{id}/{child_key}");
                add_rows(
                    child,
                    Some(child_key),
                    child_id,
                    depth + 1,
                    query,
                    collapsed,
                )
            })
            .collect()
    } else {
        vec![]
    };
    let matches = query.is_empty()
        || format!("{} {label}", key.as_deref().unwrap_or("root"))
            .to_lowercase()
            .contains(query)
        || child_rows.iter().any(|(matches, _)| *matches);
    if !matches {
        return (false, vec![]);
    }
    let mut rows = vec![JsonRow {
        id: id.into(),
        key: key.map(Into::into),
        depth,
        container,
        value: label.into(),
        kind: kind(value),
    }];
    if open {
        for (_, child) in child_rows {
            rows.extend(child);
        }
    }
    (true, rows)
}

/// Ids of every container below the root.
pub fn container_ids(value: &JsonValue, id: &str, out: &mut HashSet<String>) {
    for (key, child) in children(value) {
        if matches!(child, JsonValue::Array(_) | JsonValue::Object(_)) {
            let child_id = format!("{id}/{key}");
            container_ids(child, &child_id, out);
            out.insert(child_id);
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct TreeContent {
    pub text: SharedString,
    pub filter: String,
    pub find: String,
    pub wrap: bool,
}

pub struct JsonTree {
    store: Entity<Store>,
    content: TreeContent,
    value: Option<Rc<JsonValue>>,
    collapsed: HashSet<String>,
    rows: Rc<Vec<JsonRow>>,
    widest: usize,
    finder: Rc<Finder>,
    list: Rows,
}

struct Snapshot {
    tree: WeakEntity<JsonTree>,
    store: Entity<Store>,
    rows: Rc<Vec<JsonRow>>,
    finder: Rc<Finder>,
    collapsed: Rc<HashSet<String>>,
    wrap: bool,
}

impl JsonTree {
    pub fn new(store: Entity<Store>, _cx: &mut Context<Self>) -> Self {
        JsonTree {
            store,
            content: TreeContent {
                text: SharedString::default(),
                filter: String::new(),
                find: String::new(),
                wrap: false,
            },
            value: None,
            collapsed: HashSet::new(),
            rows: Rc::new(Vec::new()),
            widest: 0,
            finder: Rc::new(Finder::new()),
            list: Rows::new(ROW_HEIGHT),
        }
    }

    /// Show `content`. True when the text changed.
    pub fn set_content(&mut self, content: TreeContent, cx: &mut Context<Self>) -> bool {
        if content == self.content {
            return false;
        }
        let text_changed = content.text != self.content.text;
        let filter_changed = find_needle(&content.filter) != find_needle(&self.content.filter);
        let wrap_changed = content.wrap != self.content.wrap;
        self.content = content;
        if text_changed {
            self.value = parse(&self.content.text).ok().map(Rc::new);
        }
        if text_changed || filter_changed || wrap_changed {
            self.rebuild();
        } else {
            self.refind();
        }
        if filter_changed {
            self.list.set_offset(0.0, self.content.wrap, px(ROW_HEIGHT));
        }
        cx.notify();
        text_changed
    }

    fn rebuild(&mut self) {
        let rows = match &self.value {
            Some(value) => tree_rows(value, &self.content.filter, &self.collapsed),
            None => vec![],
        };
        self.widest = rows
            .iter()
            .enumerate()
            .max_by_key(|(_, row)| {
                row.depth * 3 + row.value.len() + row.key.as_ref().map_or(0, |k| k.len())
            })
            .map_or(0, |(ix, _)| ix);
        self.rows = Rc::new(rows);
        self.list.reset(self.rows.len());
        self.refind();
    }

    fn refind(&mut self) {
        let texts: Vec<String> = self.rows.iter().map(JsonRow::find_text).collect();
        let mut finder = (*self.finder).clone();
        let target = finder.update(&texts, &self.content.find);
        self.finder = Rc::new(finder);
        if let Some(row) = target {
            self.list.reveal(row, self.content.wrap);
        }
    }

    pub fn finder(&self) -> &Finder {
        &self.finder
    }

    pub fn find_step(&mut self, direction: isize, cx: &mut Context<Self>) {
        let mut finder = (*self.finder).clone();
        let target = finder.step(direction);
        self.finder = Rc::new(finder);
        if let Some(row) = target {
            self.list.reveal(row, self.content.wrap);
        }
        cx.notify();
    }

    pub fn scroll_offset(&self) -> f64 {
        self.list.offset(self.content.wrap, px(ROW_HEIGHT))
    }

    pub fn set_scroll_offset(&mut self, offset: f64) {
        self.list
            .set_offset(offset, self.content.wrap, px(ROW_HEIGHT));
    }

    pub fn toggle(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.collapsed.remove(id) {
            self.collapsed.insert(id.to_string());
        }
        self.keep_offset(cx);
    }

    fn expand_all(&mut self, cx: &mut Context<Self>) {
        self.collapsed.clear();
        self.keep_offset(cx);
    }

    fn collapse_all(&mut self, cx: &mut Context<Self>) {
        let mut ids = HashSet::new();
        if let Some(value) = &self.value {
            container_ids(value, "$", &mut ids);
        }
        self.collapsed = ids;
        self.keep_offset(cx);
    }

    /// Rebuild the rows and stay at the same scroll position.
    fn keep_offset(&mut self, cx: &mut Context<Self>) {
        let offset = self.scroll_offset();
        self.rebuild();
        self.set_scroll_offset(offset);
        cx.notify();
    }
}

fn value_color(kind: ValueKind, cx: &App) -> Hsla {
    let colors = theme::colors(cx);
    match kind {
        ValueKind::Null | ValueKind::Bool => colors.info,
        ValueKind::String => colors.success,
        ValueKind::Number => colors.warning,
        ValueKind::Container => colors.muted_foreground,
    }
}

fn render_row(snapshot: &Snapshot, ix: usize, cx: &App) -> AnyElement {
    let colors = theme::colors(cx);
    let Some(row) = snapshot.rows.get(ix).cloned() else {
        return div().into_any_element();
    };
    let is_collapsed = snapshot.collapsed.contains(row.id.as_ref());
    let text = row.find_text();
    // Marks are ranges in the row text; split them between key and value.
    let marks = find_marks(&text, &snapshot.finder, ix, cx);
    let key_text = row.key.as_ref().map(|key| format!("\"{key}\":"));
    let key_len = key_text.as_ref().map_or(0, |key| key.len() + 1);
    let split = |start: usize, end: usize| {
        marks
            .iter()
            .filter_map(|(range, style)| {
                let from = range.start.max(start);
                let to = range.end.min(end);
                (from < to).then(|| (from - start..to - start, *style))
            })
            .collect::<Vec<_>>()
    };
    let key_marks = split(0, key_len.saturating_sub(1));
    let value_marks = split(key_len, text.len());
    let tree = snapshot.tree.clone();
    let toggle_id = row.id.clone();
    let toggle = if row.container {
        div()
            .id(("json-toggle", ix))
            .flex()
            .flex_none()
            .w(r(14.))
            .h(r(20.))
            .items_center()
            .justify_center()
            .text_color(colors.muted_foreground)
            .hover(|this| this.text_color(colors.foreground).bg(colors.accent))
            .cursor_pointer()
            .child(if is_collapsed { "›" } else { "⌄" })
            .on_click(move |_, _, cx| {
                let id = toggle_id.clone();
                tree.update(cx, |tree, cx| tree.toggle(&id, cx)).ok();
            })
            .into_any_element()
    } else {
        div().flex_none().w(r(14.)).h(r(20.)).into_any_element()
    };
    let wrap = snapshot.wrap;
    let value = StyledText::new(row.value.clone()).with_highlights(value_marks);
    let menu_row = row.clone();
    let tree = snapshot.tree.clone();
    let store = snapshot.store.clone();
    let any_collapsed = !snapshot.collapsed.is_empty();
    div()
        .id(("json-row", ix))
        .flex()
        .when(!wrap, |this| {
            this.items_center().h(r(ROW_HEIGHT)).whitespace_nowrap()
        })
        .when(wrap, |this| {
            this.items_start().min_h(r(ROW_HEIGHT)).w_full()
        })
        .min_w_full()
        .gap(r(7.))
        .pl(r((row.depth * 18 + 12) as f32))
        .child(toggle)
        .when_some(key_text, |this, key| {
            this.child(
                div()
                    .text_color(colors.foreground)
                    .flex_none()
                    .child(StyledText::new(key).with_highlights(key_marks)),
            )
        })
        .child(
            div()
                .text_color(value_color(row.kind, cx))
                .when(wrap, |this| this.min_w_0().flex_1())
                .child(value),
        )
        .context_menu(move |menu, _, _| {
            let path = menu_row.id.to_string();
            let value = menu_row.value.to_string();
            let copy_store = store.clone();
            let value_store = store.clone();
            let mut menu = menu
                .item(PopupMenuItem::new("Copy path").on_click(move |_, _, cx| {
                    copy_store.update(cx, |store, cx| store.copy(path.clone(), cx));
                }))
                .item(PopupMenuItem::new("Copy value").on_click(move |_, _, cx| {
                    value_store.update(cx, |store, cx| store.copy(value.clone(), cx));
                }))
                .separator();
            if menu_row.container {
                let tree = tree.clone();
                let id = menu_row.id.clone();
                menu = menu.item(
                    PopupMenuItem::new(if is_collapsed { "Expand" } else { "Collapse" }).on_click(
                        move |_, _, cx| {
                            tree.update(cx, |tree, cx| tree.toggle(&id, cx)).ok();
                        },
                    ),
                );
            }
            let expand = tree.clone();
            let collapse = tree.clone();
            menu.item(
                PopupMenuItem::new("Expand all")
                    .disabled(!any_collapsed)
                    .on_click(move |_, _, cx| {
                        expand.update(cx, |tree, cx| tree.expand_all(cx)).ok();
                    }),
            )
            .item(
                PopupMenuItem::new("Collapse all").on_click(move |_, _, cx| {
                    collapse.update(cx, |tree, cx| tree.collapse_all(cx)).ok();
                }),
            )
        })
        .into_any_element()
}

impl Render for JsonTree {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        if self.rows.is_empty() {
            return div()
                .flex_1()
                .min_h_0()
                .p(r(16.))
                .font_family(theme::SANS)
                .text_size(r(12.))
                .text_color(colors.muted_foreground)
                .child("No JSON values match this filter.")
                .into_any_element();
        }
        let snapshot = Rc::new(Snapshot {
            tree: cx.entity().downgrade(),
            store: self.store.clone(),
            rows: self.rows.clone(),
            finder: self.finder.clone(),
            collapsed: Rc::new(self.collapsed.clone()),
            wrap: self.content.wrap,
        });
        let row: RowRenderer = Rc::new(move |ix, _, cx| render_row(&snapshot, ix, cx));
        div()
            .flex_1()
            .min_h_0()
            .size_full()
            .font_family(theme::MONO)
            .text_size(r(13.))
            .line_height(r(22.75))
            .text_color(colors.foreground)
            .child(
                self.list
                    .render("json-tree", self.content.wrap, self.widest, row, r(0.)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` exports its own `test` attribute; use the standard one.
    use core::prelude::v1::test;

    fn labels(rows: &[JsonRow]) -> Vec<String> {
        rows.iter().map(JsonRow::find_text).collect()
    }

    #[test]
    fn flattens_and_collapses_the_tree() {
        let value = parse(r#"{"a":{"b":1},"c":[true,null,"x"]}"#).unwrap();
        let rows = tree_rows(&value, "", &HashSet::new());
        assert_eq!(
            labels(&rows),
            [
                "{2}",
                "\"a\": {1}",
                "\"b\": 1",
                "\"c\": [3]",
                "\"0\": true",
                "\"1\": null",
                "\"2\": \"x\""
            ]
        );
        assert_eq!(rows[2].id.as_ref(), "$/a/b");
        assert_eq!(rows[2].depth, 2);
        let collapsed = HashSet::from(["$/c".to_string()]);
        assert_eq!(tree_rows(&value, "", &collapsed).len(), 4);
    }

    #[test]
    fn filters_rows_and_keeps_their_parents() {
        let value = parse(r#"{"a":{"needle":1},"c":[2]}"#).unwrap();
        let collapsed = HashSet::from(["$/a".to_string()]);
        let rows = tree_rows(&value, " NEEDLE ", &collapsed);
        assert_eq!(labels(&rows), ["{2}", "\"a\": {1}", "\"needle\": 1"]);
    }

    #[test]
    fn lists_every_container_below_the_root() {
        let value = parse(r#"{"a":{"b":[1]},"c":2}"#).unwrap();
        let mut ids = HashSet::new();
        container_ids(&value, "$", &mut ids);
        assert_eq!(ids, HashSet::from(["$/a".to_string(), "$/a/b".to_string()]));
    }
}
