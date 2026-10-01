//! Headless flows through the real response panel: saving bodies,
//! formatting HTML, the JSON views, and selecting body text.

use core::prelude::v1::test;

use blink_core::model::JsonView;
use gpui_kit::{Entity, Modifiers, MouseButton, MouseDownEvent, TestAppContext, VisualTestContext};

use super::ResponsePanel;
use crate::actions::SendRequest;
use crate::test_support::{self, Harness, Reply, serve, wait};

fn panel(harness: &Harness, cx: &TestAppContext) -> Entity<ResponsePanel> {
    let id = harness.active_id(cx);
    cx.read(|cx| {
        harness
            .app
            .read(cx)
            .pane(id)
            .expect("pane")
            .read(cx)
            .response()
            .clone()
    })
}

/// Point the active request at `url`, send, and wait for the response.
fn send(harness: &Harness, cx: &mut TestAppContext, url: &str) {
    let url = url.to_string();
    harness.edit_draft(cx, |draft| draft.url = url);
    harness.dispatch(cx, SendRequest);
    let id = harness.active_id(cx);
    wait(cx, "response", |cx| {
        let session = harness.store.read(cx).workspace.session(id).unwrap();
        !session.busy && session.response.is_some()
    });
    harness.draw(cx);
}

/// Start "Save…", pick `path` in the simulated save dialog, wait for the write.
fn save_to(harness: &Harness, cx: &mut TestAppContext, path: Option<std::path::PathBuf>) -> String {
    let panel = panel(harness, cx);
    harness.update(cx, |window, cx| {
        panel.update(cx, |panel, cx| panel.save_body(window, cx));
    });
    assert!(
        cx.read(|cx| panel.read(cx).saving),
        "busy while the dialog is open"
    );
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(move |_| path);
    wait(cx, "save done", |cx| !panel.read(cx).saving);
    cx.read(|cx| panel.read(cx).save_error.clone())
}

#[gpui_kit::test]
fn saves_the_full_stored_body_byte_for_byte(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let big: Vec<u8> = (0..(4 * 1024 * 1024 + 123_457))
        .map(|i: u32| b"abcdefghij\n"[i as usize % 11])
        .collect();
    let binary: Vec<u8> = (0..70_000u32).map(|i| (i % 256) as u8).collect();
    let (big_reply, binary_reply) = (big.clone(), binary.clone());
    let (base, _requests) = serve(move |request| {
        if test_support::find(request, b"GET /big").is_some() {
            Reply::ok("text/plain", big_reply.clone())
        } else if test_support::find(request, b"GET /binary").is_some() {
            Reply::ok("application/octet-stream", binary_reply.clone())
        } else {
            Reply::ok("application/json", br#"{"ok":true}"#.to_vec())
        }
    });
    let harness = test_support::open(cx, &engine);
    let out = tempfile::tempdir().unwrap();

    // A body over the preview limit: the preview is cut, the file is whole.
    send(&harness, cx, &format!("{base}/big"));
    assert!(harness.session(cx, |s| s.response.as_ref().unwrap().is_truncated()));
    let path = out.path().join("big.txt");
    assert_eq!(save_to(&harness, cx, Some(path.clone())), "");
    assert_eq!(std::fs::read(&path).unwrap(), big);

    // A binary body.
    send(&harness, cx, &format!("{base}/binary"));
    assert!(harness.session(cx, |s| s.response.as_ref().unwrap().is_binary()));
    let path = out.path().join("binary.bin");
    assert_eq!(save_to(&harness, cx, Some(path.clone())), "");
    assert_eq!(std::fs::read(&path).unwrap(), binary);

    // Cancel: nothing is written and "Save…" is usable again.
    assert_eq!(save_to(&harness, cx, None), "");
    let shown = panel(&harness, cx);
    assert!(!cx.read(|cx| shown.read(cx).saving));

    // A complete text preview without a stored body (as after a restart).
    send(&harness, cx, &format!("{base}/json"));
    let id = harness.active_id(cx);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            let response = workspace
                .session_mut(id)
                .unwrap()
                .response
                .as_mut()
                .unwrap();
            response.body_id = None;
        });
    });
    let path = out.path().join("preview.json");
    assert_eq!(save_to(&harness, cx, Some(path.clone())), "");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), r#"{"ok":true}"#);

    // A binary preview without a stored body cannot be saved: no dialog.
    send(&harness, cx, &format!("{base}/binary"));
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            let response = workspace
                .session_mut(id)
                .unwrap()
                .response
                .as_mut()
                .unwrap();
            response.body_id = None;
        });
    });
    let panel = panel(&harness, cx);
    harness.update(cx, |window, cx| {
        panel.update(cx, |panel, cx| panel.save_body(window, cx));
    });
    assert!(!cx.did_prompt_for_new_path());
    assert!(!cx.read(|cx| panel.read(cx).saving));
}

#[gpui_kit::test]
fn pretty_formats_html_and_raw_shows_the_body(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let body = "<html><body><h1>Not Found</h1></body></html>";
    let (base, _requests) =
        serve(move |_| Reply::ok("text/html; charset=utf-8", body.as_bytes().to_vec()));
    let harness = test_support::open(cx, &engine);

    send(&harness, cx, &format!("{base}/missing"));
    assert!(
        harness.session(cx, |s| s.view.pretty),
        "pretty is the default"
    );
    let panel = panel(&harness, cx);
    let shown = |cx: &TestAppContext, pretty| cx.read(|cx| panel.read(cx).text(pretty).to_string());
    assert_eq!(
        shown(cx, true),
        "<html>\n  <body>\n    <h1>Not Found</h1>\n  </body>\n</html>"
    );
    assert_eq!(shown(cx, false), body);

    harness.update(cx, |window, cx| {
        panel.update(cx, |panel, cx| panel.toggle_pretty(window, cx));
    });
    assert!(!harness.session(cx, |s| s.view.pretty));
}

#[gpui_kit::test]
fn json_shows_as_a_tree_formatted_text_or_raw(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let body = r#"{"name":"blink","tags":[1,2]}"#;
    let (base, _requests) = serve(move |_| Reply::ok("application/json", body.as_bytes().to_vec()));
    let harness = test_support::open(cx, &engine);

    send(&harness, cx, &format!("{base}/item"));
    let panel = panel(&harness, cx);
    let tree = |cx: &TestAppContext| cx.read(|cx| panel.read(cx).tree_shown(cx));
    let code = |cx: &TestAppContext| {
        cx.read(|cx| {
            let panel = panel.read(cx);
            panel.code.read(cx).text().to_string()
        })
    };
    assert!(tree(cx), "the tree is the default pretty view");
    assert_eq!(harness.session(cx, |s| s.view.json_view), JsonView::Tree);

    let show = |view, cx: &mut TestAppContext| {
        panel.update(cx, |panel, cx| panel.set_json_view(view, cx));
        harness.draw(cx);
    };
    show(Some(JsonView::Formatted), cx);
    assert!(!tree(cx));
    assert!(harness.session(cx, |s| s.view.pretty));
    assert_eq!(
        code(cx),
        "{\n  \"name\": \"blink\",\n  \"tags\": [\n    1,\n    2\n  ]\n}"
    );

    // Raw keeps the chosen pretty view for the next time.
    show(None, cx);
    assert!(!tree(cx));
    assert!(!harness.session(cx, |s| s.view.pretty));
    assert_eq!(
        harness.session(cx, |s| s.view.json_view),
        JsonView::Formatted
    );
    assert_eq!(code(cx), body);

    show(Some(JsonView::Tree), cx);
    assert!(tree(cx));
}

#[gpui_kit::test]
fn the_pointer_selects_body_text_to_copy(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let (base, _requests) = serve(|request| {
        let body = if request.starts_with(b"GET /again") {
            "delta"
        } else {
            "alpha\nbeta\ngamma"
        };
        Reply::ok("text/plain", body.as_bytes().to_vec())
    });
    let harness = test_support::open(cx, &engine);
    send(&harness, cx, &format!("{base}/text"));
    let panel = panel(&harness, cx);
    let code = cx.read(|cx| panel.read(cx).code.clone());
    let at = |row, offset, cx: &TestAppContext| {
        cx.read(|cx| code.read(cx).position_of(row, offset))
            .expect("the row is drawn")
    };
    let mut visual = VisualTestContext::from_window(harness.window, cx);

    // Drag from "a|lpha" to "gam|ma", then copy.
    let (from, to) = (at(0, 1, &visual), at(2, 3, &visual));
    visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    visual.simulate_keystrokes("cmd-c");
    let copied = visual.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("lpha\nbeta\ngam"));

    // A double click selects a word; Select All selects every line.
    visual.simulate_event(MouseDownEvent {
        position: at(1, 2, &visual),
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    assert_eq!(
        visual.read(|cx| code.read(cx).selected_text()).as_deref(),
        Some("beta")
    );
    visual.simulate_keystrokes("cmd-a");
    assert_eq!(
        visual.read(|cx| code.read(cx).selected_text()).as_deref(),
        Some("alpha\nbeta\ngamma")
    );

    // A new response clears the selection.
    send(&harness, cx, &format!("{base}/again"));
    assert_eq!(cx.read(|cx| code.read(cx).selected_text()), None);
}

#[test]
fn save_folder_does_not_need_downloads() {
    let home = tempfile::tempdir().unwrap();
    // No Downloads folder: fall back to a folder that exists.
    let folder = super::save_directory(Some(home.path().as_os_str().to_owned()));
    assert!(folder.is_dir(), "{folder:?}");
    std::fs::create_dir(home.path().join("Downloads")).unwrap();
    let folder = super::save_directory(Some(home.path().as_os_str().to_owned()));
    assert_eq!(folder, home.path().join("Downloads"));
    assert!(super::save_directory(None).is_dir());
}

/// An event stream that sends one event every 40 ms until the client leaves.
fn serve_events() -> String {
    use std::io::Write as _;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            std::thread::spawn(move || {
                test_support::read_request(&mut stream);
                let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                            Cache-Control: no-cache\r\nConnection: close\r\n\r\n";
                if stream.write_all(head.as_bytes()).is_err() {
                    return;
                }
                for n in 1.. {
                    let event = format!("event: tick\nid: {n}\ndata: {{\"n\":{n}}}\n\n");
                    if stream
                        .write_all(event.as_bytes())
                        .and_then(|_| stream.flush())
                        .is_err()
                    {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(40));
                }
            });
        }
    });
    format!("http://{address}/events")
}

#[gpui_kit::test]
fn streams_live_events_and_cancel_keeps_them(cx: &mut TestAppContext) {
    use crate::actions::CancelRequest;
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let url = serve_events();
    let harness = test_support::open(cx, &engine);
    harness.edit_draft(cx, |draft| draft.url = url);
    harness.dispatch(cx, SendRequest);
    let id = harness.active_id(cx);

    // Live: the events arrive in the panel's live list while the send runs.
    wait(cx, "three live events", |cx| {
        let session = harness.store.read(cx).workspace.session(id).unwrap();
        session.busy && session.stream.as_ref().is_some_and(|s| s.events.len() >= 3)
    });
    harness.draw(cx);
    let panel = panel(&harness, cx);
    let (busy, live_rows, streamed) = cx.read(|cx| {
        let session = harness.store.read(cx).workspace.session(id).unwrap();
        let rows = panel.read(cx).live_events.rows();
        (
            session.busy,
            rows,
            session.stream.as_ref().unwrap().events.len(),
        )
    });
    assert!(busy);
    assert!(
        live_rows >= 3 && live_rows <= streamed,
        "{live_rows} of {streamed}"
    );
    assert!(harness.session(cx, |s| s.response.is_none()));

    // Cancel stops the stream and keeps the events as the response.
    harness.dispatch(cx, CancelRequest);
    wait(cx, "canceled", |cx| {
        !harness.store.read(cx).workspace.session(id).unwrap().busy
    });
    harness.draw(cx);
    let (tab, events, error) = harness.session(cx, |s| {
        let response = s
            .response
            .as_ref()
            .expect("the stream becomes the response");
        (
            s.view.response_tab.clone(),
            blink_core::sse::parse_sse(&response.body),
            s.error.clone(),
        )
    });
    assert_eq!(error, "");
    assert_eq!(tab, "events", "the Events tab opens after the stream");
    assert!(events.len() >= 3);
    assert_eq!(events[0].event, "tick");
    assert_eq!(events[0].data, r#"{"n":1}"#);
    let rows = cx.read(|cx| panel.read(cx).events.rows());
    assert_eq!(rows, events.len(), "the Events tab lists every kept event");
    assert!(harness.session(cx, |s| s.stream.is_none()));

    // The stream stays stopped: nothing new arrives.
    std::thread::sleep(std::time::Duration::from_millis(150));
    harness.draw(cx);
    let after = harness.session(cx, |s| s.response.as_ref().unwrap().body.clone());
    assert_eq!(blink_core::sse::parse_sse(&after).len(), events.len());
    // History has the canceled stream.
    assert_eq!(harness.session(cx, |s| s.history.len()), 1);
}
