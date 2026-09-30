//! Past sends of a request. Port of `HistoryView.vue`.

use blink_core::diff::{DIFF_EDIT_LIMIT, DiffHunk, DiffKind, DiffLine, diff_lines, fold_diff};
use blink_core::history::{clock_time, diff_text, header_lines, now_ms, relative_time};
use blink_core::model::HistoryEntry;
use blink_core::request::format_bytes;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::Store;
use crate::theme;
use crate::ui::response_panel::r;

/// Unchanged lines kept around each change, as the TS default.
const DIFF_CONTEXT: usize = 3;

pub struct DiffSection {
    pub title: &'static str,
    pub changed: bool,
    /// None: too many changes to compare.
    pub hunks: Option<Vec<DiffHunk>>,
}

/// The Headers and Body sections of a comparison.
pub fn diff_sections(before: &HistoryEntry, after: &HistoryEntry) -> Vec<DiffSection> {
    [
        (
            "Headers",
            diff_lines(&header_lines(before), &header_lines(after), DIFF_EDIT_LIMIT),
        ),
        (
            "Body",
            diff_lines(&diff_text(before), &diff_text(after), DIFF_EDIT_LIMIT),
        ),
    ]
    .into_iter()
    .map(|(title, lines)| DiffSection {
        title,
        changed: lines
            .as_ref()
            .is_none_or(|lines| lines.iter().any(|line| line.kind != DiffKind::Same)),
        hunks: lines.map(|lines| fold_diff(&lines, DIFF_CONTEXT)),
    })
    .collect()
}

/// The older entry of the comparison `open` starts: the send before it, or
/// for the oldest, itself against the latest.
pub fn open_pair(history: &[HistoryEntry], index: usize) -> Option<(u64, u64)> {
    let entry = history.get(index)?;
    match history.get(index + 1) {
        Some(previous) => Some((previous.id, entry.id)),
        None if index != 0 => Some((entry.id, history[0].id)),
        None => None,
    }
}

pub fn status_label(entry: &HistoryEntry) -> String {
    if entry.error.is_some() {
        "ERR".into()
    } else {
        entry
            .status
            .map_or("undefined".into(), |status| status.to_string())
    }
}

fn tone(entry: &HistoryEntry, cx: &App) -> Hsla {
    let colors = theme::colors(cx);
    let status = entry.status.unwrap_or(0);
    if entry.error.is_some() || status >= 400 {
        colors.destructive
    } else if status >= 300 {
        colors.warning
    } else {
        colors.success
    }
}

/// The path and query of a URL; the URL itself when it does not parse.
pub fn url_path(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let rest = &url[scheme_end + 3..];
    match rest.find(['/', '?', '#']) {
        None => "/".into(),
        Some(at) => {
            let tail = &rest[at..];
            let tail = tail.split('#').next().unwrap_or("");
            if tail.starts_with('?') {
                format!("/{tail}")
            } else {
                tail.to_string()
            }
        }
    }
}

fn clock(entry: &HistoryEntry) -> String {
    clock_time(entry.sent_at, true)
}

pub struct HistoryView {
    store: Entity<Store>,
    session_id: u64,
    /// Ids of the older and newer entry in the open comparison.
    comparing: Option<(u64, u64)>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl HistoryView {
    pub fn new(
        store: Entity<Store>,
        session_id: u64,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let _subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        HistoryView {
            store,
            session_id,
            comparing: None,
            scroll: ScrollHandle::new(),
            _subscriptions,
        }
    }

    fn compare(&mut self, pair: Option<(u64, u64)>, cx: &mut Context<Self>) {
        if pair.is_some() {
            self.comparing = pair;
            self.scroll.set_offset(point(px(0.), px(0.)));
            cx.notify();
        }
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        let id = self.session_id;
        self.store.update(cx, |store, cx| {
            store.update_workspace(cx, |workspace| {
                if let Some(session) = workspace.session_mut(id) {
                    session.history.clear();
                }
            })
        });
    }

    fn render_comparison(
        &self,
        before: &HistoryEntry,
        after: &HistoryEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = theme::colors(cx);
        let sections = diff_sections(before, after);
        let header = div()
            .flex()
            .flex_none()
            .min_h(r(38.))
            .items_center()
            .gap(r(8.))
            .border_b_1()
            .border_color(colors.border)
            .px(r(8.))
            .font_family(theme::MONO)
            .text_size(r(11.))
            .child(
                Button::new("history-back")
                    .ghost()
                    .small()
                    .w(r(28.))
                    .h(r(28.))
                    .icon(Icon::new(IconName::ArrowLeft).size(r(14.)))
                    .tooltip("Back to history")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.comparing = None;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap(r(4.))
                    .min_w_0()
                    .truncate()
                    .child(
                        div()
                            .text_color(tone(before, cx))
                            .child(status_label(before)),
                    )
                    .child(clock(before))
                    .child(
                        div()
                            .mx(r(4.))
                            .text_color(colors.muted_foreground)
                            .child("→"),
                    )
                    .child(div().text_color(tone(after, cx)).child(status_label(after)))
                    .child(clock(after)),
            )
            .child(
                div()
                    .ml_auto()
                    .flex_none()
                    .text_color(colors.muted_foreground)
                    .child(format!(
                        "{} → {} ms",
                        blink_core::json::js_number_string(before.duration_ms),
                        blink_core::json::js_number_string(after.duration_ms)
                    )),
            );
        let line_row = |line: &DiffLine, cx: &App| {
            let colors = theme::colors(cx);
            let (background, sign_color, sign) = match line.kind {
                DiffKind::Add => (Some(colors.success.opacity(0.14)), colors.success, "+"),
                DiffKind::Remove => (
                    Some(colors.destructive.opacity(0.14)),
                    colors.destructive,
                    "−",
                ),
                DiffKind::Same => (None, colors.foreground, ""),
            };
            let number = |value: Option<usize>| {
                div()
                    .w(r(40.))
                    .flex_none()
                    .pr(r(8.))
                    .text_right()
                    .text_color(colors.muted_foreground)
                    .child(value.map(|n| n.to_string()).unwrap_or_default())
            };
            div()
                .flex()
                .when_some(background, |this, bg| this.bg(bg))
                .child(number(line.before))
                .child(number(line.after))
                .child(
                    div()
                        .w(r(16.))
                        .flex_none()
                        .text_color(sign_color)
                        .child(sign),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .pr(r(16.))
                        .whitespace_normal()
                        .child(line.text.clone()),
                )
        };
        let body = div()
            .id("history-diff")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .pb(r(16.))
            .children(sections.into_iter().map(|section| {
                let title = div()
                    .flex()
                    .items_center()
                    .gap(r(8.))
                    .border_b_1()
                    .border_color(colors.border)
                    .bg(colors.muted)
                    .px(r(16.))
                    .py(r(6.))
                    .font_family(theme::MONO)
                    .text_size(r(10.))
                    .text_color(colors.muted_foreground)
                    .child(section.title.to_uppercase())
                    .when(!section.changed, |this| this.child("· No changes"));
                let content = match &section.hunks {
                    None => Some(
                        div()
                            .px(r(16.))
                            .py(r(8.))
                            .text_size(r(12.))
                            .text_color(colors.muted_foreground)
                            .child("Too many changes to compare.")
                            .into_any_element(),
                    ),
                    Some(hunks) if section.changed => Some(
                        div()
                            .font_family(theme::MONO)
                            .text_size(r(12.))
                            .line_height(r(20.4))
                            .children(hunks.iter().flat_map(|hunk| {
                                match hunk {
                                    DiffHunk::Hidden(hidden) => vec![
                                        div()
                                            .border_y_1()
                                            .border_color(colors.border)
                                            .bg(colors.muted.opacity(0.5))
                                            .px(r(16.))
                                            .text_size(r(10.))
                                            .text_color(colors.muted_foreground)
                                            .child(format!(
                                                "⋯ {hidden} unchanged {}",
                                                if *hidden == 1 { "line" } else { "lines" }
                                            ))
                                            .into_any_element(),
                                    ],
                                    DiffHunk::Lines(lines) => lines
                                        .iter()
                                        .map(|line| line_row(line, cx).into_any_element())
                                        .collect(),
                                }
                            }))
                            .into_any_element(),
                    ),
                    Some(_) => None,
                };
                div().child(title).children(content)
            }));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(header)
            .child(body)
            .into_any_element()
    }

    fn render_list(&self, history: &[HistoryEntry], cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::colors(cx);
        let now = now_ms();
        let count = history.len();
        let header = div()
            .flex()
            .flex_none()
            .min_h(r(38.))
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(colors.border)
            .px(r(16.))
            .font_family(theme::MONO)
            .text_size(r(10.))
            .text_color(colors.muted_foreground)
            .child(format!(
                "{count} {}",
                if count == 1 { "SEND" } else { "SENDS" }
            ))
            .child(
                Button::new("history-clear")
                    .ghost()
                    .small()
                    .w(r(28.))
                    .h(r(28.))
                    .icon(Icon::new(IconName::Trash).size(r(13.)))
                    .tooltip("Clear history")
                    .disabled(history.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
            );
        let latest = history.first().map(|entry| entry.id);
        let entity = cx.entity();
        let rows = history.iter().enumerate().map(|(index, entry)| {
            let title: SharedString = format!(
                "{} {}{}",
                entry.method,
                entry.url,
                entry
                    .error
                    .as_ref()
                    .map_or(String::new(), |error| format!(" · {error}"))
            )
            .into();
            let open = open_pair(history, index);
            let previous = history.get(index + 1).map(|older| (older.id, entry.id));
            let with_latest = latest.filter(|&id| id != entry.id).map(|id| (entry.id, id));
            let group: SharedString = format!("history-row-{}", entry.id).into();
            div()
                .id(("history-entry", entry.id))
                .group(group.clone())
                .flex()
                .min_h(r(36.))
                .cursor_pointer()
                .items_center()
                .gap(r(12.))
                .border_b_1()
                .border_color(colors.border)
                .px(r(16.))
                .font_family(theme::MONO)
                .text_size(r(11.))
                .text_color(colors.foreground)
                .hover(|this| this.bg(colors.muted))
                .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
                .on_click(cx.listener(move |this, _, _, cx| this.compare(open, cx)))
                .child(
                    div()
                        .w(r(32.))
                        .flex_none()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tone(entry, cx))
                        .child(status_label(entry)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .line_height(r(14.3))
                        .child(
                            div()
                                .flex()
                                .gap(r(6.))
                                .min_w_0()
                                .child(
                                    div()
                                        .flex_none()
                                        .text_color(theme::method_color(&entry.method, cx))
                                        .child(entry.method.clone()),
                                )
                                .child(div().min_w_0().truncate().child(url_path(&entry.url))),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(r(9.))
                                .text_color(colors.muted_foreground)
                                .child(format!(
                                    "{} · {}",
                                    clock(entry),
                                    relative_time(entry.sent_at, now)
                                )),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(colors.muted_foreground)
                        .child(format!(
                            "{} ms",
                            blink_core::json::js_number_string(entry.duration_ms)
                        )),
                )
                .child(
                    div()
                        .w(r(64.))
                        .flex_none()
                        .text_right()
                        .text_color(colors.muted_foreground)
                        .child(if entry.error.is_some() {
                            String::new()
                        } else {
                            format_bytes(entry.size_bytes)
                        }),
                )
                .child(match with_latest {
                    Some(pair) => div()
                        .flex_none()
                        .invisible()
                        .group_hover(group.clone(), |style| style.visible())
                        .child(
                            Button::new(("compare-latest", entry.id))
                                .ghost()
                                .small()
                                .w(r(24.))
                                .h(r(24.))
                                .icon(Icon::new(IconName::GitCompare).size(r(13.)))
                                .tooltip("Compare with latest")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.compare(Some(pair), cx)
                                })),
                        )
                        .into_any_element(),
                    None => div().w(r(24.)).flex_none().into_any_element(),
                })
                .context_menu({
                    let entity = entity.clone();
                    let store = self.store.clone();
                    let url = entry.url.clone();
                    let body = entry.body.clone();
                    move |menu, _, _| {
                        let (a, b) = (entity.clone(), entity.clone());
                        let (c, d) = (store.clone(), store.clone());
                        let (url, body) = (url.clone(), body.clone());
                        let has_body = !body.is_empty();
                        menu.item(
                            PopupMenuItem::new("Compare with previous")
                                .disabled(previous.is_none())
                                .on_click(move |_, _, cx| {
                                    a.update(cx, |this, cx| this.compare(previous, cx));
                                }),
                        )
                        .item(
                            PopupMenuItem::new("Compare with latest")
                                .disabled(with_latest.is_none())
                                .on_click(move |_, _, cx| {
                                    b.update(cx, |this, cx| this.compare(with_latest, cx));
                                }),
                        )
                        .separator()
                        .item(PopupMenuItem::new("Copy URL").on_click(move |_, _, cx| {
                            c.update(cx, |store, cx| store.copy(url.clone(), cx));
                        }))
                        .item(
                            PopupMenuItem::new("Copy body")
                                .disabled(!has_body)
                                .on_click(move |_, _, cx| {
                                    d.update(cx, |store, cx| store.copy(body.clone(), cx));
                                }),
                        )
                    }
                })
        });
        let rows: Vec<_> = rows.collect();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(header)
            .when(history.is_empty(), |this| {
                this.child(
                    div()
                        .p(r(16.))
                        .text_size(r(12.))
                        .text_color(colors.muted_foreground)
                        .child("No sends yet. Each send of this request is kept here."),
                )
            })
            .child(
                div()
                    .id("history-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element()
    }
}

impl Render for HistoryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let history = self
            .store
            .read(cx)
            .workspace
            .session(self.session_id)
            .map(|session| session.history.clone())
            .unwrap_or_default();
        let pair = self.comparing.and_then(|(before, after)| {
            let find = |id| history.iter().find(|entry| entry.id == id);
            Some((find(before)?.clone(), find(after)?.clone()))
        });
        let content = match pair {
            Some((before, after)) => self.render_comparison(&before, &after, cx),
            None => self.render_list(&history, cx),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .font_family(theme::SANS)
            .child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` exports its own `test` attribute; use the standard one.
    use core::prelude::v1::test;

    fn entry(id: u64, status: u16, body: &str) -> HistoryEntry {
        HistoryEntry {
            id,
            sent_at: 0.0,
            method: "GET".into(),
            url: "https://api.test/items?page=2".into(),
            status: Some(status),
            status_text: None,
            error: None,
            duration_ms: 10.0,
            size_bytes: 2,
            headers: vec![],
            body: body.into(),
            body_omitted: None,
            timing: None,
        }
    }

    #[test]
    fn opens_a_comparison_with_the_send_before() {
        let history = [entry(3, 200, ""), entry(2, 200, ""), entry(1, 200, "")];
        assert_eq!(open_pair(&history, 0), Some((2, 3)));
        // The oldest compares with the latest.
        assert_eq!(open_pair(&history, 2), Some((1, 3)));
        assert_eq!(open_pair(&history[..1], 0), None);
    }

    #[test]
    fn reports_changed_sections() {
        let sections = diff_sections(&entry(1, 200, "{\"a\":1}"), &entry(2, 200, "{\"a\":2}"));
        assert_eq!(sections[0].title, "Headers");
        assert!(!sections[0].changed);
        assert!(sections[1].changed);
    }

    #[test]
    fn shows_the_path_and_query() {
        assert_eq!(url_path("https://api.test/items?page=2"), "/items?page=2");
        assert_eq!(url_path("https://api.test"), "/");
        assert_eq!(url_path("not a url"), "not a url");
    }

    #[test]
    fn labels_errors() {
        let mut failed = entry(1, 0, "");
        failed.error = Some("timeout".into());
        assert_eq!(status_label(&failed), "ERR");
        assert_eq!(status_label(&entry(2, 404, "")), "404");
    }
}
