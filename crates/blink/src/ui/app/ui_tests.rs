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
use crate::test_support::{self, Harness, Reply, engine, init, open, serve, wait};
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

#[gpui_kit::test]
fn a_2xx_send_of_a_source_request_records_its_token_values(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let (url, _requests) = serve(|_| Reply::ok("application/json", r#"{"access_token":"abc"}"#));
    let login = harness.active_id(cx);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.set_global_response_tokens(vec![blink_core::model::ResponseToken {
                id: 1,
                name: "access_token".into(),
                request_id: login,
                source: blink_core::model::CheckSource::Json,
                path: ".access_token".into(),
                max_age_secs: None,
            }]);
        })
    });
    harness.send(cx, &format!("{url}/login"));
    let value = cx.read(|cx| {
        let workspace = &harness.store.read(cx).workspace;
        workspace
            .token_sources(blink_core::history::now_ms())
            .context(None)
            .workspace_definitions
            .get("access_token")
            .cloned()
    });
    assert_eq!(value.as_deref(), Some("abc"));
    wait_for_saved_entries(cx, &engine, 1);
}

fn source_token(request_id: u64) -> blink_core::model::ResponseToken {
    blink_core::model::ResponseToken {
        id: 1,
        name: "access_token".into(),
        request_id,
        source: blink_core::model::CheckSource::Json,
        path: ".access_token".into(),
        max_age_secs: None,
    }
}

/// Wait until the cache file holds `count` entries.
fn wait_for_saved_entries(
    cx: &mut TestAppContext,
    engine: &blink_core::engine::Engine,
    count: usize,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while engine.load_response_tokens().entries().len() != count {
        assert!(
            std::time::Instant::now() < deadline,
            "cache never held {count} entries"
        );
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[gpui_kit::test]
fn a_non_2xx_send_of_a_source_request_records_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let (url, _requests) = serve(|_| {
        let mut reply = Reply::ok("application/json", r#"{"access_token":"abc"}"#);
        reply.status = "500 Internal Server Error";
        reply
    });
    let login = harness.active_id(cx);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.set_global_response_tokens(vec![source_token(login)]);
        })
    });
    harness.send(cx, &format!("{url}/login"));
    let recorded = cx.read(|cx| {
        harness
            .store
            .read(cx)
            .workspace
            .response_cache
            .entries()
            .len()
    });
    assert_eq!(recorded, 0);
    assert!(engine.load_response_tokens().entries().is_empty());
}

#[gpui_kit::test]
fn reset_clears_the_cache_on_disk(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let (url, _requests) = serve(|_| Reply::ok("application/json", r#"{"access_token":"abc"}"#));
    let login = harness.active_id(cx);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.set_global_response_tokens(vec![source_token(login)]);
        })
    });
    harness.send(cx, &format!("{url}/login"));
    wait_for_saved_entries(cx, &engine, 1);
    harness.store.update(cx, |store, cx| store.reset(cx));
    wait_for_saved_entries(cx, &engine, 0);
}

#[gpui_kit::test]
fn restore_prunes_cache_entries_of_missing_requests(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    let mut cache = blink_core::response_token_cache::ResponseTokenCache::default();
    cache.record(
        999_999,
        "fingerprint",
        1,
        vec![(
            blink_core::response_token_cache::ValueKey {
                source: blink_core::model::CheckSource::Json,
                path: ".access_token".into(),
            },
            "abc".into(),
        )],
    );
    std::fs::write(
        dir.path().join("response-tokens.json"),
        serde_json::to_vec(&cache).unwrap(),
    )
    .unwrap();
    assert_eq!(engine.load_response_tokens().entries().len(), 1);
    test_support::init(cx, &engine);
    let harness = test_support::open(cx, &engine);
    let kept = cx.read(|cx| {
        harness
            .store
            .read(cx)
            .workspace
            .response_cache
            .entries()
            .len()
    });
    assert_eq!(kept, 0);
}

/// A Login request at `/login` and a dependent at `/me` that sends
/// `Authorization: Bearer {{access_token}}`. Returns (login, me).
fn login_and_me(harness: &Harness, cx: &mut TestAppContext, url: &str) -> (u64, u64) {
    let login = harness.active_id(cx);
    let url = url.to_string();
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.session_mut(login).unwrap().draft.url = format!("{url}/login");
            workspace.set_global_response_tokens(vec![source_token(login)]);
            let mut me = blink_core::session::create_session(None);
            me.draft.url = format!("{url}/me");
            me.draft.local_auth = Some(blink_core::model::AuthorizationConfig::Bearer {
                token: "{{access_token}}".into(),
            });
            let id = me.id;
            workspace.sessions.push(me);
            workspace.open_request(id);
        })
    });
    cx.run_until_parked();
    (login, harness.active_id(cx))
}

/// The paths of the requests the server saw since the last call.
fn paths(requests: &std::sync::mpsc::Receiver<Vec<u8>>) -> Vec<String> {
    requests
        .try_iter()
        .map(|raw| {
            String::from_utf8_lossy(&raw)
                .split(' ')
                .nth(1)
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

#[gpui_kit::test]
fn sends_the_source_request_first_and_uses_its_token(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    init(cx, &engine);
    let harness = open(cx, &engine);
    let (url, requests) = serve(|raw| {
        if raw.starts_with(b"POST /login") || raw.starts_with(b"GET /login") {
            Reply::ok("application/json", r#"{"access_token":"abc"}"#)
        } else {
            Reply::ok("text/plain", "me")
        }
    });
    let (_, me) = login_and_me(&harness, cx, &url);
    harness.send_draft(cx);
    let seen = paths(&requests);
    assert_eq!(seen, vec!["/login", "/me"]);
    // The second send reuses the cached token.
    harness.send_draft(cx);
    assert_eq!(paths(&requests), vec!["/me"]);
    let raw = harness.session(cx, |s| s.response.as_ref().map(|r| r.status));
    assert_eq!(raw, Some(200));
    assert_eq!(harness.active_id(cx), me);
}

#[gpui_kit::test]
fn a_failed_source_request_stops_the_dependent(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    init(cx, &engine);
    let harness = open(cx, &engine);
    let (url, requests) = serve(|raw| {
        if raw.windows(6).any(|w| w == b"/login") {
            Reply {
                status: "401 Unauthorized",
                headers: vec![],
                body: b"no".to_vec(),
            }
        } else {
            Reply::ok("text/plain", "me")
        }
    });
    let (_, me) = login_and_me(&harness, cx, &url);
    harness.dispatch(cx, crate::actions::SendRequest);
    wait(cx, "dependent failed", |cx| {
        let session = harness
            .store
            .read(cx)
            .workspace
            .session(me)
            .unwrap()
            .clone();
        session.waiting_on.is_none() && !session.error.is_empty()
    });
    assert_eq!(paths(&requests), vec!["/login"]);
    assert_eq!(
        harness.session(cx, |s| s.error.clone()),
        "Could not get \"access_token\": \"/login\" returned 401."
    );
}

#[gpui_kit::test]
fn a_missing_path_stops_the_dependent_without_a_loop(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    init(cx, &engine);
    let harness = open(cx, &engine);
    let (url, requests) = serve(|_| Reply::ok("application/json", r#"{"other":1}"#));
    let (_, me) = login_and_me(&harness, cx, &url);
    harness.dispatch(cx, crate::actions::SendRequest);
    wait(cx, "dependent failed", |cx| {
        let session = harness
            .store
            .read(cx)
            .workspace
            .session(me)
            .unwrap()
            .clone();
        session.waiting_on.is_none() && !session.error.is_empty()
    });
    assert_eq!(paths(&requests), vec!["/login"]);
    assert!(harness.session(cx, |s| s.error.contains("has no value at .access_token")));
}

#[gpui_kit::test]
fn two_dependents_share_one_source_send(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    init(cx, &engine);
    let harness = open(cx, &engine);
    let (url, requests) = serve(|raw| {
        if raw.windows(6).any(|w| w == b"/login") {
            std::thread::sleep(std::time::Duration::from_millis(200));
            Reply::ok("application/json", r#"{"access_token":"abc"}"#)
        } else {
            Reply::ok("text/plain", "ok")
        }
    });
    let (_, me) = login_and_me(&harness, cx, &url);
    let other = harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            let mut copy = workspace.session(me).unwrap().clone();
            copy.id = SESSIONS.next();
            let id = copy.id;
            workspace.sessions.push(copy);
            id
        })
    });
    harness.update(cx, |window, cx| {
        harness.store.update(cx, |store, cx| {
            store.send(me, window, cx);
            store.send(other, window, cx);
        })
    });
    wait(cx, "both sent", |cx| {
        let ws = &harness.store.read(cx).workspace;
        [me, other]
            .iter()
            .all(|id| ws.session(*id).unwrap().response.is_some())
    });
    let seen = paths(&requests);
    assert_eq!(
        seen.iter().filter(|p| p.as_str() == "/login").count(),
        1,
        "{seen:?}"
    );
}

#[gpui_kit::test]
fn cancel_while_waiting_stops_the_dependent_and_its_source(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    init(cx, &engine);
    let harness = open(cx, &engine);
    let (url, _requests) = serve(|raw| {
        if raw.windows(6).any(|w| w == b"/login") {
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
        Reply::ok("application/json", r#"{"access_token":"abc"}"#)
    });
    let (login, me) = login_and_me(&harness, cx, &url);
    harness.dispatch(cx, crate::actions::SendRequest);
    wait(cx, "waiting on login", |cx| {
        let workspace = &harness.store.read(cx).workspace;
        workspace.session(me).unwrap().waiting_on.as_deref() == Some("/login")
            && workspace.session(login).unwrap().busy
    });
    harness.draw(cx);
    // Send while waiting is Cancel.
    harness.dispatch(cx, crate::actions::SendRequest);
    wait(cx, "login cancelled", |cx| {
        !harness
            .store
            .read(cx)
            .workspace
            .session(login)
            .unwrap()
            .busy
    });
    let session = cx.read(|cx| {
        harness
            .store
            .read(cx)
            .workspace
            .session(me)
            .unwrap()
            .clone()
    });
    assert_eq!(session.waiting_on, None);
    assert_eq!(
        session.error,
        "Could not get \"access_token\": \"/login\" was cancelled."
    );
    assert!(session.response.is_none());
}
