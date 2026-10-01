//! Headless flow: pasting a cURL command into the URL field.

use blink_core::model::{BodyMode, Pair};
use gpui_kit::component::input::Paste;
use gpui_kit::{ClipboardItem, Entity, TestAppContext};

use super::RequestPane;
use crate::test_support::{self, Harness};

fn pane(harness: &Harness, cx: &TestAppContext) -> Entity<RequestPane> {
    let id = harness.active_id(cx);
    cx.read(|cx| harness.app.read(cx).pane(id).unwrap().clone())
}

/// Focus the URL field and paste `text` from the clipboard.
fn paste_into_url(harness: &Harness, cx: &mut TestAppContext, text: &str) {
    let pane = pane(harness, cx);
    harness.update(cx, |window, cx| {
        pane.update(cx, |pane, cx| pane.focus_url(window, cx));
    });
    harness.draw(cx);
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    harness.dispatch(cx, Paste);
    harness.draw(cx);
}

fn rows(pairs: &[Pair]) -> Vec<(String, String)> {
    pairs
        .iter()
        .filter(|pair| !pair.key.is_empty())
        .map(|pair| (pair.key.clone(), pair.value.clone()))
        .collect()
}

#[gpui_kit::test]
fn pasting_curl_replaces_the_draft_and_lists_ignored_options(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    harness.edit_draft(cx, |draft| draft.url = "https://old.example/keep".into());
    let pane = pane(&harness, cx);

    paste_into_url(
        &harness,
        cx,
        "curl -sS -X POST 'https://api.example.com/v1/items?limit=5' \
         -H 'Content-Type: application/json' -H 'X-Trace: 7' \
         --data-raw '{\"name\":\"blink\"}' --compressed",
    );
    let draft = harness.session(cx, |s| s.draft.clone());
    assert_eq!(draft.method, "POST");
    assert_eq!(draft.url, "https://api.example.com/v1/items?limit=5");
    assert_eq!(
        rows(&draft.headers),
        [
            ("Content-Type".to_string(), "application/json".to_string()),
            ("X-Trace".to_string(), "7".to_string()),
        ]
    );
    assert_eq!(draft.body, r#"{"name":"blink"}"#);
    assert_eq!(draft.body_mode, BodyMode::Json);
    let (notice, error, url, method) = cx.read(|cx| {
        let pane = pane.read(cx);
        (
            pane.import_notice.clone(),
            pane.import_error.clone(),
            pane.url.read(cx).value(cx),
            pane.method.read(cx).value().to_string(),
        )
    });
    assert_eq!(notice, "Imported cURL command. Ignored: -s -S --compressed");
    assert_eq!(error, "");
    // The fields show the imported draft, not the pasted text.
    assert_eq!(url, "https://api.example.com/v1/items?limit=5");
    assert_eq!(method, "POST");

    // A broken command shows its error and keeps the draft.
    paste_into_url(&harness, cx, "curl 'https://x.example");
    let (notice, error) = cx.read(|cx| {
        let pane = pane.read(cx);
        (pane.import_notice.clone(), pane.import_error.clone())
    });
    assert_eq!(notice, "");
    assert_eq!(error, "The cURL command has an unclosed quote.");
    assert_eq!(
        harness.session(cx, |s| s.draft.url.clone()),
        "https://api.example.com/v1/items?limit=5"
    );

    // Other text pastes as text into the URL.
    paste_into_url(&harness, cx, "https://plain.example/path");
    let url = harness.session(cx, |s| s.draft.url.clone());
    assert!(url.contains("https://plain.example/path"), "{url}");
}
