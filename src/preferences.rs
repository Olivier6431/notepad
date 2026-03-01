use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{DEFAULT_FONT_SIZE, DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH};

fn dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

// --- User preferences ---

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct UserPreferences {
    pub font_size: f32,
    pub dark_mode: bool,
    pub word_wrap: bool,
    pub window_width: f32,
    pub window_height: f32,
    pub restore_session: bool,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            font_size: DEFAULT_FONT_SIZE,
            dark_mode: false,
            word_wrap: true,
            window_width: DEFAULT_WINDOW_WIDTH,
            window_height: DEFAULT_WINDOW_HEIGHT,
            restore_session: true,
        }
    }
}

impl UserPreferences {
    pub fn path() -> PathBuf {
        dir().join("preferences.json")
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

// --- Session data ---

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionTab {
    pub file_path: Option<PathBuf>,
    pub unsaved_content: Option<String>,
    pub is_modified: bool,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct SessionData {
    pub tabs: Vec<SessionTab>,
    pub active_tab: usize,
}

impl SessionData {
    pub fn path() -> PathBuf {
        dir().join("session.json")
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

    pub fn clear() {
        let _ = std::fs::remove_file(Self::path());
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
        assert!(prefs.word_wrap);
        assert_eq!(prefs.window_width, DEFAULT_WINDOW_WIDTH);
        assert_eq!(prefs.window_height, DEFAULT_WINDOW_HEIGHT);
        assert!(prefs.restore_session);
    }

    #[test]
    fn serde_round_trip() {
        let prefs = UserPreferences {
            font_size: 18.0,
            dark_mode: true,
            word_wrap: false,
            window_width: 1024.0,
            window_height: 768.0,
            restore_session: false,
        };
        let json = serde_json::to_string(&prefs).unwrap();
        let restored: UserPreferences = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.font_size, 18.0);
        assert!(restored.dark_mode);
        assert!(!restored.word_wrap);
        assert_eq!(restored.window_width, 1024.0);
        assert_eq!(restored.window_height, 768.0);
        assert!(!restored.restore_session);
    }

    #[test]
    fn serde_backwards_compat() {
        // Old preferences.json without restore_session should deserialize with default
        let json = r#"{"font_size":14.0,"dark_mode":false,"word_wrap":true,"window_width":800.0,"window_height":600.0}"#;
        let prefs: UserPreferences = serde_json::from_str(json).unwrap();
        assert!(prefs.restore_session);
    }

    #[test]
    fn load_missing_file_returns_defaults() {
        let prefs = UserPreferences::load();
        assert_eq!(prefs.font_size, DEFAULT_FONT_SIZE);
    }

    #[test]
    fn session_data_round_trip() {
        let session = SessionData {
            tabs: vec![
                SessionTab {
                    file_path: Some(PathBuf::from("/tmp/test.txt")),
                    unsaved_content: None,
                    is_modified: false,
                },
                SessionTab {
                    file_path: None,
                    unsaved_content: Some("hello world".to_string()),
                    is_modified: true,
                },
            ],
            active_tab: 1,
        };
        let json = serde_json::to_string(&session).unwrap();
        let restored: SessionData = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.tabs.len(), 2);
        assert_eq!(
            restored.tabs[0].file_path,
            Some(PathBuf::from("/tmp/test.txt"))
        );
        assert!(restored.tabs[0].unsaved_content.is_none());
        assert!(!restored.tabs[0].is_modified);
        assert!(restored.tabs[1].file_path.is_none());
        assert_eq!(
            restored.tabs[1].unsaved_content.as_deref(),
            Some("hello world")
        );
        assert!(restored.tabs[1].is_modified);
        assert_eq!(restored.active_tab, 1);
    }

    #[test]
    fn session_data_default_empty() {
        let session = SessionData::default();
        assert!(session.tabs.is_empty());
        assert_eq!(session.active_tab, 0);
    }
}
