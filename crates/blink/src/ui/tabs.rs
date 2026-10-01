//! The request tab strip. Port of `RequestTabs.vue`.

use std::time::Duration;

use blink_core::drag_drop::{
    DragPayload, DropBox, DropZone, Point as DropPoint, RowKind, TabDrop, hit_zone,
    resolve_tab_drop, step_tab,
};
use blink_core::model::{RequestSession, SocketState};
use blink_core::preferences::transport_options;
use blink_core::session::{LabelTokens, display_method, session_host, session_label, session_status};
use blink_core::session_curl::session_curl;
use blink_core::shortcut::{IS_MAC, shortcut_label};
use blink_core::workspace_state::Workspace;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::actions::{DuplicateRequest, NewRequest};
use crate::store::Store;
use crate::theme;
use crate::ui::browser::DragPreview;
use crate::ui::app::NARROW_WIDTH;
use crate::ui::status_bar::css;
use crate::ui::widgets::tracked;

/// Requests being dragged, from the tab strip or the Browser. Both surfaces
/// accept it: the tab strip opens or reorders them, the Browser moves them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraggedRequests {
    pub ids: Vec<u64>,
}

/// What one tab shows, read from the workspace before rendering.
struct TabInfo {
    id: u64,
    method: String,
    label: String,
    host: String,
    status: String,
    busy: bool,
    failed: bool,
    socket_open: bool,
    response_status: Option<u16>,
    stale: bool,
    failed_tests: usize,
}

impl TabInfo {
    fn new(session: &RequestSession, tokens: LabelTokens) -> Self {
        TabInfo {
            id: session.id,
            method: display_method(session).to_string(),
            label: session_label(session, Some(tokens)),
            host: session_host(session, Some(tokens)),
            status: session_status(session),
            busy: session.busy,
            failed: !session.error.is_empty(),
            socket_open: session
                .socket
                .as_ref()
                .is_some_and(|socket| socket.state == SocketState::Open),
            response_status: session.response.as_ref().map(|response| response.status),
            stale: session.stale,
            failed_tests: session
                .test_results
                .as_ref()
                .map_or(0, |results| results.iter().filter(|r| !r.pass).count()),
        }
    }
}

type MenuBuilder = Box<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// Key context of the strip, for the tab keys.
const CONTEXT: &str = "RequestTabs";

/// The editor card's 8 px radius inside its 1 px border.
const CARD_INNER_RADIUS: Pixels = px(7.);

/// Where a corner of `radius` crosses the active tab's 2 px top bar, measured
/// across the bar's middle row, so the bar starts inside the curve.
fn corner_inset(radius: Pixels) -> Pixels {
    let r = f32::from(radius);
    let y = r - 1.;
    px(r - (r * r - y * y).sqrt())
}

pub struct RequestTabs {
    store: Entity<Store>,
    focus_handle: FocusHandle,
    scroll: ScrollHandle,
    /// The drop the pointer is over during a drag.
    drop: Option<TabDrop>,
    /// Ids in the drag, shown faded.
    dragging: Vec<u64>,
    /// The active tab last scrolled into view.
    revealed: Option<u64>,
    _subscriptions: Vec<Subscription>,
}

impl RequestTabs {
    pub fn new(store: Entity<Store>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        RequestTabs {
            store,
            focus_handle: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            drop: None,
            dragging: Vec::new(),
            revealed: None,
            _subscriptions,
        }
    }

    fn update_workspace<R>(&self, cx: &mut App, change: impl FnOnce(&mut Workspace) -> R) -> R {
        self.store
            .update(cx, |store, cx| store.update_workspace(cx, change))
    }

    fn open_ids(&self, cx: &App) -> Vec<u64> {
        self.store.read(cx).workspace.visible_ids()
    }

    /// Focus the strip, as `App.vue` focused the active tab after a close,
    /// a reopen, or a search pick, so the tab keys work next.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle.focus(window, cx);
    }

    fn select(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.update_workspace(cx, |workspace| workspace.select(id));
        self.focus_handle.focus(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        let ids = self.open_ids(cx);
        let Some(active) = self.store.read(cx).workspace.shown_active_id() else {
            return;
        };
        let Some(index) = ids.iter().position(|id| *id == active) else {
            return;
        };
        let key = keystroke.key.as_str();
        if modifiers.alt && (key == "left" || key == "right") {
            cx.stop_propagation();
            let direction = if key == "left" { -1 } else { 1 };
            if let Some(before_id) = step_tab(&ids, active, direction) {
                self.update_workspace(cx, |workspace| workspace.place_tabs(&[active], before_id));
            }
            return;
        }
        if modifiers.alt || modifiers.control || modifiers.platform {
            return;
        }
        let len = ids.len();
        let next = match key {
            "right" => (index + 1) % len,
            "left" => (index + len - 1) % len,
            "home" => 0,
            "end" => len - 1,
            "delete" => {
                cx.stop_propagation();
                self.update_workspace(cx, |workspace| workspace.close(active));
                return;
            }
            _ => return,
        };
        cx.stop_propagation();
        self.select(ids[next], window, cx);
    }

    /// Resolve the drop under the pointer, or none.
    fn hover_drop(
        &mut self,
        ids: &[u64],
        target: Option<(u64, Bounds<Pixels>)>,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let open_ids = self.open_ids(cx);
        let payload = DragPayload::Requests(ids.to_vec());
        let drop = match target {
            Some((target_id, bounds)) => {
                let zone = hit_zone(drop_box(bounds), drop_point(position), RowKind::Tab);
                resolve_tab_drop(&payload, Some(target_id), zone, &open_ids)
            }
            // Past the last tab (over the buttons or empty bar) appends.
            None => resolve_tab_drop(&payload, open_ids.last().copied(), DropZone::After, &open_ids),
        };
        if self.drop != drop || self.dragging != ids {
            self.drop = drop;
            self.dragging = ids.to_vec();
            cx.notify();
        }
    }

    fn commit_drop(&mut self, cx: &mut Context<Self>) {
        self.dragging.clear();
        if let Some(drop) = self.drop.take() {
            self.update_workspace(cx, |workspace| workspace.place_tabs(&drop.ids, drop.before_id));
        }
        cx.notify();
    }

    fn render_tab(
        &self,
        tab: TabInfo,
        active: bool,
        menu: MenuBuilder,
        narrow: bool,
        corner: Option<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = theme::colors(cx);
        let TabInfo {
            id,
            method,
            label,
            host,
            status,
            busy,
            failed,
            socket_open,
            response_status,
            stale,
            failed_tests,
        } = tab;
        let title = format!("{method} {label} · {host} · {status}");
        let drop_zone = self
            .drop
            .as_ref()
            .filter(|drop| drop.key == format!("tab-{id}"))
            .map(|drop| drop.zone);
        let dragged = cx.has_active_drag() && self.dragging.contains(&id);

        let indicator = if busy {
            Some(
                div()
                    .text_size(css(9.))
                    .text_color(colors.primary)
                    .child("↗")
                    .with_animation(
                        ("tab-sending", id),
                        Animation::new(Duration::from_secs(1))
                            .repeat()
                            .with_easing(bounce(ease_in_out)),
                        |this, delta| this.opacity(0.4 + 0.6 * delta),
                    )
                    .into_any_element(),
            )
        } else if failed {
            Some(
                div()
                    .text_size(css(9.))
                    .text_color(colors.destructive)
                    .child("!")
                    .into_any_element(),
            )
        } else if socket_open {
            Some(
                div()
                    .id(("tab-socket", id))
                    .text_size(css(9.))
                    .text_color(colors.success)
                    .tooltip(|window, cx| Tooltip::new("WebSocket connected").build(window, cx))
                    .child("WS")
                    .into_any_element(),
            )
        } else {
            response_status.map(|response_status| {
                div()
                    .text_size(css(9.))
                    .text_color(if response_status >= 400 {
                        colors.destructive
                    } else {
                        colors.success
                    })
                    .when(stale, |this| this.opacity(0.5))
                    .child(status.clone())
                    .into_any_element()
            })
        };

        let preview_method = method.clone();
        let preview_label = label.clone();
        let close_label = label.clone();

        div()
            .id(("request-tab", id))
            .relative()
            .flex()
            .items_stretch()
            .flex_shrink_0()
            .w(css(if narrow { 185. } else { 210. }))
            .border_r_1()
            .border_color(colors.border)
            .text_color(colors.muted_foreground)
            .when_some(corner, |this, radius| this.rounded_tl(radius))
            .when(active, |this| {
                this.bg(colors.secondary).text_color(colors.foreground).child(
                    // The 2 px top border. In the corner tab the card's curve
                    // cuts its start, as the Vue card's `overflow-hidden`
                    // clipped it: the curve of radius r reaches the bar's
                    // bottom edge about r − √(r² − (r − 2)²) from the left.
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .h(px(2.))
                        .bg(colors.primary)
                        .map(|this| match corner {
                            Some(radius) => this.left(corner_inset(radius)).rounded_tl(px(2.)),
                            None => this.left_0(),
                        }),
                )
            })
            .when(!active, |this| this.hover(|style| style.bg(colors.accent)))
            .when(dragged, |this| this.opacity(0.4))
            // The same chip as a Browser drag (`DragPreview.vue`).
            .on_drag(DraggedRequests { ids: vec![id] }, move |_, offset, _, cx| {
                cx.new(|_| DragPreview {
                    label: preview_label.clone(),
                    method: Some(preview_method.clone()),
                    folder: false,
                    offset,
                })
            })
            .on_drag_move(cx.listener(move |this, event: &DragMoveEvent<DraggedRequests>, _, cx| {
                if event.bounds.contains(&event.event.position) {
                    let ids = event.drag(cx).ids.clone();
                    this.hover_drop(&ids, Some((id, event.bounds)), event.event.position, cx);
                }
            }))
            .when_some(drop_zone, |this, zone| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(colors.primary)
                        .map(|this| {
                            if zone == DropZone::Before {
                                this.left_0()
                            } else {
                                this.right(px(-1.))
                            }
                        }),
                )
            })
            .child(
                div()
                    .id(("request-tab-button", id))
                    .flex()
                    .items_center()
                    .gap(css(9.))
                    .pl(css(14.))
                    .pr(css(8.))
                    .min_w_0()
                    .flex_1()
                    .cursor_pointer()
                    .font_family(theme::MONO)
                    .text_size(css(11.))
                    .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _, window, cx| this.select(id, window, cx)))
                    .child(
                        div()
                            .text_size(css(9.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme::method_color(&method, cx))
                            .child(tracked(method.clone(), 0.04)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .min_w_0()
                            .flex_1()
                            .line_height(relative(1.3))
                            .child(truncated(label.clone()))
                            .when(!host.is_empty() && host != label, |this| {
                                this.child(
                                    truncated(host.clone())
                                        .text_size(css(9.))
                                        .text_color(colors.muted_foreground),
                                )
                            }),
                    )
                    .children(indicator)
                    .when(!busy && failed_tests > 0, |this| {
                        let title = format!("{failed_tests} failed tests");
                        this.child(
                            div()
                                .id(("tab-tests-failed", id))
                                .text_size(css(9.))
                                .text_color(colors.destructive)
                                .tooltip(move |window, cx| {
                                    Tooltip::new(title.clone()).build(window, cx)
                                })
                                .child("✕"),
                        )
                    })
                    .when(stale && !busy, |this| {
                        this.child(
                            div()
                                .id(("tab-edited", id))
                                .size(css(6.))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(colors.foreground)
                                .tooltip(|window, cx| {
                                    Tooltip::new("Edited since sent").build(window, cx)
                                }),
                        )
                    }),
            )
            .child(
                div()
                    .id(("request-tab-close", id))
                    .aria_label(format!("Close {close_label}"))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(css(26.))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .text_color(colors.muted_foreground)
                    .hover(|style| style.text_color(colors.foreground).bg(colors.accent))
                    .tooltip(|window, cx| {
                        Tooltip::new(format!("Close tab · {}", blink_core::shortcut::shortcut_label(&["mod", "w"], blink_core::shortcut::IS_MAC))).build(window, cx)
                    })
                    // A press on the close button never starts a drag.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.update_workspace(cx, |workspace| workspace.close(id));
                        this.focus(window, cx);
                    }))
                    .child(Icon::new(IconName::X).size(css(12.))),
            )
            .context_menu(menu)
            .into_any_element()
    }

    /// The tab context menu, in the VS Code order of `RequestTabs.vue`.
    fn tab_menu(&self, id: u64, workspace: &Workspace) -> MenuBuilder {
        let store = self.store.clone();
        let strip = self.focus_handle.clone();
        let all = workspace.visible_ids();
        let others: Vec<u64> = all.iter().copied().filter(|other| *other != id).collect();
        let right: Vec<u64> = all
            .iter()
            .position(|open| *open == id)
            .map(|index| all[index + 1..].to_vec())
            .unwrap_or_default();
        let url = workspace
            .session(id)
            .map(|session| session.draft.url.clone())
            .unwrap_or_default();
        let curl = workspace
            .session(id)
            .map(|session| {
                session_curl(
                    session,
                    &workspace.groups,
                    &workspace.global_definitions,
                    &transport_options(&workspace.preferences),
                )
            })
            .unwrap_or_default();
        Box::new(move |menu, _, _| {
            // Closing from the menu focuses the active tab, as `close` did.
            let change = |change: Box<dyn Fn(&mut Workspace)>| {
                let store = store.clone();
                let strip = strip.clone();
                move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    store.update(cx, |store, cx| store.update_workspace(cx, |w| change(w)));
                    strip.focus(window, cx);
                }
            };
            let copy = |text: String| {
                let store = store.clone();
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    store.update(cx, |store, cx| store.copy(text.clone(), cx))
                }
            };
            let (others, right, all) = (others.clone(), right.clone(), all.clone());
            menu.item(
                shortcut_item("Close", &["mod", "w"])
                    .on_click(change(Box::new(move |w| w.close(id)))),
            )
            .item(
                PopupMenuItem::new("Close others")
                    .disabled(others.is_empty())
                    .on_click(change(Box::new(move |w| w.close_many(&others)))),
            )
            .item(
                PopupMenuItem::new("Close to the right")
                    .disabled(right.is_empty())
                    .on_click(change(Box::new(move |w| w.close_many(&right)))),
            )
            .item(
                PopupMenuItem::new("Close all")
                    .on_click(change(Box::new(move |w| w.close_many(&all)))),
            )
            .separator()
            .item(
                shortcut_item("Duplicate", &["mod", "shift", "d"]).on_click(change(Box::new(
                    move |w| {
                        w.duplicate(Some(id));
                    },
                ))),
            )
            .separator()
            .item(
                PopupMenuItem::new("Copy URL")
                    .disabled(url.is_empty())
                    .on_click(copy(url.clone())),
            )
            .item(
                PopupMenuItem::new("Copy as cURL")
                    .disabled(curl.is_empty())
                    .on_click(copy(curl.clone())),
            )
            .separator()
            .item(
                PopupMenuItem::new("Reveal in Browser").on_click(move |_, window, cx| {
                    window.dispatch_action(Box::new(crate::actions::RevealRequest { id }), cx)
                }),
            )
        })
    }

    fn render_overflow(
        &self,
        hidden: usize,
        tabs: &[(TabInfo, MenuBuilder)],
        active: Option<u64>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = theme::colors(cx);
        let store = self.store.clone();
        let rows: Vec<(u64, String, String)> = tabs
            .iter()
            .map(|(tab, _)| (tab.id, tab.method.clone(), tab.label.clone()))
            .collect();
        Button::new("tab-overflow")
            .ghost()
            .small()
            .h_full()
            .rounded_none()
            .border_x_1()
            .border_color(colors.border)
            .icon(Icon::new(IconName::ChevronDown).size(css(13.)))
            .label(hidden.to_string())
            .font_family(theme::MONO)
            .text_size(css(10.))
            .text_color(colors.muted_foreground)
            .tooltip("Show all open tabs")
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, cx| {
                let colors = theme::colors(cx);
                let mut menu = menu.min_w(px(224.)).max_w(px(320.)).scrollable(true);
                for (id, method, label) in &rows {
                    let id = *id;
                    let label = label.clone();
                    let method = method.clone();
                    let method_color = theme::method_color(&method, cx);
                    let text_color = if Some(id) == active {
                        colors.foreground
                    } else {
                        colors.muted_foreground
                    };
                    let store = store.clone();
                    menu = menu.item(
                        PopupMenuItem::element(move |_, _| {
                            div()
                                .flex()
                                .items_center()
                                .min_w_0()
                                .text_color(text_color)
                                .child(
                                    div()
                                        .w(px(44.))
                                        .flex_shrink_0()
                                        .font_family(theme::MONO)
                                        .text_size(px(9.))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(method_color)
                                        .child(tracked(method.clone(), 0.04)),
                                )
                                .child(truncated(label.clone()))
                        })
                        .on_click(move |_, _, cx| {
                            store.update(cx, |store, cx| {
                                store.update_workspace(cx, |workspace| workspace.select(id))
                            })
                        }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// Scroll the tab at `index` into view. False until the strip and the
    /// tab have a layout.
    fn reveal(&self, index: usize) -> bool {
        let viewport = self.scroll.bounds();
        let Some(cell) = self.scroll.bounds_for_item(index) else {
            return false;
        };
        if viewport.size.width <= px(0.) {
            return false;
        }
        let mut offset = self.scroll.offset();
        if cell.left() < viewport.left() {
            offset.x += viewport.left() - cell.left();
        } else if cell.right() > viewport.right() {
            offset.x -= cell.right() - viewport.right();
        }
        offset.x = offset.x.min(px(0.)).max(-self.scroll.max_offset().x);
        self.scroll.set_offset(offset);
        true
    }

    /// Hidden tab count and whether each side has hidden tabs, from the last layout.
    fn overflow(&self, count: usize) -> (bool, bool, usize) {
        let viewport = self.scroll.bounds();
        let offset = -self.scroll.offset().x;
        let max = self.scroll.max_offset().x;
        let left = offset > px(1.);
        let right = offset < max - px(1.);
        let start = viewport.left();
        let end = viewport.right();
        let hidden = (0..count)
            .filter_map(|ix| self.scroll.bounds_for_item(ix))
            .filter(|cell| cell.left() < start - px(1.) || cell.right() > end + px(1.))
            .count();
        (left, right, hidden)
    }
}

fn drop_box(bounds: Bounds<Pixels>) -> DropBox {
    DropBox {
        left: f64::from(f32::from(bounds.left())),
        top: f64::from(f32::from(bounds.top())),
        width: f64::from(f32::from(bounds.size.width)),
        height: f64::from(f32::from(bounds.size.height)),
    }
}

fn drop_point(position: Point<Pixels>) -> DropPoint {
    DropPoint {
        x: f64::from(f32::from(position.x)),
        y: f64::from(f32::from(position.y)),
    }
}

fn truncated(text: String) -> Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(text)
}

/// A menu item with its shortcut on the right, as `ContextMenuShortcut`.
fn shortcut_item(label: &'static str, keys: &'static [&'static str]) -> PopupMenuItem {
    PopupMenuItem::element(move |_, cx| {
        let colors = theme::colors(cx);
        div()
            .flex()
            .items_center()
            .w_full()
            .gap(px(16.))
            .child(div().flex_1().child(label))
            .child(
                div()
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child(shortcut_label(keys, IS_MAC)),
            )
    })
}

/// A square bar button: `+` and duplicate.
fn bar_button(
    id: &'static str,
    icon: IconName,
    size: f32,
    tooltip: impl Into<SharedString>,
    cx: &App,
) -> Stateful<Div> {
    let tooltip: SharedString = tooltip.into();
    let colors = theme::colors(cx);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(css(38.))
        .border_r_1()
        .border_color(colors.border)
        .cursor_pointer()
        .text_color(colors.muted_foreground)
        .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .child(Icon::new(icon).size(css(size)))
}

impl Focusable for RequestTabs {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RequestTabs {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let narrow = window.viewport_size().width <= px(NARROW_WIDTH);
        if !cx.has_active_drag() && (self.drop.is_some() || !self.dragging.is_empty()) {
            self.drop = None;
            self.dragging.clear();
        }
        let store = self.store.clone();
        let workspace = &store.read(cx).workspace;
        let active = workspace.shown_active_id();
        let tokens = LabelTokens {
            groups: &workspace.groups,
            global_definitions: &workspace.global_definitions,
        };
        let infos: Vec<(TabInfo, MenuBuilder)> = workspace
            .open_sessions()
            .into_iter()
            .map(|session| (TabInfo::new(session, tokens), self.tab_menu(session.id, workspace)))
            .collect();
        let all = workspace.visible_ids();
        // Scroll the active tab into view when it changes, once the strip
        // has a layout.
        if active != self.revealed {
            let index = active.and_then(|id| all.iter().position(|open| *open == id));
            if index.is_none_or(|index| self.reveal(index)) {
                self.revealed = active;
            } else {
                cx.notify();
            }
        }
        let (fade_left, fade_right, hidden) = self.overflow(infos.len());
        let overflow = if hidden > 0 { Some(self.render_overflow(hidden, &infos, active, cx)) } else { None };
        let tabs: Vec<AnyElement> = infos
            .into_iter()
            .enumerate()
            .map(|(index, (tab, menu))| {
                let selected = Some(tab.id) == active;
                // The first tab sits in the card's rounded top-left corner.
                // GPUI clips children to a rectangle, not to the card radius
                // as the Vue `overflow-hidden` did, so round the tab itself.
                let corner = (index == 0 && !narrow).then_some(CARD_INNER_RADIUS);
                self.render_tab(tab, selected, menu, narrow, corner, cx)
            })
            .collect();
        let strip_menu_store = self.store.clone();
        let fade = |left: bool| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(24.))
                .map(|this| if left { this.left_0() } else { this.right_0() })
                .bg(linear_gradient(
                    if left { 90. } else { 270. },
                    linear_color_stop(colors.muted, 0.),
                    linear_color_stop(colors.muted.opacity(0.), 1.),
                ))
        };

        div()
            .id("tab-bar")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .min_w_0()
            .flex_shrink_0()
            .h(css(36.))
            .bg(colors.muted)
            .border_b_1()
            .border_color(colors.border)
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<DraggedRequests>, _, cx| {
                let position = event.event.position;
                if !event.bounds.contains(&position) {
                    if this.drop.is_some() {
                        this.drop = None;
                        cx.notify();
                    }
                    return;
                }
                // Over a tab, the tab resolves the drop.
                let over_tab = (0..this.open_ids(cx).len())
                    .filter_map(|ix| this.scroll.bounds_for_item(ix))
                    .any(|cell| cell.contains(&position) && this.scroll.bounds().contains(&position));
                if !over_tab {
                    let ids = event.drag(cx).ids.clone();
                    this.hover_drop(&ids, None, position, cx);
                }
            }))
            .on_drop(cx.listener(|this, _: &DraggedRequests, _, cx| this.commit_drop(cx)))
            .child(
                div()
                    .relative()
                    .flex()
                    .min_w_0()
                    .child(
                        div()
                            .id("tab-strip")
                            .flex()
                            .min_w_0()
                            .overflow_x_scroll()
                            .track_scroll(&self.scroll)
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .children(tabs),
                    )
                    .when(fade_left, |this| this.child(fade(true)))
                    .when(fade_right, |this| this.child(fade(false))),
            )
            .children(overflow)
            .child(
                bar_button(
                    "tab-new-request",
                    IconName::Plus,
                    15.,
                    format!("New request · {}", blink_core::shortcut::shortcut_label(&["mod", "t"], blink_core::shortcut::IS_MAC)),
                    cx,
                )
                .aria_label("New request")
                .on_click(|_, window, cx| window.dispatch_action(Box::new(NewRequest), cx)),
            )
            .child(
                bar_button(
                    "tab-duplicate-request",
                    IconName::CopyPlus,
                    14.,
                    format!("Duplicate request · {}", blink_core::shortcut::shortcut_label(&["mod", "shift", "d"], blink_core::shortcut::IS_MAC)),
                    cx,
                )
                .aria_label("Duplicate request")
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(DuplicateRequest), cx)
                }),
            )
            .child(
                div()
                    .id("tab-strip-empty")
                    .flex_1()
                    .h_full()
                    .context_menu(move |menu, _, _| {
                        let store = strip_menu_store.clone();
                        let all = all.clone();
                        menu.item(
                            shortcut_item("New request", &["mod", "t"]).on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(NewRequest), cx)
                            }),
                        )
                        .item(
                            PopupMenuItem::new("Close all")
                                .disabled(all.is_empty())
                                .on_click(move |_, _, cx| {
                                    store.update(cx, |store, cx| {
                                        store.update_workspace(cx, |w| w.close_many(&all))
                                    })
                                }),
                        )
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn converts_bounds_for_the_drop_rules() {
        let bounds = Bounds::new(point(px(10.), px(0.)), size(px(200.), px(36.)));
        let before = hit_zone(drop_box(bounds), drop_point(point(px(20.), px(5.))), RowKind::Tab);
        let after = hit_zone(drop_box(bounds), drop_point(point(px(150.), px(5.))), RowKind::Tab);
        assert_eq!(before, DropZone::Before);
        assert_eq!(after, DropZone::After);
    }
}
