//! Headless flows: response tokens typed in Group Settings save with the
//! group, and a name another token uses blocks the save.

use blink_core::model::{CheckSource, ResponseToken};
use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};

use crate::actions::OpenGroupSettings;

use super::ResponseTokensEditor;
use crate::test_support::{self, Harness};
use crate::ui::group_settings::GroupForm;

/// A root group named API with `text_tokens`, the harness, and the active
/// request id.
fn setup(
    cx: &mut TestAppContext,
    text_tokens: &[(&str, &str)],
) -> (tempfile::TempDir, Harness, u64, u64) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let definitions: blink_core::model::Definitions = text_tokens
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let group_id = harness.store.update(cx, |store, cx| {
        let mut id = 0;
        store.update_workspace(cx, |workspace| {
            id = workspace.add_group("API", None);
            let group = workspace.groups.iter_mut().find(|g| g.id == id).unwrap();
            group.local_definitions = Some(definitions);
        });
        id
    });
    let active = harness.active_id(cx);
    (dir, harness, group_id, active)
}

fn open_form(harness: &Harness, cx: &mut TestAppContext, group_id: u64) -> Entity<GroupForm> {
    let store = harness.store.clone();
    harness.update(cx, |window, cx| {
        cx.new(|cx| GroupForm::new(store, group_id, window, cx))
    })
}

/// Add a row and type into it, as the user would.
fn add_row(
    harness: &Harness,
    cx: &mut TestAppContext,
    editor: &Entity<ResponseTokensEditor>,
    (name, request_id, source, path, max_age): (&str, u64, CheckSource, &str, &str),
) -> u64 {
    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| {
            let id = editor.add_row(window, cx);
            let row = editor.rows.iter().find(|row| row.id == id).unwrap();
            let (name_input, path_input, age_input) =
                (row.name.clone(), row.path.clone(), row.max_age.clone());
            let request = row.request.clone();
            name_input.update(cx, |input, cx| input.set_value(name, window, cx));
            path_input.update(cx, |input, cx| input.set_value(path, window, cx));
            age_input.update(cx, |input, cx| input.set_value(max_age, window, cx));
            request.update(cx, |select, cx| {
                select.set_selected_value(&request_id.to_string().into(), window, cx)
            });
            editor.set_source(id, source, window, cx);
            id
        })
    })
}

fn save(harness: &Harness, cx: &mut TestAppContext, form: &Entity<GroupForm>) -> bool {
    harness.update(cx, |_, cx| form.update(cx, |form, cx| form.save(cx)))
}

#[gpui_kit::test]
fn saving_group_settings_keeps_response_tokens(cx: &mut TestAppContext) {
    let (_dir, harness, group_id, active) = setup(cx, &[]);
    let form = open_form(&harness, cx, group_id);
    let editor = cx.read(|cx| form.read(cx).response_tokens.clone());
    let id = add_row(
        &harness,
        cx,
        &editor,
        (
            "access_token",
            active,
            CheckSource::Json,
            ".access_token",
            "15m",
        ),
    );

    assert!(save(&harness, cx, &form));

    let saved = cx.read(|cx| {
        harness
            .store
            .read(cx)
            .workspace
            .group(group_id)
            .and_then(|group| group.response_tokens.clone())
    });
    assert_eq!(
        saved,
        Some(vec![ResponseToken {
            id,
            name: "access_token".into(),
            request_id: active,
            source: CheckSource::Json,
            path: ".access_token".into(),
            max_age_secs: Some(900),
        }])
    );

    // Opening the dialog again shows the saved row with its max age.
    let again = open_form(&harness, cx, group_id);
    let tokens = cx.read(|cx| {
        let editor = again.read(cx).response_tokens.read(cx);
        (
            editor.tokens(cx),
            editor.rows[0].max_age.read(cx).value().to_string(),
        )
    });
    assert_eq!(tokens.0.unwrap().len(), 1);
    assert_eq!(tokens.1, "15m");

    // The real dialog paints the saved row.
    harness.dispatch(cx, OpenGroupSettings { group_id });
    harness.draw(cx);
    harness.draw(cx);
    assert!(harness.update(cx, |window, cx| window.has_active_dialog(cx)));
    let mut visual = VisualTestContext::from_window(harness.window, cx);
    assert!(visual.debug_bounds("response-tokens-table").is_some());
}

#[gpui_kit::test]
fn a_duplicate_name_blocks_the_save(cx: &mut TestAppContext) {
    let (_dir, harness, group_id, active) = setup(cx, &[("access_token", "abc")]);
    let form = open_form(&harness, cx, group_id);
    let editor = cx.read(|cx| form.read(cx).response_tokens.clone());
    let id = add_row(
        &harness,
        cx,
        &editor,
        (
            "access_token",
            active,
            CheckSource::Json,
            ".access_token",
            "",
        ),
    );

    // Save returns false, so the dialog stays open.
    assert!(!save(&harness, cx, &form));

    let error = cx.read(|cx| editor.read(cx).errors.get(&id).cloned());
    assert_eq!(
        error.as_deref(),
        Some("Another token is named \"access_token\".")
    );
    let saved = cx.read(|cx| {
        harness
            .store
            .read(cx)
            .workspace
            .group(group_id)
            .and_then(|group| group.response_tokens.clone())
    });
    assert_eq!(saved, None);
}

#[gpui_kit::test]
fn a_bad_max_age_blocks_the_save(cx: &mut TestAppContext) {
    let (_dir, harness, group_id, active) = setup(cx, &[]);
    let form = open_form(&harness, cx, group_id);
    let editor = cx.read(|cx| form.read(cx).response_tokens.clone());
    let id = add_row(
        &harness,
        cx,
        &editor,
        (
            "access_token",
            active,
            CheckSource::Json,
            ".access_token",
            "soon",
        ),
    );

    assert!(!save(&harness, cx, &form));
    let error = cx.read(|cx| editor.read(cx).errors.get(&id).cloned());
    assert_eq!(
        error.as_deref(),
        Some("Enter a max age such as 30s, 15m, or 1h.")
    );
}

#[gpui_kit::test]
fn application_settings_apply_global_response_tokens_without_save(cx: &mut TestAppContext) {
    let (_dir, harness, _, active) = setup(cx, &[]);
    let store = harness.store.clone();
    let settings = harness.update(cx, |window, cx| {
        cx.new(|cx| crate::ui::settings_window::Settings::new(store, window, cx))
    });
    let editor = cx.read(|cx| settings.read(cx).response_tokens.clone());
    // An unfinished row must not block a different, valid row.
    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| editor.add_row(window, cx));
    });
    add_row(
        &harness,
        cx,
        &editor,
        ("session", active, CheckSource::Header, "Set-Cookie", "1h"),
    );

    cx.run_until_parked();
    let saved = cx.read(|cx| {
        harness
            .store
            .read(cx)
            .workspace
            .global_response_tokens
            .clone()
    });
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "session");
    assert_eq!(saved[0].path, "Set-Cookie");
    assert_eq!(saved[0].max_age_secs, Some(3600));

    let second = add_row(
        &harness,
        cx,
        &editor,
        ("other", active, CheckSource::Header, "X-Key", "1h"),
    );
    cx.run_until_parked();
    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| {
            let first = editor
                .rows
                .iter()
                .find(|row| row.id == saved[0].id)
                .unwrap();
            first.name.update(cx, |input, cx| {
                input.set_value("other", window, cx);
                cx.emit(super::InputEvent::Change);
            });
            let second = editor.rows.iter().find(|row| row.id == second).unwrap();
            second.max_age.update(cx, |input, cx| {
                input.set_value("2h", window, cx);
                cx.emit(super::InputEvent::Change);
            });
        });
    });
    cx.run_until_parked();
    let saved = cx.read(|cx| {
        harness
            .store
            .read(cx)
            .workspace
            .global_response_tokens
            .clone()
    });
    assert_eq!(saved[0].name, "session");
    assert_eq!(
        saved
            .iter()
            .find(|token| token.id == second)
            .unwrap()
            .max_age_secs,
        Some(7200)
    );
}

#[gpui_kit::test]
fn request_choices_show_the_group_path(cx: &mut TestAppContext) {
    let (_dir, harness, group_id, _) = setup(cx, &[]);
    let inner = harness.store.update(cx, |store, cx| {
        let mut inner = 0;
        store.update_workspace(cx, |workspace| {
            let sub = workspace.add_group("Sub", Some(group_id));
            inner = workspace.sessions[0].id;
            workspace.sessions[0].group_id = Some(sub);
        });
        inner
    });
    let choices = cx.read(|cx| super::request_choices(&harness.store.read(cx).workspace));
    let (_, label) = choices.iter().find(|(id, _)| *id == inner).unwrap();
    assert!(label.starts_with("API / Sub / "), "{label}");
}

#[gpui_kit::test]
fn a_deleted_source_request_shows_when_the_dialog_opens(cx: &mut TestAppContext) {
    let (_dir, harness, group_id, active) = setup(cx, &[]);
    let token = ResponseToken {
        id: blink_core::ids::RESPONSE_TOKENS.next(),
        name: "access_token".into(),
        request_id: active,
        source: CheckSource::Json,
        path: ".access_token".into(),
        max_age_secs: None,
    };
    let token_id = token.id;
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            let group = workspace.groups.iter_mut().find(|g| g.id == group_id);
            group.unwrap().response_tokens = Some(vec![token]);
            workspace.delete_request(active);
        })
    });
    assert!(cx.read(|cx| harness.store.read(cx).workspace.session(active).is_none()));

    // The error shows before any save.
    let form = open_form(&harness, cx, group_id);
    let editor = cx.read(|cx| form.read(cx).response_tokens.clone());
    let deleted = Some("Reads a deleted request. Choose a request.");
    let error = cx.read(|cx| editor.read(cx).errors.get(&token_id).cloned());
    assert_eq!(error.as_deref(), deleted);

    // Save stays blocked and keeps the same error.
    assert!(!save(&harness, cx, &form));
    let error = cx.read(|cx| editor.read(cx).errors.get(&token_id).cloned());
    assert_eq!(error.as_deref(), deleted);

    // The real dialog paints the row with its error.
    harness.dispatch(cx, OpenGroupSettings { group_id });
    harness.draw(cx);
    assert!(harness.update(cx, |window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
fn every_row_error_shows_at_once(cx: &mut TestAppContext) {
    let (_dir, harness, group_id, active) = setup(cx, &[]);
    let form = open_form(&harness, cx, group_id);
    let editor = cx.read(|cx| form.read(cx).response_tokens.clone());
    let bad_age = add_row(
        &harness,
        cx,
        &editor,
        ("a", active, CheckSource::Json, ".a", "soon"),
    );
    let no_path = add_row(
        &harness,
        cx,
        &editor,
        ("b", active, CheckSource::Json, "", ""),
    );

    assert!(!save(&harness, cx, &form));
    let errors = cx.read(|cx| editor.read(cx).errors.clone());
    assert_eq!(
        errors.get(&bad_age).map(String::as_str),
        Some("Enter a max age such as 30s, 15m, or 1h.")
    );
    assert_eq!(
        errors.get(&no_path).map(String::as_str),
        Some("Enter a path.")
    );
}

#[gpui_kit::test]
fn a_source_without_a_path_clears_the_path(cx: &mut TestAppContext) {
    let (_dir, harness, group_id, active) = setup(cx, &[]);
    let form = open_form(&harness, cx, group_id);
    let editor = cx.read(|cx| form.read(cx).response_tokens.clone());
    let id = add_row(
        &harness,
        cx,
        &editor,
        ("a", active, CheckSource::Json, ".a", ""),
    );
    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| {
            editor.set_source(id, CheckSource::Status, window, cx)
        })
    });
    let path = cx.read(|cx| editor.read(cx).rows[0].path.read(cx).value().to_string());
    assert_eq!(path, "");
}

#[gpui_kit::test]
fn a_source_that_supplies_its_own_token_shows_a_warning(cx: &mut TestAppContext) {
    let (_dir, harness, group_id, active) = setup(cx, &[]);
    let token = ResponseToken {
        id: blink_core::ids::RESPONSE_TOKENS.next(),
        name: "access_token".into(),
        request_id: active,
        source: CheckSource::Json,
        path: ".access_token".into(),
        max_age_secs: None,
    };
    let token_id = token.id;
    // The source request is in the group, whose auth sends the token.
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.move_request(active, Some(group_id));
            workspace.session_mut(active).unwrap().draft.url = "https://api.test/login".into();
            let group = workspace.groups.iter_mut().find(|g| g.id == group_id);
            let group = group.unwrap();
            group.local_auth = Some(blink_core::model::AuthorizationConfig::Bearer {
                token: "{{access_token}}".into(),
            });
            group.response_tokens = Some(vec![token]);
        })
    });

    // The warning shows when the dialog opens.
    let form = open_form(&harness, cx, group_id);
    let editor = cx.read(|cx| form.read(cx).response_tokens.clone());
    let warning = cx.read(|cx| editor.read(cx).warnings.get(&token_id).cloned());
    assert_eq!(
        warning.as_deref(),
        Some("\"/login\" uses \"{{access_token}}\", which it supplies. Set its auth to No auth.")
    );
    assert!(cx.read(|cx| editor.read(cx).errors.is_empty()));

    // It does not block the save.
    assert!(save(&harness, cx, &form));

    // The real dialog paints the row with its warning.
    harness.dispatch(cx, OpenGroupSettings { group_id });
    harness.draw(cx);
    assert!(harness.update(cx, |window, cx| window.has_active_dialog(cx)));
}
