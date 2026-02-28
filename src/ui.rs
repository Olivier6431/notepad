use iced::widget::{
    button, container, horizontal_space, mouse_area, row, scrollable, text, text_editor,
    text_input, Column, Row, Space, Stack,
};
use iced::{Element, Length, Padding, Theme};

use crate::app::{
    find_input_id, goto_input_id, replace_input_id, EditMsg, FileMsg, Menu, MenuMsg, Message,
    Notepad, SearchMsg, ViewMsg, MENU_BAR_HEIGHT, MENU_ITEM_WIDTH, TAB_BAR_HEIGHT,
};
use crate::DEFAULT_FONT_SIZE;

pub fn line_numbers_id() -> scrollable::Id {
    scrollable::Id::new("line_numbers")
}

const MENU_LABELS: &[(Menu, &str)] = &[
    (Menu::File, "Fichier"),
    (Menu::Edit, "Edition"),
    (Menu::Search, "Recherche"),
    (Menu::View, "Affichage"),
];

const MENU_FONT_SIZE: f32 = 12.0;
const MENU_H_PADDING: f32 = 12.0;

fn menu_left_offset(menu: Menu) -> f32 {
    let mut offset = 0.0;
    for &(m, label) in MENU_LABELS {
        if m == menu {
            break;
        }
        let text_width = label.chars().count() as f32 * MENU_FONT_SIZE * 0.6;
        offset += text_width + MENU_H_PADDING * 2.0;
    }
    offset
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

const MENU_ITEM_HEIGHT: f32 = 24.0;
const MENU_ITEM_SPACING: f32 = 2.0;
const MENU_CONTAINER_PADDING: f32 = 4.0;

fn menu_popup_size(item_count: usize) -> (f32, f32) {
    let height = item_count as f32 * MENU_ITEM_HEIGHT
        + (item_count.saturating_sub(1)) as f32 * MENU_ITEM_SPACING
        + MENU_CONTAINER_PADDING * 2.0;
    let width = MENU_ITEM_WIDTH + MENU_CONTAINER_PADDING * 2.0;
    (width, height)
}

fn clamp_popup_position(
    mut x: f32,
    mut y: f32,
    popup_w: f32,
    popup_h: f32,
    window_w: f32,
    window_h: f32,
) -> (f32, f32) {
    if x + popup_w > window_w {
        x = (window_w - popup_w).max(0.0);
    }
    if y + popup_h > window_h {
        y = (window_h - popup_h).max(0.0);
    }
    (x, y)
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

impl Notepad {
    pub fn view(&self) -> Element<'_, Message> {
        let theme = self.theme();
        let palette = theme.extended_palette();

        let bg_weak = palette.background.weak.color;
        let bg_strong = palette.background.strong.color;
        let bg_base = palette.background.base.color;
        let bg_text = palette.background.base.text;
        let primary_weak = palette.primary.weak.color;
        let shortcut_color = iced::Color { a: 0.5, ..bg_text };

        let doc = self.active_doc();
        let mut layout = Column::new();

        // --- Menu bar ---
        let mut menu_row = Row::new().spacing(0);
        for &(menu, label) in MENU_LABELS {
            let is_active = self.active_menu == Some(menu);
            let btn = button(text(label).size(MENU_FONT_SIZE))
                .on_press(Message::Menu(MenuMsg::Toggle(menu)))
                .padding(Padding {
                    top: 6.0,
                    bottom: 6.0,
                    left: MENU_H_PADDING,
                    right: MENU_H_PADDING,
                })
                .style(if is_active {
                    button::primary
                } else {
                    button::text
                });
            let area = mouse_area(btn).on_enter(Message::Menu(MenuMsg::Hover(menu)));
            menu_row = menu_row.push(area);
        }
        let menu_bar = container(menu_row)
            .style(bar_style(bg_weak, bg_strong))
            .width(Length::Fill)
            .height(MENU_BAR_HEIGHT);
        layout = layout.push(menu_bar);

        // --- Tab bar ---
        let mut tab_row = Row::new().spacing(0);
        for (i, tab_doc) in self.tabs.iter().enumerate() {
            let is_active_tab = i == self.active_tab;
            let label = tab_doc.title_label();

            // Tab button with close X
            let tab_content = Row::new()
                .push(text(label).size(11))
                .push(
                    button(text("×").size(11))
                        .on_press(Message::File(FileMsg::CloseTab(i)))
                        .padding(Padding {
                            top: 0.0,
                            bottom: 0.0,
                            left: 6.0,
                            right: 0.0,
                        })
                        .style(button::text),
                )
                .spacing(2)
                .align_y(iced::Alignment::Center);

            let tab_btn = button(tab_content)
                .on_press(Message::File(FileMsg::SwitchTab(i)))
                .padding(Padding {
                    top: 6.0,
                    bottom: 6.0,
                    left: 10.0,
                    right: 6.0,
                })
                .style(if is_active_tab {
                    button::primary
                } else {
                    button::text
                });

            tab_row = tab_row.push(tab_btn);
        }

        // "+" button for new tab
        tab_row = tab_row.push(
            button(text("+").size(12))
                .on_press(Message::File(FileMsg::NewTab))
                .padding(Padding {
                    top: 6.0,
                    bottom: 6.0,
                    left: 8.0,
                    right: 8.0,
                })
                .style(button::text),
        );

        let tab_bar = container(tab_row)
            .style(bar_style(bg_weak, bg_strong))
            .width(Length::Fill)
            .height(TAB_BAR_HEIGHT);
        layout = layout.push(tab_bar);

        // --- Find bar ---
        if self.show_find {
            let case_style = if self.case_sensitive {
                button::primary
            } else {
                button::secondary
            };
            let regex_style = if self.use_regex {
                button::primary
            } else {
                button::secondary
            };
            let mut find_row = row![
                text("Rechercher:").size(12),
                text_input("Rechercher...", &self.find_query)
                    .id(find_input_id())
                    .on_input(|s| Message::Search(SearchMsg::FindQueryChanged(s)))
                    .on_submit(Message::Search(SearchMsg::FindNext))
                    .size(12)
                    .width(200),
                button(text("Aa").size(11))
                    .on_press(Message::Search(SearchMsg::ToggleCaseSensitive))
                    .padding(4)
                    .style(case_style),
                button(text(".*").size(11))
                    .on_press(Message::Search(SearchMsg::ToggleRegex))
                    .padding(4)
                    .style(regex_style),
                button(text("Suivant").size(11))
                    .on_press(Message::Search(SearchMsg::FindNext))
                    .padding(4)
                    .style(button::secondary),
                button(text("Précédent").size(11))
                    .on_press(Message::Search(SearchMsg::FindPrevious))
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
                            .on_input(|s| Message::Search(SearchMsg::ReplaceQueryChanged(s)))
                            .on_submit(Message::Search(SearchMsg::ReplaceOne))
                            .size(12)
                            .width(200),
                    )
                    .push(
                        button(text("Remplacer").size(11))
                            .on_press(Message::Search(SearchMsg::ReplaceOne))
                            .padding(4)
                            .style(button::secondary),
                    )
                    .push(
                        button(text("Tout").size(11))
                            .on_press(Message::Search(SearchMsg::ReplaceAll))
                            .padding(4)
                            .style(button::secondary),
                    );
            }

            find_row = find_row.push(horizontal_space()).push(
                button(text("X").size(11))
                    .on_press(Message::Search(SearchMsg::CloseFind))
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
                    .on_input(|s| Message::Search(SearchMsg::GoToInputChanged(s)))
                    .on_submit(Message::Search(SearchMsg::GoToLineSubmit))
                    .size(12)
                    .width(150),
                button(text("Aller").size(11))
                    .on_press(Message::Search(SearchMsg::GoToLineSubmit))
                    .padding(4)
                    .style(button::secondary),
                horizontal_space(),
                button(text("X").size(11))
                    .on_press(Message::Search(SearchMsg::CloseGoTo))
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

        // --- Editor with line numbers ---
        let total_lines = doc.content.line_count();
        let digits = total_lines.max(1).to_string().len().max(3);
        let gutter_width = digits as f32 * self.font_size * 0.6 + 20.0;
        let line_number_color = iced::Color { a: 0.45, ..bg_text };

        let mut line_nums = Column::new();
        for i in 1..=total_lines {
            line_nums = line_nums.push(
                container(
                    text(i.to_string())
                        .size(self.font_size)
                        .color(line_number_color),
                )
                .width(gutter_width)
                .align_x(iced::Alignment::End)
                .padding(Padding {
                    top: 0.0,
                    right: 8.0,
                    bottom: 0.0,
                    left: 4.0,
                }),
            );
        }

        let line_gutter = scrollable(
            container(line_nums).padding(Padding {
                top: 10.0,
                right: 0.0,
                bottom: 10.0,
                left: 0.0,
            }),
        )
        .id(line_numbers_id())
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(0).scroller_width(0),
        ))
        .height(Length::Fill);

        let gutter_container = container(line_gutter)
            .style(bar_style(bg_weak, bg_strong))
            .height(Length::Fill);

        let editor = text_editor(&doc.content)
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
        let editor_area =
            mouse_area(editor).on_right_press(Message::Menu(MenuMsg::ShowContext));

        let editor_row = Row::new()
            .push(gutter_container)
            .push(editor_area)
            .height(Length::Fill);
        layout = layout.push(editor_row);

        // --- Status bar ---
        let (line, col) = doc.content.cursor_position();
        let line_count = doc.content.line_count();
        let content_text = doc.content.text();
        let char_count = content_text.len();
        let word_count = content_text.split_whitespace().count();
        let zoom_pct = (self.font_size / DEFAULT_FONT_SIZE * 100.0) as u32;

        let mut status_row = row![
            text(format!("Ln {}, Col {}", line + 1, col + 1)).size(11),
        ]
        .spacing(0)
        .padding(6);

        if let Some(msg) = &doc.status_message {
            status_row = status_row
                .push(container(text("|").size(11)).padding([0, 8]))
                .push(text(msg.clone()).size(11).color(palette.success.base.color));
        }

        status_row = status_row
            .push(horizontal_space())
            .push(text(format!("{} mots", word_count)).size(11))
            .push(container(text("|").size(11)).padding([0, 8]))
            .push(text(format!("{} caractères", char_count)).size(11))
            .push(container(text("|").size(11)).padding([0, 8]))
            .push(text(format!("{} lignes", line_count)).size(11))
            .push(container(text("|").size(11)).padding([0, 8]))
            .push(text(format!("Zoom: {}%", zoom_pct)).size(11))
            .push(container(text("|").size(11)).padding([0, 8]))
            .push(text(doc.line_ending.label()).size(11))
            .push(container(text("|").size(11)).padding([0, 8]))
            .push(text("UTF-8").size(11));

        let status_bar = container(status_row)
            .style(bar_style(bg_weak, bg_strong))
            .width(Length::Fill);
        layout = layout.push(status_bar);

        // --- Stack overlays ---
        let mut layers = Stack::new().push(layout);

        if self.active_menu.is_some() || self.show_context_menu {
            layers = layers.push(
                mouse_area(Space::new(Length::Fill, Length::Fill))
                    .on_press(Message::Menu(MenuMsg::CloseAll)),
            );
        }

        // Dropdown overlay
        if let Some(menu) = self.active_menu {
            let items: Vec<Element<'_, Message>> = match menu {
                Menu::File => vec![
                    menu_item_widget(
                        "Nouvel onglet",
                        "Ctrl+N",
                        Message::File(FileMsg::NewTab),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Ouvrir...",
                        "Ctrl+O",
                        Message::File(FileMsg::Open),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Enregistrer",
                        "Ctrl+S",
                        Message::File(FileMsg::Save),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Enregistrer sous...",
                        "Ctrl+Shift+S",
                        Message::File(FileMsg::SaveAs),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Fermer l'onglet",
                        "Ctrl+W",
                        Message::File(FileMsg::CloseTab(self.active_tab)),
                        shortcut_color,
                    ),
                ],
                Menu::Edit => vec![
                    menu_item_widget(
                        "Annuler",
                        "Ctrl+Z",
                        Message::Edit(EditMsg::Undo),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Rétablir",
                        "Ctrl+Y",
                        Message::Edit(EditMsg::Redo),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Couper",
                        "Ctrl+X",
                        Message::Edit(EditMsg::Cut),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Copier",
                        "Ctrl+C",
                        Message::Edit(EditMsg::Copy),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Coller",
                        "Ctrl+V",
                        Message::Edit(EditMsg::Paste),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Tout sélectionner",
                        "Ctrl+A",
                        Message::Edit(EditMsg::SelectAll),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Date/Heure",
                        "F5",
                        Message::Edit(EditMsg::InsertDateTime),
                        shortcut_color,
                    ),
                ],
                Menu::Search => vec![
                    menu_item_widget(
                        "Rechercher...",
                        "Ctrl+F",
                        Message::Search(SearchMsg::OpenFind),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Remplacer...",
                        "Ctrl+H",
                        Message::Search(SearchMsg::OpenReplace),
                        shortcut_color,
                    ),
                    menu_item_widget(
                        "Aller à la ligne...",
                        "Ctrl+G",
                        Message::Search(SearchMsg::OpenGoTo),
                        shortcut_color,
                    ),
                ],
                Menu::View => {
                    let theme_label = if self.dark_mode {
                        "Mode clair"
                    } else {
                        "Mode sombre"
                    };
                    vec![
                        menu_item_widget(
                            theme_label,
                            "",
                            Message::View(ViewMsg::ToggleDarkMode),
                            shortcut_color,
                        ),
                        menu_item_widget(
                            "Zoom +",
                            "Ctrl+=",
                            Message::View(ViewMsg::ZoomIn),
                            shortcut_color,
                        ),
                        menu_item_widget(
                            "Zoom -",
                            "Ctrl+-",
                            Message::View(ViewMsg::ZoomOut),
                            shortcut_color,
                        ),
                        menu_item_widget(
                            "Zoom réinitialiser",
                            "Ctrl+0",
                            Message::View(ViewMsg::ZoomReset),
                            shortcut_color,
                        ),
                    ]
                }
            };

            let item_count = items.len();
            let dropdown = container(
                Column::with_children(items)
                    .spacing(MENU_ITEM_SPACING)
                    .padding(MENU_CONTAINER_PADDING),
            )
            .style(popup_style(bg_weak, bg_strong));

            let left_offset = menu_left_offset(menu);
            let (popup_w, popup_h) = menu_popup_size(item_count);
            let (left_offset, top_offset) = clamp_popup_position(
                left_offset,
                MENU_BAR_HEIGHT,
                popup_w,
                popup_h,
                self.window_width,
                self.window_height,
            );
            layers = layers.push(overlay_at(dropdown, top_offset, left_offset));
        }

        // Context menu overlay
        if self.show_context_menu {
            let ctx_items: Vec<Element<'_, Message>> = vec![
                menu_item_widget(
                    "Couper",
                    "Ctrl+X",
                    Message::Edit(EditMsg::Cut),
                    shortcut_color,
                ),
                menu_item_widget(
                    "Copier",
                    "Ctrl+C",
                    Message::Edit(EditMsg::Copy),
                    shortcut_color,
                ),
                menu_item_widget(
                    "Coller",
                    "Ctrl+V",
                    Message::Edit(EditMsg::Paste),
                    shortcut_color,
                ),
                menu_item_widget(
                    "Tout sélectionner",
                    "Ctrl+A",
                    Message::Edit(EditMsg::SelectAll),
                    shortcut_color,
                ),
            ];

            let ctx_count = ctx_items.len();
            let ctx_menu = container(
                Column::with_children(ctx_items)
                    .spacing(MENU_ITEM_SPACING)
                    .padding(MENU_CONTAINER_PADDING),
            )
            .style(popup_style(bg_weak, bg_strong));

            let (popup_w, popup_h) = menu_popup_size(ctx_count);
            let (ctx_x, ctx_y) = clamp_popup_position(
                self.context_menu_position.x,
                self.context_menu_position.y,
                popup_w,
                popup_h,
                self.window_width,
                self.window_height,
            );
            layers = layers.push(overlay_at(ctx_menu, ctx_y, ctx_x));
        }

        layers.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Menu;

    // ============================
    // menu_left_offset
    // ============================

    #[test]
    fn menu_left_offset_file_is_zero() {
        assert_eq!(menu_left_offset(Menu::File), 0.0);
    }

    #[test]
    fn menu_left_offset_edit_after_fichier() {
        let fichier_width = "Fichier".chars().count() as f32 * MENU_FONT_SIZE * 0.6;
        let expected = fichier_width + MENU_H_PADDING * 2.0;
        assert!((menu_left_offset(Menu::Edit) - expected).abs() < 0.01);
    }

    #[test]
    fn menu_left_offset_view_after_three_labels() {
        let mut expected = 0.0;
        for label in ["Fichier", "Edition", "Recherche"] {
            let w = label.chars().count() as f32 * MENU_FONT_SIZE * 0.6;
            expected += w + MENU_H_PADDING * 2.0;
        }
        assert!((menu_left_offset(Menu::View) - expected).abs() < 0.01);
    }

    // ============================
    // menu_popup_size
    // ============================

    #[test]
    fn menu_popup_size_one_item() {
        let (w, h) = menu_popup_size(1);
        assert_eq!(w, MENU_ITEM_WIDTH + MENU_CONTAINER_PADDING * 2.0);
        assert_eq!(h, MENU_ITEM_HEIGHT + MENU_CONTAINER_PADDING * 2.0);
    }

    #[test]
    fn menu_popup_size_four_items() {
        let (w, h) = menu_popup_size(4);
        let expected_h = 4.0 * MENU_ITEM_HEIGHT
            + 3.0 * MENU_ITEM_SPACING
            + MENU_CONTAINER_PADDING * 2.0;
        assert_eq!(w, MENU_ITEM_WIDTH + MENU_CONTAINER_PADDING * 2.0);
        assert!((h - expected_h).abs() < 0.01);
    }

    #[test]
    fn menu_popup_size_width_always_same() {
        let (w1, _) = menu_popup_size(1);
        let (w4, _) = menu_popup_size(4);
        assert_eq!(w1, w4);
    }

    // ============================
    // clamp_popup_position
    // ============================

    #[test]
    fn clamp_within_bounds_unchanged() {
        let (x, y) = clamp_popup_position(10.0, 20.0, 100.0, 50.0, 800.0, 600.0);
        assert_eq!((x, y), (10.0, 20.0));
    }

    #[test]
    fn clamp_overflow_right() {
        let (x, y) = clamp_popup_position(750.0, 20.0, 100.0, 50.0, 800.0, 600.0);
        assert_eq!(x, 700.0);
        assert_eq!(y, 20.0);
    }

    #[test]
    fn clamp_overflow_bottom() {
        let (x, y) = clamp_popup_position(10.0, 580.0, 100.0, 50.0, 800.0, 600.0);
        assert_eq!(x, 10.0);
        assert_eq!(y, 550.0);
    }

    #[test]
    fn clamp_overflow_both() {
        let (x, y) = clamp_popup_position(750.0, 580.0, 100.0, 50.0, 800.0, 600.0);
        assert_eq!(x, 700.0);
        assert_eq!(y, 550.0);
    }

    #[test]
    fn clamp_window_smaller_than_popup() {
        let (x, y) = clamp_popup_position(10.0, 10.0, 200.0, 200.0, 100.0, 100.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
    }
}
