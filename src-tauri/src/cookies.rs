//! The cookie jar: shared by all requests when the setting is on, saved to
//! the app data folder, and listed or cleared from the frontend.

use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;

use cookie_store::{CookieExpiration, CookieStore};
use reqwest_cookie_store::CookieStoreMutex;
use serde::Serialize;

pub struct Cookies {
    jar: Arc<CookieStoreMutex>,
    path: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieOutput {
    domain: String,
    path: String,
    name: String,
    value: String,
    /// Epoch milliseconds. None for a session cookie.
    expires: Option<i64>,
    secure: bool,
    http_only: bool,
}

impl Cookies {
    /// Read the saved jar. A missing or unreadable file starts empty.
    pub fn load(path: PathBuf) -> Self {
        let store = std::fs::File::open(&path)
            .ok()
            .and_then(|file| cookie_store::serde::json::load_all(BufReader::new(file)).ok())
            .unwrap_or_default();
        Self {
            jar: Arc::new(CookieStoreMutex::new(store)),
            path,
        }
    }

    pub fn jar(&self) -> Arc<CookieStoreMutex> {
        self.jar.clone()
    }

    /// Write the jar, session cookies included, so it survives a restart.
    pub fn save(&self) -> Result<(), String> {
        let mut bytes = Vec::new();
        {
            let store = self.jar.lock().map_err(|_| "Cookie jar is unavailable.")?;
            cookie_store::serde::json::save_incl_expired_and_nonpersistent(&store, &mut bytes)
                .map_err(|_| "Cannot save cookies.")?;
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| "Cannot save cookies.")?;
        }
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes).map_err(|_| "Cannot save cookies.")?;
        std::fs::rename(&temporary, &self.path).map_err(|_| "Cannot save cookies.".to_string())
    }

    pub fn list(&self) -> Vec<CookieOutput> {
        let Ok(store) = self.jar.lock() else {
            return Vec::new();
        };
        let mut cookies: Vec<CookieOutput> = store
            .iter_unexpired()
            .map(|cookie| CookieOutput {
                domain: String::from(&cookie.domain),
                path: cookie.path.as_ref().to_string(),
                name: cookie.name().to_string(),
                value: cookie.value().to_string(),
                expires: match &cookie.expires {
                    CookieExpiration::AtUtc(at) => {
                        Some((at.unix_timestamp_nanos() / 1_000_000) as i64)
                    }
                    CookieExpiration::SessionEnd => None,
                },
                secure: cookie.secure().unwrap_or(false),
                http_only: cookie.http_only().unwrap_or(false),
            })
            .collect();
        cookies.sort_by(|a, b| (&a.domain, &a.path, &a.name).cmp(&(&b.domain, &b.path, &b.name)));
        cookies
    }

    fn change(&self, edit: impl FnOnce(&mut CookieStore)) -> Result<(), String> {
        edit(&mut *self.jar.lock().map_err(|_| "Cookie jar is unavailable.")?);
        self.save()
    }
}

#[tauri::command]
pub fn list_cookies(cookies: tauri::State<'_, Cookies>) -> Vec<CookieOutput> {
    cookies.list()
}

#[tauri::command]
pub fn delete_cookie(
    domain: String,
    path: String,
    name: String,
    cookies: tauri::State<'_, Cookies>,
) -> Result<(), String> {
    cookies.change(|store| {
        store.remove(&domain, &path, &name);
    })
}

#[tauri::command]
pub fn clear_cookies(cookies: tauri::State<'_, Cookies>) -> Result<(), String> {
    cookies.change(CookieStore::clear)
}
