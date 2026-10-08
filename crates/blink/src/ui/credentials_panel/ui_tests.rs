//! Credential form validation without Keychain writes or browser sign-in.

use gpui_kit::{AppContext as _, TestAppContext};

use super::CredentialsPanel;
use crate::test_support;

#[gpui_kit::test]
fn masks_secret_input_and_rejects_invalid_names_without_saving(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let store = harness.store.clone();
    let panel = harness.update(cx, |window, cx| {
        cx.new(|cx| CredentialsPanel::new(store, Some("/credential-ui-test".into()), window, cx))
    });
    let secret = cx.read(|cx| panel.read(cx).secret.clone());
    let name = cx.read(|cx| panel.read(cx).name.clone());
    assert!(cx.read(|cx| secret.read(cx).presentation().is_masked()));
    assert_eq!(
        cx.read(|cx| panel.read(cx).scope.clone()),
        Some("/credential-ui-test".into())
    );
    harness.update(cx, |window, cx| {
        secret.update(cx, |input, cx| {
            input.set_value("fake-secret-for-ui-test", window, cx)
        });
        name.update(cx, |input, cx| input.set_value("invalid name", window, cx));
    });
    // Masking changes presentation, not the value supplied to credential storage.
    assert_eq!(
        cx.read(|cx| secret.read(cx).value().to_string()),
        "fake-secret-for-ui-test"
    );
    assert!(
        !cx.read(|cx| harness.store.read(cx).workspace.encode())
            .contains("fake-secret-for-ui-test")
    );
    // OAuth rejects the invalid name before starting an engine/browser operation.
    panel.update(cx, |panel, cx| panel.oauth(cx));
    assert!(cx.read(|cx| panel.read(cx).message.contains("valid secret name")));
    assert!(!cx.read(|cx| panel.read(cx).busy));
    // Save validates the name before any Keychain call and clears the input.
    harness.update(cx, |window, cx| {
        panel.update(cx, |panel, cx| panel.save(window, cx))
    });
    test_support::wait(cx, "invalid credential rejected", |cx| !panel.read(cx).busy);
    assert!(cx.read(|cx| panel.read(cx).message.contains("valid name")));
    assert!(cx.read(|cx| secret.read(cx).value().is_empty()));
    assert!(cx.read(|cx| secret.read(cx).presentation().is_masked()));
    assert!(
        !cx.read(|cx| harness.store.read(cx).workspace.encode())
            .contains("fake-secret-for-ui-test")
    );
}
