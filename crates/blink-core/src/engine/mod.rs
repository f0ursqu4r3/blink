//! The request engine: port of the former Tauri backend in `src-tauri/src`.
//!
//! `Engine` owns a tokio runtime and all backend state. The UI runs on its
//! own executor, so every async method returns a `Send + 'static` future that
//! any executor can poll, and streams arrive on `futures` channels. Dropping
//! a returned future does not stop the work; cancel a request by its id.

pub mod cookies;
pub mod ghostty_themes;
pub mod http;
pub mod paths;
pub mod request_files;
pub mod response_store;
mod response_token_state;
pub mod storage;
pub mod timing;
pub mod update_state;
pub mod websocket;
pub mod window_state;

#[cfg(test)]
mod tests;

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

pub use cookies::StoredCookie;
pub use http::{CANCELED, StreamMessage};
pub use paths::Paths;
pub use request_files::PickedFile;
pub use update_state::UpdateChoices;
pub use websocket::SocketEvent;
pub use window_state::WindowBounds;

use crate::model::{ApiResponse, Header, RequestInput, TransportOptions};
use crate::response_token_cache::ResponseTokenCache;

/// A handle to the engine. Clones share one runtime and one state.
#[derive(Clone)]
pub struct Engine(Arc<Inner>);

struct Inner {
    /// Taken on drop to shut down without blocking.
    runtime: Option<tokio::runtime::Runtime>,
    paths: Paths,
    storage: storage::Storage,
    grants: Arc<request_files::FileGrants>,
    in_flight: Arc<http::InFlight>,
    sockets: websocket::Sockets,
    cookies: Arc<cookies::Cookies>,
    store: Arc<response_store::ResponseStore>,
    window_state: window_state::WindowState,
    update_state: update_state::UpdateState,
    response_tokens: response_token_state::ResponseTokenState,
    sequence: AtomicU64,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl Engine {
    /// Start with the platform data dirs, or `BLINK_DATA_DIR`.
    pub fn open() -> Result<Self, String> {
        Self::new(Paths::resolve()?)
    }

    /// Start with state in `paths`. Clears response bodies left by an
    /// earlier run.
    pub fn new(paths: Paths) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("blink-engine")
            .build()
            .map_err(|_| "Cannot start the request engine.".to_string())?;
        Ok(Self(Arc::new(Inner {
            runtime: Some(runtime),
            storage: storage::Storage::new(&paths.data_dir),
            grants: Arc::new(request_files::FileGrants::load(paths.file_grants())),
            in_flight: Arc::default(),
            sockets: websocket::Sockets::default(),
            cookies: Arc::new(cookies::Cookies::load(paths.cookies())),
            store: Arc::new(response_store::ResponseStore::new(paths.responses())),
            window_state: window_state::WindowState::new(paths.window_state()),
            update_state: update_state::UpdateState::new(paths.update_state()),
            response_tokens: response_token_state::ResponseTokenState::new(paths.response_tokens()),
            sequence: AtomicU64::new(0),
            paths,
        })))
    }

    pub fn paths(&self) -> &Paths {
        &self.0.paths
    }

    fn runtime(&self) -> &tokio::runtime::Runtime {
        self.0.runtime.as_ref().expect("engine runtime")
    }

    fn spawn<T: Send + 'static>(
        &self,
        task: impl Future<Output = Result<T, String>> + Send + 'static,
        failed: &'static str,
    ) -> impl Future<Output = Result<T, String>> + Send + 'static {
        let handle = self.runtime().spawn(task);
        async move { handle.await.unwrap_or_else(|_| Err(failed.to_string())) }
    }

    fn blocking<T: Send + 'static>(
        &self,
        task: impl FnOnce() -> Result<T, String> + Send + 'static,
        failed: &'static str,
    ) -> impl Future<Output = Result<T, String>> + Send + 'static {
        let handle = self.runtime().spawn_blocking(task);
        async move { handle.await.unwrap_or_else(|_| Err(failed.to_string())) }
    }

    /// A new id for `send_request` or `ws_connect`.
    pub fn next_id(&self, prefix: &str) -> String {
        let id = self.0.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        format!("{prefix}-{id}")
    }

    /// Send a request. An event stream response sends its head and chunks to
    /// `on_stream` and has no total timeout; only `cancel_request` ends it.
    pub fn send_request(
        &self,
        request: RequestInput,
        options: TransportOptions,
        request_id: String,
        on_stream: Option<UnboundedSender<StreamMessage>>,
    ) -> impl Future<Output = Result<ApiResponse, String>> + Send + 'static {
        let store = self.0.store.clone();
        let grants = self.0.grants.clone();
        let in_flight = self.0.in_flight.clone();
        let cookies = self.0.cookies.clone();
        self.spawn(
            async move {
                let jar = options.store_cookies.then(|| cookies.jar());
                let sink = on_stream.map(|sender| {
                    move |message: StreamMessage| {
                        let _ = sender.unbounded_send(message);
                    }
                });
                let sink = sink
                    .as_ref()
                    .map(|sink| sink as &(dyn Fn(StreamMessage) + Send + Sync));
                let result = in_flight
                    .run(
                        request_id,
                        http::execute(
                            request,
                            options,
                            &store,
                            &grants,
                            http::DOWNLOAD_LIMIT,
                            jar.clone(),
                            sink,
                        ),
                    )
                    .await;
                if jar.is_some() {
                    // A failed cookie save does not fail the request.
                    let _ = cookies.save();
                }
                result
            },
            "Request task failed.",
        )
    }

    pub fn cancel_request(&self, request_id: &str) {
        self.0.in_flight.cancel(request_id);
    }

    /// Delete a stored response body.
    pub fn release_response(&self, body_id: &str) {
        self.0.store.release(body_id);
    }

    /// Copy a stored response body to `path`, which the user picked.
    pub fn save_response(
        &self,
        body_id: String,
        path: PathBuf,
    ) -> impl Future<Output = Result<(), String>> + Send + 'static {
        let store = self.0.store.clone();
        self.blocking(
            move || store.copy_body(&body_id, &path),
            "Response save task failed.",
        )
    }

    /// Write a response preview to `path`, which the user picked.
    pub fn save_response_text(
        &self,
        text: String,
        path: PathBuf,
    ) -> impl Future<Output = Result<(), String>> + Send + 'static {
        self.blocking(
            move || std::fs::write(path, text).map_err(|error| error.to_string()),
            "Response save task failed.",
        )
    }

    /// Allow requests to read a file the user picked in the open dialog.
    pub fn grant_file(
        &self,
        path: PathBuf,
    ) -> impl Future<Output = Result<PickedFile, String>> + Send + 'static {
        let grants = self.0.grants.clone();
        self.blocking(move || grants.pick(&path), "File access task failed.")
    }

    pub fn list_cookies(&self) -> Vec<StoredCookie> {
        self.0.cookies.list()
    }

    pub fn delete_cookie(&self, domain: &str, path: &str, name: &str) -> Result<(), String> {
        self.0.cookies.delete(domain, path, name)
    }

    pub fn clear_cookies(&self) -> Result<(), String> {
        self.0.cookies.clear()
    }

    /// Open a WebSocket. Events arrive on the returned channel, which ends
    /// after a close or an error event.
    pub fn ws_connect(
        &self,
        connection_id: String,
        url: &str,
        headers: Vec<Header>,
        connect_timeout_seconds: u64,
    ) -> Result<UnboundedReceiver<SocketEvent>, String> {
        let (url, headers) = websocket::Sockets::prepare(url, headers)?;
        let (sender, events) = unbounded();
        let sockets = self.0.sockets.clone();
        let outgoing = sockets.insert(connection_id.clone());
        self.runtime().spawn(async move {
            websocket::run(
                url,
                headers,
                Duration::from_secs(connect_timeout_seconds.clamp(1, 600)),
                |event| {
                    let _ = sender.unbounded_send(event);
                },
                outgoing,
            )
            .await;
            sockets.remove(&connection_id);
        });
        Ok(events)
    }

    pub fn ws_send(&self, connection_id: &str, text: String) -> Result<(), String> {
        self.0.sockets.send(connection_id, text)
    }

    pub fn ws_close(&self, connection_id: &str) {
        self.0.sockets.close(connection_id);
    }

    pub fn list_ghostty_themes(&self) -> impl Future<Output = Vec<String>> + Send + 'static {
        let list = self.blocking(
            || Ok(ghostty_themes::list_ghostty_themes()),
            "Theme list task failed.",
        );
        async move { list.await.unwrap_or_default() }
    }

    pub fn read_ghostty_theme(
        &self,
        name: String,
    ) -> impl Future<Output = Result<String, String>> + Send + 'static {
        self.blocking(
            move || ghostty_themes::read_ghostty_theme(&name),
            "Theme read task failed.",
        )
    }

    /// The saved workspace JSON, or None before the first save.
    pub fn load_workspace(
        &self,
    ) -> impl Future<Output = Result<Option<String>, String>> + Send + 'static {
        let storage = self.0.storage.clone();
        self.blocking(move || storage.load(), "Workspace load task failed.")
    }

    /// Check and write the workspace JSON. A failed save keeps the last one.
    pub fn save_workspace(
        &self,
        content: String,
    ) -> impl Future<Output = Result<(), String>> + Send + 'static {
        let storage = self.0.storage.clone();
        let ticket = storage.ticket();
        self.blocking(
            move || storage.save_ticketed(ticket, &content),
            "Workspace save task failed.",
        )
    }

    /// Blocking: the workspace JSON, for a save on quit that must finish
    /// before the process exits.
    pub fn save_workspace_now(&self, content: &str) -> Result<(), String> {
        self.0.storage.save(content)
    }

    pub fn load_window_state(&self) -> Option<WindowBounds> {
        self.0.window_state.load()
    }

    pub fn save_window_state(&self, bounds: &WindowBounds) -> Result<(), String> {
        self.0.window_state.save(bounds)
    }

    pub fn load_response_tokens(&self) -> ResponseTokenCache {
        self.0.response_tokens.load()
    }

    pub fn save_response_tokens(&self, cache: &ResponseTokenCache) -> Result<(), String> {
        self.0.response_tokens.save(cache)
    }

    pub fn load_update_choices(&self) -> UpdateChoices {
        self.0.update_state.load()
    }

    pub fn save_update_choices(&self, choices: &UpdateChoices) -> Result<(), String> {
        self.0.update_state.save(choices)
    }
}
