//! Shared fixtures for the headless UI integration tests: a real `Store` on
//! an `Engine` in a temp dir, the real `BlinkApp` in a test-platform window
//! (nothing opens on screen), and small local 127.0.0.1 servers.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use blink_core::engine::{Engine, Paths};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, Point, TestAppContext, Window,
    WindowBounds, WindowOptions, px, size,
};

use crate::store::Store;
use crate::ui::app::BlinkApp;

/// An engine with every file under `dir`.
pub fn engine(dir: &std::path::Path) -> Engine {
    Engine::new(Paths::with_base(dir.to_path_buf())).expect("engine")
}

/// Global setup the app does in `main` before the first window.
pub fn init(cx: &mut TestAppContext, engine: &Engine) {
    // Engine futures finish on tokio threads, outside the test scheduler.
    cx.executor().allow_parking();
    let engine = engine.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::init(cx);
        crate::theme::init(&engine, cx);
    });
}

pub struct Harness {
    pub store: Entity<Store>,
    pub window: AnyWindowHandle,
    pub app: Entity<BlinkApp>,
}

/// A ready store and the real app in a 1280x800 test window.
pub fn open(cx: &mut TestAppContext, engine: &Engine) -> Harness {
    let store = cx.new(|cx| Store::new(engine.clone(), cx));
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
        .expect("open test window")
    });
    let harness = Harness { store, window, app };
    wait(cx, "store ready", |cx| harness.store.read(cx).ready);
    harness.draw(cx);
    harness
}

impl Harness {
    /// Run `f` with the window.
    pub fn update<R>(
        &self,
        cx: &mut TestAppContext,
        f: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> R {
        cx.update_window(self.window, |_, window, cx| f(window, cx))
            .expect("window")
    }

    /// Render a frame, as the next display refresh would.
    pub fn draw(&self, cx: &mut TestAppContext) {
        cx.run_until_parked();
        self.update(cx, |window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }

    /// Dispatch an app action to the focused element.
    pub fn dispatch(&self, cx: &mut TestAppContext, action: impl gpui_kit::Action) {
        cx.dispatch_action(self.window, action);
    }

    pub fn active_id(&self, cx: &TestAppContext) -> u64 {
        cx.read(|cx| {
            self.store
                .read(cx)
                .workspace
                .shown_active_id()
                .expect("active request")
        })
    }

    /// Change the active request's draft through the store.
    pub fn edit_draft(
        &self,
        cx: &mut TestAppContext,
        change: impl FnOnce(&mut blink_core::model::Draft),
    ) {
        let id = self.active_id(cx);
        self.store.update(cx, |store, cx| {
            store.update_workspace(cx, |workspace| {
                change(&mut workspace.session_mut(id).expect("session").draft);
            });
            cx.emit(crate::store::StoreEvent::DraftReplaced(id));
        });
        cx.run_until_parked();
    }

    pub fn session<R>(
        &self,
        cx: &TestAppContext,
        read: impl FnOnce(&blink_core::model::RequestSession) -> R,
    ) -> R {
        let id = self.active_id(cx);
        cx.read(|cx| read(self.store.read(cx).workspace.session(id).expect("session")))
    }

    /// Point the active request at `url`, send it with the Send action, and
    /// wait for the result.
    pub fn send(&self, cx: &mut TestAppContext, url: &str) {
        let url = url.to_string();
        self.edit_draft(cx, |draft| draft.url = url);
        self.send_draft(cx);
    }

    /// Send the active draft as it is and wait for the result.
    pub fn send_draft(&self, cx: &mut TestAppContext) {
        let id = self.active_id(cx);
        let before = cx.read(|cx| {
            self.store
                .read(cx)
                .workspace
                .session(id)
                .unwrap()
                .history
                .len()
        });
        self.dispatch(cx, crate::actions::SendRequest);
        wait(cx, "send finished", |cx| {
            let session = self.store.read(cx).workspace.session(id).unwrap();
            !session.busy && session.history.len() > before
        });
        self.draw(cx);
    }

    /// The response panel of the active request.
    pub fn response_panel(
        &self,
        cx: &TestAppContext,
    ) -> Entity<crate::ui::response_panel::ResponsePanel> {
        let id = self.active_id(cx);
        cx.read(|cx| {
            self.app
                .read(cx)
                .pane(id)
                .expect("pane")
                .read(cx)
                .response()
                .clone()
        })
    }
}

/// Run the app until `done`, letting engine threads finish between rounds.
pub fn wait(cx: &mut TestAppContext, what: &str, mut done: impl FnMut(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        cx.run_until_parked();
        if cx.read(|cx| done(cx)) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(5));
    }
}

/// Read one HTTP/1.1 request: head and a Content-Length body.
pub fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let read = match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(end) = find(&bytes, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                break;
            }
        }
    }
    bytes
}

pub fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// A response: status line, extra header lines, body.
pub struct Reply {
    pub status: &'static str,
    pub headers: Vec<String>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn ok(content_type: &str, body: impl Into<Vec<u8>>) -> Self {
        Reply {
            status: "200 OK",
            headers: vec![format!("Content-Type: {content_type}")],
            body: body.into(),
        }
    }

    pub fn header(mut self, line: impl Into<String>) -> Self {
        self.headers.push(line.into());
        self
    }
}

/// A server that answers every connection with `reply(request)` and reports
/// each raw request on the returned channel. Returns its base URL.
pub fn serve(reply: impl Fn(&[u8]) -> Reply + Send + 'static) -> (String, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let request = read_request(&mut stream);
            let answer = reply(&request);
            let _ = tx.send(request);
            let mut head = format!(
                "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                answer.status,
                answer.body.len()
            );
            for line in &answer.headers {
                head.push_str(line);
                head.push_str("\r\n");
            }
            head.push_str("\r\n");
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&answer.body);
            let _ = stream.flush();
        }
    });
    (format!("http://{address}"), rx)
}
