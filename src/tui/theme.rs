//! Colors and the logo. Everything else uses the terminal's own foreground
//! and background, so hush fits whatever theme the terminal already has.

use ratatui::style::{Color, Modifier, Style};

/// 256-color pink (#ff87d7). Used for the logo, focus, and selection only.
pub const ACCENT: Color = Color::Indexed(212);
pub const DIM: Color = Color::DarkGray;
pub const OK: Color = Color::Green;
pub const ERROR: Color = Color::Red;
/// A revealed secret value.
pub const EXPOSED: Color = Color::Yellow;

pub const LOGO: [&str; 3] = [
    "█              █   ",
    "█▀▀▄ █  █ ▄▀▀▀ █▀▀▄",
    "█  █ ▀▄▄▀ ▄▄▄▀ █  █",
];

pub fn accent() -> Style {
    Style::new().fg(ACCENT)
}

pub fn dim() -> Style {
    Style::new().fg(DIM)
}

pub fn bold() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

/// Border for a pane or dialog, pink when it has focus.
pub fn border(focused: bool) -> Style {
    if focused { accent() } else { dim() }
}
