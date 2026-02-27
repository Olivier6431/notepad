#![windows_subsystem = "windows"]

use iced::keyboard::key::Named;
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::{
    button, container, horizontal_space, mouse_area, row, text, text_editor, text_input, Column,
    Row, Space, Stack,
};
use iced::{Element, Event, Length, Padding, Subscription, Task, Theme};
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_FONT_SIZE: f32 = 14.0;
const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 40.0;
const ZOOM_STEP: f32 = 2.0;
const MENU_BAR_HEIGHT: f32 = 30.0;
const MENU_ITEM_WIDTH: f32 = 220.0;

fn find_input_id() -> text_input::Id {
    text_input::Id::new("find_input")
}

fn replace_input_id() -> text_input::Id {
    text_input::Id::new("replace_input")
}

fn goto_input_id() -> text_input::Id {
    text_input::Id::new("goto_input")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Menu {
    File,
    Edit,
    Search,
    View,
}

struct Notepad {
    content: text_editor::Content,
    file_path: Option<PathBuf>,
    is_modified: bool,
    clipboard: Option<arboard::Clipboard>,

    // View settings
    font_size: f32,
    dark_mode: bool,

    // Find & Replace
    show_find: bool,
    show_replace: bool,
    find_query: String,
    replace_query: String,
    find_cursor: usize,

    // Go to line
    show_goto: bool,
    goto_input: String,

    // Menu state
    active_menu: Option<Menu>,
    show_context_menu: bool,
    mouse_position: iced::Point,
    context_menu_position: iced::Point,
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
            show_find: false,
            show_replace: false,
            find_query: String::new(),
            replace_query: String::new(),
            find_cursor: 0,
            show_goto: false,
            goto_input: String::new(),
            active_menu: None,
            show_context_menu: false,
            mouse_position: iced::Point::ORIGIN,
            context_menu_position: iced::Point::ORIGIN,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    EditorAction(text_editor::Action),
    New,
    Save,
    SaveAs,
    Open,
    Copy,
    Cut,
    Paste,
    SaveFileSelected(Option<PathBuf>),
    OpenFileSelected(Option<PathBuf>),
    EventOccurred(Event),

    // Zoom
    ZoomIn,
    ZoomOut,
    ZoomReset,

    // View
    ToggleDarkMode,

    // Date/Time
    InsertDateTime,

    // Find & Replace
    OpenFind,
    OpenReplace,
    CloseFind,
    FindQueryChanged(String),
    ReplaceQueryChanged(String),
    FindNext,
    FindPrevious,
    ReplaceOne,
    ReplaceAll,

    // Go to line
    OpenGoTo,
    CloseGoTo,
    GoToInputChanged(String),
    GoToLineSubmit,

    // Menu
    ToggleMenu(Menu),
    HoverMenu(Menu),
    CloseMenus,
    ShowContextMenu,
    SelectAll,
}

fn menu_left_offset(menu: Menu) -> f32 {
    match menu {
        Menu::File => 0.0,
        Menu::Edit => 70.0,
        Menu::Search => 140.0,
        Menu::View => 225.0,
    }
}

fn menu_item_widget<'a>(
    label: &str,
    shortcut: &str,
    msg: Message,
    shortcut_color: iced::Color,
) -> Element<'a, Message> {
    let mut content = Row::new()
        .push(text(label.to_string()).size(12))
        .push(horizontal_space())
        .spacing(8);
    if !shortcut.is_empty() {
        content = content.push(text(shortcut.to_string()).size(11).color(shortcut_color));
    }
    button(content)
        .on_press(msg)
        .style(button::text)
        .padding([4, 8])
        .width(MENU_ITEM_WIDTH)
        .into()
}

fn bar_style(
    bg_weak: iced::Color,
    bg_strong: iced::Color,
) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(bg_weak)),
        border: iced::Border {
            color: bg_strong,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn popup_style(
    bg_weak: iced::Color,
    bg_strong: iced::Color,
) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(bg_weak)),
        border: iced::Border {
            color: bg_strong,
            width: 1.0,
            radius: 4.0.into(),
        },
        shadow: iced::Shadow {
            color: iced::Color {
                a: 0.2,
                ..iced::Color::BLACK
            },
            offset: iced::Vector::new(2.0, 2.0),
            blur_radius: 8.0,
        },
        ..Default::default()
    }
}

fn overlay_at<'a>(
    content: impl Into<Element<'a, Message>>,
    top: f32,
    left: f32,
) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top,
            left,
            right: 0.0,
            bottom: 0.0,
        })
        .align_x(iced::Alignment::Start)
        .align_y(iced::Alignment::Start)
        .into()
}

fn byte_pos_to_line_col(text: &str, byte_pos: usize) -> (usize, usize) {
    let before = &text[..byte_pos];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let col = text[line_start..byte_pos].chars().count();
    (line, col)
}

impl Notepad {
    fn new() -> (Self, Task<Message>) {
        (Self::default(), Task::none())
    }

    fn title(&self) -> String {
        let name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Sans titre");
        let modified = if self.is_modified { " *" } else { "" };
        format!("{name}{modified} - Notepad")
    }

    fn theme(&self) -> Theme {
        if self.dark_mode {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        // Auto-close menus on most actions
        match &message {
            Message::HoverMenu(_)
            | Message::EventOccurred(_)
            | Message::ToggleMenu(_)
            | Message::ShowContextMenu
            | Message::CloseMenus
            | Message::FindQueryChanged(_)
            | Message::ReplaceQueryChanged(_)
            | Message::GoToInputChanged(_) => {}
            _ => {
                self.active_menu = None;
                self.show_context_menu = false;
            }
        }

        match message {
            Message::EditorAction(action) => {
                let is_edit = matches!(&action, text_editor::Action::Edit(_));
                self.content.perform(action);
                if is_edit {
                    self.is_modified = true;
                }
                Task::none()
            }
            Message::New => {
                self.content = text_editor::Content::with_text("");
                self.content
                    .perform(text_editor::Action::Click(iced::Point::new(0.0, 0.0)));
                self.file_path = None;
                self.is_modified = false;
                Task::none()
            }
            Message::Save => {
                if let Some(path) = self.file_path.clone() {
                    self.save_to_file(path);
                    Task::none()
                } else {
                    self.save_as()
                }
            }
            Message::SaveAs => self.save_as(),
            Message::Open => self.open_file(),
            Message::Copy => {
                if let Some(clipboard) = &mut self.clipboard {
                    if let Some(selected) = self.content.selection() {
                        let _ = clipboard.set_text(selected);
                    }
                }
                Task::none()
            }
            Message::Cut => {
                if let Some(clipboard) = &mut self.clipboard {
                    if let Some(selected) = self.content.selection() {
                        let _ = clipboard.set_text(selected);
                        self.content.perform(text_editor::Action::Edit(
                            text_editor::Edit::Backspace,
                        ));
                        self.is_modified = true;
                    }
                }
                Task::none()
            }
            Message::Paste => {
                if let Some(clipboard) = &mut self.clipboard {
                    if let Ok(clip_text) = clipboard.get_text() {
                        self.content.perform(text_editor::Action::Edit(
                            text_editor::Edit::Paste(Arc::new(clip_text)),
                        ));
                        self.is_modified = true;
                    }
                }
                Task::none()
            }
            Message::SaveFileSelected(path) => {
                if let Some(path) = path {
                    self.save_to_file(path);
                }
                Task::none()
            }
            Message::OpenFileSelected(path) => {
                if let Some(path) = path {
                    self.load_from_file(path);
                }
                Task::none()
            }

            // Zoom
            Message::ZoomIn => {
                self.font_size = (self.font_size + ZOOM_STEP).min(MAX_FONT_SIZE);
                Task::none()
            }
            Message::ZoomOut => {
                self.font_size = (self.font_size - ZOOM_STEP).max(MIN_FONT_SIZE);
                Task::none()
            }
            Message::ZoomReset => {
                self.font_size = DEFAULT_FONT_SIZE;
                Task::none()
            }

            // View
            Message::ToggleDarkMode => {
                self.dark_mode = !self.dark_mode;
                Task::none()
            }

            // Date/Time
            Message::InsertDateTime => {
                let now = chrono::Local::now();
                let datetime_str = now.format("%H:%M %d/%m/%Y").to_string();
                self.content.perform(text_editor::Action::Edit(
                    text_editor::Edit::Paste(Arc::new(datetime_str)),
                ));
                self.is_modified = true;
                Task::none()
            }

            // Find & Replace
            Message::OpenFind => {
                self.show_find = true;
                self.show_replace = false;
                self.show_goto = false;
                text_input::focus(find_input_id())
            }
            Message::OpenReplace => {
                self.show_find = true;
                self.show_replace = true;
                self.show_goto = false;
                text_input::focus(find_input_id())
            }
            Message::CloseFind => {
                self.show_find = false;
                self.show_replace = false;
                Task::none()
            }
            Message::FindQueryChanged(query) => {
                self.find_query = query;
                self.find_cursor = 0;
                Task::none()
            }
            Message::ReplaceQueryChanged(query) => {
                self.replace_query = query;
                Task::none()
            }
            Message::FindNext => {
                self.find_next();
                Task::none()
            }
            Message::FindPrevious => {
                self.find_previous();
                Task::none()
            }
            Message::ReplaceOne => {
                self.replace_one();
                Task::none()
            }
            Message::ReplaceAll => {
                self.replace_all();
                Task::none()
            }

            // Go to line
            Message::OpenGoTo => {
                self.show_goto = true;
                self.show_find = false;
                self.show_replace = false;
                self.goto_input.clear();
                text_input::focus(goto_input_id())
            }
            Message::CloseGoTo => {
                self.show_goto = false;
                Task::none()
            }
            Message::GoToInputChanged(value) => {
                self.goto_input = value;
                Task::none()
            }
            Message::GoToLineSubmit => {
                if let Ok(line_num) = self.goto_input.parse::<usize>() {
                    let target = line_num.saturating_sub(1);
                    self.navigate_to(target, 0);
                    self.show_goto = false;
                }
                Task::none()
            }

            // Menu
            Message::ToggleMenu(menu) => {
                if self.active_menu == Some(menu) {
                    self.active_menu = None;
                } else {
                    self.active_menu = Some(menu);
                }
                self.show_context_menu = false;
                Task::none()
            }
            Message::HoverMenu(menu) => {
                if self.active_menu.is_some() {
                    self.active_menu = Some(menu);
                }
                Task::none()
            }
            Message::CloseMenus => {
                self.active_menu = None;
                self.show_context_menu = false;
                Task::none()
            }
            Message::ShowContextMenu => {
                self.show_context_menu = true;
                self.context_menu_position = self.mouse_position;
                self.active_menu = None;
                Task::none()
            }
            Message::SelectAll => {
                self.content
                    .perform(text_editor::Action::Move(text_editor::Motion::DocumentStart));
                self.content
                    .perform(text_editor::Action::Select(text_editor::Motion::DocumentEnd));
                Task::none()
            }

            // Events
            Message::EventOccurred(event) => {
                // Track mouse position for context menu
                if let Event::Mouse(iced::mouse::Event::CursorMoved { position }) = &event {
                    self.mouse_position = *position;
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
                            return self.update(Message::FindNext);
                        }
                        // F5 - Date/Time
                        (Key::Named(Named::F5), _) => {
                            return self.update(Message::InsertDateTime);
                        }
                        // Ctrl+Shift+S - Save As
                        (Key::Character("s"), m)
                            if m == (Modifiers::CTRL | Modifiers::SHIFT) =>
                        {
                            return self.update(Message::SaveAs);
                        }
                        // Ctrl+key combinations
                        (Key::Character("n"), Modifiers::CTRL) => {
                            return self.update(Message::New);
                        }
                        (Key::Character("s"), Modifiers::CTRL) => {
                            return self.update(Message::Save);
                        }
                        (Key::Character("o"), Modifiers::CTRL) => {
                            return self.update(Message::Open);
                        }
                        (Key::Character("f"), Modifiers::CTRL) => {
                            return self.update(Message::OpenFind);
                        }
                        (Key::Character("h"), Modifiers::CTRL) => {
                            return self.update(Message::OpenReplace);
                        }
                        (Key::Character("g"), Modifiers::CTRL) => {
                            return self.update(Message::OpenGoTo);
                        }
                        // Zoom
                        (Key::Character("="), Modifiers::CTRL) => {
                            return self.update(Message::ZoomIn);
                        }
                        (Key::Character("+"), m) if m.contains(Modifiers::CTRL) => {
                            return self.update(Message::ZoomIn);
                        }
                        (Key::Character("-"), Modifiers::CTRL) => {
                            return self.update(Message::ZoomOut);
                        }
                        (Key::Character("0"), Modifiers::CTRL) => {
                            return self.update(Message::ZoomReset);
                        }
                        _ => {}
                    }
                }
                Task::none()
            }
        }
    }

    // --- File operations ---

    fn save_to_file(&mut self, path: PathBuf) {
        let content = self.content.text();
        if let Err(e) = std::fs::write(&path, content) {
            eprintln!("Erreur lors de l'enregistrement: {e}");
        } else {
            self.file_path = Some(path);
            self.is_modified = false;
        }
    }

    fn load_from_file(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(content_text) => {
                let mut content = text_editor::Content::with_text(&content_text);
                content.perform(text_editor::Action::Move(
                    text_editor::Motion::DocumentEnd,
                ));
                self.content = content;
                self.file_path = Some(path);
                self.is_modified = false;
            }
            Err(e) => eprintln!("Erreur lors de l'ouverture du fichier: {e}"),
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
            Message::SaveFileSelected,
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
            Message::OpenFileSelected,
        )
    }

    // --- Find & Replace ---

    fn navigate_to(&mut self, line: usize, col: usize) {
        self.content
            .perform(text_editor::Action::Move(text_editor::Motion::DocumentStart));
        for _ in 0..line {
            self.content
                .perform(text_editor::Action::Move(text_editor::Motion::Down));
        }
        self.content
            .perform(text_editor::Action::Move(text_editor::Motion::Home));
        for _ in 0..col {
            self.content
                .perform(text_editor::Action::Move(text_editor::Motion::Right));
        }
    }

    fn select_chars(&mut self, count: usize) {
        for _ in 0..count {
            self.content
                .perform(text_editor::Action::Select(text_editor::Motion::Right));
        }
    }

    fn highlight_match(&mut self, byte_pos: usize, text: &str) {
        self.find_cursor = byte_pos + self.find_query.len();
        let (line, col) = byte_pos_to_line_col(text, byte_pos);
        self.navigate_to(line, col);
        let query_chars = self.find_query.chars().count();
        self.select_chars(query_chars);
    }

    fn find_next(&mut self) {
        let text = self.content.text();
        if self.find_query.is_empty() || text.is_empty() {
            return;
        }

        let search_from = self.find_cursor.min(text.len());
        let found = if search_from < text.len() {
            text[search_from..]
                .find(&self.find_query)
                .map(|p| search_from + p)
        } else {
            None
        };

        // Wrap around if not found
        let found = found.or_else(|| text.find(&self.find_query));

        if let Some(byte_pos) = found {
            self.highlight_match(byte_pos, &text);
        }
    }

    fn find_previous(&mut self) {
        let text = self.content.text();
        if self.find_query.is_empty() || text.is_empty() {
            return;
        }

        let search_until = if self.find_cursor > self.find_query.len() {
            self.find_cursor - self.find_query.len()
        } else {
            0
        };

        let found = if search_until > 0 {
            text[..search_until].rfind(&self.find_query)
        } else {
            None
        };

        // Wrap around if not found
        let found = found.or_else(|| text.rfind(&self.find_query));

        if let Some(byte_pos) = found {
            self.highlight_match(byte_pos, &text);
        }
    }

    fn replace_one(&mut self) {
        if self.find_query.is_empty() {
            return;
        }
        if let Some(selected) = self.content.selection() {
            if selected == self.find_query {
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
        let text = self.content.text();
        let new_text = text.replace(&self.find_query, &self.replace_query);
        if text != new_text {
            self.content = text_editor::Content::with_text(&new_text);
            self.is_modified = true;
        }
    }

    // --- View ---

    fn view(&self) -> Element<'_, Message> {
        let theme = self.theme();
        let palette = theme.extended_palette();

        // Extract colors as owned values to avoid lifetime issues with closures
        let bg_weak = palette.background.weak.color;
        let bg_strong = palette.background.strong.color;
        let bg_base = palette.background.base.color;
        let bg_text = palette.background.base.text;
        let primary_weak = palette.primary.weak.color;
        let shortcut_color = iced::Color { a: 0.5, ..bg_text };

        let mut layout = Column::new();

        // --- Menu bar ---
        let menus = [
            (Menu::File, "Fichier"),
            (Menu::Edit, "Edition"),
            (Menu::Search, "Recherche"),
            (Menu::View, "Affichage"),
        ];
        let mut menu_row = Row::new().spacing(0);
        for (menu, label) in menus {
            let is_active = self.active_menu == Some(menu);
            let btn = button(text(label).size(12))
                .on_press(Message::ToggleMenu(menu))
                .padding([6, 12])
                .style(if is_active { button::primary } else { button::text });
            let area = mouse_area(btn).on_enter(Message::HoverMenu(menu));
            menu_row = menu_row.push(area);
        }
        let menu_bar = container(menu_row)
            .style(bar_style(bg_weak, bg_strong))
            .width(Length::Fill)
            .height(MENU_BAR_HEIGHT);
        layout = layout.push(menu_bar);

        // --- Find bar ---
        if self.show_find {
            let mut find_row = row![
                text("Rechercher:").size(12),
                text_input("Rechercher...", &self.find_query)
                    .id(find_input_id())
                    .on_input(Message::FindQueryChanged)
                    .on_submit(Message::FindNext)
                    .size(12)
                    .width(200),
                button(text("Suivant").size(11))
                    .on_press(Message::FindNext)
                    .padding(4)
                    .style(button::secondary),
                button(text("Précédent").size(11))
                    .on_press(Message::FindPrevious)
                    .padding(4)
                    .style(button::secondary),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center);

            if self.show_replace {
                find_row = find_row
                    .push(container(text("|").size(14)).padding([0, 4]))
                    .push(text("Remplacer:").size(12))
                    .push(
                        text_input("Remplacer par...", &self.replace_query)
                            .id(replace_input_id())
                            .on_input(Message::ReplaceQueryChanged)
                            .on_submit(Message::ReplaceOne)
                            .size(12)
                            .width(200),
                    )
                    .push(
                        button(text("Remplacer").size(11))
                            .on_press(Message::ReplaceOne)
                            .padding(4)
                            .style(button::secondary),
                    )
                    .push(
                        button(text("Tout").size(11))
                            .on_press(Message::ReplaceAll)
                            .padding(4)
                            .style(button::secondary),
                    );
            }

            find_row = find_row.push(horizontal_space()).push(
                button(text("X").size(11))
                    .on_press(Message::CloseFind)
                    .padding(4)
                    .style(button::secondary),
            );

            let find_bar = container(find_row.padding(5))
                .style(bar_style(bg_weak, bg_strong))
                .width(Length::Fill);
            layout = layout.push(find_bar);
        }

        // --- Go to line bar ---
        if self.show_goto {
            let goto_row = row![
                text("Aller à la ligne:").size(12),
                text_input("Numéro de ligne...", &self.goto_input)
                    .id(goto_input_id())
                    .on_input(Message::GoToInputChanged)
                    .on_submit(Message::GoToLineSubmit)
                    .size(12)
                    .width(150),
                button(text("Aller").size(11))
                    .on_press(Message::GoToLineSubmit)
                    .padding(4)
                    .style(button::secondary),
                horizontal_space(),
                button(text("X").size(11))
                    .on_press(Message::CloseGoTo)
                    .padding(4)
                    .style(button::secondary),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center);

            let goto_bar = container(goto_row.padding(5))
                .style(bar_style(bg_weak, bg_strong))
                .width(Length::Fill);
            layout = layout.push(goto_bar);
        }

        // --- Editor ---
        let editor = text_editor(&self.content)
            .on_action(Message::EditorAction)
            .padding(10)
            .size(self.font_size)
            .wrapping(text::Wrapping::Word)
            .height(Length::Fill)
            .style(move |_theme, _status| text_editor::Style {
                background: iced::Background::Color(bg_base),
                border: iced::Border {
                    color: bg_strong,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                icon: bg_text,
                placeholder: iced::Color {
                    a: 0.4,
                    ..bg_text
                },
                value: bg_text,
                selection: primary_weak,
            });
        let editor_area = mouse_area(editor)
            .on_right_press(Message::ShowContextMenu);
        layout = layout.push(editor_area);

        // --- Status bar ---
        let (line, col) = self.content.cursor_position();
        let line_count = self.content.line_count();
        let char_count = self.content.text().len();
        let zoom_pct = (self.font_size / DEFAULT_FONT_SIZE * 100.0) as u32;

        let status_bar = container(
            row![
                text(format!("Ln {}, Col {}", line + 1, col + 1)).size(11),
                horizontal_space(),
                text(format!("{} caractères", char_count)).size(11),
                container(text("|").size(11)).padding([0, 8]),
                text(format!("{} lignes", line_count)).size(11),
                container(text("|").size(11)).padding([0, 8]),
                text(format!("Zoom: {}%", zoom_pct)).size(11),
                container(text("|").size(11)).padding([0, 8]),
                text("UTF-8").size(11),
            ]
            .padding(6),
        )
        .style(bar_style(bg_weak, bg_strong))
        .width(Length::Fill);
        layout = layout.push(status_bar);

        // --- Stack overlays ---
        let mut layers = Stack::new().push(layout);

        // Click-catcher to close menus when clicking outside
        if self.active_menu.is_some() || self.show_context_menu {
            layers = layers.push(
                mouse_area(Space::new(Length::Fill, Length::Fill))
                    .on_press(Message::CloseMenus),
            );
        }

        // Dropdown overlay
        if let Some(menu) = self.active_menu {
            let items: Vec<Element<'_, Message>> = match menu {
                Menu::File => vec![
                    menu_item_widget("Nouveau", "Ctrl+N", Message::New, shortcut_color),
                    menu_item_widget("Ouvrir...", "Ctrl+O", Message::Open, shortcut_color),
                    menu_item_widget("Enregistrer", "Ctrl+S", Message::Save, shortcut_color),
                    menu_item_widget("Enregistrer sous...", "Ctrl+Shift+S", Message::SaveAs, shortcut_color),
                ],
                Menu::Edit => vec![
                    menu_item_widget("Couper", "Ctrl+X", Message::Cut, shortcut_color),
                    menu_item_widget("Copier", "Ctrl+C", Message::Copy, shortcut_color),
                    menu_item_widget("Coller", "Ctrl+V", Message::Paste, shortcut_color),
                    menu_item_widget("Tout sélectionner", "Ctrl+A", Message::SelectAll, shortcut_color),
                    menu_item_widget("Date/Heure", "F5", Message::InsertDateTime, shortcut_color),
                ],
                Menu::Search => vec![
                    menu_item_widget("Rechercher...", "Ctrl+F", Message::OpenFind, shortcut_color),
                    menu_item_widget("Remplacer...", "Ctrl+H", Message::OpenReplace, shortcut_color),
                    menu_item_widget("Aller à la ligne...", "Ctrl+G", Message::OpenGoTo, shortcut_color),
                ],
                Menu::View => {
                    let theme_label = if self.dark_mode { "Mode clair" } else { "Mode sombre" };
                    vec![
                        menu_item_widget(theme_label, "", Message::ToggleDarkMode, shortcut_color),
                        menu_item_widget("Zoom +", "Ctrl+=", Message::ZoomIn, shortcut_color),
                        menu_item_widget("Zoom -", "Ctrl+-", Message::ZoomOut, shortcut_color),
                        menu_item_widget("Zoom réinitialiser", "Ctrl+0", Message::ZoomReset, shortcut_color),
                    ]
                }
            };

            let dropdown = container(Column::with_children(items).spacing(2).padding(4))
                .style(popup_style(bg_weak, bg_strong));

            let left_offset = menu_left_offset(menu);
            layers = layers.push(overlay_at(dropdown, MENU_BAR_HEIGHT, left_offset));
        }

        // Context menu overlay
        if self.show_context_menu {
            let ctx_items: Vec<Element<'_, Message>> = vec![
                menu_item_widget("Couper", "Ctrl+X", Message::Cut, shortcut_color),
                menu_item_widget("Copier", "Ctrl+C", Message::Copy, shortcut_color),
                menu_item_widget("Coller", "Ctrl+V", Message::Paste, shortcut_color),
                menu_item_widget("Tout sélectionner", "Ctrl+A", Message::SelectAll, shortcut_color),
            ];

            let ctx_menu = container(Column::with_children(ctx_items).spacing(2).padding(4))
                .style(popup_style(bg_weak, bg_strong));

            layers = layers.push(overlay_at(
                ctx_menu,
                self.context_menu_position.y,
                self.context_menu_position.x,
            ));
        }

        layers.into()
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::event::listen().map(Message::EventOccurred)
    }
}

fn main() -> iced::Result {
    iced::application(Notepad::title, Notepad::update, Notepad::view)
        .theme(Notepad::theme)
        .subscription(Notepad::subscription)
        .run_with(Notepad::new)
}
