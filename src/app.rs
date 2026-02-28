use iced::widget::{text_editor, text_input};
use iced::{Event, Subscription, Task, Theme};
use std::time::Duration;
use std::path::PathBuf;
use std::time::Instant;

use crate::preferences::UserPreferences;
use crate::{DEFAULT_FONT_SIZE, DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MAX_FONT_SIZE, MIN_FONT_SIZE};

pub const MAX_UNDO_HISTORY: usize = 200;
pub const UNDO_BATCH_TIMEOUT_MS: u128 = 300;
pub const MENU_BAR_HEIGHT: f32 = 30.0;
pub const MENU_ITEM_WIDTH: f32 = 220.0;

pub fn find_input_id() -> text_input::Id {
    text_input::Id::new("find_input")
}

pub fn replace_input_id() -> text_input::Id {
    text_input::Id::new("replace_input")
}

pub fn goto_input_id() -> text_input::Id {
    text_input::Id::new("goto_input")
}

pub struct TextSnapshot {
    pub text: String,
    pub cursor_line: usize,
    pub cursor_col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    File,
    Edit,
    Search,
    View,
}

#[derive(Debug, Clone)]
pub enum FileMsg {
    New,
    Save,
    SaveAs,
    Open,
    SaveFileSelected(Option<PathBuf>),
    OpenFileSelected(Option<PathBuf>),
    ConfirmNewResult(bool),
    ConfirmOpenResult(bool),
    CloseRequested(iced::window::Id),
    ConfirmCloseResult(bool, iced::window::Id),
    AutoSave,
}

#[derive(Debug, Clone)]
pub enum EditMsg {
    Copy,
    Cut,
    Paste,
    SelectAll,
    Undo,
    Redo,
    InsertDateTime,
}

#[derive(Debug, Clone)]
pub enum SearchMsg {
    OpenFind,
    OpenReplace,
    CloseFind,
    FindQueryChanged(String),
    ReplaceQueryChanged(String),
    FindNext,
    FindPrevious,
    ReplaceOne,
    ReplaceAll,
    OpenGoTo,
    CloseGoTo,
    GoToInputChanged(String),
    GoToLineSubmit,
    ToggleCaseSensitive,
    ToggleRegex,
}

#[derive(Debug, Clone)]
pub enum ViewMsg {
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleDarkMode,
}

#[derive(Debug, Clone)]
pub enum MenuMsg {
    Toggle(Menu),
    Hover(Menu),
    CloseAll,
    ShowContext,
}

#[derive(Debug, Clone)]
pub enum Message {
    EditorAction(text_editor::Action),
    EventOccurred(Event),
    File(FileMsg),
    Edit(EditMsg),
    Search(SearchMsg),
    View(ViewMsg),
    Menu(MenuMsg),
}

pub struct Notepad {
    pub content: text_editor::Content,
    pub file_path: Option<PathBuf>,
    pub is_modified: bool,
    pub clipboard: Option<arboard::Clipboard>,

    // View settings
    pub font_size: f32,
    pub dark_mode: bool,
    pub window_width: f32,
    pub window_height: f32,

    // Find & Replace
    pub show_find: bool,
    pub show_replace: bool,
    pub find_query: String,
    pub replace_query: String,
    pub find_cursor: usize,
    pub case_sensitive: bool,
    pub use_regex: bool,

    // Go to line
    pub show_goto: bool,
    pub goto_input: String,

    // Line numbers scroll sync
    pub scroll_offset: f32,

    // Undo/Redo
    pub undo_stack: Vec<TextSnapshot>,
    pub redo_stack: Vec<TextSnapshot>,
    pub last_edit_time: Option<Instant>,

    // Menu state
    pub active_menu: Option<Menu>,
    pub show_context_menu: bool,
    pub mouse_position: iced::Point,
    pub context_menu_position: iced::Point,

    // Status message
    pub status_message: Option<String>,

    // Line ending
    pub line_ending: LineEnding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    pub fn detect(text: &str) -> Self {
        if text.contains("\r\n") {
            Self::CrLf
        } else {
            Self::Lf
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::CrLf => "CRLF",
        }
    }
}

impl Default for Notepad {
    fn default() -> Self {
        let mut content = text_editor::Content::with_text("");
        content.perform(text_editor::Action::Click(iced::Point::new(0.0, 0.0)));

        Self {
            content,
            file_path: None,
            is_modified: false,
            clipboard: arboard::Clipboard::new().ok(),
            font_size: DEFAULT_FONT_SIZE,
            dark_mode: false,
            window_width: DEFAULT_WINDOW_WIDTH,
            window_height: DEFAULT_WINDOW_HEIGHT,
            show_find: false,
            show_replace: false,
            find_query: String::new(),
            replace_query: String::new(),
            find_cursor: 0,
            case_sensitive: true,
            use_regex: false,
            show_goto: false,
            goto_input: String::new(),
            scroll_offset: 0.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_edit_time: None,
            active_menu: None,
            show_context_menu: false,
            mouse_position: iced::Point::ORIGIN,
            context_menu_position: iced::Point::ORIGIN,
            status_message: None,
            line_ending: LineEnding::Lf,
        }
    }
}

impl Notepad {
    /// Create a Notepad with default state, without loading preferences.
    /// Useful for tests that don't need disk I/O.
    #[cfg(test)]
    pub fn test_default() -> Self {
        Self::default()
    }

    pub fn new() -> (Self, Task<Message>) {
        let prefs = UserPreferences::load();
        let notepad = Self {
            font_size: prefs.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE),
            dark_mode: prefs.dark_mode,
            window_width: prefs.window_width,
            window_height: prefs.window_height,
            ..Self::default()
        };
        (notepad, Task::none())
    }

    pub fn title(&self) -> String {
        let name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Sans titre");
        let modified = if self.is_modified { " *" } else { "" };
        format!("{name}{modified} - Notepad")
    }

    pub fn theme(&self) -> Theme {
        if self.dark_mode {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            iced::event::listen().map(Message::EventOccurred),
            iced::window::close_requests()
                .map(|id| Message::File(FileMsg::CloseRequested(id))),
        ];
        if self.is_modified && self.file_path.is_some() {
            subs.push(
                iced::time::every(Duration::from_secs(30))
                    .map(|_| Message::File(FileMsg::AutoSave)),
            );
        }
        Subscription::batch(subs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- LineEnding::detect ---

    #[test]
    fn detect_crlf() {
        assert_eq!(LineEnding::detect("hello\r\nworld"), LineEnding::CrLf);
    }

    #[test]
    fn detect_lf_only() {
        assert_eq!(LineEnding::detect("hello\nworld"), LineEnding::Lf);
    }

    #[test]
    fn detect_no_newline() {
        assert_eq!(LineEnding::detect("hello world"), LineEnding::Lf);
    }

    #[test]
    fn detect_mixed_prefers_crlf() {
        assert_eq!(LineEnding::detect("a\nb\r\nc"), LineEnding::CrLf);
    }

    // --- LineEnding::label ---

    #[test]
    fn label_lf() {
        assert_eq!(LineEnding::Lf.label(), "LF");
    }

    #[test]
    fn label_crlf() {
        assert_eq!(LineEnding::CrLf.label(), "CRLF");
    }

    // --- Notepad::title ---

    #[test]
    fn title_no_file_not_modified() {
        let n = Notepad::test_default();
        assert_eq!(n.title(), "Sans titre - Notepad");
    }

    #[test]
    fn title_no_file_modified() {
        let mut n = Notepad::test_default();
        n.is_modified = true;
        assert_eq!(n.title(), "Sans titre * - Notepad");
    }

    #[test]
    fn title_with_file_not_modified() {
        let mut n = Notepad::test_default();
        n.file_path = Some(PathBuf::from("/tmp/test.txt"));
        assert_eq!(n.title(), "test.txt - Notepad");
    }

    #[test]
    fn title_with_file_modified() {
        let mut n = Notepad::test_default();
        n.file_path = Some(PathBuf::from("/tmp/test.txt"));
        n.is_modified = true;
        assert_eq!(n.title(), "test.txt * - Notepad");
    }
}
