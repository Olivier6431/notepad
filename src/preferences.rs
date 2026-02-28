use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{DEFAULT_FONT_SIZE, DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH};

#[derive(Serialize, Deserialize)]
pub struct UserPreferences {
    pub font_size: f32,
    pub dark_mode: bool,
    pub window_width: f32,
    pub window_height: f32,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            font_size: DEFAULT_FONT_SIZE,
            dark_mode: false,
            window_width: DEFAULT_WINDOW_WIDTH,
            window_height: DEFAULT_WINDOW_HEIGHT,
        }
    }
}

impl UserPreferences {
    pub fn path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("preferences.json")))
            .unwrap_or_else(|| PathBuf::from("preferences.json"))
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEFAULT_FONT_SIZE, DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH};

    #[test]
    fn default_values() {
        let prefs = UserPreferences::default();
        assert_eq!(prefs.font_size, DEFAULT_FONT_SIZE);
        assert!(!prefs.dark_mode);
        assert_eq!(prefs.window_width, DEFAULT_WINDOW_WIDTH);
        assert_eq!(prefs.window_height, DEFAULT_WINDOW_HEIGHT);
    }

    #[test]
    fn serde_round_trip() {
        let prefs = UserPreferences {
            font_size: 18.0,
            dark_mode: true,
            window_width: 1024.0,
            window_height: 768.0,
        };
        let json = serde_json::to_string(&prefs).unwrap();
        let restored: UserPreferences = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.font_size, 18.0);
        assert!(restored.dark_mode);
        assert_eq!(restored.window_width, 1024.0);
        assert_eq!(restored.window_height, 768.0);
    }

    #[test]
    fn load_missing_file_returns_defaults() {
        // UserPreferences::load() with no file on disk should not panic
        // and should return defaults (since the test binary path likely
        // has no preferences.json next to it)
        let prefs = UserPreferences::load();
        assert_eq!(prefs.font_size, DEFAULT_FONT_SIZE);
    }
}
