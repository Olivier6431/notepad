use iced::keyboard::key::Named;
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::{scrollable, text_editor, text_input};
use iced::{Event, Task};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::app::{
    find_input_id, goto_input_id, EditMsg, FileMsg, LineEnding, MenuMsg, Message, Notepad,
    SearchMsg, TextSnapshot, ViewMsg, MAX_UNDO_HISTORY, UNDO_BATCH_TIMEOUT_MS,
};
use crate::preferences::UserPreferences;
use crate::ui::line_numbers_id;
use crate::{DEFAULT_FONT_SIZE, MAX_FONT_SIZE, MIN_FONT_SIZE, ZOOM_STEP};

fn byte_pos_to_line_col(text: &str, byte_pos: usize) -> (usize, usize) {
    let before = &text[..byte_pos];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let col = text[line_start..byte_pos].chars().count();
    (line, col)
}

impl Notepad {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        // Auto-close menus on most actions
        match &message {
            Message::Menu(MenuMsg::Hover(_))
            | Message::Menu(MenuMsg::Toggle(_))
            | Message::Menu(MenuMsg::ShowContext)
            | Message::Menu(MenuMsg::CloseAll)
            | Message::EventOccurred(_)
            | Message::Search(SearchMsg::FindQueryChanged(_))
            | Message::Search(SearchMsg::ReplaceQueryChanged(_))
            | Message::Search(SearchMsg::GoToInputChanged(_))
            | Message::File(FileMsg::AutoSave) => {}
            _ => {
                self.active_menu = None;
                self.show_context_menu = false;
            }
        }

        match message {
            Message::EditorAction(action) => self.handle_editor_action(action),
            Message::EventOccurred(event) => self.handle_event(event),
            Message::File(msg) => self.handle_file(msg),
            Message::Edit(msg) => self.handle_edit(msg),
            Message::Search(msg) => self.handle_search(msg),
            Message::View(msg) => self.handle_view(msg),
            Message::Menu(msg) => self.handle_menu(msg),
        }
    }

    // --- Editor action ---

    fn handle_editor_action(&mut self, action: text_editor::Action) -> Task<Message> {
        let is_edit = matches!(&action, text_editor::Action::Edit(_));
        let scroll_delta = if let text_editor::Action::Scroll { lines } = &action {
            Some(*lines)
        } else {
            None
        };
        if is_edit {
            self.save_snapshot_if_needed();
        }
        self.content.perform(action);
        if is_edit {
            self.is_modified = true;
            self.status_message = None;
        }
        if let Some(delta) = scroll_delta {
            let max_offset = self.content.line_count().saturating_sub(1) as f32;
            self.scroll_offset = (self.scroll_offset + delta as f32).clamp(0.0, max_offset);
            self.sync_line_numbers()
        } else {
            Task::none()
        }
    }

    // --- File operations ---

    fn handle_file(&mut self, msg: FileMsg) -> Task<Message> {
        match msg {
            FileMsg::New => {
                if self.is_modified {
                    Task::perform(
                        async {
                            matches!(
                                rfd::AsyncMessageDialog::new()
                                    .set_title("Notepad")
                                    .set_description("Le document a été modifié. Voulez-vous continuer sans enregistrer ?")
                                    .set_buttons(rfd::MessageButtons::OkCancel)
                                    .set_level(rfd::MessageLevel::Warning)
                                    .show()
                                    .await,
                                rfd::MessageDialogResult::Ok
                            )
                        },
                        |confirmed| Message::File(FileMsg::ConfirmNewResult(confirmed)),
                    )
                } else {
                    self.reset_document();
                    Task::none()
                }
            }
            FileMsg::Save => {
                if let Some(path) = self.file_path.clone() {
                    self.save_to_file(path);
                    Task::none()
                } else {
                    self.save_as()
                }
            }
            FileMsg::SaveAs => self.save_as(),
            FileMsg::Open => {
                if self.is_modified {
                    Task::perform(
                        async {
                            matches!(
                                rfd::AsyncMessageDialog::new()
                                    .set_title("Notepad")
                                    .set_description("Le document a été modifié. Voulez-vous continuer sans enregistrer ?")
                                    .set_buttons(rfd::MessageButtons::OkCancel)
                                    .set_level(rfd::MessageLevel::Warning)
                                    .show()
                                    .await,
                                rfd::MessageDialogResult::Ok
                            )
                        },
                        |confirmed| Message::File(FileMsg::ConfirmOpenResult(confirmed)),
                    )
                } else {
                    self.open_file()
                }
            }
            FileMsg::SaveFileSelected(path) => {
                if let Some(path) = path {
                    self.save_to_file(path);
                }
                Task::none()
            }
            FileMsg::OpenFileSelected(path) => {
                if let Some(path) = path {
                    self.load_from_file(path);
                }
                Task::none()
            }
            FileMsg::ConfirmNewResult(confirmed) => {
                if confirmed {
                    self.reset_document();
                }
                Task::none()
            }
            FileMsg::ConfirmOpenResult(confirmed) => {
                if confirmed {
                    self.open_file()
                } else {
                    Task::none()
                }
            }
            FileMsg::CloseRequested(id) => {
                if self.is_modified {
                    Task::perform(
                        async {
                            matches!(
                                rfd::AsyncMessageDialog::new()
                                    .set_title("Notepad")
                                    .set_description("Le document a été modifié. Voulez-vous quitter sans enregistrer ?")
                                    .set_buttons(rfd::MessageButtons::OkCancel)
                                    .set_level(rfd::MessageLevel::Warning)
                                    .show()
                                    .await,
                                rfd::MessageDialogResult::Ok
                            )
                        },
                        move |confirmed| Message::File(FileMsg::ConfirmCloseResult(confirmed, id)),
                    )
                } else {
                    iced::window::close(id)
                }
            }
            FileMsg::ConfirmCloseResult(confirmed, id) => {
                if confirmed {
                    iced::window::close(id)
                } else {
                    Task::none()
                }
            }
            FileMsg::AutoSave => {
                if let Some(path) = self.file_path.clone() {
                    if self.is_modified {
                        self.save_to_file(path);
                    }
                }
                Task::none()
            }
        }
    }

    // --- Edit operations ---

    fn handle_edit(&mut self, msg: EditMsg) -> Task<Message> {
        match msg {
            EditMsg::Copy => {
                if let Some(clipboard) = &mut self.clipboard {
                    if let Some(selected) = self.content.selection() {
                        if let Err(e) = clipboard.set_text(selected) {
                            rfd::MessageDialog::new()
                                .set_title("Erreur")
                                .set_description(format!(
                                    "Impossible de copier dans le presse-papiers :\n{e}"
                                ))
                                .set_level(rfd::MessageLevel::Error)
                                .set_buttons(rfd::MessageButtons::Ok)
                                .show();
                        }
                    }
                }
                Task::none()
            }
            EditMsg::Cut => {
                if let Some(clipboard) = &mut self.clipboard {
                    if let Some(selected) = self.content.selection() {
                        if let Err(e) = clipboard.set_text(selected) {
                            rfd::MessageDialog::new()
                                .set_title("Erreur")
                                .set_description(format!(
                                    "Impossible de copier dans le presse-papiers :\n{e}"
                                ))
                                .set_level(rfd::MessageLevel::Error)
                                .set_buttons(rfd::MessageButtons::Ok)
                                .show();
                        } else {
                            self.save_snapshot();
                            self.content.perform(text_editor::Action::Edit(
                                text_editor::Edit::Backspace,
                            ));
                            self.is_modified = true;
                        }
                    }
                }
                Task::none()
            }
            EditMsg::Paste => {
                if let Some(clipboard) = &mut self.clipboard {
                    match clipboard.get_text() {
                        Ok(clip_text) => {
                            self.save_snapshot();
                            self.content.perform(text_editor::Action::Edit(
                                text_editor::Edit::Paste(Arc::new(clip_text)),
                            ));
                            self.is_modified = true;
                        }
                        Err(e) => {
                            rfd::MessageDialog::new()
                                .set_title("Erreur")
                                .set_description(format!(
                                    "Impossible de lire le presse-papiers :\n{e}"
                                ))
                                .set_level(rfd::MessageLevel::Error)
                                .set_buttons(rfd::MessageButtons::Ok)
                                .show();
                        }
                    }
                }
                Task::none()
            }
            EditMsg::SelectAll => {
                self.content
                    .perform(text_editor::Action::Move(text_editor::Motion::DocumentStart));
                self.content
                    .perform(text_editor::Action::Select(text_editor::Motion::DocumentEnd));
                Task::none()
            }
            EditMsg::Undo => {
                self.undo();
                Task::none()
            }
            EditMsg::Redo => {
                self.redo();
                Task::none()
            }
            EditMsg::InsertDateTime => {
                let now = chrono::Local::now();
                let datetime_str = now.format("%H:%M %d/%m/%Y").to_string();
                self.save_snapshot();
                self.content.perform(text_editor::Action::Edit(
                    text_editor::Edit::Paste(Arc::new(datetime_str)),
                ));
                self.is_modified = true;
                Task::none()
            }
        }
    }

    // --- Search operations ---

    fn handle_search(&mut self, msg: SearchMsg) -> Task<Message> {
        match msg {
            SearchMsg::OpenFind => {
                self.show_find = true;
                self.show_replace = false;
                self.show_goto = false;
                text_input::focus(find_input_id())
            }
            SearchMsg::OpenReplace => {
                self.show_find = true;
                self.show_replace = true;
                self.show_goto = false;
                text_input::focus(find_input_id())
            }
            SearchMsg::CloseFind => {
                self.show_find = false;
                self.show_replace = false;
                Task::none()
            }
            SearchMsg::FindQueryChanged(query) => {
                self.find_query = query;
                self.find_cursor = 0;
                Task::none()
            }
            SearchMsg::ReplaceQueryChanged(query) => {
                self.replace_query = query;
                Task::none()
            }
            SearchMsg::FindNext => {
                self.find_next();
                self.sync_line_numbers()
            }
            SearchMsg::FindPrevious => {
                self.find_previous();
                self.sync_line_numbers()
            }
            SearchMsg::ReplaceOne => {
                self.replace_one();
                Task::none()
            }
            SearchMsg::ReplaceAll => {
                self.replace_all();
                Task::none()
            }
            SearchMsg::OpenGoTo => {
                self.show_goto = true;
                self.show_find = false;
                self.show_replace = false;
                self.goto_input.clear();
                text_input::focus(goto_input_id())
            }
            SearchMsg::CloseGoTo => {
                self.show_goto = false;
                Task::none()
            }
            SearchMsg::GoToInputChanged(value) => {
                self.goto_input = value;
                Task::none()
            }
            SearchMsg::GoToLineSubmit => {
                if let Ok(line_num) = self.goto_input.parse::<usize>() {
                    let target = line_num.saturating_sub(1);
                    self.navigate_to(target, 0);
                    self.show_goto = false;
                    return self.sync_line_numbers();
                }
                Task::none()
            }
            SearchMsg::ToggleCaseSensitive => {
                self.case_sensitive = !self.case_sensitive;
                self.find_cursor = 0;
                Task::none()
            }
            SearchMsg::ToggleRegex => {
                self.use_regex = !self.use_regex;
                self.find_cursor = 0;
                Task::none()
            }
        }
    }

    // --- View operations ---

    fn handle_view(&mut self, msg: ViewMsg) -> Task<Message> {
        match msg {
            ViewMsg::ZoomIn => {
                self.font_size = (self.font_size + ZOOM_STEP).min(MAX_FONT_SIZE);
                self.save_preferences();
            }
            ViewMsg::ZoomOut => {
                self.font_size = (self.font_size - ZOOM_STEP).max(MIN_FONT_SIZE);
                self.save_preferences();
            }
            ViewMsg::ZoomReset => {
                self.font_size = DEFAULT_FONT_SIZE;
                self.save_preferences();
            }
            ViewMsg::ToggleDarkMode => {
                self.dark_mode = !self.dark_mode;
                self.save_preferences();
            }
        }
        Task::none()
    }

    // --- Menu operations ---

    fn handle_menu(&mut self, msg: MenuMsg) -> Task<Message> {
        match msg {
            MenuMsg::Toggle(menu) => {
                if self.active_menu == Some(menu) {
                    self.active_menu = None;
                } else {
                    self.active_menu = Some(menu);
                }
                self.show_context_menu = false;
            }
            MenuMsg::Hover(menu) => {
                if self.active_menu.is_some() {
                    self.active_menu = Some(menu);
                }
            }
            MenuMsg::CloseAll => {
                self.active_menu = None;
                self.show_context_menu = false;
            }
            MenuMsg::ShowContext => {
                self.show_context_menu = true;
                self.context_menu_position = self.mouse_position;
                self.active_menu = None;
            }
        }
        Task::none()
    }

    // --- Event handling ---

    fn handle_event(&mut self, event: Event) -> Task<Message> {
        // Track mouse position for context menu
        if let Event::Mouse(iced::mouse::Event::CursorMoved { position }) = &event {
            self.mouse_position = *position;
        }

        // Track window resize
        if let Event::Window(iced::window::Event::Resized(size)) = &event {
            self.window_width = size.width;
            self.window_height = size.height;
            self.save_preferences();
        }

        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key, modifiers, ..
        }) = event
        {
            match (key.as_ref(), modifiers) {
                // Escape - close menus first, then panels
                (Key::Named(Named::Escape), _) => {
                    if self.active_menu.is_some() || self.show_context_menu {
                        self.active_menu = None;
                        self.show_context_menu = false;
                    } else if self.show_find || self.show_goto {
                        self.show_find = false;
                        self.show_replace = false;
                        self.show_goto = false;
                    }
                }
                // F3 - Find Next
                (Key::Named(Named::F3), _) => {
                    return self.handle_search(SearchMsg::FindNext);
                }
                // F5 - Date/Time
                (Key::Named(Named::F5), _) => {
                    return self.handle_edit(EditMsg::InsertDateTime);
                }
                // Ctrl+Shift+S - Save As
                (Key::Character("s"), m)
                    if m == (Modifiers::CTRL | Modifiers::SHIFT) =>
                {
                    return self.handle_file(FileMsg::SaveAs);
                }
                // Ctrl+key combinations
                (Key::Character("n"), Modifiers::CTRL) => {
                    return self.handle_file(FileMsg::New);
                }
                (Key::Character("s"), Modifiers::CTRL) => {
                    return self.handle_file(FileMsg::Save);
                }
                (Key::Character("o"), Modifiers::CTRL) => {
                    return self.handle_file(FileMsg::Open);
                }
                (Key::Character("z"), Modifiers::CTRL) => {
                    return self.handle_edit(EditMsg::Undo);
                }
                (Key::Character("y"), Modifiers::CTRL) => {
                    return self.handle_edit(EditMsg::Redo);
                }
                (Key::Character("f"), Modifiers::CTRL) => {
                    return self.handle_search(SearchMsg::OpenFind);
                }
                (Key::Character("h"), Modifiers::CTRL) => {
                    return self.handle_search(SearchMsg::OpenReplace);
                }
                (Key::Character("g"), Modifiers::CTRL) => {
                    return self.handle_search(SearchMsg::OpenGoTo);
                }
                // Zoom
                (Key::Character("="), Modifiers::CTRL) => {
                    return self.handle_view(ViewMsg::ZoomIn);
                }
                (Key::Character("+"), m) if m.contains(Modifiers::CTRL) => {
                    return self.handle_view(ViewMsg::ZoomIn);
                }
                (Key::Character("-"), Modifiers::CTRL) => {
                    return self.handle_view(ViewMsg::ZoomOut);
                }
                (Key::Character("0"), Modifiers::CTRL) => {
                    return self.handle_view(ViewMsg::ZoomReset);
                }
                _ => {}
            }
        }
        Task::none()
    }

    // --- Preferences ---

    pub fn save_preferences(&self) {
        UserPreferences {
            font_size: self.font_size,
            dark_mode: self.dark_mode,
            window_width: self.window_width,
            window_height: self.window_height,
        }
        .save();
    }

    // --- Document management ---

    fn reset_document(&mut self) {
        self.content = text_editor::Content::with_text("");
        self.content
            .perform(text_editor::Action::Click(iced::Point::new(0.0, 0.0)));
        self.file_path = None;
        self.is_modified = false;
        self.scroll_offset = 0.0;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.last_edit_time = None;
        self.status_message = Some("Nouveau document".to_string());
    }

    // --- Undo/Redo ---

    fn push_snapshot(&mut self, snapshot: TextSnapshot) {
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
    }

    fn save_snapshot(&mut self) {
        let (cursor_line, cursor_col) = self.content.cursor_position();
        self.push_snapshot(TextSnapshot {
            text: self.content.text(),
            cursor_line,
            cursor_col,
        });
        self.redo_stack.clear();
        self.last_edit_time = None;
    }

    fn save_snapshot_if_needed(&mut self) {
        let now = Instant::now();
        let should_save = match self.last_edit_time {
            Some(last) => now.duration_since(last).as_millis() > UNDO_BATCH_TIMEOUT_MS,
            None => true,
        };
        if should_save {
            let (cursor_line, cursor_col) = self.content.cursor_position();
            self.push_snapshot(TextSnapshot {
                text: self.content.text(),
                cursor_line,
                cursor_col,
            });
            self.redo_stack.clear();
        }
        self.last_edit_time = Some(now);
    }

    fn undo(&mut self) {
        if let Some(snapshot) = self.undo_stack.pop() {
            let (cursor_line, cursor_col) = self.content.cursor_position();
            self.redo_stack.push(TextSnapshot {
                text: self.content.text(),
                cursor_line,
                cursor_col,
            });
            self.content = text_editor::Content::with_text(&snapshot.text);
            self.navigate_to(snapshot.cursor_line, snapshot.cursor_col);
            self.is_modified = true;
        }
    }

    fn redo(&mut self) {
        if let Some(snapshot) = self.redo_stack.pop() {
            let (cursor_line, cursor_col) = self.content.cursor_position();
            self.undo_stack.push(TextSnapshot {
                text: self.content.text(),
                cursor_line,
                cursor_col,
            });
            self.content = text_editor::Content::with_text(&snapshot.text);
            self.navigate_to(snapshot.cursor_line, snapshot.cursor_col);
            self.is_modified = true;
        }
    }

    // --- Line numbers sync ---

    fn sync_line_numbers(&self) -> Task<Message> {
        let line_height = self.font_size * 1.3;
        scrollable::scroll_to(
            line_numbers_id(),
            scrollable::AbsoluteOffset {
                x: 0.0,
                y: self.scroll_offset * line_height,
            },
        )
    }

    // --- File I/O ---

    fn save_to_file(&mut self, path: PathBuf) {
        let content = self.content.text();
        if let Err(e) = std::fs::write(&path, content) {
            rfd::MessageDialog::new()
                .set_title("Erreur")
                .set_description(format!("Impossible d'enregistrer le fichier :\n{e}"))
                .set_level(rfd::MessageLevel::Error)
                .set_buttons(rfd::MessageButtons::Ok)
                .show();
        } else {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("fichier")
                .to_string();
            self.file_path = Some(path);
            self.is_modified = false;
            self.status_message = Some(format!("Enregistré : {name}"));
        }
    }

    fn load_from_file(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(content_text) => {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("fichier")
                    .to_string();
                self.line_ending = LineEnding::detect(&content_text);
                let mut content = text_editor::Content::with_text(&content_text);
                content.perform(text_editor::Action::Move(
                    text_editor::Motion::DocumentEnd,
                ));
                self.content = content;
                self.file_path = Some(path);
                self.is_modified = false;
                self.scroll_offset = 0.0;
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.last_edit_time = None;
                self.status_message = Some(format!("Ouvert : {name}"));
            }
            Err(e) => {
                rfd::MessageDialog::new()
                    .set_title("Erreur")
                    .set_description(format!("Impossible d'ouvrir le fichier :\n{e}"))
                    .set_level(rfd::MessageLevel::Error)
                    .set_buttons(rfd::MessageButtons::Ok)
                    .show();
            }
        }
    }

    fn save_as(&self) -> Task<Message> {
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Enregistrer sous")
                    .add_filter("Fichiers texte", &["txt"])
                    .add_filter("Tous les fichiers", &["*"])
                    .save_file()
                    .await
                    .map(|handle| handle.path().to_path_buf())
            },
            |path| Message::File(FileMsg::SaveFileSelected(path)),
        )
    }

    fn open_file(&self) -> Task<Message> {
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Ouvrir un fichier")
                    .add_filter("Fichiers texte", &["txt"])
                    .add_filter("Tous les fichiers", &["*"])
                    .pick_file()
                    .await
                    .map(|handle| handle.path().to_path_buf())
            },
            |path| Message::File(FileMsg::OpenFileSelected(path)),
        )
    }

    // --- Find & Replace ---

    fn navigate_to(&mut self, line: usize, col: usize) {
        let (current_line, _) = self.content.cursor_position();
        let last_line = self.content.line_count().saturating_sub(1);
        let target_line = line.min(last_line);

        let from_start = target_line;
        let from_end = last_line - target_line;
        let from_current = target_line.abs_diff(current_line);

        let min_moves = from_start.min(from_end).min(from_current);

        if min_moves == from_current {
            if target_line > current_line {
                for _ in 0..(target_line - current_line) {
                    self.content
                        .perform(text_editor::Action::Move(text_editor::Motion::Down));
                }
            } else {
                for _ in 0..(current_line - target_line) {
                    self.content
                        .perform(text_editor::Action::Move(text_editor::Motion::Up));
                }
            }
        } else if min_moves == from_start {
            self.content
                .perform(text_editor::Action::Move(text_editor::Motion::DocumentStart));
            for _ in 0..target_line {
                self.content
                    .perform(text_editor::Action::Move(text_editor::Motion::Down));
            }
        } else {
            self.content
                .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
            for _ in 0..from_end {
                self.content
                    .perform(text_editor::Action::Move(text_editor::Motion::Up));
            }
        }

        self.content
            .perform(text_editor::Action::Move(text_editor::Motion::Home));
        for _ in 0..col {
            self.content
                .perform(text_editor::Action::Move(text_editor::Motion::Right));
        }

        // Update scroll estimate for line number sync
        self.scroll_offset = target_line as f32;
    }

    fn select_chars(&mut self, count: usize) {
        for _ in 0..count {
            self.content
                .perform(text_editor::Action::Select(text_editor::Motion::Right));
        }
    }

    fn highlight_match(&mut self, byte_pos: usize, match_len: usize, text: &str) {
        self.find_cursor = byte_pos + match_len;
        let (line, col) = byte_pos_to_line_col(text, byte_pos);
        self.navigate_to(line, col);
        let match_chars = text[byte_pos..byte_pos + match_len].chars().count();
        self.select_chars(match_chars);
    }

    /// Build a regex from the current find_query and settings.
    /// Returns None if use_regex is on and the pattern is invalid.
    fn build_regex(&self) -> Option<regex::Regex> {
        let pattern = if self.use_regex {
            self.find_query.clone()
        } else {
            regex::escape(&self.find_query)
        };
        let full = if self.case_sensitive {
            pattern
        } else {
            format!("(?i){pattern}")
        };
        regex::Regex::new(&full).ok()
    }

    /// Find first match in `haystack` starting at byte offset `from`.
    /// Returns (byte_pos, match_len) in the original text.
    fn find_in(&self, haystack: &str, from: usize) -> Option<(usize, usize)> {
        let re = self.build_regex()?;
        re.find(&haystack[from..])
            .map(|m| (from + m.start(), m.len()))
    }

    /// Find last match in `haystack` up to byte offset `until`.
    /// Returns (byte_pos, match_len) in the original text.
    fn rfind_in(&self, haystack: &str, until: usize) -> Option<(usize, usize)> {
        let re = self.build_regex()?;
        let mut last = None;
        for m in re.find_iter(&haystack[..until]) {
            last = Some((m.start(), m.len()));
        }
        last
    }

    fn find_next(&mut self) {
        let text = self.content.text();
        if self.find_query.is_empty() || text.is_empty() {
            return;
        }

        let search_from = self.find_cursor.min(text.len());
        let found = if search_from < text.len() {
            self.find_in(&text, search_from)
        } else {
            None
        };

        // Wrap around if not found
        let found = found.or_else(|| self.find_in(&text, 0));

        if let Some((byte_pos, mlen)) = found {
            self.highlight_match(byte_pos, mlen, &text);
        }
    }

    fn find_previous(&mut self) {
        let text = self.content.text();
        if self.find_query.is_empty() || text.is_empty() {
            return;
        }

        let search_until = self.find_cursor.saturating_sub(1);

        let found = if search_until > 0 {
            self.rfind_in(&text, search_until)
        } else {
            None
        };

        // Wrap around if not found
        let found = found.or_else(|| self.rfind_in(&text, text.len()));

        if let Some((byte_pos, mlen)) = found {
            self.highlight_match(byte_pos, mlen, &text);
        }
    }

    fn replace_one(&mut self) {
        if self.find_query.is_empty() {
            return;
        }
        if let Some(selected) = self.content.selection() {
            let is_match = if let Some(re) = self.build_regex() {
                re.is_match(&selected)
                    && re.find(&selected).is_some_and(|m| m.len() == selected.len())
            } else {
                false
            };
            if is_match {
                self.save_snapshot();
                self.content.perform(text_editor::Action::Edit(
                    text_editor::Edit::Paste(Arc::new(self.replace_query.clone())),
                ));
                self.is_modified = true;
            }
        }
        self.find_next();
    }

    fn replace_all(&mut self) {
        if self.find_query.is_empty() {
            return;
        }
        let Some(re) = self.build_regex() else {
            return;
        };
        let text = self.content.text();
        let new_text = re.replace_all(&text, self.replace_query.as_str()).into_owned();
        if text != new_text {
            self.save_snapshot();
            self.content = text_editor::Content::with_text(&new_text);
            self.is_modified = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Notepad, MAX_UNDO_HISTORY};

    // Helper to create a Notepad with specific text content
    fn notepad_with(text: &str) -> Notepad {
        let mut n = Notepad::test_default();
        n.content = text_editor::Content::with_text(text);
        n
    }

    // ============================
    // byte_pos_to_line_col
    // ============================

    #[test]
    fn byte_pos_start_of_file() {
        assert_eq!(byte_pos_to_line_col("hello\nworld", 0), (0, 0));
    }

    #[test]
    fn byte_pos_mid_first_line() {
        assert_eq!(byte_pos_to_line_col("hello\nworld", 3), (0, 3));
    }

    #[test]
    fn byte_pos_start_second_line() {
        // byte 6 is 'w' in "world"
        assert_eq!(byte_pos_to_line_col("hello\nworld", 6), (1, 0));
    }

    #[test]
    fn byte_pos_mid_second_line() {
        assert_eq!(byte_pos_to_line_col("hello\nworld", 9), (1, 3));
    }

    #[test]
    fn byte_pos_end_of_text() {
        let text = "abc\ndef";
        assert_eq!(byte_pos_to_line_col(text, text.len()), (1, 3));
    }

    #[test]
    fn byte_pos_multibyte_chars() {
        // 'é' is 2 bytes in UTF-8
        let text = "café\nbar";
        // byte 6 = after "café\n" (c=1, a=1, f=1, é=2, \n=1 → 6)
        assert_eq!(byte_pos_to_line_col(text, 6), (1, 0));
        // byte 3 = 'é' starts at byte 3, col should count chars not bytes
        assert_eq!(byte_pos_to_line_col(text, 3), (0, 3));
    }

    #[test]
    fn byte_pos_three_lines() {
        let text = "aaa\nbbb\nccc";
        assert_eq!(byte_pos_to_line_col(text, 8), (2, 0));
        assert_eq!(byte_pos_to_line_col(text, 10), (2, 2));
    }

    // ============================
    // build_regex
    // ============================

    #[test]
    fn build_regex_case_sensitive_literal() {
        let mut n = Notepad::test_default();
        n.find_query = "Hello".to_string();
        n.case_sensitive = true;
        n.use_regex = false;
        let re = n.build_regex().unwrap();
        assert!(re.is_match("Hello"));
        assert!(!re.is_match("hello"));
    }

    #[test]
    fn build_regex_case_insensitive_literal() {
        let mut n = Notepad::test_default();
        n.find_query = "hello".to_string();
        n.case_sensitive = false;
        n.use_regex = false;
        let re = n.build_regex().unwrap();
        assert!(re.is_match("HELLO"));
        assert!(re.is_match("Hello"));
        assert!(re.is_match("hello"));
    }

    #[test]
    fn build_regex_valid_pattern() {
        let mut n = Notepad::test_default();
        n.find_query = r"\d+".to_string();
        n.case_sensitive = true;
        n.use_regex = true;
        let re = n.build_regex().unwrap();
        assert!(re.is_match("abc123"));
        assert!(!re.is_match("abc"));
    }

    #[test]
    fn build_regex_invalid_pattern() {
        let mut n = Notepad::test_default();
        n.find_query = "[unclosed".to_string();
        n.use_regex = true;
        assert!(n.build_regex().is_none());
    }

    #[test]
    fn build_regex_case_insensitive_regex() {
        let mut n = Notepad::test_default();
        n.find_query = "abc".to_string();
        n.case_sensitive = false;
        n.use_regex = true;
        let re = n.build_regex().unwrap();
        assert!(re.is_match("ABC"));
    }

    #[test]
    fn build_regex_escapes_special_chars_in_literal() {
        let mut n = Notepad::test_default();
        n.find_query = "a.b".to_string();
        n.case_sensitive = true;
        n.use_regex = false;
        let re = n.build_regex().unwrap();
        assert!(re.is_match("a.b"));
        assert!(!re.is_match("axb")); // '.' is escaped, not wildcard
    }

    // ============================
    // find_in / rfind_in
    // ============================

    #[test]
    fn find_in_from_start() {
        let n = notepad_with("hello world hello");
        let mut n = n;
        n.find_query = "hello".to_string();
        n.case_sensitive = true;
        assert_eq!(n.find_in("hello world hello", 0), Some((0, 5)));
    }

    #[test]
    fn find_in_from_offset() {
        let mut n = notepad_with("hello world hello");
        n.find_query = "hello".to_string();
        n.case_sensitive = true;
        assert_eq!(n.find_in("hello world hello", 1), Some((12, 5)));
    }

    #[test]
    fn find_in_no_match() {
        let mut n = notepad_with("hello world");
        n.find_query = "xyz".to_string();
        n.case_sensitive = true;
        assert_eq!(n.find_in("hello world", 0), None);
    }

    #[test]
    fn rfind_in_last_occurrence() {
        let mut n = notepad_with("hello world hello");
        n.find_query = "hello".to_string();
        n.case_sensitive = true;
        let text = "hello world hello";
        assert_eq!(n.rfind_in(text, text.len()), Some((12, 5)));
    }

    #[test]
    fn find_in_case_insensitive() {
        let mut n = notepad_with("Hello World");
        n.find_query = "hello".to_string();
        n.case_sensitive = false;
        assert_eq!(n.find_in("Hello World", 0), Some((0, 5)));
    }

    // ============================
    // find_next / find_previous
    // ============================

    #[test]
    fn find_next_empty_query_no_crash() {
        let mut n = notepad_with("some text");
        n.find_query = String::new();
        n.find_next(); // should not panic
    }

    #[test]
    fn find_next_empty_text_no_crash() {
        let mut n = notepad_with("");
        n.find_query = "abc".to_string();
        n.find_next(); // should not panic
    }

    #[test]
    fn find_previous_empty_query_no_crash() {
        let mut n = notepad_with("some text");
        n.find_query = String::new();
        n.find_previous(); // should not panic
    }

    #[test]
    fn find_next_wraps_around() {
        let mut n = notepad_with("abc def abc");
        n.find_query = "abc".to_string();
        n.case_sensitive = true;
        // Set cursor past last match
        n.find_cursor = 100;
        n.find_next();
        // After wrap-around, find_cursor should be updated (past position 0)
        assert!(n.find_cursor > 0);
    }

    // ============================
    // replace_all
    // ============================

    #[test]
    fn replace_all_simple() {
        let mut n = notepad_with("hello world hello");
        n.find_query = "hello".to_string();
        n.replace_query = "hi".to_string();
        n.case_sensitive = true;
        n.replace_all();
        assert_eq!(n.content.text().trim_end(), "hi world hi");
        assert!(n.is_modified);
    }

    #[test]
    fn replace_all_case_insensitive() {
        let mut n = notepad_with("Hello HELLO hello");
        n.find_query = "hello".to_string();
        n.replace_query = "hi".to_string();
        n.case_sensitive = false;
        n.replace_all();
        assert_eq!(n.content.text().trim_end(), "hi hi hi");
    }

    #[test]
    fn replace_all_empty_query_no_change() {
        let mut n = notepad_with("hello world");
        n.find_query = String::new();
        n.replace_query = "hi".to_string();
        n.replace_all();
        assert!(!n.is_modified);
    }

    #[test]
    fn replace_all_no_match() {
        let mut n = notepad_with("hello world");
        n.find_query = "xyz".to_string();
        n.replace_query = "hi".to_string();
        n.case_sensitive = true;
        n.replace_all();
        assert!(!n.is_modified);
    }

    // ============================
    // push_snapshot / undo / redo
    // ============================

    #[test]
    fn push_snapshot_respects_max_history() {
        let mut n = Notepad::test_default();
        for i in 0..MAX_UNDO_HISTORY + 10 {
            n.push_snapshot(crate::app::TextSnapshot {
                text: format!("text{i}"),
                cursor_line: 0,
                cursor_col: 0,
            });
        }
        assert_eq!(n.undo_stack.len(), MAX_UNDO_HISTORY);
    }

    #[test]
    fn undo_restores_previous_text() {
        let mut n = notepad_with("original");
        n.save_snapshot();
        n.content = text_editor::Content::with_text("modified");
        n.undo();
        assert_eq!(n.content.text().trim_end(), "original");
    }

    #[test]
    fn redo_after_undo() {
        let mut n = notepad_with("original");
        n.save_snapshot();
        n.content = text_editor::Content::with_text("modified");
        n.is_modified = true;
        // Save current state to redo stack via undo
        n.undo();
        assert_eq!(n.content.text().trim_end(), "original");
        n.redo();
        assert_eq!(n.content.text().trim_end(), "modified");
    }

    #[test]
    fn undo_on_empty_stack_is_noop() {
        let mut n = notepad_with("hello");
        n.undo(); // should not panic
        assert_eq!(n.content.text().trim_end(), "hello");
    }

    #[test]
    fn redo_on_empty_stack_is_noop() {
        let mut n = notepad_with("hello");
        n.redo(); // should not panic
        assert_eq!(n.content.text().trim_end(), "hello");
    }

    // ============================
    // reset_document
    // ============================

    #[test]
    fn reset_document_clears_state() {
        let mut n = notepad_with("some content");
        n.file_path = Some(PathBuf::from("/tmp/test.txt"));
        n.is_modified = true;
        n.save_snapshot();
        n.undo_stack.push(crate::app::TextSnapshot {
            text: "old".to_string(),
            cursor_line: 0,
            cursor_col: 0,
        });
        n.reset_document();

        assert!(n.file_path.is_none());
        assert!(!n.is_modified);
        assert!(n.undo_stack.is_empty());
        assert!(n.redo_stack.is_empty());
        assert_eq!(n.scroll_offset, 0.0);
        assert_eq!(
            n.status_message,
            Some("Nouveau document".to_string())
        );
    }
}
