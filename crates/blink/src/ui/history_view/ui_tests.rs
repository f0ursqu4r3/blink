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

#[gpui_kit::test]
fn pinned_response_compares_across_requests_and_survives_clear(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let (base, _requests) =
        serve(|_| Reply::ok("application/json", r#"{"value":1,"timestamp":1}"#));
    let harness = test_support::open(cx, &engine);
    harness.send(cx, &format!("{base}/staging"));
    let panel = harness.response_panel(cx);
    harness.update(cx, |window, cx| {
        panel.update(cx, |panel, cx| panel.toggle_history(window, cx));
    });
    harness.draw(cx);
    let view = cx.read(|cx| panel.read(cx).history_view().clone());
    let baseline = harness.session(cx, |s| s.history[0].clone());
    view.update(cx, |view, cx| view.pin_baseline(baseline.clone(), cx));
    view.update(cx, |view, cx| view.clear(cx));
    assert!(harness.session(cx, |s| s.history.is_empty()));

    let mut other = baseline.clone();
    other.url = "https://production.test/other-request".into();
    other.body = r#"{"timestamp":2,"value":2}"#.into();
    view.update(cx, |view, cx| {
        let mut state = view.store.read(cx).comparison.clone();
        state.ignored_paths.push("/timestamp".into());
        view.save_comparison(state, cx);
        view.compare_baseline(other.clone(), cx);
    });
    harness.draw(cx);
    let pair = cx.read(|cx| view.read(cx).snapshot_pair.clone().unwrap());
    assert_eq!(pair.0, baseline);
    assert_eq!(pair.1, other);
    let saved =
        blink_core::response_comparison::ComparisonState::load(&engine.paths().data_dir).unwrap();
    let mut persisted = saved.baseline.unwrap().entry;
    // JSON decimal parsing can round fractional milliseconds by one ULP.
    assert!((persisted.sent_at - baseline.sent_at).abs() < 0.001);
    assert!((persisted.duration_ms - baseline.duration_ms).abs() < 0.000_001);
    if let (Some(actual), Some(expected)) = (persisted.timing, baseline.timing) {
        for (actual, expected) in [
            (actual.dns_ms, expected.dns_ms),
            (actual.connect_ms, expected.connect_ms),
            (actual.tls_ms, expected.tls_ms),
            (Some(actual.wait_ms), Some(expected.wait_ms)),
            (Some(actual.download_ms), Some(expected.download_ms)),
        ] {
            assert_eq!(actual.is_some(), expected.is_some());
            if let (Some(actual), Some(expected)) = (actual, expected) {
                assert!((actual - expected).abs() < 0.000_001);
            }
        }
    } else {
        assert_eq!(persisted.timing, baseline.timing);
    }
    persisted.sent_at = baseline.sent_at;
    persisted.duration_ms = baseline.duration_ms;
    persisted.timing = baseline.timing;
    assert_eq!(persisted, baseline);
    let changes =
        blink_core::response_comparison::compare_json(&pair.0, &pair.1, &saved.ignored_paths)
            .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path, "/value");
}
