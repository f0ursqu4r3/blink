//! Headless flow: the response tab and scroll position survive a restart.

use blink_core::ids::SESSIONS;
use gpui_kit::{
    AppContext as _, Bounds, Point, TestAppContext, WindowBounds, WindowOptions, px, size,
};

use super::BlinkApp;
use crate::store::Store;
use crate::test_support::{self, Reply, serve, wait};

const SCROLL: f64 = 360.0;

#[gpui_kit::test]
fn response_tab_and_scroll_survive_a_restart(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let lines: String = (1..=600).map(|n| format!("line {n}\n")).collect();
    let (base, _requests) = serve(move |_| Reply::ok("text/plain", lines.clone()));
    let first = test_support::open(cx, &engine);

    // Request A: body tab scrolled down. Request B: the Headers tab.
    first.send(cx, &format!("{base}/a"));
    let a = first.active_id(cx);
    first.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.session_mut(a).unwrap().view.response_scroll = SCROLL;
        });
    });
    first.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| workspace.create(None))
    });
    first.draw(cx);
    first.send(cx, &format!("{base}/b"));
    let b = first.active_id(cx);
    assert_ne!(a, b);
    first.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.session_mut(b).unwrap().view.response_tab = "headers".into();
        });
    });
    first.draw(cx);

    // Encode, and give the saved requests the ids the next placeholder
    // request takes, as a real restart does (ids start again at 1).
    let saved = cx.read(|cx| first.store.read(cx).workspace.clone());
    let base_id = SESSIONS.next() + 10_000;
    let mut restarted = saved.clone();
    for session in &mut restarted.sessions {
        session.id = if session.id == a { base_id } else { base_id + 1 };
    }
    restarted.open_ids = vec![base_id, base_id + 1];
    restarted.active_id = Some(base_id);
    engine.save_workspace_now(&restarted.encode()).unwrap();

    // Restart: a new engine, store, and app on the same data dir.
    let engine = test_support::engine(dir.path());
    SESSIONS.reserve(base_id - 1);
    let store = cx.new(|cx| Store::new(engine.clone(), cx));
    let placeholder = cx.read(|cx| store.read(cx).workspace.sessions[0].id);
    let for_window = store.clone();
    let (window, app) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(1280.), px(800.)),
                })),
                ..Default::default()
            },
            cx,
            move |window, cx| cx.new(|cx| BlinkApp::new(for_window, window, cx)),
        )
        .unwrap()
    });
    // A frame before the restore lands: no pane exists for the placeholder.
    cx.update_window(window, |_, window, cx| {
        window.draw(cx).clear(cx);
    })
    .unwrap();
    assert!(!cx.read(|cx| store.read(cx).ready));
    assert!(cx.read(|cx| app.read(cx).panes.is_empty()), "placeholder id {placeholder}");

    wait(cx, "restored", |cx| store.read(cx).ready);
    cx.update_window(window, |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();

    let view = |cx: &TestAppContext, id: u64| {
        cx.read(|cx| store.read(cx).workspace.session(id).unwrap().view.clone())
    };
    assert_eq!(view(cx, base_id).response_tab, "body");
    assert_eq!(view(cx, base_id).response_scroll, SCROLL);
    assert_eq!(view(cx, base_id + 1).response_tab, "headers");
    // The body view of A opens at the saved offset.
    let panel = cx.read(|cx| {
        app.read(cx)
            .panes
            .get(&base_id)
            .unwrap()
            .read(cx)
            .response()
            .clone()
    });
    assert_eq!(cx.read(|cx| panel.read(cx).body_scroll(cx)), SCROLL);
    // The restored response still shows.
    assert!(cx.read(|cx| {
        store
            .read(cx)
            .workspace
            .session(base_id)
            .unwrap()
            .response
            .as_ref()
            .is_some_and(|response| response.body.starts_with("line 1\n"))
    }));
    // Both restored requests have panes; the placeholder has none.
    let mut panes: Vec<u64> = cx.read(|cx| app.read(cx).panes.keys().copied().collect());
    panes.sort();
    assert_eq!(panes, [base_id, base_id + 1]);
}
