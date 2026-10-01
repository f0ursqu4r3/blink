//! The 24 px status bar of `App.vue`, and `WorkspaceStorageNotice.vue`.

use blink_core::command_center::group_path;
use blink_core::preferences::transport_options;
use blink_core::request::format_bytes;
use blink_core::session::display_method;
use blink_core::shortcut::{IS_MAC, shortcut_label};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::actions::{UndoDelete, ZoomReset};
use crate::store::Store;
use crate::theme;
use crate::ui::app::NARROW_WIDTH;
use crate::ui::widgets::{Tracked, tracked};

/// The footer's `tracking-[0.07em]`, inherited by every label in it.
fn t(text: impl Into<SharedString>) -> Tracked {
    tracked(text, 0.07)
}

/// Local storage tooltip, word for word from `App.vue`.
pub const STORAGE_TOOLTIP: &str =
    "Saved on this device, including credentials and response content. Not encrypted.";

/// A size in CSS pixels at zoom 1. It scales with the root rem size, as the
/// webview zoom scales the Vue layout.
pub(crate) fn css(value: f32) -> Rems {
    rems(value / 16.0)
}

/// `N REQUEST(S)`.
pub fn request_count_label(count: usize) -> String {
    format!(
        "{count} {}",
        if count == 1 { "REQUEST" } else { "REQUESTS" }
    )
}

/// The right side: `PROXY · 30 s TIMEOUT · 4 MiB LIMIT`.
pub fn transport_label(proxy: bool, timeout_seconds: u64, limit_mib: u64) -> String {
    format!(
        "{}{timeout_seconds} s TIMEOUT · {limit_mib} MiB LIMIT",
        if proxy { "PROXY · " } else { "" }
    )
}

/// The status bar for the current store state.
pub fn render(store: &Entity<Store>, window: &mut Window, cx: &mut App) -> AnyElement {
    let colors = theme::colors(cx);
    let narrow = window.viewport_size().width <= px(NARROW_WIDTH);
    let state = store.read(cx);
    let workspace = &state.workspace;
    let preferences = &workspace.preferences;
    let transport = transport_options(preferences);
    let sending = workspace.sessions.iter().filter(|s| s.busy).count();
    let zoom = preferences.zoom;
    let deletion = workspace.deletion_label();

    let summary = workspace.active().filter(|_| !narrow).map(|active| {
        let method = display_method(active).to_string();
        let mut row = div().flex().min_w_0().items_center().gap(css(6.)).child(
            div()
                .text_color(theme::method_color(&method, cx))
                .child(t(method)),
        );
        if active.busy {
            row = row.child(t(format!("· {:.1} s", active.elapsed / 1000.0)));
        } else if !active.error.is_empty() {
            row = row.child(div().text_color(colors.destructive).child(t("· FAILED")));
        } else if let Some(response) = &active.response {
            row = row
                .child(t("·"))
                .child(
                    div()
                        .text_color(theme::status_color(response.status, cx))
                        .child(t(response.status.to_string())),
                )
                .child(t(format!(
                    "· {} ms · {}",
                    response.duration_ms,
                    format_bytes(response.size_bytes)
                )))
                .when(active.stale, |row| row.child(t("· EDITED")));
        }
        let path = group_path(&workspace.groups, active.group_id);
        if !path.is_empty() {
            row = row.child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(t(format!("· {}", path.to_uppercase()))),
            );
        }
        if let Some(environment) = workspace.active_environment() {
            row = row.child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::environment_color(environment.color, cx))
                    .child(t(format!("· {}", environment.name))),
            );
        }
        row
    });

    let import = (!state.import_notice.is_empty()).then(|| {
        let notice = state.import_notice.clone();
        let details = [state.import_notice.as_str(), state.import_details.as_str()]
            .iter()
            .filter(|part| !part.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        div()
            .id("status-import")
            .max_w(css(400.))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .when(state.import_failed, |this| {
                this.text_color(colors.destructive)
            })
            .tooltip(move |window, cx| Tooltip::new(details.clone()).build(window, cx))
            .child(t(notice))
    });

    let undo = (!deletion.is_empty()).then(|| {
        let store = store.clone();
        let undo_title = format!("Undo · {}", shortcut_label(&["mod", "z"], IS_MAC));
        div()
            .flex()
            .items_center()
            .gap(css(8.))
            .child(
                div()
                    .max_w(css(240.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(t(deletion)),
            )
            .child(
                div()
                    .id("status-undo")
                    .test_support()
                    .cursor_pointer()
                    .text_color(colors.foreground)
                    .tooltip(move |window, cx| Tooltip::new(undo_title.clone()).build(window, cx))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(UndoDelete), cx))
                    // `underline decoration-dotted underline-offset-3`.
                    .child(t("UNDO").dotted_underline(None)),
            )
            .child(
                div()
                    .id("status-undo-dismiss")
                    .cursor_pointer()
                    .hover(|style| style.text_color(colors.foreground))
                    .tooltip(|window, cx| Tooltip::new("Dismiss").build(window, cx))
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.update_workspace(cx, |workspace| workspace.discard_deletion())
                        })
                    })
                    .child(t("×")),
            )
    });

    let zoom_button = (zoom != 1.0).then(|| {
        let title = format!("Reset zoom · {}", shortcut_label(&["mod", "0"], IS_MAC));
        div()
            .id("status-zoom")
            .cursor_pointer()
            .hover(|style| style.text_color(colors.foreground))
            .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
            .on_click(|_, window, cx| window.dispatch_action(Box::new(ZoomReset), cx))
            .child(t(format!("{}%", (zoom * 100.0).round())))
    });

    div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .min_h(css(24.))
        .gap(css(if narrow { 12. } else { 18. }))
        .px(css(if narrow { 12. } else { 14. }))
        .when(narrow, |this| this.flex_wrap().py(css(8.)))
        .font_family(theme::MONO)
        .text_size(css(9.))
        .text_color(colors.muted_foreground)
        .bg(colors.frame)
        .child(
            div()
                .id("status-storage")
                .flex()
                .items_center()
                .gap(css(6.))
                .hover(|style| style.text_color(colors.foreground))
                .tooltip(|window, cx| Tooltip::new(STORAGE_TOOLTIP).build(window, cx))
                .child(Icon::new(IconName::HardDrive).size(css(11.)))
                .child(t(state.status())),
        )
        .child(t(request_count_label(workspace.sessions.len())))
        .children(summary)
        .when(sending > 0, |this| {
            this.child(
                div()
                    .text_color(colors.primary)
                    .child(t(format!("{sending} SENDING"))),
            )
        })
        .when(state.copied, |this| this.child(t("COPIED")))
        .children(import)
        .children(undo)
        .when(!transport.verify_tls, |this| {
            this.child(
                div()
                    .id("status-tls")
                    .test_support()
                    .aria_label("TLS VERIFY OFF")
                    .text_color(colors.warning)
                    .child(t("TLS VERIFY OFF")),
            )
        })
        .child(div().flex_1())
        .when(!narrow, |this| {
            this.child(t(transport_label(
                !transport.proxy_url.is_empty(),
                transport.timeout_seconds,
                transport.inspection_limit_mi_b,
            )))
        })
        .children(zoom_button)
        .child(t(theme::theme_name(cx)))
        .into_any_element()
}

/// A `Button.vue` button: `h-7 px-2.5 font-mono text-xs font-medium`,
/// secondary (bordered) or ghost.
fn notice_button(id: &'static str, label: &'static str, secondary: bool) -> Button {
    // XSmall sets the label to `text-xs`; the label ignores `text_size`.
    let button = Button::new(id)
        .label(label)
        .xsmall()
        .h(css(28.))
        .px(css(10.))
        .font_family(theme::MONO)
        .font_weight(FontWeight::MEDIUM);
    if secondary {
        button.outline()
    } else {
        button.ghost()
    }
}

/// Local state of the storage notice: the Start fresh confirmation.
struct NoticeState {
    confirm_reset: bool,
}

/// The storage notice above the workspace: load and save failures.
pub fn render_storage_notice(
    store: &Entity<Store>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let colors = theme::colors(cx);
    let state = window.use_keyed_state("storage-notice", cx, |_, _| NoticeState {
        confirm_reset: false,
    });
    let (error, ready, exit_blocked) = {
        let store = store.read(cx);
        (store.error.clone(), store.ready, store.exit_blocked)
    };
    if error.is_empty() {
        return div().into_any_element();
    }
    let confirm_reset = state.read(cx).confirm_reset;
    let set_confirm = |value: bool| {
        let state = state.clone();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            state.update(cx, |state, cx| {
                state.confirm_reset = value;
                cx.notify();
            })
        }
    };
    let retry = {
        let store = store.clone();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            store.update(cx, |store, cx| store.retry(cx))
        }
    };
    let replace = {
        let store = store.clone();
        let state = state.clone();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            store.update(cx, |store, cx| store.reset(cx));
            state.update(cx, |state, cx| {
                state.confirm_reset = false;
                cx.notify();
            })
        }
    };
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(css(8.))
        .px(css(16.))
        .py(css(8.))
        .border_b_1()
        .border_color(colors.border)
        .text_color(colors.destructive)
        .text_size(css(12.))
        .child(div().flex_1().min_w(css(180.)).child(if ready {
            format!("{error} Changes are not saved.")
        } else {
            error
        }))
        .child(notice_button("storage-retry", "Retry", true).on_click(retry))
        .when(!ready && !confirm_reset, |this| {
            this.child(
                notice_button("storage-start-fresh", "Start fresh", false)
                    .on_click(set_confirm(true)),
            )
        })
        .when(!ready && confirm_reset, |this| {
            this.child("Replace the existing saved workspace?")
                .child(
                    notice_button("storage-replace", "Replace saved workspace", true)
                        .on_click(replace),
                )
                .child(
                    notice_button("storage-cancel-reset", "Cancel", false)
                        .on_click(set_confirm(false)),
                )
        })
        .when(exit_blocked, |this| {
            this.child(
                notice_button("storage-quit", "Quit without saving", false).on_click({
                    let store = store.clone();
                    move |_, _, cx| store.update(cx, |store, cx| store.quit_without_saving(cx))
                }),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod ui_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn counts_requests_in_singular_and_plural() {
        assert_eq!(request_count_label(1), "1 REQUEST");
        assert_eq!(request_count_label(0), "0 REQUESTS");
        assert_eq!(request_count_label(3), "3 REQUESTS");
    }

    #[test]
    fn labels_the_transport() {
        assert_eq!(transport_label(false, 30, 4), "30 s TIMEOUT · 4 MiB LIMIT");
        assert_eq!(
            transport_label(true, 10, 8),
            "PROXY · 10 s TIMEOUT · 8 MiB LIMIT"
        );
    }
}
