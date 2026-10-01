//! Headless flows: Application Settings opens in its own window, and its
//! theme preview, save, cancel, and reset change the palette and
//! `theme.json` in the data dir.

use blink_core::theme::{DEFAULT_ACCENT, ThemeSetting, ThemeState};
use gpui_kit::component::WindowExt as _;
use gpui_kit::{
    Action, AppContext as _, Bounds, Entity, Hsla, Pixels, TestAppContext, VisualTestContext, px,
    rgb, size,
};

use super::Settings;
use crate::actions::{ManageCookies, OpenGroupSettings, OpenSettings};
use crate::test_support::{self, Harness};
use crate::theme;

fn paper() -> ThemeSetting {
    ThemeSetting {
        name: "Paper".into(),
        text: "background = #ffffff\nforeground = #111111\npalette = 4=#0055ff\n".into(),
        accent: DEFAULT_ACCENT,
    }
}

fn background(cx: &TestAppContext) -> Hsla {
    cx.read(|cx| theme::colors(cx).background)
}

fn preview(cx: &mut TestAppContext, next: Option<ThemeSetting>) {
    cx.update(|cx| theme::update_theme(cx, |theme| theme.state.preview(next)));
}

fn save(harness: &Harness, cx: &mut TestAppContext, settings: &Entity<Settings>) -> bool {
    harness.update(cx, |window, cx| {
        settings.update(cx, |settings, cx| settings.save(window, cx))
    })
}

#[gpui_kit::test]
fn previews_saves_cancels_and_resets_the_theme(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let file = dir.path().join("theme.json");
    let default_background = background(cx);
    assert!(!file.exists());
    assert_eq!(cx.read(theme::theme_name), "Blink");

    // The real settings window opens beside the main window; opening it
    // again brings the same window forward.
    harness.dispatch(cx, OpenSettings);
    assert_eq!(cx.read(|cx| cx.windows().len()), 2);
    assert!(!harness.update(cx, |window, cx| window.has_active_dialog(cx)));
    harness.dispatch(cx, OpenSettings);
    assert_eq!(cx.read(|cx| cx.windows().len()), 2);
    let store = harness.store.clone();
    let settings = harness.update(cx, |window, cx| cx.new(|cx| Settings::new(store, window, cx)));

    // Preview: the palette changes at once; nothing is written.
    preview(cx, Some(paper()));
    let white: Hsla = rgb(0xffffff).into();
    assert_eq!(background(cx), white);
    assert!(!file.exists());

    // Cancel reverts to the saved (default) theme.
    cx.update(|cx| theme::update_theme(cx, |theme| theme.state.revert()));
    assert_eq!(background(cx), default_background);

    // Preview again, then Save writes theme.json.
    preview(cx, Some(paper()));
    assert!(save(&harness, cx, &settings));
    let stored = std::fs::read_to_string(&file).expect("theme.json is written");
    assert_eq!(
        ThemeState::new(Some(&stored)).draft.as_ref().map(|d| d.name.as_str()),
        Some("Paper")
    );
    assert_eq!(cx.read(theme::theme_name), "Paper");
    assert_eq!(background(cx), white);
    // Cancel after saving keeps the saved theme.
    cx.update(|cx| theme::update_theme(cx, |theme| theme.state.revert()));
    assert_eq!(background(cx), white);

    // A restart reads the saved theme.
    cx.update(|cx| theme::init(&engine, cx));
    assert_eq!(background(cx), white);

    // An invalid preview blocks Save and keeps the file.
    preview(
        cx,
        Some(ThemeSetting {
            name: "Custom".into(),
            text: "background = nope".into(),
            accent: DEFAULT_ACCENT,
        }),
    );
    assert!(!cx.read(|cx| cx.global::<theme::AppTheme>().state.error.is_empty()));
    assert!(!save(&harness, cx, &settings));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), stored);

    // Reset to default, then Save removes theme.json.
    preview(cx, None);
    assert_eq!(background(cx), default_background);
    assert!(save(&harness, cx, &settings));
    assert!(!file.exists());
    assert_eq!(cx.read(theme::theme_name), "Blink");
    cx.update(|cx| theme::init(&engine, cx));
    assert_eq!(background(cx), default_background);
}

/// The bounds of the open dialog's surface.
fn dialog_bounds(harness: &Harness, cx: &mut TestAppContext) -> Bounds<Pixels> {
    harness.draw(cx);
    harness.draw(cx);
    let mut visual = VisualTestContext::from_window(harness.window, cx);
    visual.debug_bounds("dialog-0").expect("the dialog is painted")
}

/// As Vue `top-1/2 -translate-y-1/2 max-h-[90dvh]` (`80dvh` for Cookies):
/// each dialog is centered in the window and at most that tall. Application
/// Settings is a window, not a dialog.
#[gpui_kit::test]
fn dialogs_are_centered_and_capped(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    cx.update(|cx| cx.set_reduce_motion(true));
    let harness = test_support::open(cx, &engine);
    let group_id = harness.store.update(cx, |store, cx| {
        let mut id = 0;
        store.update_workspace(cx, |workspace| id = workspace.add_group("API", None));
        id
    });

    for height in [800., 480.] {
        let viewport = size(px(1280.), px(height));
        VisualTestContext::from_window(harness.window, cx).simulate_resize(viewport);
        harness.draw(cx);
        let opens: [(&str, f32, Box<dyn Action>); 2] = [
            ("group", 0.9, Box::new(OpenGroupSettings { group_id })),
            ("cookies", 0.8, Box::new(ManageCookies)),
        ];
        for (name, cap, action) in opens {
            harness.update(cx, |window, cx| window.dispatch_action(action, cx));
            assert!(harness.update(cx, |window, cx| window.has_active_dialog(cx)), "{name}");
            let bounds = dialog_bounds(&harness, cx);
            let middle = bounds.origin.y + bounds.size.height / 2.;
            assert!(
                (middle - viewport.height / 2.).abs() <= px(1.),
                "{name} at {height}: {bounds:?} is not centered"
            );
            assert!(
                bounds.size.height <= viewport.height * cap + px(0.5),
                "{name} at {height}: {bounds:?} is taller than {cap} of the window"
            );
            harness.update(cx, |window, cx| window.close_dialog(cx));
            harness.draw(cx);
            assert!(!harness.update(cx, |window, cx| window.has_active_dialog(cx)));
        }
    }
}
