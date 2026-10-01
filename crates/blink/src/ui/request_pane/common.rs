//! Pieces shared by the request pane views: draft edits, the native-select
//! stand-in, and help links.

use std::rc::Rc;

use blink_core::authorization::ResolvedRequestContext;
use blink_core::model::{Draft, RequestSession};
use blink_core::runner::{refresh_stale, request_context};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::Store;
use crate::theme;

/// Change a request draft as the user typed it, then refresh its stale flag
/// so EDITED shows. Never emits `DraftReplaced`: the editor that made the
/// change already shows it.
pub fn edit_draft(store: &Entity<Store>, id: u64, cx: &mut App, change: impl FnOnce(&mut Draft)) {
    store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            let Some(index) = workspace.sessions.iter().position(|s| s.id == id) else {
                return;
            };
            let session = &mut workspace.sessions[index];
            change(&mut session.draft);
            refresh_stale(session, &workspace.groups, &workspace.global_definitions);
        })
    });
}

/// Replace a draft from outside its inputs (cURL import, format): change it,
/// then tell every editor of the request to reload.
pub fn replace_draft(
    store: &Entity<Store>,
    id: u64,
    cx: &mut App,
    change: impl FnOnce(&mut Draft),
) {
    edit_draft(store, id, cx, change);
    store.update(cx, |_, cx| {
        cx.emit(crate::store::StoreEvent::DraftReplaced(id))
    });
}

/// The session and its resolved context, read from the store.
pub fn session_context(store: &Store, id: u64) -> Option<(RequestSession, ResolvedRequestContext)> {
    let workspace = &store.workspace;
    let session = workspace.session(id)?;
    let ctx = request_context(session, &workspace.groups, &workspace.global_definitions);
    Some((session.clone(), ctx))
}

/// How a select stand-in sits in its layout.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SelectLook {
    /// A bordered native select as wide as its widest option.
    Native,
    /// A bordered native select that fills its container.
    Fill,
    /// A borderless select that fills a table cell.
    Cell,
}

/// The width of a native `<select>` for `options`: the widest label in
/// 12 px monospace, its padding, and the chevron.
fn native_width<T>(options: &[(T, SharedString)]) -> f32 {
    let chars = options
        .iter()
        .map(|(_, label)| label.chars().count())
        .max()
        .unwrap_or(0);
    chars as f32 * 7.2 + 46.
}

/// A native-looking select (`h-7 px-2 font-mono text-xs`, bordered): the
/// current label and a chevron, opening a checked list. Stands in for the
/// Vue native `<select>`, as wide as its widest option.
pub fn select_button<T: Copy + PartialEq + 'static>(
    id: impl Into<ElementId>,
    options: Vec<(T, SharedString)>,
    current: T,
    disabled: bool,
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    select(
        id,
        options,
        current,
        disabled,
        on_select,
        cx,
        SelectLook::Native,
    )
}

/// A native-looking select that fills its container (a grid column).
pub fn fill_select<T: Copy + PartialEq + 'static>(
    id: impl Into<ElementId>,
    options: Vec<(T, SharedString)>,
    current: T,
    disabled: bool,
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    select(
        id,
        options,
        current,
        disabled,
        on_select,
        cx,
        SelectLook::Fill,
    )
}

/// A select that fills a table cell without a border
/// (`h-8 px-1.5 text-xs`, sans).
pub fn cell_select<T: Copy + PartialEq + 'static>(
    id: impl Into<ElementId>,
    options: Vec<(T, SharedString)>,
    current: T,
    disabled: bool,
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    select(
        id,
        options,
        current,
        disabled,
        on_select,
        cx,
        SelectLook::Cell,
    )
}

fn select<T: Copy + PartialEq + 'static>(
    id: impl Into<ElementId>,
    options: Vec<(T, SharedString)>,
    current: T,
    disabled: bool,
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
    cx: &App,
    look: SelectLook,
) -> impl IntoElement {
    let colors = theme::colors(cx);
    let label = options
        .iter()
        .find(|(value, _)| *value == current)
        .map(|(_, label)| label.clone())
        .unwrap_or_default();
    let width = native_width(&options);
    let on_select = Rc::new(on_select);
    let button = Button::new(id)
        .ghost()
        .xsmall()
        .disabled(disabled)
        .rounded(px(4.))
        .map(|this| match look {
            SelectLook::Cell => this
                .w_full()
                .h(css(32.))
                .px(css(6.))
                .rounded(px(0.))
                .text_color(colors.foreground)
                .font_family(theme::SANS),
            SelectLook::Native | SelectLook::Fill => this
                .h(css(28.))
                .px(css(8.))
                .border_1()
                .border_color(colors.input)
                .bg(colors.muted)
                .text_color(colors.foreground)
                .font_family(theme::MONO)
                .when(look == SelectLook::Native, |this| this.w(css(width)))
                .when(look == SelectLook::Fill, |this| this.w_full()),
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_left()
                .overflow_hidden()
                .child(label),
        )
        .child(Icon::new(IconName::ChevronDown).size(css(14.)))
        .dropdown_menu(move |menu, _, _| {
            options.iter().fold(menu, |menu, (value, label)| {
                let on_select = on_select.clone();
                let value = *value;
                menu.item(
                    PopupMenuItem::new(label.clone())
                        .checked(value == current)
                        .on_click(move |_, window, cx| on_select(value, window, cx)),
                )
            })
        });
    // A column parent stretches the popover trigger, so the button can fill it.
    div()
        .flex()
        .flex_col()
        .when(look != SelectLook::Native, |this| this.flex_1().min_w_0())
        .child(button)
}

/// Tailwind sizes are rem based: they scale with the app zoom.
pub fn css(value: f32) -> Rems {
    rems(value / 16.)
}

/// Underlined help text with a tooltip. `HelpTooltip.vue`.
pub fn help_link(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    text: impl Into<SharedString>,
    cx: &App,
) -> Stateful<Div> {
    let colors = theme::colors(cx);
    let text: SharedString = text.into();
    div()
        .id(id)
        .text_size(px(11.))
        .text_color(colors.muted_foreground)
        .cursor_default()
        // `underline decoration-dotted underline-offset-3`.
        .child(crate::ui::widgets::dotted(label.into()))
        .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
}
