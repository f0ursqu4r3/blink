//! Headless flow: files picked in the (simulated) open dialog are granted
//! through the engine and sent by the request.

use blink_core::model::BodyMode;
use gpui_kit::{Entity, TestAppContext};

use super::RequestEditor;
use crate::test_support::{self, Harness, Reply, serve, wait};
use crate::ui::key_value_editor::KeyValueEvent;

fn editor(harness: &Harness, cx: &TestAppContext) -> Entity<RequestEditor> {
    let id = harness.active_id(cx);
    cx.read(|cx| harness.app.read(cx).pane(id).unwrap().read(cx).editor.clone())
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
    assert_eq!(harness.session(cx, |s| s.draft.body_file.clone()), Some(stored));

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
    assert_eq!(row.key, "report", "named after the file without its extension");
    assert_eq!(std::fs::canonicalize(&row.value).unwrap(), canonical);
    harness.send_draft(cx);
    assert_eq!(harness.session(cx, |s| s.error.clone()), "");
    let request = requests.recv().unwrap();
    let text = String::from_utf8_lossy(&request);
    assert!(text.starts_with("POST /multipart"));
    assert!(text.to_ascii_lowercase().contains("content-type: multipart/form-data; boundary="));
    assert!(text.contains("name=\"report\"; filename=\"report.csv\""), "{text}");
    assert!(test_support::find(body(&request), &content).is_some());
}
