#![windows_subsystem = "windows"]

mod app;
mod preferences;
mod ui;
mod update;

use app::Notepad;
use preferences::UserPreferences;

pub const DEFAULT_WINDOW_WIDTH: f32 = 800.0;
pub const DEFAULT_WINDOW_HEIGHT: f32 = 600.0;

pub const DEFAULT_FONT_SIZE: f32 = 14.0;
pub const MIN_FONT_SIZE: f32 = 8.0;
pub const MAX_FONT_SIZE: f32 = 40.0;
pub const ZOOM_STEP: f32 = 2.0;

fn main() -> iced::Result {
    let prefs = UserPreferences::load();
    iced::application(Notepad::new, Notepad::update, Notepad::view)
        .title(Notepad::title)
        .theme(Notepad::theme)
        .subscription(Notepad::subscription)
        .window_size(iced::Size::new(prefs.window_width, prefs.window_height))
        .exit_on_close_request(false)
        .run()
}
