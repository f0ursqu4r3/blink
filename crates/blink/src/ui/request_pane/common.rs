//! Pieces shared by the request pane views: draft edits, the native-select
//! stand-in, and help links.

use std::rc::Rc;

use blink_core::authorization::ResolvedRequestContext;
use blink_core::model::{Draft, RequestSession};
use blink_core::runner::{refresh_stale, request_context};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable as _, Icon, Sizable as _};
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
    store.update(cx, |_, cx| cx.emit(crate::store::StoreEvent::DraftReplaced(id)));
}

/// The session and its resolved context, read from the store.
pub fn session_context(store: &Store, id: u64) -> Option<(RequestSession, ResolvedRequestContext)> {
    let workspace = &store.workspace;
    let session = workspace.session(id)?;
    let ctx = request_context(session, &workspace.groups, &workspace.global_definitions);
    Some((session.clone(), ctx))
}

/// A compact select: the current label and a chevron, opening a checked
/// list. Stands in for the Vue native `<select>`.
pub fn select_button<T: Copy + PartialEq + 'static>(
    id: impl Into<ElementId>,
    options: Vec<(T, SharedString)>,
    current: T,
    disabled: bool,
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    select(id, options, current, disabled, on_select, false)
}

/// A select that fills a table cell without a border.
pub fn cell_select<T: Copy + PartialEq + 'static>(
    id: impl Into<ElementId>,
    options: Vec<(T, SharedString)>,
    current: T,
    disabled: bool,
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    select(id, options, current, disabled, on_select, true)
}

/// A compact select: the current label and a chevron, opening a checked
/// list. Stands in for the Vue native `<select>`.
fn select<T: Copy + PartialEq + 'static>(
    id: impl Into<ElementId>,
    options: Vec<(T, SharedString)>,
    current: T,
    disabled: bool,
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
    borderless: bool,
) -> impl IntoElement {
    let label = options
        .iter()
        .find(|(value, _)| *value == current)
        .map(|(_, label)| label.clone())
        .unwrap_or_default();
    let on_select = Rc::new(on_select);
    Button::new(id)
        .when(borderless, |this| this.ghost())
        .when(!borderless, |this| this.outline())
        .small()
        .disabled(disabled)
        .font_family(theme::MONO)
        .text_size(px(12.))
        .child(div().text_size(px(12.)).child(label))
        .child(Icon::new(IconName::ChevronDown).size(px(12.)))
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
        })
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
        .underline()
        .text_decoration_color(colors.muted_foreground.opacity(0.7))
        .cursor_default()
        .child(label.into())
        .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
}
