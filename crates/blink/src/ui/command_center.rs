//! The command center in the title bar. Port of `CommandCenter.vue`.
//!
//! The command list belongs to the root view: it calls `set_commands` before
//! `open`. The trigger button dispatches `SearchRequests`, so a click takes
//! the same path as Cmd+P.

use std::ops::Range;

use blink_core::command_center::{
    COMMAND_PREFIX, Command, RankedCommand, RequestMatch, highlight_runs, is_command_query,
    match_commands, match_requests,
};
use blink_core::shortcut::{IS_MAC, shortcut_label};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::actions::{RunCommandId, SearchRequests};
use crate::store::Store;
use crate::theme;
use crate::ui::status_bar::css;

/// One row of the open list, so keys work the same in both modes.
enum Choice {
    Request(u64),
    Command { id: String, disabled: bool },
}

pub struct CommandCenter {
    store: Entity<Store>,
    input: Entity<InputState>,
    open: bool,
    index: usize,
    commands: Vec<Command>,
    /// Focus before opening, restored on close.
    opener: Option<FocusHandle>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl CommandCenter {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx));
        let _subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => {
                        this.index = 0;
                        this.sync_placeholder(window, cx);
                        this.scroll.scroll_to_item(0);
                        cx.notify();
                    }
                    // Focus left the popover: close without stealing it back.
                    InputEvent::Blur if this.open => {
                        this.open = false;
                        this.opener = None;
                        cx.notify();
                    }
                    _ => {}
                }
            }),
        ];
        CommandCenter {
            store,
            input,
            open: false,
            index: 0,
            commands: Vec::new(),
            opener: None,
            scroll: ScrollHandle::new(),
            _subscriptions,
        }
    }

    /// The commands `>` lists. The root view supplies them from
    /// `BlinkApp::commands` before each `open`.
    pub fn set_commands(&mut self, commands: Vec<Command>) {
        self.commands = commands;
    }

    /// Open with `query`: "" searches requests, ">" lists commands.
    pub fn open(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.opener = window.focused(cx);
        }
        self.open = true;
        self.index = 0;
        self.input.update(cx, |input, cx| {
            input.set_value(query.to_string(), window, cx);
            input.focus(window, cx);
        });
        self.sync_placeholder(window, cx);
        cx.notify();
    }

    fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    fn sync_placeholder(&self, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = if is_command_query(&self.query(cx)) {
            "Run a command".to_string()
        } else {
            format!("Search requests by name, URL, or group. Type {COMMAND_PREFIX} for commands")
        };
        self.input
            .update(cx, |input, cx| input.set_placeholder(placeholder, window, cx));
    }

    /// Close and return focus to where it was.
    fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        if let Some(opener) = self.opener.take() {
            opener.focus(window, cx);
        } else {
            window.blur(cx);
        }
        cx.notify();
    }

    fn request_matches(&self, cx: &App) -> Vec<RequestMatch> {
        let workspace = &self.store.read(cx).workspace;
        match_requests(
            &workspace.sessions,
            &workspace.groups,
            &self.query(cx),
            &workspace.global_definitions,
        )
    }

    fn command_matches(&self, cx: &App) -> Vec<RankedCommand> {
        match_commands(&self.commands, &self.query(cx))
    }

    fn choices(&self, cx: &App) -> Vec<Choice> {
        if is_command_query(&self.query(cx)) {
            self.command_matches(cx)
                .into_iter()
                .map(|ranked| Choice::Command {
                    id: ranked.command.id,
                    disabled: ranked.command.disabled,
                })
                .collect()
        } else {
            self.request_matches(cx)
                .into_iter()
                .map(|found| Choice::Request(found.id))
                .collect()
        }
    }

    fn choose(&mut self, choice: Choice, window: &mut Window, cx: &mut Context<Self>) {
        match choice {
            Choice::Request(id) => {
                self.hide(window, cx);
                self.store.update(cx, |store, cx| {
                    store.update_workspace(cx, |workspace| workspace.select(id))
                });
            }
            Choice::Command { disabled: true, .. } => {}
            Choice::Command { id, .. } => {
                // Return focus first, so a command that moves focus keeps it.
                self.hide(window, cx);
                window.dispatch_action(Box::new(RunCommandId { id }), cx);
            }
        }
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.choices(cx).len() as isize;
        if count == 0 {
            return;
        }
        self.index = (self.index as isize + delta).rem_euclid(count) as usize;
        self.scroll.scroll_to_item(self.index);
        cx.notify();
    }

    fn on_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.step(-1, cx);
    }

    fn on_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.step(1, cx);
    }

    fn on_enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let mut choices = self.choices(cx);
        if self.index < choices.len() {
            let choice = choices.swap_remove(self.index);
            self.choose(choice, window, cx);
        }
    }

    fn on_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.hide(window, cx);
    }

    fn render_trigger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let title = format!(
            "Search requests · {}. Type {COMMAND_PREFIX} for commands · {}",
            shortcut_label(&["mod", "p"], IS_MAC),
            shortcut_label(&["mod", "shift", "p"], IS_MAC)
        );
        div()
            .id("command-center-trigger")
            .flex()
            .items_center()
            .gap(css(8.))
            .w_full()
            .h(css(26.))
            .px(css(10.))
            .rounded(css(4.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.muted)
            .text_size(css(12.))
            .text_color(colors.muted_foreground)
            .cursor_pointer()
            .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
            .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
            .on_click(|_, window, cx| window.dispatch_action(Box::new(SearchRequests), cx))
            .child(Icon::new(IconName::Search).size(css(13.)))
            .child(div().flex_1().child("Search requests"))
            .child(div().text_size(css(10.)).child("⌘P"))
    }

    fn render_popover(&self, width: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let query = self.query(cx);
        let command_mode = is_command_query(&query);
        let highlight = HighlightStyle {
            color: Some(colors.primary),
            font_weight: Some(FontWeight::SEMIBOLD),
            ..Default::default()
        };
        let rows: Vec<AnyElement> = if command_mode {
            self.command_matches(cx)
                .into_iter()
                .enumerate()
                .map(|(i, ranked)| {
                    let command = ranked.command;
                    let disabled = command.disabled;
                    let label = highlighted(&command.label, &ranked.indices, highlight);
                    let id = command.id.clone();
                    self.row(i, cx)
                        .when(disabled, |row| {
                            row.cursor_default().text_color(colors.muted_foreground)
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                window.prevent_default();
                                this.choose(
                                    Choice::Command {
                                        id: id.clone(),
                                        disabled,
                                    },
                                    window,
                                    cx,
                                );
                            }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(label),
                        )
                        .when_some(command.shortcut, |row, keys| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(css(10.))
                                    .text_color(colors.muted_foreground)
                                    .child(shortcut_label(&keys, IS_MAC)),
                            )
                        })
                        .into_any_element()
                })
                .collect()
        } else {
            self.request_matches(cx)
                .into_iter()
                .enumerate()
                .map(|(i, found)| {
                    let id = found.id;
                    let label = highlighted(&found.label, &found.label_indices, highlight);
                    self.row(i, cx)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                window.prevent_default();
                                this.choose(Choice::Request(id), window, cx);
                            }),
                        )
                        .child(
                            div()
                                .w(css(56.))
                                .flex_shrink_0()
                                .font_family(theme::MONO)
                                .text_size(css(10.))
                                .text_color(theme::method_color(&found.method, cx))
                                .child(found.method.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(label),
                        )
                        .when(!found.group_path.is_empty(), |row| {
                            row.child(
                                div()
                                    .max_w(relative(0.4))
                                    .flex_shrink_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_color(colors.muted_foreground)
                                    .child(found.group_path.clone()),
                            )
                        })
                        .into_any_element()
                })
                .collect()
        };
        let empty = rows.is_empty();
        div()
            .id("command-center-popover")
            .w(width)
            .overflow_hidden()
            .rounded(css(8.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.secondary)
            .shadow(vec![BoxShadow {
                color: black().opacity(0.4),
                offset: point(px(0.), px(8.)),
                blur_radius: px(24.),
                spread_radius: px(0.),
                inset: false,
            }])
            .text_size(css(12.))
            .occlude()
            .font_family(theme::SANS)
            // A press inside keeps focus in the search field.
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                if this.open {
                    this.hide(window, cx);
                }
            }))
            .capture_action(cx.listener(Self::on_up))
            .capture_action(cx.listener(Self::on_down))
            .capture_action(cx.listener(Self::on_enter))
            .capture_action(cx.listener(Self::on_escape))
            .child(
                div()
                    .h(css(28.))
                    .border_b_1()
                    .border_color(colors.border)
                    .child(
                        Input::new(&self.input)
                            .appearance(false)
                            .h_full()
                            .px(css(10.))
                            .text_size(css(12.))
                            .aria_label("Search requests"),
                    ),
            )
            .child(
                div()
                    .id("command-center-list")
                    .max_h(css(288.))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .py(css(4.))
                    .children(rows),
            )
            .when(empty, |this| {
                this.child(
                    div()
                        .px(css(10.))
                        .pb(css(8.))
                        .text_color(colors.muted_foreground)
                        .child(if command_mode {
                            "No matching commands"
                        } else {
                            "No matching requests"
                        }),
                )
            })
    }

    /// A list row; the selected one has the accent background.
    fn row(&self, i: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let colors = theme::colors(cx);
        div()
            .id(("command-center-option", i))
            .flex()
            .items_center()
            .gap(css(8.))
            .px(css(10.))
            .py(css(4.))
            .cursor_pointer()
            .when(i == self.index, |row| row.bg(colors.accent))
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.index != i {
                    this.index = i;
                    cx.notify();
                }
            }))
    }
}

/// Byte ranges of the characters at `indices`, as `highlight_runs` marks them.
fn highlight_ranges(text: &str, indices: &[usize]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    for run in highlight_runs(text, indices) {
        let end = offset + run.text.len();
        if run.matched {
            ranges.push(offset..end);
        }
        offset = end;
    }
    ranges
}

/// `text` with the characters at `indices` in the match style.
fn highlighted(text: &str, indices: &[usize], style: HighlightStyle) -> StyledText {
    StyledText::new(text.to_string()).with_highlights(
        highlight_ranges(text, indices)
            .into_iter()
            .map(|range| (range, style)),
    )
}

/// `w-[min(420px,50vw)]`.
fn center_width(window: &Window) -> Pixels {
    let zoom = window.rem_size() / px(16.);
    (px(420.) * zoom).min(window.viewport_size().width * 0.5)
}

impl Render for CommandCenter {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = center_width(window);
        // Keep the index in range when the list changes while open.
        if self.open {
            let count = self.choices(cx).len();
            if self.index >= count {
                self.index = count.saturating_sub(1);
            }
        }
        div()
            .relative()
            .w(width)
            .h(css(26.))
            .flex_shrink_0()
            .when(!self.open, |this| this.child(self.render_trigger(cx)))
            .when(self.open, |this| {
                this.child(deferred(anchored().child(self.render_popover(width, cx))).with_priority(1))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn highlights_matched_characters_by_byte_range() {
        assert_eq!(highlight_ranges("héllo", &[1, 2]), vec![1..4]);
        assert_eq!(highlight_ranges("abc", &[0, 2]), vec![0..1, 2..3]);
        assert!(highlight_ranges("abc", &[]).is_empty());
    }
}
