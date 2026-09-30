//! The window shell. Port of `App.vue`.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::actions::*;
use crate::theme;
use crate::ui::title_bar::{self, TitleBarProps};

/// Browser card width (`w-61`) and the frame gap (`gap-1.5`).
pub const BROWSER_WIDTH: f32 = 244.0;
pub const FRAME_GAP: f32 = 6.0;
pub const STATUS_BAR_HEIGHT: f32 = 24.0;

pub struct BlinkApp {
    focus_handle: FocusHandle,
    sidebar_collapsed: bool,
    stacked: bool,
}

impl BlinkApp {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        BlinkApp {
            focus_handle: cx.focus_handle(),
            sidebar_collapsed: false,
            stacked: false,
        }
    }

    fn toggle_browser(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        cx.notify();
    }

    fn toggle_layout(&mut self, _: &ToggleLayout, _: &mut Window, cx: &mut Context<Self>) {
        self.stacked = !self.stacked;
        cx.notify();
    }

    fn render_browser(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        div()
            .id("request-browser")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(BROWSER_WIDTH))
            .min_w(px(188.))
            .overflow_hidden()
            .rounded(px(8.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.muted)
    }

    fn render_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .rounded(px(8.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        div()
            .flex()
            .items_center()
            .gap(px(18.))
            .h(px(STATUS_BAR_HEIGHT))
            .flex_shrink_0()
            .px(px(14.))
            .font_family(theme::MONO)
            .text_size(px(9.))
            .text_color(colors.muted_foreground)
            .bg(colors.frame)
    }
}

impl Focusable for BlinkApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BlinkApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let entity = cx.entity();
        let toggle_browser = {
            let entity = entity.clone();
            move |_: &mut Window, cx: &mut App| entity.update(cx, |this, cx| this.toggle_browser(cx))
        };
        let title = title_bar::render(
            TitleBarProps {
                browser_visible: !self.sidebar_collapsed,
                stacked: self.stacked,
                command_center: div().w(px(420.)).into_any_element(),
                on_toggle_browser: Box::new(toggle_browser),
                on_toggle_layout: Box::new(|window, cx| {
                    window.dispatch_action(Box::new(ToggleLayout), cx)
                }),
                on_settings: Box::new(|window, cx| {
                    window.dispatch_action(Box::new(OpenSettings), cx)
                }),
            },
            window,
            cx,
        );
        div()
            .id("blink")
            .key_context(crate::actions::APP_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::toggle_layout))
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.frame)
            .text_color(colors.foreground)
            .font_family(theme::SANS)
            .text_size(px(theme::FONT_SIZE))
            .child(title)
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .gap(px(FRAME_GAP))
                    .px(px(FRAME_GAP))
                    .when(!self.sidebar_collapsed, |this| {
                        this.child(self.render_browser(cx))
                    })
                    .child(self.render_editor(cx)),
            )
            .child(self.render_status_bar(cx))
    }
}
