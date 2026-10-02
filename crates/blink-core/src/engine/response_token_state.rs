//! The response token value cache, saved in its own file.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::response_token_cache::ResponseTokenCache;

pub struct ResponseTokenState {
    path: PathBuf,
}

impl ResponseTokenState {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The saved cache. A missing or corrupt file gives the defaults.
    pub fn load(&self) -> ResponseTokenCache {
        fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Replace the file atomically.
    pub fn save(&self, cache: &ResponseTokenCache) -> Result<(), String> {
        let error = || "Cannot save the response tokens.".to_string();
        let directory = self.path.parent().ok_or_else(error)?;
        fs::create_dir_all(directory).map_err(|_| error())?;
        let content = serde_json::to_vec(cache).map_err(|_| error())?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory).map_err(|_| error())?;
        temporary.write_all(&content).map_err(|_| error())?;
        temporary.persist(&self.path).map_err(|_| error())?;
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
