pub struct AppConfig {
    pub font_size: f32,
    pub show_virtual_keyboard: bool,
    pub theme: TerminalTheme,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            show_virtual_keyboard: false,
            theme: TerminalTheme::default(),
        }
    }
}

use serde::{Deserialize, Serialize};

/// Terminal cursor appearance (configurable in settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorStyle {
    /// Thin vertical bar at the left of the cell.
    Bar,
    /// Inverted full cell (classic block cursor).
    Block,
    /// Horizontal line at the bottom of the cell.
    Underline,
}

impl Default for CursorStyle {
    fn default() -> Self {
        Self::Bar
    }
}

impl CursorStyle {
    pub const ALL: [Self; 3] = [Self::Bar, Self::Block, Self::Underline];

    pub fn label(self) -> String {
        match self {
            Self::Bar => rust_i18n::t!("cursor_bar").into_owned(),
            Self::Block => rust_i18n::t!("cursor_block").into_owned(),
            Self::Underline => rust_i18n::t!("cursor_underline").into_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalTheme {
    pub bg: egui::Color32,
    pub fg: egui::Color32,
    pub cursor: egui::Color32,
    pub selection: egui::Color32,
    pub black: egui::Color32,
    pub red: egui::Color32,
    pub green: egui::Color32,
    pub yellow: egui::Color32,
    pub blue: egui::Color32,
    pub magenta: egui::Color32,
    pub cyan: egui::Color32,
    pub white: egui::Color32,
    pub bright_black: egui::Color32,
    pub bright_red: egui::Color32,
    pub bright_green: egui::Color32,
    pub bright_yellow: egui::Color32,
    pub bright_blue: egui::Color32,
    pub bright_magenta: egui::Color32,
    pub bright_cyan: egui::Color32,
    pub bright_white: egui::Color32,
}

impl Default for TerminalTheme {
    fn default() -> Self {
        Self {
            bg: egui::Color32::from_rgb(30, 30, 30),
            fg: egui::Color32::from_rgb(220, 220, 220),
            cursor: egui::Color32::from_rgb(255, 255, 255),
            selection: egui::Color32::from_rgba_premultiplied(100, 100, 255, 128),
            black: egui::Color32::from_rgb(0, 0, 0),
            red: egui::Color32::from_rgb(205, 49, 49),
            green: egui::Color32::from_rgb(13, 188, 121),
            yellow: egui::Color32::from_rgb(229, 229, 16),
            blue: egui::Color32::from_rgb(36, 114, 200),
            magenta: egui::Color32::from_rgb(188, 63, 188),
            cyan: egui::Color32::from_rgb(17, 168, 205),
            white: egui::Color32::from_rgb(220, 220, 220),
            bright_black: egui::Color32::from_rgb(102, 102, 102),
            bright_red: egui::Color32::from_rgb(241, 76, 76),
            bright_green: egui::Color32::from_rgb(35, 209, 139),
            bright_yellow: egui::Color32::from_rgb(245, 245, 67),
            bright_blue: egui::Color32::from_rgb(59, 142, 234),
            bright_magenta: egui::Color32::from_rgb(214, 112, 214),
            bright_cyan: egui::Color32::from_rgb(41, 184, 219),
            bright_white: egui::Color32::from_rgb(255, 255, 255),
        }
    }
}

impl TerminalTheme {
    pub fn ansi_color(&self, idx: u8) -> egui::Color32 {
        match idx {
            0 => self.black,
            1 => self.red,
            2 => self.green,
            3 => self.yellow,
            4 => self.blue,
            5 => self.magenta,
            6 => self.cyan,
            7 => self.white,
            8 => self.bright_black,
            9 => self.bright_red,
            10 => self.bright_green,
            11 => self.bright_yellow,
            12 => self.bright_blue,
            13 => self.bright_magenta,
            14 => self.bright_cyan,
            15 => self.bright_white,
            _ => self.indexed_color(idx),
        }
    }

    /// xterm 256-color palette (16–255). Used by zsh autosuggest / completion grays.
    pub fn indexed_color(&self, idx: u8) -> egui::Color32 {
        match idx {
            0..=15 => self.ansi_color(idx),
            16..=231 => {
                let i = idx - 16;
                let r = (i / 36) % 6;
                let g = (i / 6) % 6;
                let b = i % 6;
                let level = |c: u8| -> u8 {
                    if c == 0 {
                        0
                    } else {
                        55 + (c - 1) * 40
                    }
                };
                egui::Color32::from_rgb(level(r), level(g), level(b))
            }
            232..=255 => {
                let level = 8 + (idx - 232) * 10;
                egui::Color32::from_rgb(level, level, level)
            }
        }
    }
}
