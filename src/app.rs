use iced::widget::{text_editor, text_input};
use iced::{Event, Subscription, Task, Theme};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::preferences::UserPreferences;
use crate::{
    DEFAULT_FONT_SIZE, DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MAX_FONT_SIZE, MIN_FONT_SIZE,
};

pub const MAX_UNDO_HISTORY: usize = 200;
pub const UNDO_BATCH_TIMEOUT_MS: u128 = 300;
pub const MENU_BAR_HEIGHT: f32 = 30.0;
pub const TAB_BAR_HEIGHT: f32 = 32.0;
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

// --- Per-document state ---

pub struct Document {
    pub content: text_editor::Content,
    pub file_path: Option<PathBuf>,
    pub is_modified: bool,
    pub undo_stack: Vec<TextSnapshot>,
    pub redo_stack: Vec<TextSnapshot>,
    pub last_edit_time: Option<Instant>,
    pub line_ending: LineEnding,
    pub scroll_offset: f32,
    pub status_message: Option<String>,
}

impl Default for Document {
    fn default() -> Self {
        let mut content = text_editor::Content::with_text("");
        content.perform(text_editor::Action::Click(iced::Point::new(0.0, 0.0)));
        Self {
            content,
            file_path: None,
            is_modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_edit_time: None,
            line_ending: LineEnding::Lf,
            scroll_offset: 0.0,
            status_message: None,
        }
    }
}

impl Document {
    pub fn title_label(&self) -> String {
        let name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Sans titre");
        if self.is_modified {
            format!("{name} *")
        } else {
            name.to_string()
        }
    }
}

// --- Enums ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    File,
    Edit,
    Search,
    View,
}

#[derive(Debug, Clone)]
pub enum FileMsg {
    NewTab,
    CloseTab(usize),
    ConfirmCloseTabResult(bool, usize),
    SwitchTab(usize),
    Save,
    SaveAs,
    Open,
    SaveFileSelected(Option<PathBuf>),
    OpenFileSelected(Option<PathBuf>),
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

// --- Line ending ---

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

// --- Application state ---

pub struct Notepad {
    // Tabs
    pub tabs: Vec<Document>,
    pub active_tab: usize,

    // App-wide
    pub clipboard: Option<arboard::Clipboard>,
    pub font_size: f32,
    pub dark_mode: bool,
    pub window_width: f32,
    pub window_height: f32,

    // Find & Replace (shared across tabs)
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

    // Menu state
    pub active_menu: Option<Menu>,
    pub show_context_menu: bool,
    pub mouse_position: iced::Point,
    pub context_menu_position: iced::Point,
}

impl Default for Notepad {
    fn default() -> Self {
        Self {
            tabs: vec![Document::default()],
            active_tab: 0,
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
            active_menu: None,
            show_context_menu: false,
            mouse_position: iced::Point::ORIGIN,
            context_menu_position: iced::Point::ORIGIN,
        }
    }
}

impl Notepad {
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

    pub fn active_doc(&self) -> &Document {
        &self.tabs[self.active_tab]
    }

    pub fn active_doc_mut(&mut self) -> &mut Document {
        &mut self.tabs[self.active_tab]
    }

    pub fn title(&self) -> String {
        let doc = self.active_doc();
        let name = doc
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Sans titre");
        let modified = if doc.is_modified { " *" } else { "" };
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
        // Auto-save if any tab is modified and has a file path
        let any_modified = self
            .tabs
            .iter()
            .any(|doc| doc.is_modified && doc.file_path.is_some());
        if any_modified {
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

    // --- Document::title_label ---

    #[test]
    fn doc_title_no_file() {
        let doc = Document::default();
        assert_eq!(doc.title_label(), "Sans titre");
    }

    #[test]
    fn doc_title_with_file() {
        let mut doc = Document::default();
        doc.file_path = Some(PathBuf::from("/tmp/test.txt"));
        assert_eq!(doc.title_label(), "test.txt");
    }

    #[test]
    fn doc_title_modified() {
        let mut doc = Document::default();
        doc.is_modified = true;
        assert_eq!(doc.title_label(), "Sans titre *");
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
        n.active_doc_mut().is_modified = true;
        assert_eq!(n.title(), "Sans titre * - Notepad");
    }

    #[test]
    fn title_with_file_not_modified() {
        let mut n = Notepad::test_default();
        n.active_doc_mut().file_path = Some(PathBuf::from("/tmp/test.txt"));
        assert_eq!(n.title(), "test.txt - Notepad");
    }

    #[test]
    fn title_with_file_modified() {
        let mut n = Notepad::test_default();
        let doc = n.active_doc_mut();
        doc.file_path = Some(PathBuf::from("/tmp/test.txt"));
        doc.is_modified = true;
        assert_eq!(n.title(), "test.txt * - Notepad");
    }
}
