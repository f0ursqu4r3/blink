//! The response token value cache, saved in its own file.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::response_token_cache::ResponseTokenCache;

/// Each save takes a ticket when requested. A save older than the last
/// written one is skipped, so a slow save never replaces a newer cache.
#[derive(Clone)]
pub struct ResponseTokenState {
    path: PathBuf,
    written: Arc<Mutex<u64>>,
    tickets: Arc<AtomicU64>,
}

impl ResponseTokenState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            written: Arc::new(Mutex::new(0)),
            tickets: Arc::new(AtomicU64::new(0)),
        }
    }

    /// The saved cache. A missing or corrupt file gives the defaults.
    pub fn load(&self) -> ResponseTokenCache {
        fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Reserve the order of a save. Take it when the save is requested.
    pub fn ticket(&self) -> u64 {
        self.tickets.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Replace the file atomically, after every save requested before it.
    #[cfg(test)]
    pub fn save(&self, cache: &ResponseTokenCache) -> Result<(), String> {
        self.save_ticketed(self.ticket(), cache)
    }

    /// Blocking: skips the write when a newer cache is already on disk.
    pub fn save_ticketed(&self, ticket: u64, cache: &ResponseTokenCache) -> Result<(), String> {
        let error = || "Cannot save the response tokens.".to_string();
        let mut written = self.written.lock().map_err(|_| error())?;
        if ticket < *written {
            return Ok(());
        }
        let directory = self.path.parent().ok_or_else(error)?;
        fs::create_dir_all(directory).map_err(|_| error())?;
        let content = serde_json::to_vec(cache).map_err(|_| error())?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory).map_err(|_| error())?;
        temporary.write_all(&content).map_err(|_| error())?;
        temporary.persist(&self.path).map_err(|_| error())?;
        *written = ticket;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_round_trips_and_defaults_when_missing_or_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join("response-tokens.json");
        let state = ResponseTokenState::new(path.clone());
        assert_eq!(state.load(), ResponseTokenCache::default());
        let mut cache = ResponseTokenCache::default();
        cache.record(1, "f", 1, vec![]);
        state.save(&cache).unwrap();
        assert_eq!(state.load(), cache);
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(state.load(), ResponseTokenCache::default());
    }
}
