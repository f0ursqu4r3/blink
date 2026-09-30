mod actions;
mod theme;
mod ui;

use gpui_kit::component::TitleBar;
use gpui_kit::*;

fn main() {
    gpui_kit::application()
        // Every Lucide icon, as lucide-vue-next gave the Vue app.
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx| {
            gpui_kit::init(cx);
            actions::init(cx);
            theme::apply(blink_core::theme::ThemeTokens::default_tokens(), cx);
            let mut titlebar = TitleBar::title_bar_options();
            titlebar.traffic_light_position = Some(ui::title_bar::traffic_light_position());
            let options = WindowOptions {
                titlebar: Some(titlebar),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1280.), px(800.)),
                    cx,
                ))),
                window_min_size: Some(size(px(320.), px(400.))),
                ..TitleBar::window_options()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                let view = cx.new(|cx| ui::app::BlinkApp::new(window, cx));
                view.focus_handle(cx).focus(window, cx);
                view
            })
            .expect("failed to open window");
            cx.activate(true);
        });
}
