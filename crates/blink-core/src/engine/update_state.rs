//! Update choices that survive a restart: the release the user skipped.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChoices {
    /// Automatic checks do not offer this version again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_version: Option<String>,
}

pub struct UpdateState {
    path: PathBuf,
}

impl UpdateState {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The saved choices. A missing or corrupt file gives the defaults.
    pub fn load(&self) -> UpdateChoices {
        fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Replace the file atomically.
    pub fn save(&self, choices: &UpdateChoices) -> Result<(), String> {
        let error = || "Cannot save the update choices.".to_string();
        let directory = self.path.parent().ok_or_else(error)?;
        fs::create_dir_all(directory).map_err(|_| error())?;
        let content = serde_json::to_vec(choices).map_err(|_| error())?;
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
    fn choices_round_trip_and_default_when_missing_or_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let state = UpdateState::new(dir.path().join("data").join("update-state.json"));
        assert_eq!(state.load(), UpdateChoices::default());

        let choices = UpdateChoices {
            skipped_version: Some("0.2.0".into()),
        };
        state.save(&choices).unwrap();
        assert_eq!(state.load(), choices);

        fs::write(dir.path().join("data").join("update-state.json"), "{").unwrap();
        assert_eq!(state.load(), UpdateChoices::default());
    }
}
