//! The 40 px title bar of `App.vue`: Browser toggle, command center, pane
//! layout toggle, and Settings. It doubles as the window drag area.

use gpui_kit::assets::IconName;
use gpui_kit::component::TitleBar;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::theme;
use crate::ui::widgets::icon_button;

pub const HEIGHT: f32 = 40.0;

/// Where macOS draws the traffic lights: centered in the 40 px bar.
pub fn traffic_light_position() -> Point<Pixels> {
    point(px(9.0), px(13.0))
}

type Handler = Box<dyn Fn(&mut Window, &mut App)>;

pub struct TitleBarProps {
    pub browser_visible: bool,
    pub stacked: bool,
    pub command_center: AnyElement,
    pub on_toggle_browser: Handler,
    pub on_toggle_layout: Handler,
    pub on_settings: Handler,
}

pub fn render(props: TitleBarProps, window: &Window, cx: &App) -> AnyElement {
    let colors = theme::colors(cx);
    // The icon shows what a click does, as the label does.
    let (browser_icon, browser_label) = if props.browser_visible {
        (IconName::PanelLeftClose, "Hide request browser")
    } else {
        (IconName::PanelLeftOpen, "Show request browser")
    };
    let layout_label = layout_toggle_label(props.stacked);
    let mac = cfg!(target_os = "macos") && !window.is_fullscreen();
    let TitleBarProps {
        on_toggle_browser,
        on_toggle_layout,
        on_settings,
        ..
    } = props;
    TitleBar::new()
        .h(px(HEIGHT))
        .bg(colors.frame)
        .border_color(colors.frame)
        .pl_0()
        .child(
            div()
                .flex()
                .items_center()
                .size_full()
                // The side columns grow from zero with their spacing inside,
                // so the command center stays centered (`grid-cols-[1fr_auto_1fr]`).
                .child(
                    div().flex_1().flex().justify_start().child(
                        div()
                            .ml_1()
                            // Leave room for the traffic lights.
                            .when(mac, |this| this.pl(px(72.)))
                            .child(
                                icon_button("title-browser", browser_icon, browser_label)
                                    .on_click(move |_, window, cx| on_toggle_browser(window, cx)),
                            ),
                    ),
                )
                .child(props.command_center)
                .child(
                    div().flex_1().flex().justify_end().child(
                        div()
                            .flex()
                            .gap(px(2.))
                            .pr_2()
                            .child(
                                icon_button(
                                    "title-layout",
                                    if props.stacked {
                                        IconName::Rows2
                                    } else {
                                        IconName::Columns2
                                    },
                                    format!(
                                        "{layout_label} · {}",
                                        blink_core::shortcut::shortcut_label(
                                            &["mod", "\\"],
                                            blink_core::shortcut::IS_MAC
                                        )
                                    ),
                                )
                                .on_click(move |_, window, cx| on_toggle_layout(window, cx)),
                            )
                            .child(
                                icon_button(
                                    "title-settings",
                                    IconName::Settings,
                                    format!(
                                        "Application settings · {}",
                                        blink_core::shortcut::shortcut_label(
                                            &["mod", ","],
                                            blink_core::shortcut::IS_MAC
                                        )
                                    ),
                                )
                                .on_click(move |_, window, cx| on_settings(window, cx)),
                            ),
                    ),
                ),
        )
        .into_any_element()
}

/// `layoutToggleLabel` in `App.vue`.
pub fn layout_toggle_label(stacked: bool) -> &'static str {
    if stacked {
        "Place request and response side by side"
    } else {
        "Stack request above response"
    }
}
