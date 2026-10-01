//! Headless flow: theme preview, save, cancel, and reset in Application
//! Settings change the palette and `theme.json` in the data dir.

use blink_core::theme::{DEFAULT_ACCENT, ThemeSetting, ThemeState};
use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, Entity, Hsla, TestAppContext, rgb};

use super::Settings;
use crate::actions::OpenSettings;
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

    // The real dialog opens.
    harness.dispatch(cx, OpenSettings);
    assert!(harness.update(cx, |window, cx| window.has_active_dialog(cx)));
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
