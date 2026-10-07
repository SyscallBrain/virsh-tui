//! Modes: NORMAL / INSERT / VISUAL / COMMAND / PALETTE / PICKER / CONFIRM.

use ratatui::style::{Color, Style};

use crate::theme::Theme;

/// App mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    Visual,
    Command,
    Palette,
    Picker,
    Confirm,
}

impl Mode {
    /// Uppercase pill label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Visual => "VISUAL",
            Self::Command => "COMMAND",
            Self::Palette => "PALETTE",
            Self::Picker => "PICKER",
            Self::Confirm => "CONFIRM",
        }
    }

    /// Pill background colour token.
    pub fn color(self, theme: &Theme) -> Color {
        let t = &theme.tokens;
        match self {
            Self::Normal => t.blue,
            Self::Insert => t.green,
            Self::Visual => t.magenta,
            Self::Command => t.yellow,
            Self::Palette | Self::Picker => t.magenta,
            Self::Confirm => t.red,
        }
    }

    /// Pill style (bold, bg2 text).
    pub fn pill(self, theme: &Theme) -> Style {
        Style::default()
            .fg(theme.tokens.bg2)
            .bg(self.color(theme))
            .add_modifier(ratatui::style::Modifier::BOLD)
    }
}
