//! Headless flows: files picked in the (simulated) open dialog are granted
//! through the engine and sent by the request; Ctrl+Space completions.

use blink_core::model::BodyMode;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::RequestEditor;
use crate::test_support::{self, Harness, Reply, serve, wait};
use crate::ui::key_value_editor::KeyValueEvent;

fn editor(harness: &Harness, cx: &TestAppContext) -> Entity<RequestEditor> {
    let id = harness.active_id(cx);
    cx.read(|cx| {
        harness
            .app
            .read(cx)
            .pane(id)
            .unwrap()
            .read(cx)
            .editor
            .clone()
    })
}

fn body(request: &[u8]) -> &[u8] {
    let at = test_support::find(request, b"\r\n\r\n").expect("request head");
    &request[at + 4..]
}

#[gpui_kit::test]
fn sends_a_picked_file_body_and_a_multipart_file_part(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let (base, requests) = serve(|_| Reply::ok("text/plain", b"ok".to_vec()));
    let harness = test_support::open(cx, &engine);
    let files = tempfile::tempdir().unwrap();
    let upload = files.path().join("report.csv");
    let content = b"id,name\n1,\xc3\xa9t\xc3\xa9\n\x00\x01\x02binary tail".to_vec();
    std::fs::write(&upload, &content).unwrap();
    let editor = editor(&harness, cx);

    // File body: Choose file, pick it, send.
    harness.edit_draft(cx, |draft| {
        draft.method = "POST".into();
        draft.body_mode = BodyMode::File;
        draft.url = format!("{base}/upload");
    });
    harness.draw(cx);
    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| editor.choose_body_file(window, cx));
    });
    assert!(cx.did_prompt_for_paths());
    let picked = upload.clone();
    cx.simulate_path_prompt_response(move |options| {
        assert!(options.files && !options.directories && !options.multiple);
        Some(vec![picked])
    });
    wait(cx, "file body picked", |cx| {
        let id = harness.store.read(cx).workspace.shown_active_id().unwrap();
        let session = harness.store.read(cx).workspace.session(id).unwrap();
        session.draft.body_file.is_some()
    });
    let canonical = std::fs::canonicalize(&upload).unwrap();
    let stored = harness.session(cx, |s| s.draft.body_file.clone().unwrap());
    assert_eq!(std::fs::canonicalize(&stored).unwrap(), canonical);
    assert_eq!(
        cx.read(|cx| editor.read(cx).picked_file.as_ref().map(|f| f.name.clone())),
        Some("report.csv".to_string())
    );
    harness.send_draft(cx);
    assert_eq!(harness.session(cx, |s| s.error.clone()), "");
    let request = requests.recv().unwrap();
    assert!(request.starts_with(b"POST /upload"));
    assert_eq!(body(&request), content.as_slice());

    // Canceling the dialog keeps the picked file.
    harness.update(cx, |window, cx| {
        editor.update(cx, |editor, cx| editor.choose_body_file(window, cx));
    });
    cx.simulate_path_prompt_response(|_| None);
    harness.draw(cx);
    assert_eq!(
        harness.session(cx, |s| s.draft.body_file.clone()),
        Some(stored)
    );

    // Multipart: pick a file for a new part, plus a text part, send.
    harness.edit_draft(cx, |draft| {
        draft.body_mode = BodyMode::Multipart;
        draft.form = Some(vec![]);
        draft.url = format!("{base}/multipart");
    });
    harness.draw(cx);
    let form = cx.read(|cx| editor.read(cx).form.clone());
    form.update(cx, |_, cx| cx.emit(KeyValueEvent::PickFile(0)));
    assert!(cx.did_prompt_for_paths());
    let picked = upload.clone();
    cx.simulate_path_prompt_response(move |_| Some(vec![picked]));
    wait(cx, "form file picked", |cx| {
        let id = harness.store.read(cx).workspace.shown_active_id().unwrap();
        let session = harness.store.read(cx).workspace.session(id).unwrap();
        session
            .draft
            .form
            .as_ref()
            .is_some_and(|rows| rows.iter().any(|row| row.file == Some(true)))
    });
    let row = harness.session(cx, |s| s.draft.form.clone().unwrap()[0].clone());
    assert_eq!(
        row.key, "report",
        "named after the file without its extension"
    );
    assert_eq!(std::fs::canonicalize(&row.value).unwrap(), canonical);
    harness.send_draft(cx);
    assert_eq!(harness.session(cx, |s| s.error.clone()), "");
    let request = requests.recv().unwrap();
    let text = String::from_utf8_lossy(&request);
    assert!(text.starts_with("POST /multipart"));
    assert!(
        text.to_ascii_lowercase()
            .contains("content-type: multipart/form-data; boundary=")
    );
    assert!(
        text.contains("name=\"report\"; filename=\"report.csv\""),
        "{text}"
    );
    assert!(test_support::find(body(&request), &content).is_some());
}

#[gpui_kit::test]
fn ctrl_space_opens_completions_without_typing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let id = harness.active_id(cx);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace
                .global_definitions
                .insert("host".into(), "api.example.com".into());
            workspace.session_mut(id).unwrap().view.request_tab = "body".into();
        });
    });
    let text = r#"{"url": "{{ho"#;
    harness.edit_draft(cx, |draft| {
        draft.method = "POST".into();
        draft.body_mode = BodyMode::Json;
        draft.body = text.into();
    });
    harness.draw(cx);
    let editor = editor(&harness, cx);
    let body = cx.read(|cx| editor.read(cx).body.clone());
    harness.update(cx, |window, cx| {
        body.update(cx, |body, cx| {
            body.focus(window, cx);
            body.set_selected_range(text.len()..text.len(), cx);
        });
    });
    harness.draw(cx);
    let open = |cx: &TestAppContext| cx.read(|cx| body.read(cx).completion_menu_state().open);
    assert!(!open(cx), "moving the cursor does not open the menu");

    VisualTestContext::from_window(harness.window, cx).simulate_keystrokes("ctrl-space");
    harness.draw(cx);
    let (labels, query) = cx.read(|cx| {
        let menu = body.read(cx).completion_menu_state();
        let labels: Vec<_> = menu.items.iter().map(|item| item.label.clone()).collect();
        (labels, menu.query.clone())
    });
    assert!(open(cx));
    assert_eq!(labels, ["host", "_.host"]);
    assert_eq!(query, "ho");
    assert_eq!(
        cx.read(|cx| body.read(cx).value().to_string()),
        text,
        "no text typed"
    );
}

#[gpui_kit::test]
fn the_bearer_field_shows_an_undefined_reference(cx: &mut TestAppContext) {
    use blink_core::model::AuthorizationConfig;
    use blink_core::token_hints::TokenState;

    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let id = harness.active_id(cx);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.session_mut(id).unwrap().view.request_tab = "auth".into();
        });
    });
    harness.edit_draft(cx, |draft| {
        draft.local_auth = Some(AuthorizationConfig::Bearer {
            token: "{{acess_token}}".into(),
        });
    });
    harness.draw(cx);
    let editor = editor(&harness, cx);
    let token = cx.read(|cx| editor.read(cx).token.clone());
    let shown = |cx: &TestAppContext| {
        cx.read(|cx| {
            let segments = token.read(cx).secret_segments().expect("masked row");
            let text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
            (text, segments)
        })
    };

    let (text, segments) = shown(cx);
    assert_eq!(text, "{{acess_token}}");
    assert!(!text.contains('•'));
    assert_eq!(segments[0].1, Some(TokenState::Unresolved));

    harness.edit_draft(cx, |draft| {
        draft.local_auth = Some(AuthorizationConfig::Bearer {
            token: "abc{{acess_token}}".into(),
        });
    });
    harness.draw(cx);
    assert_eq!(shown(cx).0, "•••{{acess_token}}");

    // Focused: the plain field, every reference labeled with its raw text.
    // Focus events reach only an active window.
    harness.update(cx, |window, cx| {
        window.activate_window();
        token.update(cx, |token, cx| token.focus(window, cx));
    });
    harness.draw(cx);
    cx.read(|cx| {
        let input = token.read(cx);
        assert!(input.secret_segments().is_none());
        let state = input.state().read(cx);
        assert_eq!(state.value().as_ref(), "abc{{acess_token}}");
        let labels: Vec<_> = state
            .tokens()
            .iter()
            .map(|span| span.token().label().to_string())
            .collect();
        assert_eq!(labels, ["{{acess_token}}"]);
    });

    // A change in the field updates the draft's bearer token.
    token.update(cx, |_, cx| {
        cx.emit(crate::ui::token_input::TokenInputEvent::Change(
            "{{access_token}}".into(),
        ))
    });
    cx.run_until_parked();
    assert_eq!(
        harness.session(cx, |s| s.draft.local_auth.clone()),
        Some(AuthorizationConfig::Bearer {
            token: "{{access_token}}".into()
        })
    );
}

#[gpui_kit::test]
fn the_basic_password_is_a_secret_token_field(cx: &mut TestAppContext) {
    use blink_core::model::AuthorizationConfig;

    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let id = harness.active_id(cx);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace
                .global_definitions
                .insert("pass".into(), "hunter2".into());
            workspace.session_mut(id).unwrap().view.request_tab = "auth".into();
        });
    });
    harness.edit_draft(cx, |draft| {
        draft.local_auth = Some(AuthorizationConfig::Basic {
            username: "me".into(),
            password: "x{{pass}}".into(),
        });
    });
    harness.draw(cx);
    let editor = editor(&harness, cx);
    let (username, password) = cx.read(|cx| {
        let editor = editor.read(cx);
        (editor.username.clone(), editor.password.clone())
    });
    cx.read(|cx| {
        assert!(username.read(cx).secret_segments().is_none());
        let segments = password.read(cx).secret_segments().expect("masked row");
        let text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(text, "•{{pass}}", "the value of a reference never shows");
    });
}

#[gpui_kit::test]
fn secret_auth_fields_keep_their_text_out_of_the_accessibility_tree(cx: &mut TestAppContext) {
    use blink_core::model::AuthorizationConfig;
    use gpui_kit::test::TestWindowExt as _;

    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let id = harness.active_id(cx);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.session_mut(id).unwrap().view.request_tab = "auth".into();
        });
    });
    let editor = editor(&harness, cx);
    let (token, username, password) = cx.read(|cx| {
        let editor = editor.read(cx);
        (
            editor.token.clone(),
            editor.username.clone(),
            editor.password.clone(),
        )
    });
    // The accessibility value the field's input reports.
    let value = |field: &Entity<crate::ui::token_input::TokenInput>, cx: &mut TestAppContext| {
        harness.draw(cx);
        let state = cx.read(|cx| field.read(cx).state().entity_id());
        harness.update(cx, |window, _| {
            window.find(("input", state)).value().map(str::to_string)
        })
    };

    harness.edit_draft(cx, |draft| {
        draft.local_auth = Some(AuthorizationConfig::Bearer {
            token: "s3cr3t-bearer{{acess_token}}".into(),
        });
    });
    assert_eq!(value(&token, cx), None);

    harness.edit_draft(cx, |draft| {
        draft.local_auth = Some(AuthorizationConfig::Basic {
            username: "visible-user".into(),
            password: "s3cr3t-pass".into(),
        });
    });
    assert_eq!(
        value(&username, cx).as_deref(),
        Some("visible-user"),
        "the username is not secret"
    );
    assert_eq!(value(&password, cx), None);

    // Focused, the field still keeps its text out of the tree, and its
    // references stay inline tokens.
    harness.update(cx, |window, cx| {
        window.activate_window();
        password.update(cx, |password, cx| password.focus(window, cx));
    });
    assert_eq!(value(&password, cx), None);
    assert!(cx.read(|cx| password.read(cx).secret_segments().is_none()));
}

#[gpui_kit::test]
fn a_secret_field_scrolls_back_to_the_start_on_blur(cx: &mut TestAppContext) {
    use blink_core::model::AuthorizationConfig;

    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let id = harness.active_id(cx);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.session_mut(id).unwrap().view.request_tab = "auth".into();
        });
    });
    let long = "x".repeat(400);
    let value = long.clone();
    harness.edit_draft(cx, |draft| {
        draft.local_auth = Some(AuthorizationConfig::Bearer { token: value });
    });
    harness.draw(cx);
    let editor = editor(&harness, cx);
    let token = cx.read(|cx| editor.read(cx).token.clone());
    let state = cx.read(|cx| token.read(cx).state().clone());
    harness.update(cx, |window, cx| {
        window.activate_window();
        token.update(cx, |token, cx| token.focus(window, cx));
        state.update(cx, |state, cx| {
            state.set_selected_range(long.len()..long.len(), cx)
        });
    });
    harness.draw(cx);
    harness.draw(cx);
    let scrolled = cx.read(|cx| state.read(cx).scroll_offset().x);
    assert!(
        scrolled < gpui_kit::px(0.),
        "the caret at the end scrolls the text"
    );

    harness.update(cx, |window, cx| window.blur(cx));
    harness.draw(cx);
    harness.draw(cx);
    assert_eq!(
        cx.read(|cx| state.read(cx).scroll_offset().x),
        gpui_kit::px(0.)
    );
}
