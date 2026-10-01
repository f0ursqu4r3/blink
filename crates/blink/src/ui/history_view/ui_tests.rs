//! Headless flow through the real history view: two sends, compare, clear.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::TestAppContext;

use super::{diff_sections, open_pair};
use crate::test_support::{self, Reply, serve};

#[gpui_kit::test]
fn compares_two_sends_then_clears_history(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let count = Arc::new(AtomicUsize::new(0));
    let served = count.clone();
    let (base, _requests) = serve(move |_| {
        let n = served.fetch_add(1, Ordering::SeqCst) + 1;
        Reply::ok("application/json", format!("{{\"n\":{n},\"same\":true}}"))
            .header(format!("X-Run: {n}"))
    });
    let harness = test_support::open(cx, &engine);
    harness.send(cx, &format!("{base}/items"));
    harness.send(cx, &format!("{base}/items"));
    let history = harness.session(cx, |s| s.history.clone());
    assert_eq!(history.len(), 2);
    // Newest first.
    assert!(history[0].body.contains("\"n\":2"));
    assert!(history[1].body.contains("\"n\":1"));

    // Open the history from the response panel.
    let panel = harness.response_panel(cx);
    harness.update(cx, |window, cx| {
        panel.update(cx, |panel, cx| panel.toggle_history(window, cx));
    });
    harness.draw(cx);
    let view = cx.read(|cx| panel.read(cx).history_view().clone());
    assert!(cx.read(|cx| panel.read(cx).history_shown()));

    // Clicking the newest entry compares it with the send before it.
    let pair = open_pair(&history, 0);
    assert_eq!(pair, Some((history[1].id, history[0].id)));
    view.update(cx, |view, cx| view.compare(pair, cx));
    harness.draw(cx);
    assert_eq!(cx.read(|cx| view.read(cx).comparing), pair);
    let sections = diff_sections(&history[1], &history[0]);
    let titles: Vec<_> = sections.iter().map(|s| (s.title, s.changed)).collect();
    assert_eq!(titles, [("Headers", true), ("Body", true)]);

    // Clear history: the list is empty and the comparison has nothing to show.
    view.update(cx, |view, cx| {
        view.comparing = None;
        view.clear(cx);
    });
    harness.draw(cx);
    assert!(harness.session(cx, |s| s.history.is_empty()));
    // The response itself stays.
    assert!(harness.session(cx, |s| s.response.is_some()));
}
