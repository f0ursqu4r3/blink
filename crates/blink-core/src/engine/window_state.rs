//! The main window's position and size, saved so the next start restores it.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Window bounds in logical pixels, relative to the top left of the display
/// the window is on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
    /// The stable id of that display. None: the primary display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

pub struct WindowState {
    path: PathBuf,
}

impl WindowState {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The saved bounds. A missing, corrupt, or empty-sized file gives None.
    pub fn load(&self) -> Option<WindowBounds> {
        let bytes = fs::read(&self.path).ok()?;
        let bounds: WindowBounds = serde_json::from_slice(&bytes).ok()?;
        let finite = [bounds.x, bounds.y, bounds.width, bounds.height]
            .iter()
            .all(|value| value.is_finite());
        (finite && bounds.width > 0.0 && bounds.height > 0.0).then_some(bounds)
    }

    /// Replace the file atomically.
    pub fn save(&self, bounds: &WindowBounds) -> Result<(), String> {
        let error = || "Cannot save the window state.".to_string();
        let directory = self.path.parent().ok_or_else(error)?;
        fs::create_dir_all(directory).map_err(|_| error())?;
        let content = serde_json::to_vec(bounds).map_err(|_| error())?;
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
    fn window_bounds_round_trip() {
        let root = tempfile::tempdir().unwrap();
        let state = WindowState::new(root.path().join("data").join("window-state.json"));
        assert_eq!(state.load(), None);
        let bounds = WindowBounds {
            x: 10.0,
            y: 20.0,
            width: 1200.0,
            height: 800.0,
            maximized: true,
            display: Some("37D8832A-2D66-02CA-B9F7-8F30A301B230".into()),
        };
        state.save(&bounds).unwrap();
        assert_eq!(state.load(), Some(bounds));
    }

    #[test]
    fn a_file_without_a_display_loads_on_the_primary_display() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("window-state.json");
        fs::write(
            &path,
            r#"{"x":10,"y":20,"width":900,"height":700,"maximized":false}"#,
        )
        .unwrap();
        let bounds = WindowState::new(path).load().unwrap();
        assert_eq!(bounds.display, None);
        assert_eq!(bounds.width, 900.0);
    }

    #[test]
    fn invalid_window_state_loads_as_none() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("window-state.json");
        let state = WindowState::new(path.clone());
        fs::write(&path, "not json").unwrap();
        assert_eq!(state.load(), None);
        fs::write(
            &path,
            r#"{"x":0,"y":0,"width":0,"height":600,"maximized":false}"#,
        )
        .unwrap();
        assert_eq!(state.load(), None);
    }
}
