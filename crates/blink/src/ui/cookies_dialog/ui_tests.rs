//! Headless flow: response cookies are kept, sent, persisted, and managed in
//! the real cookies dialog.

use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};

use super::CookieJar;
use crate::actions::ManageCookies;
use crate::test_support::{self, Reply, serve};

/// The Cookie header of a raw request, if any.
fn cookie_header(request: &[u8]) -> Option<String> {
    String::from_utf8_lossy(request).lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("cookie")
            .then(|| value.trim().to_string())
    })
}

fn cookie_server() -> (String, std::sync::mpsc::Receiver<Vec<u8>>) {
    serve(|request| {
        let reply = Reply::ok("text/plain", b"ok".to_vec());
        if test_support::find(request, b"GET /login").is_some() {
            reply
                .header("Set-Cookie: session=abc123; Path=/; Max-Age=3600")
                .header("Set-Cookie: theme=dark; Path=/")
        } else {
            reply
        }
    })
}

#[gpui_kit::test]
fn keeps_sends_persists_and_manages_cookies(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let (base, requests) = cookie_server();
    let harness = test_support::open(cx, &engine);

    harness.send(cx, &format!("{base}/login"));
    assert_eq!(cookie_header(&requests.recv().unwrap()), None);
    harness.send(cx, &format!("{base}/me"));
    let sent = cookie_header(&requests.recv().unwrap()).expect("the next request sends cookies");
    assert!(sent.contains("session=abc123") && sent.contains("theme=dark"), "{sent}");
    assert!(dir.path().join("cookies.json").is_file());

    // A restart on the same data dir keeps them.
    let restarted = test_support::engine(dir.path());
    let names = |engine: &blink_core::engine::Engine| {
        let mut names: Vec<_> = engine.list_cookies().into_iter().map(|c| c.name).collect();
        names.sort();
        names
    };
    assert_eq!(names(&restarted), ["session", "theme"]);
    let second = test_support::open(cx, &restarted);
    second.send(cx, &format!("{base}/me"));
    let sent = cookie_header(&requests.recv().unwrap()).expect("sent after a restart");
    assert!(sent.contains("session=abc123"), "{sent}");

    // Manage Cookies opens the dialog.
    second.dispatch(cx, ManageCookies);
    assert!(second.update(cx, |window, cx| window.has_active_dialog(cx)));

    // The dialog lists them; delete one, then clear all.
    let store = second.store.clone();
    let jar = second.update(cx, |window, cx| cx.new(|cx| CookieJar::new(store, window, cx)));
    let listed = |cx: &TestAppContext| {
        cx.read(|cx| {
            let mut names: Vec<_> = jar.read(cx).cookies.iter().map(|c| c.name.clone()).collect();
            names.sort();
            names
        })
    };
    assert_eq!(listed(cx), ["session", "theme"]);
    let theme = cx.read(|cx| {
        jar.read(cx)
            .cookies
            .iter()
            .find(|c| c.name == "theme")
            .unwrap()
            .clone()
    });
    jar.update(cx, |jar, cx| jar.delete(&theme, cx));
    assert_eq!(listed(cx), ["session"]);
    assert_eq!(names(&restarted), ["session"]);
    // The deletion is saved.
    assert_eq!(names(&test_support::engine(dir.path())), ["session"]);
    jar.update(cx, |jar, cx| jar.clear(cx));
    assert!(listed(cx).is_empty());
    assert!(cx.read(|cx| jar.read(cx).error.is_empty()));
    assert!(test_support::engine(dir.path()).list_cookies().is_empty());

    // Close the dialog; the request takes the keys again.
    second.update(cx, |window, cx| window.close_dialog(cx));
    second.draw(cx);
    assert!(!second.update(cx, |window, cx| window.has_active_dialog(cx)));

    // Store cookies off: nothing is kept or sent.
    second.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.preferences.transport.store_cookies = false;
        });
    });
    second.send(cx, &format!("{base}/login"));
    requests.recv().unwrap();
    assert!(restarted.list_cookies().is_empty());
    second.send(cx, &format!("{base}/me"));
    assert_eq!(cookie_header(&requests.recv().unwrap()), None);
}
