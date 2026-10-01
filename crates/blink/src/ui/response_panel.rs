//! The response side of a request. Port of `ResponsePanel.vue`,
//! `JsonTreeView.vue`, `EventList.vue`, and `TimingCard.vue`.

mod event_list;
mod json_tree;
mod timing_card;
#[cfg(test)]
mod ui_tests;

use std::path::PathBuf;
use std::time::Duration;

use blink_core::find::{Finder, filter_headers, find_status};
use blink_core::html::format_html;
use blink_core::jq::run_jq;
use blink_core::json::format_json;
use blink_core::model::{ApiResponse, RequestSession, SseEvent};
use blink_core::preferences::transport_options;
use blink_core::request::format_bytes;
use blink_core::response_body::{BODY_UNAVAILABLE, can_save_response, suggested_file_name};
use blink_core::response_content::{ResponseLanguage, response_language};
use blink_core::shortcut::{IS_MAC, shortcut_label};
use blink_core::sse::{is_event_stream, parse_sse};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable as _, Icon, Selectable as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::Store;
use crate::theme;
use crate::ui::code_view::{CodeContent, CodeView};
use crate::ui::history_view::HistoryView;
use crate::ui::widgets::{dotted, tracked};
use event_list::EventList;
use json_tree::{JsonTree, TreeContent};

/// A size in CSS pixels at zoom 1. It scales with the root rem size, as the
/// webview zoom scaled the Vue layout.
pub(crate) fn r(value: f32) -> Rems {
    rems(value / 16.0)
}

const UNAVAILABLE: &str = "Unavailable for truncated responses";

/// Identity of a response, to notice a new one.
fn response_key(response: Option<&ApiResponse>) -> String {
    match response {
        None => String::new(),
        Some(response) => format!(
            "{}|{}|{}|{:?}|{}|{}|{:?}",
            response.status,
            response.duration_ms,
            response.size_bytes,
            response.body_id,
            response.body.len(),
            response.headers.len(),
            response.truncated,
        ),
    }
}

fn header_value<'a>(response: &'a ApiResponse, name: &str) -> Option<&'a str> {
    response
        .headers
        .iter()
        .find(|header| header.key.eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}

/// `contentType` in `ResponsePanel.vue`.
pub fn content_type(response: &ApiResponse) -> String {
    header_value(response, "content-type")
        .map(|value| value.split(';').next().unwrap_or("").to_string())
        .unwrap_or_else(|| "No Content-Type".into())
}

/// `redirectLabel` in `ResponsePanel.vue`.
pub fn redirect_label(count: Option<u64>) -> String {
    match count {
        None => "redirected".into(),
        Some(1) => "1 redirect".into(),
        Some(count) => format!("{count} redirects"),
    }
}

/// The header status text.
pub fn header_status(session: &RequestSession) -> &'static str {
    if session.busy {
        "RECEIVING"
    } else if !session.error.is_empty() {
        "FAILED"
    } else if session.response.is_some() {
        "RECEIVED"
    } else {
        "STANDBY"
    }
}

/// The Tests tab shows when the request has assertions, results, or capture
/// problems.
pub fn shows_tests(session: &RequestSession) -> bool {
    session
        .draft
        .assertions
        .as_ref()
        .is_some_and(|list| !list.is_empty())
        || session
            .test_results
            .as_ref()
            .is_some_and(|list| !list.is_empty())
        || session
            .capture_errors
            .as_ref()
            .is_some_and(|list| !list.is_empty())
}

/// "Cannot save the file: <reason>."
pub fn save_error_message(reason: &str) -> String {
    format!(
        "Cannot save the file: {}.",
        reason.strip_suffix('.').unwrap_or(reason)
    )
}

/// Where the save dialog opens: `~/Downloads` when it exists, else the
/// home folder, else the root.
fn save_directory(home: Option<std::ffi::OsString>) -> PathBuf {
    let home = home.map(PathBuf::from).filter(|path| path.is_dir());
    home.as_ref()
        .map(|home| home.join("Downloads"))
        .filter(|path| path.is_dir())
        .or(home)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Derived response data, rebuilt when the response or jq output changes.
#[derive(Default)]
struct Cache {
    key: String,
    body: SharedString,
    /// The body formats as JSON.
    source_parsed: bool,
    /// Formatted JSON of the jq output, or of the body.
    parsed: Option<SharedString>,
    /// Formatted HTML of the body.
    markup: Option<SharedString>,
    event_stream: bool,
    events: Vec<SseEvent>,
}

pub struct ResponsePanel {
    store: Entity<Store>,
    session_id: u64,
    history: Entity<HistoryView>,
    history_open: bool,
    inspector_visible: bool,
    /// Hide lines that do not match, instead of highlighting matches.
    filter_lines: bool,
    search: Entity<InputState>,
    jq_input: Entity<InputState>,
    jq_output: Option<SharedString>,
    jq_error: String,
    _jq_task: Option<Task<()>>,
    saving: bool,
    save_error: String,
    code: Entity<CodeView>,
    tree: Entity<JsonTree>,
    events: EventList,
    live_events: EventList,
    cache: Cache,
    was_busy: bool,
    _scroll_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl ResponsePanel {
    pub fn new(
        store: Entity<Store>,
        session_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
        let jq_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("jq query, e.g. .items[]"));
        let code = cx.new(CodeView::new);
        let tree = cx.new(|cx| JsonTree::new(store.clone(), cx));
        let history = cx.new(|cx| HistoryView::new(store.clone(), session_id, window, cx));
        let _subscriptions = vec![
            cx.observe_in(&store, window, |this, _, window, cx| {
                this.on_store_changed(window, cx)
            }),
            cx.observe(&code, |_, _, cx| cx.notify()),
            cx.observe(&tree, |_, _, cx| cx.notify()),
            cx.subscribe_in(&search, window, |this, _, event, _, cx| match event {
                InputEvent::Change => {
                    this.sync_body(cx);
                    cx.notify();
                }
                InputEvent::PressEnter { shift, .. } if !this.filter_lines => {
                    this.find_step(if *shift { -1 } else { 1 }, cx);
                }
                _ => {}
            }),
            cx.subscribe_in(&jq_input, window, |this, _, event, _, cx| match event {
                InputEvent::PressEnter { .. } => this.execute_jq(cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
        ];
        let mut panel = ResponsePanel {
            store,
            session_id,
            history,
            history_open: false,
            inspector_visible: false,
            filter_lines: false,
            search,
            jq_input,
            jq_output: None,
            jq_error: String::new(),
            _jq_task: None,
            saving: false,
            save_error: String::new(),
            code,
            tree,
            events: EventList::new(),
            live_events: EventList::new(),
            cache: Cache::default(),
            was_busy: false,
            _scroll_task: None,
            _subscriptions,
        };
        // A restored response keeps its tab and scroll position.
        panel.rebuild_cache(cx);
        panel.was_busy = panel.session(cx).is_some_and(|session| session.busy);
        panel.sync_body(cx);
        panel
    }

    fn session<'a>(&self, cx: &'a App) -> Option<&'a RequestSession> {
        self.store.read(cx).workspace.session(self.session_id)
    }

    fn update_session(&self, cx: &mut App, change: impl FnOnce(&mut RequestSession)) {
        let id = self.session_id;
        self.store.update(cx, |store, cx| {
            store.update_workspace(cx, |workspace| {
                if let Some(session) = workspace.session_mut(id) {
                    change(session);
                }
            })
        });
    }

    fn on_store_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx) else {
            return;
        };
        let busy = session.busy;
        let key = response_key(session.response.as_ref());
        if busy && !self.was_busy {
            self.history_open = false;
        }
        self.was_busy = busy;
        if key != self.cache.key {
            self.response_changed(window, cx);
        }
        self.sync_body(cx);
    }

    fn rebuild_cache(&mut self, cx: &App) {
        let Some(response) = self
            .session(cx)
            .and_then(|session| session.response.as_ref())
        else {
            self.cache = Cache::default();
            return;
        };
        let limited = response.is_truncated() || response.is_binary();
        let formatted = if limited {
            None
        } else {
            format_json(&response.body).ok()
        };
        let markup = (!limited
            && formatted.is_none()
            && response_language(&content_type(response)) == ResponseLanguage::Html)
            .then(|| SharedString::from(format_html(&response.body)));
        let event_stream = is_event_stream(header_value(response, "content-type"));
        self.cache = Cache {
            key: response_key(Some(response)),
            body: SharedString::from(response.body.clone()),
            source_parsed: formatted.is_some(),
            parsed: formatted.map(SharedString::from),
            markup,
            event_stream,
            events: if event_stream {
                parse_sse(&response.body)
            } else {
                vec![]
            },
        };
        self.jq_output = None;
    }

    /// A new response: `watch(() => props.response)` in the Vue panel.
    fn response_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rebuild_cache(cx);
        let event_stream = self.cache.event_stream;
        self.update_session(cx, |session| {
            // Stay on Tests across sends, so a rerun shows its results.
            if session.view.response_tab != "tests" || !shows_tests(session) {
                session.view.response_tab = if event_stream { "events" } else { "body" }.into();
            }
            session.view.response_scroll = 0.0;
        });
        self.search
            .update(cx, |search, cx| search.set_value("", window, cx));
        self.inspector_visible = false;
        self.jq_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.jq_output = None;
        self.jq_error.clear();
        self.save_error.clear();
        cx.notify();
    }

    fn search_text(&self, cx: &App) -> String {
        self.search.read(cx).value().to_string()
    }

    /// The body text shown: pretty JSON, jq output, pretty HTML, or the body.
    fn text(&self, pretty: bool) -> SharedString {
        match (&self.cache.parsed, pretty) {
            (Some(parsed), true) => parsed.clone(),
            _ => self
                .jq_output
                .clone()
                .or_else(|| self.cache.markup.clone().filter(|_| pretty))
                .unwrap_or_else(|| self.cache.body.clone()),
        }
    }

    fn shows_json_tree(&self, pretty: bool) -> bool {
        pretty && self.cache.parsed.is_some()
    }

    /// Push the body, find, and filter to the body view.
    fn sync_body(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx) else {
            return;
        };
        let Some(response) = &session.response else {
            return;
        };
        let view = session.view.clone();
        let language = if self.cache.parsed.is_some() {
            ResponseLanguage::Json
        } else {
            response_language(&content_type(response))
        };
        let search = self.search_text(cx);
        let (filter, find) = if self.filter_lines {
            (search, String::new())
        } else {
            (String::new(), search)
        };
        let text = self.text(view.pretty);
        if self.shows_json_tree(view.pretty) {
            let content = TreeContent {
                text,
                filter,
                find,
                wrap: view.wrap,
            };
            self.tree.update(cx, |tree, cx| {
                if tree.set_content(content, cx) {
                    tree.set_scroll_offset(view.response_scroll);
                }
            });
        } else {
            let content = CodeContent {
                text,
                language,
                filter,
                find,
                wrap: view.wrap,
            };
            self.code.update(cx, |code, cx| {
                if code.set_content(content, cx) {
                    code.set_scroll_offset(view.response_scroll);
                }
            });
        }
    }

    fn pretty(&self, cx: &App) -> bool {
        self.session(cx).is_some_and(|session| session.view.pretty)
    }

    fn finder<'a>(&self, cx: &'a App) -> &'a Finder {
        if self.shows_json_tree(self.pretty(cx)) {
            self.tree.read(cx).finder()
        } else {
            self.code.read(cx).finder()
        }
    }

    fn find_step(&mut self, direction: isize, cx: &mut Context<Self>) {
        if self.shows_json_tree(self.pretty(cx)) {
            self.tree
                .update(cx, |tree, cx| tree.find_step(direction, cx));
        } else {
            self.code
                .update(cx, |code, cx| code.find_step(direction, cx));
        }
    }

    /// Save the scroll position shortly after scrolling stops. Scroll
    /// offsets are not part of the autosave key, so this writes nothing.
    fn schedule_scroll_save(&mut self, cx: &mut Context<Self>) {
        self._scroll_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(200))
                .await;
            this.update(cx, |this, cx| {
                let offset = if this.shows_json_tree(this.pretty(cx)) {
                    this.tree.read(cx).scroll_offset()
                } else {
                    this.code.read(cx).scroll_offset()
                };
                let id = this.session_id;
                this.store.update(cx, |store, _| {
                    if let Some(session) = store.workspace.session_mut(id) {
                        session.view.response_scroll = offset;
                    }
                });
            })
            .ok();
        }));
    }

    fn toggle_inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inspector_visible = !self.inspector_visible;
        if self.inspector_visible {
            self.search
                .update(cx, |search, cx| search.focus(window, cx));
        } else {
            self.search
                .update(cx, |search, cx| search.set_value("", window, cx));
            self.sync_body(cx);
        }
        cx.notify();
    }

    fn set_filter_lines(&mut self, value: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.filter_lines = value;
        let placeholder = if value { "Filter lines" } else { "Find" };
        self.search.update(cx, |search, cx| {
            search.set_placeholder(placeholder, window, cx)
        });
        self.sync_body(cx);
        cx.notify();
    }

    fn set_tab(&mut self, tab: &'static str, cx: &mut Context<Self>) {
        self.update_session(cx, |session| session.view.response_tab = tab.into());
    }

    /// Open find in the response body (`Cmd/Ctrl+F`).
    pub fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(response) = self
            .session(cx)
            .and_then(|session| session.response.as_ref())
        else {
            return;
        };
        if response.is_binary() {
            return;
        }
        if self
            .session(cx)
            .is_some_and(|session| session.view.response_tab != "body")
        {
            self.set_tab("body", cx);
        }
        if self.inspector_visible {
            self.search
                .update(cx, |search, cx| search.focus(window, cx));
        } else {
            self.toggle_inspector(window, cx);
        }
    }

    /// Copy the shown result: the body, or the jq output.
    pub fn copy_result(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx) else {
            return;
        };
        let text = if session.view.response_tab == "headers" {
            session
                .response
                .as_ref()
                .map(|response| {
                    response
                        .headers
                        .iter()
                        .map(|header| format!("{}: {}", header.key, header.value))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default()
        } else if session.response.is_some() {
            self.text(session.view.pretty).to_string()
        } else {
            String::new()
        };
        self.store.update(cx, |store, cx| store.copy(text, cx));
    }

    /// The history view and whether it is shown, for the UI tests.
    #[cfg(test)]
    pub fn history_view(&self) -> &Entity<HistoryView> {
        &self.history
    }

    #[cfg(test)]
    pub fn history_shown(&self) -> bool {
        self.history_open
    }

    /// The scroll offset of the plain body view, for the UI tests.
    #[cfg(test)]
    pub fn body_scroll(&self, cx: &App) -> f64 {
        self.code.read(cx).scroll_offset()
    }

    pub fn toggle_history(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.history_open = !self.history_open;
        cx.notify();
    }

    /// Save the full body to a file the user picks.
    pub fn save_body(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx) else {
            return;
        };
        let Some(response) = session.response.clone() else {
            return;
        };
        if !can_save_response(&response) || self.saving {
            return;
        }
        let name = suggested_file_name(&response, &session.draft.url);
        let engine = self.store.read(cx).engine.clone();
        let directory = save_directory(std::env::var_os("HOME"));
        let picked = cx.prompt_for_new_path(&directory, Some(&name));
        self.saving = true;
        self.save_error.clear();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = match picked.await {
                Ok(Ok(Some(path))) => match response.body_id.clone().filter(|id| !id.is_empty()) {
                    Some(body_id) => engine.save_response(body_id, path).await,
                    None if can_save_response(&response) => {
                        engine.save_response_text(response.body.clone(), path).await
                    }
                    None => Err(BODY_UNAVAILABLE.into()),
                },
                // Cancelled: nothing to report.
                Ok(Ok(None)) | Err(_) => Ok(()),
                Ok(Err(error)) => Err(error.to_string()),
            };
            this.update(cx, |this, cx| {
                this.saving = false;
                if let Err(reason) = result {
                    this.save_error = save_error_message(&reason);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn toggle_wrap(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.update_session(cx, |session| session.view.wrap = !session.view.wrap);
    }

    pub fn toggle_pretty(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.update_session(cx, |session| session.view.pretty = !session.view.pretty);
    }

    fn execute_jq(&mut self, cx: &mut Context<Self>) {
        let query = self.jq_input.read(cx).value().to_string();
        let Some(response) = self
            .session(cx)
            .and_then(|session| session.response.as_ref())
        else {
            return;
        };
        if query.trim().is_empty() || response.is_truncated() {
            return;
        }
        self.jq_error.clear();
        let body = response.body.clone();
        let run = cx.background_spawn(async move { run_jq(&body, &query, &[]) });
        self._jq_task = Some(cx.spawn(async move |this, cx| {
            let result = run.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(output) => {
                        this.set_jq_output(Some(output.trim_end().to_string().into()));
                        let id = this.session_id;
                        this.store.update(cx, |store, _| {
                            if let Some(session) = store.workspace.session_mut(id) {
                                session.view.response_scroll = 0.0;
                            }
                        });
                    }
                    Err(error) => {
                        this.jq_error = if error.is_empty() {
                            "jq query failed.".into()
                        } else {
                            error
                        };
                    }
                }
                this.sync_body(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    fn set_jq_output(&mut self, output: Option<SharedString>) {
        self.cache.parsed = match &output {
            Some(output) => format_json(output).ok().map(SharedString::from),
            None if self.cache.source_parsed => {
                format_json(&self.cache.body).ok().map(SharedString::from)
            }
            None => None,
        };
        self.jq_output = output;
    }

    fn recheck(&mut self, cx: &mut Context<Self>) {
        let id = self.session_id;
        self.store.update(cx, |store, cx| {
            store.update_workspace(cx, |workspace| {
                let Some(session) = workspace.session_mut(id) else {
                    return;
                };
                let group_id = session.group_id;
                let captured = blink_core::runner::recheck(session);
                if !captured.is_empty() {
                    workspace.capture(group_id, &captured);
                }
            })
        });
    }
}

// ── Rendering ───────────────────────────────────────────────────────────────

fn muted_text(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .p(r(16.))
        .font_family(theme::SANS)
        .text_size(r(12.))
        .text_color(theme::colors(cx).muted_foreground)
        .child(text.into())
}

fn alert_text(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .p(r(16.))
        .font_family(theme::SANS)
        .text_size(r(12.))
        .text_color(theme::colors(cx).destructive)
        .child(text.into())
}

/// The Vue `variant="ghost"` button: monospace 12 px medium text in the
/// muted color, foreground on hover.
fn ghost_button(id: impl Into<ElementId>, cx: &App) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .font_family(theme::MONO)
        .text_size(r(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::colors(cx).muted_foreground)
}

/// A 28 px ghost icon button, as the Vue `size-7` buttons.
fn tool_button(
    cx: &App,
    id: &'static str,
    icon: IconName,
    tooltip: impl Into<SharedString>,
) -> Button {
    ghost_button(id, cx)
        .w(r(28.))
        .h(r(28.))
        .icon(Icon::new(icon).size(r(14.)))
        .tooltip(tooltip)
}

/// A small 22 px icon button in the find field.
fn find_button(cx: &App, id: &'static str, icon: IconName, tooltip: &'static str) -> Button {
    ghost_button(id, cx)
        .w(r(22.))
        .h(r(22.))
        .icon(Icon::new(icon).size(r(13.)))
        .tooltip(tooltip)
}

fn tab_trigger(
    id: &'static str,
    label: &'static str,
    count: Option<(String, Option<Hsla>)>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let colors = theme::colors(cx);
    div()
        .id(id)
        .flex()
        .items_center()
        .h(r(38.))
        .px(r(10.))
        .border_b_1()
        .border_color(if selected {
            colors.primary
        } else {
            gpui_kit::transparent_black()
        })
        .font_family(theme::SANS)
        .text_size(r(12.))
        .whitespace_nowrap()
        .cursor_pointer()
        .text_color(if selected {
            colors.foreground
        } else {
            colors.muted_foreground
        })
        .hover(|this| this.bg(colors.muted).text_color(colors.foreground))
        .child(label)
        .when_some(count, |this, (count, color)| {
            this.child(
                div()
                    .ml(r(4.))
                    .font_family(theme::MONO)
                    .text_size(r(10.))
                    .when_some(color, |this, color| this.text_color(color))
                    .child(count),
            )
        })
}

/// Five bars that pulse while waiting, as the Vue `receive` animation.
fn receiving_bars(cx: &App) -> impl IntoElement {
    let primary = theme::colors(cx).primary;
    div()
        .flex()
        .gap(r(4.))
        .mb(r(24.))
        .children((1..=5).map(move |index: usize| {
            // `nth-child(2n)` waits 0.2 s, `nth-child(3n)` 0.4 s.
            let delay = if index.is_multiple_of(3) {
                0.4
            } else if index.is_multiple_of(2) {
                0.2
            } else {
                0.0
            };
            div().w(r(5.)).h(r(14.)).bg(primary).with_animation(
                ("receive", index),
                Animation::new(Duration::from_secs(2)).repeat(),
                move |bar, delta| {
                    // 1 s up, 1 s down: `infinite alternate`.
                    let t = (delta * 2.0 - delay + 2.0) % 2.0;
                    let t = if t > 1.0 { 2.0 - t } else { t };
                    bar.opacity(0.2 + 0.8 * t)
                },
            )
        }))
}

impl ResponsePanel {
    fn render_header(&self, session: &RequestSession, cx: &mut Context<Self>) -> Div {
        let colors = theme::colors(cx);
        let count = session.history.len();
        div()
            .h(r(36.))
            .flex_none()
            .px(r(16.))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.muted)
            .font_family(theme::MONO)
            .text_size(r(10.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(r(11.))
                    .text_color(colors.foreground)
                    .child(tracked("RESPONSE", 0.12)),
            )
            .child(
                div()
                    .ml_auto()
                    .text_color(colors.muted_foreground)
                    .child(tracked(header_status(session), 0.12)),
            )
            .child(
                ghost_button("response-history", cx)
                    .text_size(r(10.))
                    .gap(r(4.))
                    .ml(r(8.))
                    .mr(r(-8.))
                    .h(r(24.))
                    .px(r(6.))
                    .compact()
                    .tooltip("Request history")
                    .icon(Icon::new(IconName::RotateCcwClock).size(r(13.)))
                    .when(count > 0, |this| {
                        this.child(div().text_size(r(10.)).child(count.to_string()))
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_history(window, cx))),
            )
    }

    fn render_status_line(&self, response: &ApiResponse, cx: &App) -> Div {
        let colors = theme::colors(cx);
        let tone = theme::status_color(response.status, cx);
        let divider = || div().border_l_1().border_color(colors.border).pl(r(16.));
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .min_h(r(44.))
            .gap(r(16.))
            .px(r(16.))
            .py(r(8.))
            .border_b_1()
            .border_color(colors.border)
            .font_family(theme::MONO)
            .text_size(r(11.))
            .text_color(colors.foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(r(8.))
                    .text_color(tone)
                    .child(div().size(r(5.)).bg(tone))
                    .child(format!("{} {}", response.status, response.status_text)),
            )
            .child(divider().child(timing_card::render(
                response.timing.as_ref(),
                response.duration_ms,
                cx,
            )))
            .child(divider().child(format_bytes(response.size_bytes)))
            .when_some(response.final_url.clone(), |this, url| {
                let title: SharedString = url.clone().into();
                this.child(
                    div()
                        .id("response-redirect")
                        .w_full()
                        .min_w_0()
                        .truncate()
                        .text_color(colors.muted_foreground)
                        .child(format!(
                            "→ {url} · {}",
                            redirect_label(response.redirect_count)
                        ))
                        .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx)),
                )
            })
    }

    fn render_toolbar(
        &self,
        session: &RequestSession,
        response: &ApiResponse,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = theme::colors(cx);
        let tab = session.view.response_tab.as_str();
        let pretty = session.view.pretty;
        let wrap = session.view.wrap;
        let truncated = response.is_truncated();
        let binary = response.is_binary();
        let body_tab = tab == "body";
        let formatted = self.cache.parsed.is_some() || self.cache.markup.is_some();
        let savable = can_save_response(response);
        let show_tests = shows_tests(session);
        let tests = session.test_results.clone().unwrap_or_default();
        let passed = tests.iter().filter(|result| result.pass).count();
        let copied = self.store.read(cx).copied;
        let entity = cx.entity();

        let tabs = div()
            .flex()
            .items_center()
            .child(
                tab_trigger("response-tab-body", "Body", None, tab == "body", cx)
                    .on_click(cx.listener(|this, _, _, cx| this.set_tab("body", cx))),
            )
            .child(
                tab_trigger(
                    "response-tab-headers",
                    "Headers",
                    Some((response.headers.len().to_string(), None)),
                    tab == "headers",
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| this.set_tab("headers", cx))),
            )
            .when(self.cache.event_stream, |this| {
                this.child(
                    tab_trigger(
                        "response-tab-events",
                        "Events",
                        Some((self.cache.events.len().to_string(), None)),
                        tab == "events",
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.set_tab("events", cx))),
                )
            })
            .when(show_tests, |this| {
                let count = (!tests.is_empty()).then(|| {
                    (
                        format!("{passed}/{}", tests.len()),
                        Some(if passed == tests.len() {
                            colors.success
                        } else {
                            colors.destructive
                        }),
                    )
                });
                this.child(
                    tab_trigger("response-tab-tests", "Tests", count, tab == "tests", cx)
                        .on_click(cx.listener(|this, _, _, cx| this.set_tab("tests", cx))),
                )
            });

        let tools = div()
            .flex()
            .items_center()
            .when(body_tab && (formatted || truncated) && !binary, |this| {
                this.child(
                    ghost_button("response-pretty", cx)
                        .h(r(28.))
                        .px(r(10.))
                        .label(if pretty && !truncated {
                            "Pretty"
                        } else {
                            "Raw"
                        })
                        .disabled(truncated)
                        .when(truncated, |this| this.tooltip(UNAVAILABLE))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.toggle_pretty(window, cx)),
                        ),
                )
            })
            .when(body_tab && !binary, |this| {
                this.child(
                    tool_button(cx, "response-wrap", IconName::TextWrap, "Wrap lines")
                        .on_click(cx.listener(|this, _, window, cx| this.toggle_wrap(window, cx))),
                )
                .child(
                    tool_button(
                        cx,
                        "response-find",
                        IconName::Search,
                        format!(
                            "Find and filter response · {}",
                            blink_core::shortcut::shortcut_label(
                                &["mod", "f"],
                                blink_core::shortcut::IS_MAC
                            )
                        ),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_inspector(window, cx))),
                )
            })
            .child(
                ghost_button("response-save", cx)
                    .w(r(28.))
                    .h(r(28.))
                    .icon(Icon::new(IconName::Download).size(r(14.)))
                    .tooltip(if savable {
                        "Save response body…"
                    } else {
                        "Body is no longer available. Send the request again."
                    })
                    .disabled(!savable || self.saving)
                    .on_click(cx.listener(|this, _, window, cx| this.save_body(window, cx))),
            )
            .child(
                tool_button(
                    cx,
                    "response-copy",
                    if copied {
                        IconName::Check
                    } else {
                        IconName::Copy
                    },
                    "Copy response",
                )
                .on_click(cx.listener(|this, _, window, cx| this.copy_result(window, cx))),
            );

        let find_shortcut = shortcut_label(&["mod", "f"], IS_MAC);
        div()
            .id("response-toolbar")
            .flex()
            .flex_none()
            .justify_between()
            .items_center()
            .border_b_1()
            .border_color(colors.border)
            .px(r(8.))
            .gap(r(4.))
            .child(tabs)
            .child(tools)
            .context_menu(move |menu, _, _| {
                let copy = entity.clone();
                let save = entity.clone();
                let mut menu = menu
                    .item(
                        PopupMenuItem::new("Copy response").on_click(move |_, window, cx| {
                            copy.update(cx, |this, cx| this.copy_result(window, cx));
                        }),
                    )
                    .item(
                        PopupMenuItem::new("Save response body…")
                            .disabled(!savable)
                            .on_click(move |_, window, cx| {
                                save.update(cx, |this, cx| this.save_body(window, cx));
                            }),
                    );
                if body_tab && !binary {
                    menu = menu.separator();
                    if formatted {
                        let pretty_entity = entity.clone();
                        menu = menu.item(PopupMenuItem::new("Pretty").checked(pretty).on_click(
                            move |_, window, cx| {
                                pretty_entity.update(cx, |this, cx| this.toggle_pretty(window, cx));
                            },
                        ));
                    }
                    let wrap_entity = entity.clone();
                    let find_entity = entity.clone();
                    let shortcut = find_shortcut.clone();
                    menu = menu
                        .item(PopupMenuItem::new("Wrap lines").checked(wrap).on_click(
                            move |_, window, cx| {
                                wrap_entity.update(cx, |this, cx| this.toggle_wrap(window, cx));
                            },
                        ))
                        .item(
                            PopupMenuItem::element(move |_, cx| {
                                h_flex_between(
                                    "Find",
                                    shortcut.clone(),
                                    theme::colors(cx).muted_foreground,
                                )
                            })
                            .on_click(move |_, window, cx| {
                                find_entity
                                    .update(cx, |this, cx| this.toggle_inspector(window, cx));
                            }),
                        );
                }
                menu
            })
    }

    fn render_inspector(&self, response: &ApiResponse, cx: &mut Context<Self>) -> Div {
        let colors = theme::colors(cx);
        let status = find_status(self.finder(cx), &self.search_text(cx), self.filter_lines);
        let count = self.finder(cx).count();
        let truncated = response.is_truncated();
        let jq_empty = self.jq_input.read(cx).value().trim().is_empty();
        let field = |cx: &App| {
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(r(6.))
                .rounded(px(4.))
                .border_1()
                .border_color(theme::colors(cx).input)
                .bg(theme::colors(cx).background)
        };
        let find_field = field(cx)
            .flex_basis(r(180.))
            .pl(r(8.))
            .text_color(colors.muted_foreground)
            .child(Icon::new(IconName::Search).size(r(13.)))
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.search)
                        .appearance(false)
                        .text_color(colors.foreground)
                        .h(r(26.))
                        .px(r(7.))
                        .font_family(theme::MONO)
                        .text_size(r(11.)),
                ),
            )
            .when(!status.is_empty(), |this| {
                let no_results = status == "No results";
                this.child(
                    div()
                        .flex_none()
                        .whitespace_nowrap()
                        .font_family(theme::MONO)
                        .text_size(r(10.))
                        .when(no_results, |this| this.text_color(colors.destructive))
                        .child(status.clone()),
                )
            })
            .when(!self.filter_lines, |this| {
                this.child(
                    find_button(
                        cx,
                        "find-previous",
                        IconName::ChevronUp,
                        "Previous match · Shift+Enter",
                    )
                    .disabled(count == 0)
                    .on_click(cx.listener(|this, _, _, cx| this.find_step(-1, cx))),
                )
                .child(
                    find_button(cx, "find-next", IconName::ChevronDown, "Next match · Enter")
                        .disabled(count == 0)
                        .on_click(cx.listener(|this, _, _, cx| this.find_step(1, cx))),
                )
            })
            .child(
                find_button(
                    cx,
                    "filter-lines",
                    IconName::ListFilter,
                    "Show only matching lines",
                )
                .mr(r(2.))
                .selected(self.filter_lines)
                .on_click(cx.listener(|this, _, window, cx| {
                    let value = !this.filter_lines;
                    this.set_filter_lines(value, window, cx)
                })),
            );
        div()
            .flex()
            .flex_none()
            .min_h(r(38.))
            .gap(r(8.))
            .px(r(8.))
            .py(r(5.))
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.muted)
            .on_action(
                cx.listener(|this, _: &Escape, window, cx| this.toggle_inspector(window, cx)),
            )
            .child(find_field)
            .when(
                (self.cache.source_parsed || truncated) && !response.is_binary(),
                |this| {
                    this.child(
                        field(cx)
                            .flex_basis(r(260.))
                            .child(
                                div()
                                    .id("response-jq")
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        Input::new(&self.jq_input)
                                            .appearance(false)
                                            .text_color(colors.foreground)
                                            .disabled(truncated)
                                            .h(r(26.))
                                            .px(r(7.))
                                            .font_family(theme::MONO)
                                            .text_size(r(11.)),
                                    )
                                    .when(truncated, |this| {
                                        this.tooltip(|window, cx| {
                                            Tooltip::new(UNAVAILABLE).build(window, cx)
                                        })
                                    }),
                            )
                            .child(
                                ghost_button("run-jq", cx)
                                    .h(r(28.))
                                    .px(r(10.))
                                    .label("Run jq")
                                    .disabled(jq_empty || truncated)
                                    .on_click(cx.listener(|this, _, _, cx| this.execute_jq(cx))),
                            )
                            .when(self.jq_output.is_some(), |this| {
                                this.child(
                                    tool_button(cx, "clear-jq", IconName::X, "Clear jq result")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.set_jq_output(None);
                                            this.sync_body(cx);
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                },
            )
    }

    fn render_truncated(&self, response: &ApiResponse, cx: &mut Context<Self>) -> Div {
        let colors = theme::colors(cx);
        let restored = response.is_truncated() && !response.is_binary() && response.body.is_empty();
        let savable = can_save_response(response);
        let notice = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(r(4.))
            .py(r(6.))
            .px(r(14.))
            .border_b_1()
            .border_color(colors.border)
            .text_color(colors.muted_foreground)
            .font_family(theme::MONO)
            .text_size(r(10.));
        if restored {
            return notice.child(
                "Preview is not kept after a restart. Send the request again to inspect it.",
            );
        }
        let enabled = savable && !self.saving;
        notice
            .child(format!(
                "Preview shows the first {} of {}.",
                format_bytes(response.body.len() as u64),
                format_bytes(response.size_bytes)
            ))
            .child(
                div()
                    .id("truncated-save")
                    .when(enabled, |this| {
                        this.cursor_pointer()
                            .hover(|this| this.text_color(colors.foreground))
                            .on_click(cx.listener(|this, _, window, cx| this.save_body(window, cx)))
                    })
                    .when(!enabled, |this| this.opacity(0.5))
                    // `underline decoration-dotted underline-offset-3
                    // disabled:no-underline`.
                    .child(if enabled {
                        dotted("Save…")
                    } else {
                        tracked("Save…", 0.)
                    }),
            )
            .child("to get the full body.")
    }

    fn render_body(&self, response: &ApiResponse, cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::colors(cx);
        let restored = response.is_truncated() && !response.is_binary() && response.body.is_empty();
        if response.is_binary() {
            let savable = can_save_response(response);
            return div()
                .flex()
                .flex_1()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(r(12.))
                .p(r(32.))
                .text_color(colors.muted_foreground)
                .text_size(r(12.))
                .child(div().font_family(theme::MONO).child(format!(
                    "Binary response · {} · {}",
                    format_bytes(response.size_bytes),
                    content_type(response)
                )))
                .child(
                    Button::new("binary-save")
                        .xsmall()
                        .font_family(theme::MONO)
                        .text_size(r(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .h(r(28.))
                        .px(r(10.))
                        .label("Save…")
                        .disabled(!savable || self.saving)
                        .on_click(cx.listener(|this, _, window, cx| this.save_body(window, cx))),
                )
                .into_any_element();
        }
        if response.body.is_empty() {
            return if restored {
                div().into_any_element()
            } else {
                muted_text("Empty response body.", cx).into_any_element()
            };
        }
        let pretty = self.pretty(cx);
        let view: AnyView = if self.shows_json_tree(pretty) {
            self.tree.clone().into()
        } else {
            self.code.clone().into()
        };
        div()
            .id("response-body-view")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .on_scroll_wheel(cx.listener(|this, _, _, cx| this.schedule_scroll_save(cx)))
            .child(view)
            .into_any_element()
    }

    fn render_headers(&self, response: &ApiResponse, cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::colors(cx);
        let query = self.search_text(cx);
        let headers = filter_headers(&response.headers, &query);
        let cell = || {
            div()
                .px(r(16.))
                .py(r(8.))
                .border_b_1()
                .border_color(colors.border)
                .min_w_0()
                .whitespace_normal()
        };
        let head = div()
            .flex()
            .text_size(r(10.))
            .text_color(colors.muted_foreground)
            .child(cell().w(relative(0.38)).flex_none().child("NAME"))
            .child(cell().flex_1().child("VALUE"));
        let store = self.store.clone();
        let rows = headers.into_iter().enumerate().map(|(index, header)| {
            let key = header.key.clone();
            let value = header.value.clone();
            let store = store.clone();
            div()
                .id(("response-header", index))
                .flex()
                .child(
                    cell()
                        .w(relative(0.38))
                        .flex_none()
                        .text_color(colors.info)
                        .child(key.clone()),
                )
                .child(cell().flex_1().child(value.clone()))
                .context_menu(move |menu, _, _| {
                    let (a, b, c) = (store.clone(), store.clone(), store.clone());
                    let (name, value) = (key.clone(), value.clone());
                    let pair = format!("{name}: {value}");
                    menu.item(PopupMenuItem::new("Copy name").on_click(move |_, _, cx| {
                        a.update(cx, |store, cx| store.copy(name.clone(), cx));
                    }))
                    .item(PopupMenuItem::new("Copy value").on_click(move |_, _, cx| {
                        b.update(cx, |store, cx| store.copy(value.clone(), cx));
                    }))
                    .item(
                        PopupMenuItem::new("Copy name: value").on_click(move |_, _, cx| {
                            c.update(cx, |store, cx| store.copy(pair.clone(), cx));
                        }),
                    )
                })
        });
        let rows: Vec<_> = rows.collect();
        let empty = rows.is_empty();
        div()
            .id("response-headers")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .font_family(theme::MONO)
            .text_size(r(11.))
            .line_height(r(18.7))
            .text_color(colors.foreground)
            .child(head)
            .children(rows)
            .when(empty, |this| {
                this.child(muted_text("No response headers match this filter.", cx))
            })
            .into_any_element()
    }

    fn render_tests(&self, session: &RequestSession, cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::colors(cx);
        let results = session.test_results.clone().unwrap_or_default();
        let passed = results.iter().filter(|result| result.pass).count();
        let captures = session.capture_errors.clone().unwrap_or_default();
        div()
            .id("response-tests")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .h(r(32.))
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(colors.border)
                    .px(r(16.))
                    .font_family(theme::MONO)
                    .text_size(r(10.))
                    .text_color(colors.muted_foreground)
                    .child(tracked(
                        if results.is_empty() {
                            "NO RESULTS".to_string()
                        } else {
                            format!("{passed} OF {} PASSED", results.len())
                        },
                        0.1,
                    ))
                    .child(
                        Button::new("tests-recheck")
                            .ghost()
                            .xsmall()
                            .label("Run again")
                            .on_click(cx.listener(|this, _, _, cx| this.recheck(cx))),
                    ),
            )
            .children(results.iter().map(|result| {
                div()
                    .flex()
                    .items_start()
                    .gap(r(10.))
                    .border_b_1()
                    .border_color(colors.border)
                    .px(r(16.))
                    .py(r(8.))
                    .font_family(theme::MONO)
                    .text_size(r(11.))
                    .text_color(colors.foreground)
                    .child(
                        div()
                            .w(r(32.))
                            .flex_none()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if result.pass {
                                colors.success
                            } else {
                                colors.destructive
                            })
                            .child(if result.pass { "PASS" } else { "FAIL" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().child(result.description.clone()))
                            .when(!result.pass, |this| {
                                this.child(
                                    div()
                                        .text_color(colors.muted_foreground)
                                        .child(format!("Actual: {}", result.actual)),
                                )
                            }),
                    )
            }))
            .when(results.is_empty(), |this| {
                this.child(muted_text(
                    "Add assertions in the request Tests tab, then send or run again.",
                    cx,
                ))
            })
            .when(!captures.is_empty(), |this| {
                this.child(
                    div()
                        .border_t_1()
                        .border_color(colors.border)
                        .px(r(16.))
                        .py(r(8.))
                        .font_family(theme::MONO)
                        .text_size(r(11.))
                        .text_color(colors.warning)
                        .children(
                            captures
                                .iter()
                                .map(|message| div().child(format!("Capture {message}"))),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_response(
        &mut self,
        session: &RequestSession,
        response: &ApiResponse,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = theme::colors(cx);
        let tab = session.view.response_tab.clone();
        let body_tab = tab == "body";
        let mut children = Vec::new();
        if session.stale {
            children.push(
                div()
                    .py(r(6.))
                    .px(r(14.))
                    .border_b_1()
                    .border_color(colors.border)
                    .text_color(colors.warning)
                    .font_family(theme::MONO)
                    .text_size(r(10.))
                    .child("Previous response · request edited since send")
                    .into_any_element(),
            );
        }
        children.push(self.render_status_line(response, cx).into_any_element());
        children.push(
            self.render_toolbar(session, response, cx)
                .into_any_element(),
        );
        if body_tab && self.inspector_visible {
            children.push(self.render_inspector(response, cx).into_any_element());
        }
        if !self.jq_error.is_empty() {
            children.push(alert_text(self.jq_error.clone(), cx).into_any_element());
        }
        if !self.save_error.is_empty() {
            children.push(alert_text(self.save_error.clone(), cx).into_any_element());
        }
        if body_tab && response.is_truncated() && !response.is_binary() {
            children.push(self.render_truncated(response, cx).into_any_element());
        }
        let content = match tab.as_str() {
            "headers" => self.render_headers(response, cx),
            "tests" => self.render_tests(session, cx),
            "events" => {
                let events = std::mem::take(&mut self.cache.events);
                let list = self.events.render(&events, false, cx);
                self.cache.events = events;
                list
            }
            _ => self.render_body(response, cx),
        };
        children.push(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(content)
                .into_any_element(),
        );
        children.push(
            div()
                .flex_none()
                .min_h(r(28.))
                .px(r(16.))
                .border_t_1()
                .border_color(colors.border)
                .flex()
                .items_center()
                .font_family(theme::MONO)
                .text_size(r(9.))
                .text_color(colors.muted_foreground)
                .child(div().truncate().child(content_type(response)))
                .into_any_element(),
        );
        children
    }

    fn render_live_stream(
        &mut self,
        session: &RequestSession,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = theme::colors(cx);
        let Some(stream) = &session.stream else {
            return div().into_any_element();
        };
        let tone = if stream.status >= 400 {
            colors.destructive
        } else {
            colors.success
        };
        let count = stream.events.len();
        let divider = || div().border_l_1().border_color(colors.border).pl(r(16.));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_h(r(44.))
                    .gap(r(16.))
                    .px(r(16.))
                    .py(r(8.))
                    .border_b_1()
                    .border_color(colors.border)
                    .font_family(theme::MONO)
                    .text_size(r(11.))
                    .text_color(colors.foreground)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(r(8.))
                            .text_color(tone)
                            .child(div().size(r(5.)).bg(tone).with_animation(
                                "stream-pulse",
                                Animation::new(Duration::from_secs(2)).repeat(),
                                |dot, delta| {
                                    let t = delta * 2.0;
                                    let t = if t > 1.0 { 2.0 - t } else { t };
                                    dot.opacity(0.2 + 0.8 * t)
                                },
                            ))
                            .child(format!("{} {}", stream.status, stream.status_text)),
                    )
                    .child(divider().child(format!(
                        "{count} {}",
                        if count == 1 { "event" } else { "events" }
                    )))
                    .child(divider().child(format_bytes(stream.bytes)))
                    .child(
                        div()
                            .ml_auto()
                            .text_color(colors.muted_foreground)
                            .child(format!("STREAMING · {:.1} s", session.elapsed / 1000.0)),
                    ),
            )
            .child(self.live_events.render(&stream.events, true, cx))
            .child(
                div()
                    .flex_none()
                    .min_h(r(28.))
                    .px(r(16.))
                    .border_t_1()
                    .border_color(colors.border)
                    .flex()
                    .items_center()
                    .font_family(theme::MONO)
                    .text_size(r(9.))
                    .text_color(colors.muted_foreground)
                    .child("Cancel stops the stream and keeps the events."),
            )
            .into_any_element()
    }
}

fn h_flex_between(label: &'static str, shortcut: String, muted: Hsla) -> Div {
    div()
        .flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(r(16.))
        .child(label)
        .child(div().text_color(muted).text_size(r(11.)).child(shortcut))
}

fn centered_state(cx: &App) -> Div {
    div()
        .flex()
        .flex_1()
        .flex_col()
        .items_center()
        .justify_center()
        .p(r(32.))
        .min_h(r(220.))
        .text_color(theme::colors(cx).muted_foreground)
}

fn state_title(text: &'static str, cx: &App) -> Div {
    div()
        .font_family(theme::MONO)
        .text_size(r(11.))
        .text_color(theme::colors(cx).foreground)
        .child(tracked(text, 0.14))
}

impl Render for ResponsePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let Some(session) = self.session(cx).cloned() else {
            return div().size_full().into_any_element();
        };
        let timeout = transport_options(&self.store.read(cx).workspace.preferences).timeout_seconds;
        let root = div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(colors.background)
            .font_family(theme::SANS)
            .child(self.render_header(&session, cx));
        let root = if self.history_open {
            root.child(self.history.clone())
        } else if let (Some(response), false, true) =
            (&session.response, session.busy, session.error.is_empty())
        {
            let children = self.render_response(&session, response, cx);
            root.children(children)
        } else if !session.error.is_empty() {
            root.child(
                div()
                    .flex()
                    .flex_1()
                    .flex_col()
                    .items_start()
                    .justify_start()
                    .p(r(32.))
                    .min_h(r(220.))
                    .gap(r(16.))
                    .text_color(colors.destructive)
                    .child(Icon::new(IconName::TriangleAlert).size(r(22.)))
                    .child(state_title("REQUEST FAILED", cx))
                    .child(
                        div()
                            .font_family(theme::MONO)
                            .text_size(r(12.))
                            .line_height(r(21.6))
                            .child(session.error.clone()),
                    ),
            )
        } else if session.busy && session.stream.is_some() {
            root.child(self.render_live_stream(&session, cx))
        } else if session.busy {
            root.child(
                centered_state(cx)
                    .child(receiving_bars(cx))
                    .child(state_title("AWAITING RESPONSE", cx))
                    .child(div().mt(r(10.)).text_size(r(12.)).child(format!(
                        "{:.1} s elapsed · {timeout} s timeout",
                        session.elapsed / 1000.0
                    ))),
            )
        } else {
            root.child(centered_state(cx).child(state_title("AWAITING REQUEST", cx)))
        };
        root.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` exports its own `test` attribute; use the standard one.
    use blink_core::model::Header;
    use core::prelude::v1::test;

    fn response(headers: &[(&str, &str)]) -> ApiResponse {
        ApiResponse {
            status: 200,
            status_text: "OK".into(),
            duration_ms: 12.0,
            headers: headers
                .iter()
                .map(|(key, value)| Header {
                    key: (*key).into(),
                    value: (*value).into(),
                })
                .collect(),
            body: "{}".into(),
            size_bytes: 2,
            body_id: None,
            truncated: None,
            binary: None,
            final_url: None,
            redirect_count: None,
            timing: None,
        }
    }

    #[test]
    fn reads_the_content_type_without_parameters() {
        assert_eq!(
            content_type(&response(&[(
                "Content-Type",
                "application/json; charset=utf-8"
            )])),
            "application/json"
        );
        assert_eq!(content_type(&response(&[])), "No Content-Type");
    }

    #[test]
    fn labels_redirects() {
        assert_eq!(redirect_label(None), "redirected");
        assert_eq!(redirect_label(Some(1)), "1 redirect");
        assert_eq!(redirect_label(Some(3)), "3 redirects");
    }

    #[test]
    fn writes_save_errors_as_one_sentence() {
        assert_eq!(
            save_error_message("Permission denied."),
            "Cannot save the file: Permission denied."
        );
        assert_eq!(
            save_error_message("Disk full"),
            "Cannot save the file: Disk full."
        );
    }

    #[test]
    fn a_new_response_has_a_new_key() {
        let a = response(&[]);
        let mut b = a.clone();
        b.duration_ms = 13.0;
        assert_ne!(response_key(Some(&a)), response_key(Some(&b)));
        assert_eq!(response_key(None), "");
    }
}
