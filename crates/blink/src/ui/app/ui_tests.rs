//! Headless flows: the response tab and scroll position survive a restart;
//! the Browser slides open and closed and resizes.

use blink_core::ids::SESSIONS;
use gpui_kit::{
    AppContext as _, Bounds, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext, WindowBounds, WindowOptions, point, px, size,
};

use super::{BlinkApp, FRAME_GAP};
use crate::actions::ToggleBrowser;
use crate::store::Store;
use crate::test_support::{self, Reply, serve, wait};
use crate::ui::browser;

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
        session.id = if session.id == a {
            base_id
        } else {
            base_id + 1
        };
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
    assert!(
        cx.read(|cx| app.read(cx).panes.is_empty()),
        "placeholder id {placeholder}"
    );

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

/// The Browser box after the frames that run until the app is idle.
fn browser_box(harness: &test_support::Harness, cx: &mut TestAppContext) -> Option<Bounds<Pixels>> {
    harness.draw(cx);
    VisualTestContext::from_window(harness.window, cx).debug_bounds("browser-box")
}

#[gpui_kit::test]
fn the_browser_slides_closed_and_open(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let open = px(browser::WIDTH + FRAME_GAP);
    assert_eq!(browser_box(&harness, cx).unwrap().size.width, open);

    // The test platform draws the slide frames until the slide ends.
    harness.dispatch(cx, ToggleBrowser);
    assert_eq!(browser_box(&harness, cx), None);
    harness.dispatch(cx, ToggleBrowser);
    assert_eq!(browser_box(&harness, cx).unwrap().size.width, open);
}

#[gpui_kit::test]
fn dragging_the_handle_resizes_the_browser_within_limits(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let width = |cx: &mut TestAppContext| cx.read(|cx| harness.app.read(cx).browser_width);

    let drag = |to: f32, cx: &mut TestAppContext| {
        harness.draw(cx);
        let mut visual = VisualTestContext::from_window(harness.window, cx);
        let handle = visual
            .debug_bounds("browser-resize")
            .expect("the handle is painted");
        let from = handle.center();
        visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        visual.simulate_mouse_move(
            point(from.x + px(4.), from.y),
            MouseButton::Left,
            Modifiers::none(),
        );
        visual.simulate_mouse_move(point(px(to), from.y), MouseButton::Left, Modifiers::none());
        visual.simulate_mouse_up(point(px(to), from.y), MouseButton::Left, Modifiers::none());
    };

    // The pointer holds the middle of the gap after the Browser.
    drag(FRAME_GAP + 300. + FRAME_GAP / 2., cx);
    assert_eq!(width(cx), 300.);
    drag(1200., cx);
    assert_eq!(width(cx), browser::MAX_WIDTH);
    drag(20., cx);
    assert_eq!(width(cx), browser::MIN_WIDTH);
    let shown = browser_box(&harness, cx).unwrap().size.width;
    assert_eq!(shown, px(browser::MIN_WIDTH + FRAME_GAP));
}
