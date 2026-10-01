//! Headless flows through the real Browser: inline delete confirmations and
//! the menu row metrics.

use core::prelude::v1::test;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ElementId, Pixels, TestAppContext, px};

use super::{CONFIRM_BUTTONS, CONFIRM_GAP, CONFIRM_LINE, CONFIRM_PADDING, confirm_height};
use crate::test_support::{self, Harness};

/// The real app with `confirm_close_drafts` on, and two top-level requests:
/// the first has a URL (a draft), the second is blank.
fn setup(cx: &mut TestAppContext) -> (tempfile::TempDir, Harness, u64, u64) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let first = harness.active_id(cx);
    let second = harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            let mut preferences = workspace.preferences.clone();
            preferences.confirm_close_drafts = true;
            workspace.set_preferences(preferences);
            workspace.session_mut(first).unwrap().draft.url = "https://a.test/users".into();
            workspace.create(Some(None))
        })
    });
    harness.draw(cx);
    (dir, harness, first, second)
}

fn exists(harness: &Harness, cx: &TestAppContext, id: u64) -> bool {
    cx.read(|cx| harness.store.read(cx).workspace.session(id).is_some())
}

fn deletion(harness: &Harness, cx: &TestAppContext) -> String {
    cx.read(|cx| harness.store.read(cx).workspace.deletion_label())
}

fn found(harness: &Harness, cx: &mut TestAppContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    harness.update(cx, |window, _| window.try_find(id).is_some())
}

fn bounds(
    harness: &Harness,
    cx: &mut TestAppContext,
    id: impl Into<ElementId>,
) -> gpui_kit::Bounds<Pixels> {
    let id = id.into();
    harness.update(cx, |window, _| window.find(id).bounds())
}

/// Right-click `target` and wait for its menu.
fn open_menu(harness: &Harness, cx: &mut TestAppContext, target: impl Into<ElementId>) {
    let target = target.into();
    harness.update(cx, |window, cx| {
        window.render_frame(cx);
        window.right_click(target, cx);
    });
    harness.draw(cx);
    assert!(found(harness, cx, "popup-menu"), "the menu opens");
}

/// Right-click `target` and pick the last menu item: Delete.
fn pick_delete(harness: &Harness, cx: &mut TestAppContext, target: impl Into<ElementId>) {
    open_menu(harness, cx, target);
    harness.update(cx, |window, cx| {
        let mut menu = window.within("popup-menu");
        let last = (0usize..64)
            .rev()
            .find(|ix| menu.try_find(*ix).is_some())
            .expect("menu items");
        menu.click(last, cx);
    });
    harness.draw(cx);
    assert!(!found(harness, cx, "popup-menu"), "the menu closes");
}

#[gpui_kit::test]
fn confirming_an_inline_request_delete_deletes_and_offers_undo(cx: &mut TestAppContext) {
    let (_dir, harness, first, second) = setup(cx);
    let strip = ("browser-confirm-delete-request", first);

    pick_delete(&harness, cx, ("browser-request", first));
    // A request with a draft asks first, in the tree, not in a dialog.
    assert!(exists(&harness, cx, first));
    assert!(found(&harness, cx, strip));
    assert_eq!(
        harness.update(cx, |window, _| window
            .find(strip)
            .label()
            .map(str::to_string)),
        Some("Confirm delete request".to_string())
    );
    // The strip sits between its row and the next one, at its own height.
    let row = bounds(&harness, cx, ("browser-request", first));
    let confirm = bounds(&harness, cx, strip);
    let next = bounds(&harness, cx, ("browser-request", second));
    assert!(
        (confirm.top() - row.bottom()).abs() < px(0.5),
        "{row:?} {confirm:?}"
    );
    assert!(
        (next.top() - confirm.bottom()).abs() < px(0.5),
        "{confirm:?} {next:?}"
    );
    let lines = ((confirm.size.height.as_f32()
        - (CONFIRM_PADDING * 2. + CONFIRM_GAP + CONFIRM_BUTTONS + 1.))
        / CONFIRM_LINE)
        .round() as usize;
    assert!(lines >= 1);
    assert!((confirm.size.height.as_f32() - confirm_height(lines)).abs() < 0.5);

    harness.update(cx, |window, cx| {
        window.within(strip).click("browser-confirm-delete", cx)
    });
    harness.draw(cx);
    assert!(!exists(&harness, cx, first));
    assert!(exists(&harness, cx, second));
    assert!(!found(&harness, cx, strip));
    assert!(!deletion(&harness, cx).is_empty());
    assert!(harness.update(cx, |window, _| window.find("status-undo").visible()));
}

#[gpui_kit::test]
fn cancelling_an_inline_request_delete_keeps_the_request(cx: &mut TestAppContext) {
    let (_dir, harness, first, _) = setup(cx);
    let strip = ("browser-confirm-delete-request", first);

    pick_delete(&harness, cx, ("browser-request", first));
    assert!(found(&harness, cx, strip));
    harness.update(cx, |window, cx| {
        window.within(strip).click("browser-confirm-cancel", cx)
    });
    harness.draw(cx);
    assert!(exists(&harness, cx, first));
    assert!(!found(&harness, cx, strip));
    assert!(deletion(&harness, cx).is_empty());
    assert!(!found(&harness, cx, "status-undo"));
}

#[gpui_kit::test]
fn a_blank_request_deletes_without_asking(cx: &mut TestAppContext) {
    let (_dir, harness, _, second) = setup(cx);
    pick_delete(&harness, cx, ("browser-request", second));
    assert!(!exists(&harness, cx, second));
    assert!(!found(
        &harness,
        cx,
        ("browser-confirm-delete-request", second)
    ));
    assert!(found(&harness, cx, "status-undo"));
}

#[gpui_kit::test]
fn a_multi_selection_deletes_after_one_inline_confirmation(cx: &mut TestAppContext) {
    let (_dir, harness, first, second) = setup(cx);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.update_selection(vec![first, second], Some(first));
        })
    });
    harness.draw(cx);
    let strip = ("browser-confirm-delete-request", second);
    pick_delete(&harness, cx, ("browser-request", second));
    // Several targets ask even without a draft; the strip is under the
    // clicked row.
    assert!(found(&harness, cx, strip));
    assert!(exists(&harness, cx, first) && exists(&harness, cx, second));
    harness.update(cx, |window, cx| {
        window.within(strip).click("browser-confirm-delete", cx)
    });
    harness.draw(cx);
    assert!(!exists(&harness, cx, first) && !exists(&harness, cx, second));
    assert!(found(&harness, cx, "status-undo"));
}

#[gpui_kit::test]
fn a_group_delete_asks_inline_then_promotes_its_contents(cx: &mut TestAppContext) {
    let (_dir, harness, first, _) = setup(cx);
    let group = harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.create_group("Payments platform integration tests", None, &[first])
        })
    });
    harness.draw(cx);
    let strip = ("browser-confirm-delete-group", group);

    pick_delete(&harness, cx, ("browser-group", group));
    assert!(found(&harness, cx, strip));
    // The long question wraps; the strip grows with it and spans the list.
    let row = bounds(&harness, cx, ("browser-group", group));
    let confirm = bounds(&harness, cx, strip);
    assert!(
        (confirm.top() - row.bottom()).abs() < px(0.5),
        "{row:?} {confirm:?}"
    );
    assert!(
        (confirm.size.width - row.size.width).abs() < px(0.5),
        "{row:?} {confirm:?}"
    );
    assert!(
        confirm.size.height.as_f32() >= confirm_height(2) - 0.5,
        "{confirm:?}"
    );
    harness.update(cx, |window, cx| {
        window.within(strip).click("browser-confirm-cancel", cx)
    });
    harness.draw(cx);
    assert!(!found(&harness, cx, strip));
    assert!(cx.read(|cx| harness.store.read(cx).workspace.group(group).is_some()));

    pick_delete(&harness, cx, ("browser-group", group));
    harness.update(cx, |window, cx| {
        window.within(strip).click("browser-confirm-delete", cx)
    });
    harness.draw(cx);
    assert!(!found(&harness, cx, strip));
    cx.read(|cx| {
        let workspace = &harness.store.read(cx).workspace;
        assert!(workspace.group(group).is_none());
        // Its request moves to the top level.
        assert_eq!(workspace.session(first).unwrap().group_id, None);
    });
}

#[gpui_kit::test]
fn menu_rows_match_the_vue_menu_sizes(cx: &mut TestAppContext) {
    let (_dir, harness, first, _) = setup(cx);
    open_menu(&harness, cx, ("browser-request", first));
    harness.update(cx, |window, _| {
        let menu = window.within("popup-menu");
        // "Open" and "Duplicate": 24 px rows (`h-6`) with no gap between them.
        let open = menu.find(0usize).bounds();
        let duplicate = menu.find(1usize).bounds();
        assert!((open.size.height - px(24.)).abs() < px(0.01), "{open:?}");
        assert!(
            (duplicate.size.height - px(24.)).abs() < px(0.01),
            "{duplicate:?}"
        );
        assert!((duplicate.top() - open.bottom()).abs() < px(0.01));
        // `min-w-45` inside the `p-1` menu padding.
        assert!(open.size.width >= px(180. - 8.), "{open:?}");
        let popup = window.find("popup-menu").bounds();
        assert!(popup.size.width >= px(180.), "{popup:?}");
    });
}
