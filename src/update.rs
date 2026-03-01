use iced::keyboard::key::Named;
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::{scrollable, text_editor, text_input};
use iced::{Event, Task};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::app::{
    find_input_id, goto_input_id, Document, EditMsg, FileMsg, LineEnding, MenuMsg, Message,
    Notepad, SearchMsg, TextSnapshot, ViewMsg, FILE_SIZE_LIMIT_MB, FILE_SIZE_WARN_MB,
    LARGE_FILE_UNDO_HISTORY, MAX_UNDO_HISTORY, UNDO_BATCH_TIMEOUT_MS,
};
use crate::preferences::UserPreferences;
use crate::ui::line_numbers_id;
use crate::{DEFAULT_FONT_SIZE, MAX_FONT_SIZE, MIN_FONT_SIZE, ZOOM_STEP};

fn format_local_datetime(unix_secs: u64) -> String {
    // UTC offset for local time — use platform-specific API
    #[cfg(target_os = "windows")]
    fn utc_offset_secs() -> i64 {
        #[repr(C)]
        struct TimeZoneInformation {
            bias: i32,
            _rest: [u8; 168],
        }
        extern "system" {
            fn GetTimeZoneInformation(lpTimeZoneInformation: *mut TimeZoneInformation) -> u32;
        }
        let mut tzi = TimeZoneInformation {
            bias: 0,
            _rest: [0; 168],
        };
        unsafe {
            GetTimeZoneInformation(&mut tzi);
        }
        // Bias is in minutes, west-positive → negate for east-positive
        -(tzi.bias as i64) * 60
    }

    #[cfg(not(target_os = "windows"))]
    fn utc_offset_secs() -> i64 {
        0 // Fallback to UTC on non-Windows
    }

    let local_secs = unix_secs as i64 + utc_offset_secs();

    // Days since epoch → date
    let mut days = local_secs.div_euclid(86400);
    let day_secs = local_secs.rem_euclid(86400);
    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;

    // Civil date from days since 1970-01-01 (Algorithm from Howard Hinnant)
    days += 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{:02}:{:02} {:02}/{:02}/{:04}", hours, minutes, d, m, y)
}

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
        // Ctrl+wheel → zoom instead of scroll
        if self.ctrl_pressed {
            if let text_editor::Action::Scroll { lines } = &action {
                return if *lines < 0 {
                    self.handle_view(ViewMsg::ZoomIn)
                } else {
                    self.handle_view(ViewMsg::ZoomOut)
                };
            }
        }

        let is_edit = matches!(&action, text_editor::Action::Edit(_));
        let scroll_delta = if let text_editor::Action::Scroll { lines } = &action {
            Some(*lines)
        } else {
            None
        };
        if is_edit {
            self.save_snapshot_if_needed();
        }
        let doc = self.active_doc_mut();
        doc.content.perform(action);
        if is_edit {
            doc.is_modified = true;
            doc.status_message = None;
            doc.update_stats_cache();
        }
        if let Some(delta) = scroll_delta {
            let doc = self.active_doc_mut();
            let max_offset = doc.content.line_count().saturating_sub(1) as f32;
            doc.scroll_offset = (doc.scroll_offset + delta as f32).clamp(0.0, max_offset);
            self.sync_line_numbers()
        } else {
            Task::none()
        }
    }

    // --- File operations ---

    fn confirm_discard(
        description: &'static str,
        on_confirm: impl Fn(bool) -> Message + Send + 'static,
    ) -> Task<Message> {
        Task::perform(
            async move {
                matches!(
                    rfd::AsyncMessageDialog::new()
                        .set_title("Notepad")
                        .set_description(description)
                        .set_buttons(rfd::MessageButtons::OkCancel)
                        .set_level(rfd::MessageLevel::Warning)
                        .show()
                        .await,
                    rfd::MessageDialogResult::Ok
                )
            },
            on_confirm,
        )
    }

    fn handle_file(&mut self, msg: FileMsg) -> Task<Message> {
        match msg {
            FileMsg::NewTab => {
                self.tabs.push(Document::default());
                self.active_tab = self.tabs.len() - 1;
                Task::none()
            }
            FileMsg::CloseTab(index) => {
                if index >= self.tabs.len() {
                    return Task::none();
                }
                if self.tabs[index].is_modified {
                    Self::confirm_discard(
                        "Le document a été modifié. Voulez-vous fermer sans enregistrer ?",
                        move |confirmed| {
                            Message::File(FileMsg::ConfirmCloseTabResult(confirmed, index))
                        },
                    )
                } else {
                    self.remove_tab(index);
                    Task::none()
                }
            }
            FileMsg::ConfirmCloseTabResult(confirmed, index) => {
                if confirmed {
                    self.remove_tab(index);
                }
                Task::none()
            }
            FileMsg::SwitchTab(index) => {
                if index < self.tabs.len() {
                    self.active_tab = index;
                    self.find_cursor = 0;
                }
                Task::none()
            }
            FileMsg::Save => {
                if let Some(path) = self.active_doc().file_path.clone() {
                    self.save_to_file(path);
                    Task::none()
                } else {
                    self.save_as()
                }
            }
            FileMsg::SaveAs => self.save_as(),
            FileMsg::Open => {
                // Open in a new tab (like Windows Notepad)
                self.open_file()
            }
            FileMsg::SaveFileSelected(path) => {
                if let Some(path) = path {
                    self.save_to_file(path);
                }
                Task::none()
            }
            FileMsg::OpenFileSelected(path) => {
                if let Some(path) = path {
                    return self.open_dropped_file(path);
                }
                Task::none()
            }
            FileMsg::CloseRequested(id) => {
                let any_modified = self.tabs.iter().any(|doc| doc.is_modified);
                if any_modified {
                    Self::confirm_discard(
                        "Des documents ont été modifiés. Voulez-vous quitter sans enregistrer ?",
                        move |confirmed| {
                            Message::File(FileMsg::ConfirmCloseResult(confirmed, id))
                        },
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
                for doc in &mut self.tabs {
                    if doc.is_modified {
                        if let Some(path) = doc.file_path.clone() {
                            if std::fs::write(&path, doc.encode_content()).is_ok() {
                                doc.is_modified = false;
                                let name = path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("fichier")
                                    .to_string();
                                doc.status_message = Some(format!("Enregistré : {name}"));
                            }
                        }
                    }
                }
                Task::none()
            }
        }
    }

    fn remove_tab(&mut self, index: usize) {
        if self.tabs.len() <= 1 {
            // Last tab: replace with empty document
            self.tabs[0] = Document::default();
            self.active_tab = 0;
        } else {
            self.tabs.remove(index);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            } else if self.active_tab > index {
                self.active_tab -= 1;
            }
        }
    }

    fn open_dropped_file(&mut self, path: PathBuf) -> Task<Message> {
        let doc = self.active_doc();
        let reuse = !doc.is_modified
            && doc.file_path.is_none()
            && doc.content.text().trim().is_empty();
        if !reuse {
            self.tabs.push(Document::default());
            self.active_tab = self.tabs.len() - 1;
        }
        self.load_from_file(path);
        Task::none()
    }

    // --- Edit operations ---

    fn handle_edit(&mut self, msg: EditMsg) -> Task<Message> {
        match msg {
            EditMsg::Copy => {
                let doc = &self.tabs[self.active_tab];
                if let Some(clipboard) = &mut self.clipboard {
                    if let Some(selected) = doc.content.selection() {
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
                let selected = self.tabs[self.active_tab].content.selection();
                if let Some(clipboard) = &mut self.clipboard {
                    if let Some(selected) = selected {
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
                            let doc = self.active_doc_mut();
                            doc.content.perform(text_editor::Action::Edit(
                                text_editor::Edit::Backspace,
                            ));
                            doc.is_modified = true;
                            doc.update_stats_cache();
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
                            let doc = self.active_doc_mut();
                            doc.content.perform(text_editor::Action::Edit(
                                text_editor::Edit::Paste(Arc::new(clip_text)),
                            ));
                            doc.is_modified = true;
                            doc.update_stats_cache();
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
                let doc = self.active_doc_mut();
                doc.content
                    .perform(text_editor::Action::Move(text_editor::Motion::DocumentStart));
                doc.content
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
                let now = std::time::SystemTime::now();
                let secs = now
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                // Convert to local time using platform API
                let datetime_str = format_local_datetime(secs);
                self.save_snapshot();
                let doc = self.active_doc_mut();
                doc.content.perform(text_editor::Action::Edit(
                    text_editor::Edit::Paste(Arc::new(datetime_str)),
                ));
                doc.is_modified = true;
                doc.update_stats_cache();
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
            ViewMsg::ToggleWordWrap => {
                self.word_wrap = !self.word_wrap;
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
        if let Event::Mouse(iced::mouse::Event::CursorMoved { position }) = &event {
            self.mouse_position = *position;
        }

        // Track modifier keys for Ctrl+wheel zoom
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = &event {
            self.ctrl_pressed = modifiers.control();
        }

        if let Event::Window(iced::window::Event::Resized(size)) = &event {
            self.window_width = size.width;
            self.window_height = size.height;
            self.save_preferences();
        }

        if let Event::Window(iced::window::Event::FileDropped(path)) = event {
            return self.open_dropped_file(path);
        }

        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key, modifiers, ..
        }) = event
        {
            match (key.as_ref(), modifiers) {
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
                (Key::Named(Named::F3), Modifiers::SHIFT) => {
                    return self.handle_search(SearchMsg::FindPrevious);
                }
                (Key::Named(Named::F3), _) => {
                    return self.handle_search(SearchMsg::FindNext);
                }
                (Key::Named(Named::F5), _) => {
                    return self.handle_edit(EditMsg::InsertDateTime);
                }
                // Ctrl+Tab - next tab
                (Key::Named(Named::Tab), Modifiers::CTRL) => {
                    if !self.tabs.is_empty() {
                        self.active_tab = (self.active_tab + 1) % self.tabs.len();
                        self.find_cursor = 0;
                    }
                }
                // Ctrl+Shift+Tab - previous tab
                (Key::Named(Named::Tab), m) if m == (Modifiers::CTRL | Modifiers::SHIFT) => {
                    if !self.tabs.is_empty() {
                        self.active_tab = if self.active_tab == 0 {
                            self.tabs.len() - 1
                        } else {
                            self.active_tab - 1
                        };
                        self.find_cursor = 0;
                    }
                }
                // Ctrl+Shift+S - Save As
                (Key::Character("s"), m) if m == (Modifiers::CTRL | Modifiers::SHIFT) => {
                    return self.handle_file(FileMsg::SaveAs);
                }
                // Ctrl+W - Close tab
                (Key::Character("w"), Modifiers::CTRL) => {
                    let idx = self.active_tab;
                    return self.handle_file(FileMsg::CloseTab(idx));
                }
                (Key::Character("n"), Modifiers::CTRL) => {
                    return self.handle_file(FileMsg::NewTab);
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

    // --- Undo/Redo ---

    fn push_snapshot(&mut self, snapshot: TextSnapshot) {
        let doc = self.active_doc_mut();
        doc.undo_stack.push_back(snapshot);
        while doc.undo_stack.len() > doc.max_undo {
            doc.undo_stack.pop_front();
        }
    }

    fn save_snapshot(&mut self) {
        let doc = self.active_doc_mut();
        let (cursor_line, cursor_col) = doc.content.cursor_position();
        let snapshot = TextSnapshot {
            text: doc.content.text(),
            cursor_line,
            cursor_col,
        };
        self.push_snapshot(snapshot);
        let doc = self.active_doc_mut();
        doc.redo_stack.clear();
        doc.last_edit_time = None;
    }

    fn save_snapshot_if_needed(&mut self) {
        let now = Instant::now();
        let doc = self.active_doc_mut();
        let should_save = match doc.last_edit_time {
            Some(last) => now.duration_since(last).as_millis() > UNDO_BATCH_TIMEOUT_MS,
            None => true,
        };
        if should_save {
            let (cursor_line, cursor_col) = doc.content.cursor_position();
            let snapshot = TextSnapshot {
                text: doc.content.text(),
                cursor_line,
                cursor_col,
            };
            self.push_snapshot(snapshot);
            self.active_doc_mut().redo_stack.clear();
        }
        self.active_doc_mut().last_edit_time = Some(now);
    }

    fn undo(&mut self) {
        let doc = self.active_doc_mut();
        if let Some(snapshot) = doc.undo_stack.pop_back() {
            let (cursor_line, cursor_col) = doc.content.cursor_position();
            doc.redo_stack.push(TextSnapshot {
                text: doc.content.text(),
                cursor_line,
                cursor_col,
            });
            doc.content = text_editor::Content::with_text(&snapshot.text);
            doc.is_modified = true;
            doc.update_stats_cache();
            // navigate_to needs &mut self, so we drop doc first
            let line = snapshot.cursor_line;
            let col = snapshot.cursor_col;
            self.navigate_to(line, col);
        }
    }

    fn redo(&mut self) {
        let doc = self.active_doc_mut();
        if let Some(snapshot) = doc.redo_stack.pop() {
            let (cursor_line, cursor_col) = doc.content.cursor_position();
            doc.undo_stack.push_back(TextSnapshot {
                text: doc.content.text(),
                cursor_line,
                cursor_col,
            });
            doc.content = text_editor::Content::with_text(&snapshot.text);
            doc.is_modified = true;
            doc.update_stats_cache();
            let line = snapshot.cursor_line;
            let col = snapshot.cursor_col;
            self.navigate_to(line, col);
        }
    }

    // --- Line numbers sync ---

    fn sync_line_numbers(&self) -> Task<Message> {
        let line_height = self.font_size * 1.3;
        scrollable::scroll_to(
            line_numbers_id(),
            scrollable::AbsoluteOffset {
                x: 0.0,
                y: self.active_doc().scroll_offset * line_height,
            },
        )
    }

    // --- File I/O ---

    fn save_to_file(&mut self, path: PathBuf) {
        let doc = self.active_doc_mut();
        let bytes = doc.encode_content();
        if let Err(e) = std::fs::write(&path, bytes) {
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
            doc.file_path = Some(path);
            doc.is_modified = false;
            doc.status_message = Some(format!("Enregistré : {name}"));
        }
    }

    fn load_from_file(&mut self, path: PathBuf) {
        // --- File size guard ---
        let file_size_mb = std::fs::metadata(&path)
            .map(|m| m.len() / (1024 * 1024))
            .unwrap_or(0);

        if file_size_mb > FILE_SIZE_LIMIT_MB {
            rfd::MessageDialog::new()
                .set_title("Fichier trop volumineux")
                .set_description(format!(
                    "Ce fichier fait {file_size_mb} Mo.\n\
                     La limite est de {FILE_SIZE_LIMIT_MB} Mo."
                ))
                .set_level(rfd::MessageLevel::Error)
                .set_buttons(rfd::MessageButtons::Ok)
                .show();
            return;
        }

        if file_size_mb > FILE_SIZE_WARN_MB {
            let proceed = matches!(
                rfd::MessageDialog::new()
                    .set_title("Fichier volumineux")
                    .set_description(format!(
                        "Ce fichier fait {file_size_mb} Mo.\n\
                         L'ouvrir peut ralentir l'application. Continuer ?"
                    ))
                    .set_level(rfd::MessageLevel::Warning)
                    .set_buttons(rfd::MessageButtons::OkCancel)
                    .show(),
                rfd::MessageDialogResult::Ok
            );
            if !proceed {
                return;
            }
        }

        // --- Read bytes + detect encoding ---
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                rfd::MessageDialog::new()
                    .set_title("Erreur")
                    .set_description(format!("Impossible d'ouvrir le fichier :\n{e}"))
                    .set_level(rfd::MessageLevel::Error)
                    .set_buttons(rfd::MessageButtons::Ok)
                    .show();
                return;
            }
        };

        let (content_text, detected_encoding) = Self::decode_bytes(&bytes);

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("fichier")
            .to_string();

        let doc = self.active_doc_mut();
        doc.line_ending = LineEnding::detect(&content_text);
        doc.encoding = detected_encoding;
        let mut content = text_editor::Content::with_text(&content_text);
        content.perform(text_editor::Action::Move(
            text_editor::Motion::DocumentEnd,
        ));
        doc.content = content;
        doc.file_path = Some(path);
        doc.is_modified = false;
        doc.scroll_offset = 0.0;
        doc.undo_stack.clear();
        doc.redo_stack.clear();
        doc.last_edit_time = None;
        doc.status_message = Some(format!("Ouvert : {name}"));

        // Adaptive undo for large files
        if file_size_mb > 10 {
            doc.max_undo = LARGE_FILE_UNDO_HISTORY;
        } else {
            doc.max_undo = MAX_UNDO_HISTORY;
        }

        doc.update_stats_cache();
    }

    fn decode_bytes(bytes: &[u8]) -> (String, &'static encoding_rs::Encoding) {
        // 1. Check BOM
        if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
            let (text, _, _) = enc.decode(&bytes[bom_len..]);
            return (text.into_owned(), enc);
        }

        // 2. Try UTF-8
        let (text, encoding, had_errors) = encoding_rs::UTF_8.decode(bytes);
        if !had_errors {
            return (text.into_owned(), encoding);
        }

        // 3. Fallback to Windows-1252 (Latin)
        let (text, encoding, _) = encoding_rs::WINDOWS_1252.decode(bytes);
        (text.into_owned(), encoding)
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
        let doc = self.active_doc_mut();
        let (current_line, _) = doc.content.cursor_position();
        let last_line = doc.content.line_count().saturating_sub(1);
        let target_line = line.min(last_line);

        let from_start = target_line;
        let from_end = last_line - target_line;
        let from_current = target_line.abs_diff(current_line);

        let min_moves = from_start.min(from_end).min(from_current);

        if min_moves == from_current {
            if target_line > current_line {
                for _ in 0..(target_line - current_line) {
                    doc.content
                        .perform(text_editor::Action::Move(text_editor::Motion::Down));
                }
            } else {
                for _ in 0..(current_line - target_line) {
                    doc.content
                        .perform(text_editor::Action::Move(text_editor::Motion::Up));
                }
            }
        } else if min_moves == from_start {
            doc.content
                .perform(text_editor::Action::Move(text_editor::Motion::DocumentStart));
            for _ in 0..target_line {
                doc.content
                    .perform(text_editor::Action::Move(text_editor::Motion::Down));
            }
        } else {
            doc.content
                .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
            for _ in 0..from_end {
                doc.content
                    .perform(text_editor::Action::Move(text_editor::Motion::Up));
            }
        }

        doc.content
            .perform(text_editor::Action::Move(text_editor::Motion::Home));
        for _ in 0..col {
            doc.content
                .perform(text_editor::Action::Move(text_editor::Motion::Right));
        }

        doc.scroll_offset = target_line as f32;
    }

    fn select_chars(&mut self, count: usize) {
        let doc = self.active_doc_mut();
        for _ in 0..count {
            doc.content
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

    fn find_in(&self, haystack: &str, from: usize) -> Option<(usize, usize)> {
        let re = self.build_regex()?;
        re.find(&haystack[from..])
            .map(|m| (from + m.start(), m.len()))
    }

    fn rfind_in(&self, haystack: &str, until: usize) -> Option<(usize, usize)> {
        let re = self.build_regex()?;
        let mut last = None;
        for m in re.find_iter(&haystack[..until]) {
            last = Some((m.start(), m.len()));
        }
        last
    }

    fn find_next(&mut self) {
        let text = self.active_doc().content.text();
        if self.find_query.is_empty() || text.is_empty() {
            return;
        }

        let search_from = self.find_cursor.min(text.len());
        let found = if search_from < text.len() {
            self.find_in(&text, search_from)
        } else {
            None
        };

        let found = found.or_else(|| self.find_in(&text, 0));

        if let Some((byte_pos, mlen)) = found {
            self.highlight_match(byte_pos, mlen, &text);
        }
    }

    fn find_previous(&mut self) {
        let text = self.active_doc().content.text();
        if self.find_query.is_empty() || text.is_empty() {
            return;
        }

        let search_until = self.find_cursor.saturating_sub(1);

        let found = if search_until > 0 {
            self.rfind_in(&text, search_until)
        } else {
            None
        };

        let found = found.or_else(|| self.rfind_in(&text, text.len()));

        if let Some((byte_pos, mlen)) = found {
            self.highlight_match(byte_pos, mlen, &text);
        }
    }

    fn replace_one(&mut self) {
        if self.find_query.is_empty() {
            return;
        }
        if let Some(selected) = self.active_doc().content.selection() {
            let is_match = if let Some(re) = self.build_regex() {
                re.is_match(&selected)
                    && re.find(&selected).is_some_and(|m| m.len() == selected.len())
            } else {
                false
            };
            if is_match {
                self.save_snapshot();
                let replacement = self.replace_query.clone();
                let doc = self.active_doc_mut();
                doc.content.perform(text_editor::Action::Edit(
                    text_editor::Edit::Paste(Arc::new(replacement)),
                ));
                doc.is_modified = true;
                doc.update_stats_cache();
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
        let text = self.active_doc().content.text();
        let new_text = re
            .replace_all(&text, self.replace_query.as_str())
            .into_owned();
        if text != new_text {
            self.save_snapshot();
            let doc = self.active_doc_mut();
            doc.content = text_editor::Content::with_text(&new_text);
            doc.is_modified = true;
            doc.update_stats_cache();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Notepad, MAX_UNDO_HISTORY};

    fn notepad_with(text: &str) -> Notepad {
        let mut n = Notepad::test_default();
        n.active_doc_mut().content = text_editor::Content::with_text(text);
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
        let text = "café\nbar";
        assert_eq!(byte_pos_to_line_col(text, 6), (1, 0));
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
        assert!(!re.is_match("axb"));
    }

    // ============================
    // find_in / rfind_in
    // ============================

    #[test]
    fn find_in_from_start() {
        let mut n = notepad_with("hello world hello");
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
        n.find_next();
    }

    #[test]
    fn find_next_empty_text_no_crash() {
        let mut n = notepad_with("");
        n.find_query = "abc".to_string();
        n.find_next();
    }

    #[test]
    fn find_previous_empty_query_no_crash() {
        let mut n = notepad_with("some text");
        n.find_query = String::new();
        n.find_previous();
    }

    #[test]
    fn find_next_wraps_around() {
        let mut n = notepad_with("abc def abc");
        n.find_query = "abc".to_string();
        n.case_sensitive = true;
        n.find_cursor = 100;
        n.find_next();
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
        assert_eq!(n.active_doc().content.text().trim_end(), "hi world hi");
        assert!(n.active_doc().is_modified);
    }

    #[test]
    fn replace_all_case_insensitive() {
        let mut n = notepad_with("Hello HELLO hello");
        n.find_query = "hello".to_string();
        n.replace_query = "hi".to_string();
        n.case_sensitive = false;
        n.replace_all();
        assert_eq!(n.active_doc().content.text().trim_end(), "hi hi hi");
    }

    #[test]
    fn replace_all_empty_query_no_change() {
        let mut n = notepad_with("hello world");
        n.find_query = String::new();
        n.replace_query = "hi".to_string();
        n.replace_all();
        assert!(!n.active_doc().is_modified);
    }

    #[test]
    fn replace_all_no_match() {
        let mut n = notepad_with("hello world");
        n.find_query = "xyz".to_string();
        n.replace_query = "hi".to_string();
        n.case_sensitive = true;
        n.replace_all();
        assert!(!n.active_doc().is_modified);
    }

    // ============================
    // push_snapshot / undo / redo
    // ============================

    #[test]
    fn push_snapshot_respects_max_history() {
        let mut n = Notepad::test_default();
        for i in 0..MAX_UNDO_HISTORY + 10 {
            n.push_snapshot(TextSnapshot {
                text: format!("text{i}"),
                cursor_line: 0,
                cursor_col: 0,
            });
        }
        assert_eq!(n.active_doc().undo_stack.len(), MAX_UNDO_HISTORY);
    }

    #[test]
    fn undo_restores_previous_text() {
        let mut n = notepad_with("original");
        n.save_snapshot();
        n.active_doc_mut().content = text_editor::Content::with_text("modified");
        n.undo();
        assert_eq!(n.active_doc().content.text().trim_end(), "original");
    }

    #[test]
    fn redo_after_undo() {
        let mut n = notepad_with("original");
        n.save_snapshot();
        n.active_doc_mut().content = text_editor::Content::with_text("modified");
        n.active_doc_mut().is_modified = true;
        n.undo();
        assert_eq!(n.active_doc().content.text().trim_end(), "original");
        n.redo();
        assert_eq!(n.active_doc().content.text().trim_end(), "modified");
    }

    #[test]
    fn undo_on_empty_stack_is_noop() {
        let mut n = notepad_with("hello");
        n.undo();
        assert_eq!(n.active_doc().content.text().trim_end(), "hello");
    }

    #[test]
    fn redo_on_empty_stack_is_noop() {
        let mut n = notepad_with("hello");
        n.redo();
        assert_eq!(n.active_doc().content.text().trim_end(), "hello");
    }

    // ============================
    // Tab operations
    // ============================

    #[test]
    fn new_tab_adds_document() {
        let mut n = Notepad::test_default();
        assert_eq!(n.tabs.len(), 1);
        n.tabs.push(Document::default());
        n.active_tab = n.tabs.len() - 1;
        assert_eq!(n.tabs.len(), 2);
        assert_eq!(n.active_tab, 1);
    }

    #[test]
    fn close_tab_removes_document() {
        let mut n = Notepad::test_default();
        n.tabs.push(Document::default());
        n.tabs.push(Document::default());
        assert_eq!(n.tabs.len(), 3);
        n.remove_tab(1);
        assert_eq!(n.tabs.len(), 2);
    }

    #[test]
    fn close_last_tab_creates_new_empty() {
        let mut n = Notepad::test_default();
        n.active_doc_mut().is_modified = false;
        n.remove_tab(0);
        assert_eq!(n.tabs.len(), 1);
        assert_eq!(n.active_tab, 0);
        assert!(!n.active_doc().is_modified);
    }

    #[test]
    fn switch_tab_changes_active() {
        let mut n = Notepad::test_default();
        n.tabs.push(Document::default());
        n.active_tab = 0;
        n.active_tab = 1;
        assert_eq!(n.active_tab, 1);
    }

    #[test]
    fn close_tab_adjusts_active_index() {
        let mut n = Notepad::test_default();
        n.tabs.push(Document::default());
        n.tabs.push(Document::default());
        n.active_tab = 2;
        n.remove_tab(0);
        assert_eq!(n.active_tab, 1); // shifted down
    }

    // ============================
    // reset via remove_tab
    // ============================

    #[test]
    fn remove_tab_resets_when_last() {
        let mut n = notepad_with("some content");
        n.active_doc_mut().file_path = Some(PathBuf::from("/tmp/test.txt"));
        n.active_doc_mut().is_modified = true;
        n.remove_tab(0);
        assert!(n.active_doc().file_path.is_none());
        assert!(!n.active_doc().is_modified);
        assert!(n.active_doc().undo_stack.is_empty());
    }

    // ============================
    // decode_bytes / encoding
    // ============================

    #[test]
    fn decode_utf8_bytes() {
        let input = "Bonjour le monde".as_bytes();
        let (text, enc) = Notepad::decode_bytes(input);
        assert_eq!(text, "Bonjour le monde");
        assert_eq!(enc, encoding_rs::UTF_8);
    }

    #[test]
    fn decode_utf8_with_bom() {
        let mut input = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM
        input.extend_from_slice("Hello".as_bytes());
        let (text, enc) = Notepad::decode_bytes(&input);
        assert_eq!(text, "Hello");
        assert_eq!(enc, encoding_rs::UTF_8);
    }

    #[test]
    fn decode_latin1_fallback() {
        // 0xE9 = 'é' in Windows-1252, but invalid in UTF-8
        let input = vec![0x48, 0x65, 0x6C, 0x6C, 0x6F, 0xE9];
        let (text, enc) = Notepad::decode_bytes(&input);
        assert_eq!(text, "Helloé");
        assert_eq!(enc, encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn decode_utf16le_bom() {
        let mut input = vec![0xFF, 0xFE]; // UTF-16LE BOM
        input.extend_from_slice(&[0x48, 0x00, 0x69, 0x00]); // "Hi" in UTF-16LE
        let (text, enc) = Notepad::decode_bytes(&input);
        assert_eq!(text, "Hi");
        assert_eq!(enc, encoding_rs::UTF_16LE);
    }

    #[test]
    fn push_snapshot_respects_adaptive_max_undo() {
        let mut n = Notepad::test_default();
        n.active_doc_mut().max_undo = LARGE_FILE_UNDO_HISTORY;
        for i in 0..LARGE_FILE_UNDO_HISTORY + 10 {
            n.push_snapshot(TextSnapshot {
                text: format!("text{i}"),
                cursor_line: 0,
                cursor_col: 0,
            });
        }
        assert_eq!(n.active_doc().undo_stack.len(), LARGE_FILE_UNDO_HISTORY);
    }

    #[test]
    fn default_document_encoding_is_utf8() {
        let doc = Document::default();
        assert_eq!(doc.encoding, encoding_rs::UTF_8);
        assert_eq!(doc.max_undo, MAX_UNDO_HISTORY);
    }
}
