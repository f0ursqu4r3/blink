//! Headless settings flows: immediate application, persistence, and theme previews.

use blink_core::theme::{DEFAULT_ACCENT, ThemeSetting, ThemeState};
use gpui_kit::component::WindowExt as _;
use gpui_kit::{
    Action, AppContext as _, Bounds, Hsla, Pixels, TestAppContext, VisualTestContext, px, rgb, size,
};

use super::Settings;
use crate::actions::{ManageCookies, OpenGroupSettings, OpenSettings};
use crate::test_support::{self, Harness};
use crate::theme;

#[gpui_kit::test]
fn edits_apply_without_save_and_invalid_fields_do_not_block_other_settings(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let settings = harness.update(cx, |window, cx| {
        cx.new(|cx| Settings::new(harness.store.clone(), window, cx))
    });
    let (timeout, proxy) = cx.read(|cx| {
        let settings = settings.read(cx);
        (settings.numbers[0].1.clone(), settings.proxy_url.clone())
    });
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| workspace.preferences.zoom = 1.25);
    });
    let redirects = cx.read(|cx| settings.read(cx).numbers[2].1.clone());
    harness.update(cx, |window, cx| {
        redirects.update(cx, |input, cx| input.set_value("", window, cx));
    });
    harness.update(cx, |window, cx| {
        timeout.update(cx, |input, cx| {
            input.set_value("42", window, cx);
            cx.emit(gpui_kit::component::input::InputEvent::Change);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| harness
            .store
            .read(cx)
            .workspace
            .preferences
            .transport
            .timeout_seconds),
        42
    );
    harness.update(cx, |window, cx| {
        timeout.update(cx, |input, cx| {
            input.set_value("", window, cx);
            cx.emit(gpui_kit::component::input::InputEvent::Change);
        });
        proxy.update(cx, |input, cx| {
            input.set_value("http://localhost:8080", window, cx);
            cx.emit(gpui_kit::component::input::InputEvent::Change);
        });
    });
    cx.run_until_parked();
    let saved = cx.read(|cx| harness.store.read(cx).workspace.preferences.clone());
    assert_eq!(saved.transport.timeout_seconds, 42);
    assert_eq!(saved.transport.proxy_url, "http://localhost:8080");
    assert_eq!(saved.zoom, 1.25);
}

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

#[gpui_kit::test]
fn theme_edits_persist_immediately_and_hover_does_not_save(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let file = dir.path().join("theme.json");
    let default_background = background(cx);
    harness.dispatch(cx, OpenSettings);
    assert_eq!(cx.read(|cx| cx.windows().len()), 2);
    harness.dispatch(cx, OpenSettings);
    assert_eq!(cx.read(|cx| cx.windows().len()), 2);
    let settings = harness.update(cx, |window, cx| {
        cx.new(|cx| Settings::new(harness.store.clone(), window, cx))
    });
    let editor = cx.read(|cx| settings.read(cx).theme.clone());
    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| editor.edit(paper().text, window, cx));
    });
    let white: Hsla = rgb(0xffffff).into();
    assert_eq!(background(cx), white);
    let stored = std::fs::read_to_string(&file).expect("valid edits persist without Save");
    assert_eq!(ThemeState::new(Some(&stored)).name(), "Custom");

    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| editor.hover(String::new(), window, cx));
    });
    assert_eq!(background(cx), default_background);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), stored);
    editor.update(cx, |editor, cx| editor.end_preview(cx));
    assert_eq!(background(cx), white);

    // An invalid custom color keeps the last valid palette and persisted file.
    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| {
            editor.edit("background = nope".into(), window, cx)
        });
    });
    assert!(!cx.read(|cx| cx.global::<theme::AppTheme>().state.error.is_empty()));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), stored);
    assert_eq!(background(cx), white);
    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| editor.reset(window, cx));
    });
    assert!(!file.exists());
    assert_eq!(background(cx), default_background);
    cx.update(|cx| theme::init(&engine, cx));
    assert_eq!(background(cx), default_background);
}

#[gpui_kit::test]
fn text_tokens_keep_invalid_rows_and_apply_other_edits(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let settings = harness.update(cx, |window, cx| {
        cx.new(|cx| Settings::new(harness.store.clone(), window, cx))
    });
    let editor = cx.read(|cx| settings.read(cx).response_tokens.clone());
    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| editor.add_row(window, cx));
    });
    let mut host = blink_core::request::pair("host", "https://example.test");
    let mut key = blink_core::request::pair("key", "one");
    let tokens = cx.read(|cx| settings.read(cx).tokens.clone());
    tokens.update(cx, |_, cx| {
        cx.emit(crate::ui::key_value_editor::KeyValueEvent::Change(vec![
            host.clone(),
            key.clone(),
        ]))
    });
    cx.run_until_parked();
    host.key = "_invalid".into();
    key.value = "two".into();
    tokens.update(cx, |_, cx| {
        cx.emit(crate::ui::key_value_editor::KeyValueEvent::Change(vec![
            host.clone(),
            key.clone(),
        ]))
    });
    cx.run_until_parked();
    let saved = cx.read(|cx| harness.store.read(cx).workspace.global_definitions.clone());
    assert_eq!(saved["host"], "https://example.test");
    assert_eq!(saved["key"], "two");
    assert!(!saved.contains_key("_invalid"));
    // A later row can free the name of an earlier pending row.
    host.key = "key".into();
    for next_key in ["key", "third"] {
        key.key = next_key.into();
        tokens.update(cx, |_, cx| {
            cx.emit(crate::ui::key_value_editor::KeyValueEvent::Change(vec![
                host.clone(),
                key.clone(),
            ]));
        });
        cx.run_until_parked();
    }
    let saved = cx.read(|cx| harness.store.read(cx).workspace.global_definitions.clone());
    assert_eq!(saved["key"], "https://example.test");
    assert_eq!(saved["third"], "two");
    assert!(!saved.contains_key("host"));
}

#[gpui_kit::test]
fn theme_write_failure_stays_visible_when_selection_is_retried(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let settings = harness.update(cx, |window, cx| {
        cx.new(|cx| Settings::new(harness.store.clone(), window, cx))
    });
    std::fs::create_dir(dir.path().join("theme.json")).unwrap();
    let editor = cx.read(|cx| settings.read(cx).theme.clone());
    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| editor.edit(paper().text, window, cx));
    });
    for _ in 0..2 {
        assert!(!cx.read(|cx| cx.global::<theme::AppTheme>().state.save_error.is_empty()));
        harness.update(cx, |window, cx| {
            editor.update(cx, |editor, cx| editor.choose("Custom".into(), window, cx));
        });
    }
}

#[gpui_kit::test]
fn rendered_theme_rows_preview_select_and_sections_fit(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let (handle, view) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            cx.new(|cx| super::SettingsWindow::new(harness.store.clone(), window, cx))
        })
        .unwrap()
    });
    let settings = cx.read(|cx| view.read(cx).settings.clone());
    let editor = cx.read(|cx| settings.read(cx).theme.clone());
    cx.update(|cx| {
        settings.update(cx, |settings, cx| {
            settings.page = super::Page::Appearance;
            cx.notify();
        })
    });
    cx.update_window(handle, |_, window, cx| {
        editor.update(cx, |editor, cx| editor.edit(paper().text, window, cx));
    })
    .unwrap();
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.simulate_resize(size(px(760.), px(720.)));
    visual.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let default_background =
        theme::color(blink_core::theme::ThemeTokens::default_tokens().background);
    let open_menu = |visual: &mut VisualTestContext| {
        let select = visual
            .debug_bounds("theme-select")
            .expect("theme select is visible");
        visual.simulate_click(select.center(), gpui_kit::Modifiers::none());
        visual.cx.run_until_parked();
        // The menu opens at the selected theme. Search brings the default
        // into view whatever themes this machine has installed.
        visual.simulate_input("Blink (default)");
        visual.cx.run_until_parked();
        visual.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        visual
            .debug_bounds("theme-choice-Blink (default)")
            .expect("default theme row is visible")
    };

    // Closing the menu ends the preview.
    let theme_row = open_menu(&mut visual);
    visual.simulate_mouse_move(theme_row.center(), None, gpui_kit::Modifiers::none());
    visual.cx.run_until_parked();
    assert_eq!(background(&visual.cx), default_background);
    visual.simulate_keystrokes("escape");
    visual.cx.run_until_parked();
    assert_eq!(background(&visual.cx), rgb(0xffffff).into());

    // The row padding beside the label previews too. Leaving the row ends
    // the preview.
    let theme_row = open_menu(&mut visual);
    let padding = gpui_kit::point(theme_row.left() - px(4.), theme_row.center().y);
    visual.simulate_mouse_move(padding, None, gpui_kit::Modifiers::none());
    visual.cx.run_until_parked();
    assert_eq!(background(&visual.cx), default_background);
    assert!(dir.path().join("theme.json").exists());
    visual.simulate_mouse_move(
        gpui_kit::point(px(750.), px(10.)),
        None,
        gpui_kit::Modifiers::none(),
    );
    visual.cx.run_until_parked();
    assert_eq!(background(&visual.cx), rgb(0xffffff).into());
    visual.simulate_click(theme_row.center(), gpui_kit::Modifiers::none());
    visual.cx.run_until_parked();
    assert!(!dir.path().join("theme.json").exists());
    assert_eq!(background(&visual.cx), default_background);

    for viewport in [size(px(760.), px(720.)), size(px(600.), px(420.))] {
        visual.simulate_resize(viewport);
        for page in super::Page::ALL {
            settings.update(&mut visual.cx, |settings, cx| {
                settings.page = page;
                cx.notify();
            });
            visual.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            let body = visual
                .debug_bounds("settings-body")
                .expect("section is painted");
            assert!(body.size.height > px(200.));
            assert!(body.bottom() <= viewport.height);
            assert!(body.right() <= viewport.width);
        }
    }
}

/// The bounds of the open dialog's surface.
fn dialog_bounds(harness: &Harness, cx: &mut TestAppContext) -> Bounds<Pixels> {
    harness.draw(cx);
    harness.draw(cx);
    let mut visual = VisualTestContext::from_window(harness.window, cx);
    visual
        .debug_bounds("dialog-0")
        .expect("the dialog is painted")
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
            assert!(
                harness.update(cx, |window, cx| window.has_active_dialog(cx)),
                "{name}"
            );
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
