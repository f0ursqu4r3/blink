#![recursion_limit = "256"]

mod actions;
mod runner;
mod store;
mod theme;
mod ui;

use blink_core::engine::{Engine, WindowBounds as SavedBounds};
use gpui_kit::component::TitleBar;
use gpui_kit::*;

/// Saved bounds when they are still on a display, else a centered default.
fn window_bounds(engine: &Engine, cx: &App) -> WindowBounds {
    let default = size(px(1180.), px(780.));
    let Some(saved) = engine.load_window_state() else {
        return WindowBounds::Windowed(Bounds::centered(None, default, cx));
    };
    let bounds = Bounds::new(
        point(px(saved.x as f32), px(saved.y as f32)),
        size(px(saved.width as f32), px(saved.height as f32)),
    );
    let visible = cx
        .displays()
        .iter()
        .any(|display| display.bounds().intersects(&bounds));
    if !visible {
        WindowBounds::Windowed(Bounds::centered(None, default, cx))
    } else if saved.maximized {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    }
}

pub(crate) fn save_window_state(engine: &Engine, window: &Window) {
    let (bounds, maximized) = match window.window_bounds() {
        WindowBounds::Windowed(bounds) => (bounds, false),
        WindowBounds::Maximized(bounds) | WindowBounds::Fullscreen(bounds) => (bounds, true),
    };
    let _ = engine.save_window_state(&SavedBounds {
        x: f64::from(f32::from(bounds.origin.x)),
        y: f64::from(f32::from(bounds.origin.y)),
        width: f64::from(f32::from(bounds.size.width)),
        height: f64::from(f32::from(bounds.size.height)),
        maximized,
    });
}

fn main() {
    let engine = match Engine::open() {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("Blink cannot start: {error}");
            std::process::exit(1);
        }
    };
    gpui_kit::application()
        // Every Lucide icon, as lucide-vue-next gave the Vue app.
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            actions::init(cx);
            theme::init(&engine, cx);
            let mut titlebar = TitleBar::title_bar_options();
            titlebar.traffic_light_position = Some(ui::title_bar::traffic_light_position());
            let options = WindowOptions {
                titlebar: Some(titlebar),
                window_bounds: Some(window_bounds(&engine, cx)),
                window_min_size: Some(size(px(860.), px(620.))),
                app_id: Some("com.kyle.blink.gpui".into()),
                ..TitleBar::window_options()
            };
            let store = cx.new(|cx| store::Store::new(engine.clone(), cx));
            let quit_store = store.clone();
            let engine = engine.clone();
            gpui_kit::open_window(options, cx, move |window, cx| {
                let view = cx.new(|cx| ui::app::BlinkApp::new(store.clone(), window, cx));
                view.focus_handle(cx).focus(window, cx);
                // Quitting waits for the latest save; a failed save keeps the window.
                let close_store = store.clone();
                let close_engine = engine.clone();
                window.on_window_should_close(cx, move |window, cx| {
                    save_window_state(&close_engine, window);
                    let saved = close_store.update(cx, |store, cx| store.save_before_exit(cx));
                    if saved {
                        cx.quit();
                    }
                    saved
                });
                view
            })
            .expect("failed to open window");
            cx.on_app_quit(move |cx| {
                quit_store.update(cx, |store, cx| {
                    store.save_before_exit(cx);
                });
                async {}
            })
            .detach();
            cx.activate(true);
        });
}
