#![recursion_limit = "256"]

mod actions;
mod runner;
mod store;
#[cfg(test)]
mod test_support;
mod theme;
mod ui;

use blink_core::engine::{Engine, WindowBounds as SavedBounds};
use gpui_kit::component::TitleBar;
use gpui_kit::*;

/// The saved display, found by its stable id.
fn saved_display(uuid: &str, cx: &App) -> Option<std::rc::Rc<dyn PlatformDisplay>> {
    cx.displays()
        .into_iter()
        .find(|display| display.uuid().is_ok_and(|id| id.to_string() == uuid))
}

/// Saved bounds and display when the window is still on that display, else
/// a centered default on the primary display. Bounds are display-relative.
fn window_bounds(engine: &Engine, cx: &App) -> (WindowBounds, Option<DisplayId>) {
    let default = size(px(1180.), px(780.));
    let centered = || (WindowBounds::Windowed(Bounds::centered(None, default, cx)), None);
    let Some(saved) = engine.load_window_state() else {
        return centered();
    };
    let display = match saved.display.as_deref() {
        Some(uuid) => saved_display(uuid, cx),
        None => cx.primary_display(),
    };
    // The display is gone: start centered on the primary one.
    let Some(display) = display else {
        return centered();
    };
    let bounds = Bounds::new(
        point(px(saved.x as f32), px(saved.y as f32)),
        size(px(saved.width as f32), px(saved.height as f32)),
    );
    if !display.bounds().intersects(&bounds) {
        return centered();
    }
    let id = Some(display.id());
    if saved.maximized {
        (WindowBounds::Maximized(bounds), id)
    } else {
        (WindowBounds::Windowed(bounds), id)
    }
}

pub(crate) fn save_window_state(engine: &Engine, window: &Window, cx: &App) {
    let (bounds, maximized) = match window.window_bounds() {
        WindowBounds::Windowed(bounds) => (bounds, false),
        WindowBounds::Maximized(bounds) | WindowBounds::Fullscreen(bounds) => (bounds, true),
    };
    let display = window
        .display(cx)
        .and_then(|display| display.uuid().ok())
        .map(|uuid| uuid.to_string());
    let _ = engine.save_window_state(&SavedBounds {
        x: f64::from(f32::from(bounds.origin.x)),
        y: f64::from(f32::from(bounds.origin.y)),
        width: f64::from(f32::from(bounds.size.width)),
        height: f64::from(f32::from(bounds.size.height)),
        maximized,
        display,
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
            let (bounds, display_id) = window_bounds(&engine, cx);
            let options = WindowOptions {
                titlebar: Some(titlebar),
                window_bounds: Some(bounds),
                display_id,
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
                    save_window_state(&close_engine, window, cx);
                    let saved = close_store.update(cx, |store, cx| store.save_before_exit(cx));
                    if saved {
                        cx.quit();
                    }
                    saved
                });
                view
            })
            .expect("failed to open window");
            // Cmd+Q and Blink > Quit Blink save first, as the window close
            // does. This listener runs before the one in `actions::init`.
            let action_store = quit_store.clone();
            cx.on_action(move |_: &actions::Quit, cx| {
                action_store.update(cx, |store, cx| store.quit(cx));
            });
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
