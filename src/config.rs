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
    /// Blinking vertical bar.
    BarBlink,
    /// Blinking block cursor.
    BlockBlink,
    /// Blinking underline.
    UnderlineBlink,
}

impl Default for CursorStyle {
    fn default() -> Self {
        Self::Bar
    }
}

impl CursorStyle {
    pub const ALL: [Self; 6] = [
        Self::Bar, Self::Block, Self::Underline,
        Self::BarBlink, Self::BlockBlink, Self::UnderlineBlink,
    ];

    pub fn label(self) -> String {
        match self {
            Self::Bar => rust_i18n::t!("cursor_bar").into_owned(),
            Self::Block => rust_i18n::t!("cursor_block").into_owned(),
            Self::Underline => rust_i18n::t!("cursor_underline").into_owned(),
            Self::BarBlink => rust_i18n::t!("cursor_bar_blink").into_owned(),
            Self::BlockBlink => rust_i18n::t!("cursor_block_blink").into_owned(),
            Self::UnderlineBlink => rust_i18n::t!("cursor_underline_blink").into_owned(),
        }
    }
}

/// Terminal bell / alert behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BellStyle {
    /// No bell.
    Off,
    /// Visual flash only.
    Visual,
    /// System beep only.
    Audible,
    /// Both flash and beep.
    Both,
}

impl Default for BellStyle {
    fn default() -> Self {
        Self::Visual
    }
}

impl BellStyle {
    pub const ALL: [Self; 4] = [Self::Off, Self::Visual, Self::Audible, Self::Both];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Visual => "Visual",
            Self::Audible => "Audible",
            Self::Both => "Visual + Audible",
        }
    }
}

/// Terminal type reported via $TERM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalType {
    Xterm256,
    Xterm,
    Screen256,
    Screen,
    Tmux256,
    Tmux,
}

impl Default for TerminalType {
    fn default() -> Self {
        Self::Xterm256
    }
}

impl TerminalType {
    pub const ALL: [Self; 6] = [
        Self::Xterm256, Self::Xterm,
        Self::Screen256, Self::Screen,
        Self::Tmux256, Self::Tmux,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Xterm256 => "xterm-256color",
            Self::Xterm => "xterm",
            Self::Screen256 => "screen-256color",
            Self::Screen => "screen",
            Self::Tmux256 => "tmux-256color",
            Self::Tmux => "tmux",
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

    // ---- Built-in theme presets ----

    pub fn dracula() -> Self {
        Self {
            bg: egui::Color32::from_rgb(40, 42, 54),
            fg: egui::Color32::from_rgb(248, 248, 242),
            cursor: egui::Color32::from_rgb(248, 248, 242),
            selection: egui::Color32::from_rgba_premultiplied(68, 71, 90, 160),
            black: egui::Color32::from_rgb(33, 34, 44),
            red: egui::Color32::from_rgb(255, 85, 85),
            green: egui::Color32::from_rgb(80, 250, 123),
            yellow: egui::Color32::from_rgb(241, 250, 140),
            blue: egui::Color32::from_rgb(98, 114, 254),
            magenta: egui::Color32::from_rgb(255, 121, 198),
            cyan: egui::Color32::from_rgb(139, 233, 253),
            white: egui::Color32::from_rgb(248, 248, 242),
            bright_black: egui::Color32::from_rgb(98, 114, 164),
            bright_red: egui::Color32::from_rgb(255, 110, 110),
            bright_green: egui::Color32::from_rgb(105, 255, 140),
            bright_yellow: egui::Color32::from_rgb(255, 255, 170),
            bright_blue: egui::Color32::from_rgb(130, 150, 255),
            bright_magenta: egui::Color32::from_rgb(255, 140, 210),
            bright_cyan: egui::Color32::from_rgb(160, 245, 255),
            bright_white: egui::Color32::from_rgb(255, 255, 255),
        }
    }

    pub fn solarized_dark() -> Self {
        Self {
            bg: egui::Color32::from_rgb(0, 43, 54),
            fg: egui::Color32::from_rgb(131, 148, 150),
            cursor: egui::Color32::from_rgb(131, 148, 150),
            selection: egui::Color32::from_rgba_premultiplied(7, 54, 66, 160),
            black: egui::Color32::from_rgb(7, 54, 66),
            red: egui::Color32::from_rgb(220, 50, 47),
            green: egui::Color32::from_rgb(133, 153, 0),
            yellow: egui::Color32::from_rgb(181, 137, 0),
            blue: egui::Color32::from_rgb(38, 139, 210),
            magenta: egui::Color32::from_rgb(211, 54, 130),
            cyan: egui::Color32::from_rgb(42, 161, 152),
            white: egui::Color32::from_rgb(238, 232, 213),
            bright_black: egui::Color32::from_rgb(0, 43, 54),
            bright_red: egui::Color32::from_rgb(203, 75, 22),
            bright_green: egui::Color32::from_rgb(88, 110, 117),
            bright_yellow: egui::Color32::from_rgb(101, 123, 131),
            bright_blue: egui::Color32::from_rgb(131, 148, 150),
            bright_magenta: egui::Color32::from_rgb(108, 113, 196),
            bright_cyan: egui::Color32::from_rgb(147, 161, 161),
            bright_white: egui::Color32::from_rgb(253, 246, 227),
        }
    }

    pub fn monokai() -> Self {
        Self {
            bg: egui::Color32::from_rgb(39, 40, 34),
            fg: egui::Color32::from_rgb(248, 248, 242),
            cursor: egui::Color32::from_rgb(248, 248, 240),
            selection: egui::Color32::from_rgba_premultiplied(73, 72, 62, 160),
            black: egui::Color32::from_rgb(39, 40, 34),
            red: egui::Color32::from_rgb(249, 38, 114),
            green: egui::Color32::from_rgb(166, 226, 46),
            yellow: egui::Color32::from_rgb(230, 219, 116),
            blue: egui::Color32::from_rgb(102, 217, 239),
            magenta: egui::Color32::from_rgb(174, 129, 255),
            cyan: egui::Color32::from_rgb(161, 239, 228),
            white: egui::Color32::from_rgb(248, 248, 242),
            bright_black: egui::Color32::from_rgb(117, 113, 94),
            bright_red: egui::Color32::from_rgb(249, 38, 114),
            bright_green: egui::Color32::from_rgb(166, 226, 46),
            bright_yellow: egui::Color32::from_rgb(230, 219, 116),
            bright_blue: egui::Color32::from_rgb(102, 217, 239),
            bright_magenta: egui::Color32::from_rgb(174, 129, 255),
            bright_cyan: egui::Color32::from_rgb(161, 239, 228),
            bright_white: egui::Color32::from_rgb(249, 248, 245),
        }
    }

    pub fn nord() -> Self {
        Self {
            bg: egui::Color32::from_rgb(46, 52, 64),
            fg: egui::Color32::from_rgb(216, 222, 233),
            cursor: egui::Color32::from_rgb(216, 222, 233),
            selection: egui::Color32::from_rgba_premultiplied(67, 76, 94, 160),
            black: egui::Color32::from_rgb(59, 66, 82),
            red: egui::Color32::from_rgb(191, 97, 106),
            green: egui::Color32::from_rgb(163, 190, 140),
            yellow: egui::Color32::from_rgb(235, 203, 139),
            blue: egui::Color32::from_rgb(129, 161, 193),
            magenta: egui::Color32::from_rgb(180, 142, 173),
            cyan: egui::Color32::from_rgb(136, 192, 208),
            white: egui::Color32::from_rgb(229, 233, 240),
            bright_black: egui::Color32::from_rgb(76, 86, 106),
            bright_red: egui::Color32::from_rgb(191, 97, 106),
            bright_green: egui::Color32::from_rgb(163, 190, 140),
            bright_yellow: egui::Color32::from_rgb(235, 203, 139),
            bright_blue: egui::Color32::from_rgb(129, 161, 193),
            bright_magenta: egui::Color32::from_rgb(180, 142, 173),
            bright_cyan: egui::Color32::from_rgb(136, 192, 208),
            bright_white: egui::Color32::from_rgb(236, 239, 244),
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            bg: egui::Color32::from_rgb(26, 27, 38),
            fg: egui::Color32::from_rgb(169, 177, 214),
            cursor: egui::Color32::from_rgb(169, 177, 214),
            selection: egui::Color32::from_rgba_premultiplied(54, 57, 79, 160),
            black: egui::Color32::from_rgb(26, 27, 38),
            red: egui::Color32::from_rgb(247, 118, 142),
            green: egui::Color32::from_rgb(158, 206, 106),
            yellow: egui::Color32::from_rgb(224, 175, 104),
            blue: egui::Color32::from_rgb(122, 162, 247),
            magenta: egui::Color32::from_rgb(187, 154, 247),
            cyan: egui::Color32::from_rgb(42, 195, 222),
            white: egui::Color32::from_rgb(169, 177, 214),
            bright_black: egui::Color32::from_rgb(65, 72, 104),
            bright_red: egui::Color32::from_rgb(247, 118, 142),
            bright_green: egui::Color32::from_rgb(158, 206, 106),
            bright_yellow: egui::Color32::from_rgb(224, 175, 104),
            bright_blue: egui::Color32::from_rgb(122, 162, 247),
            bright_magenta: egui::Color32::from_rgb(187, 154, 247),
            bright_cyan: egui::Color32::from_rgb(42, 195, 222),
            bright_white: egui::Color32::from_rgb(197, 202, 229),
        }
    }

    pub fn gruvbox_dark() -> Self {
        Self {
            bg: egui::Color32::from_rgb(40, 40, 40),
            fg: egui::Color32::from_rgb(235, 219, 178),
            cursor: egui::Color32::from_rgb(235, 219, 178),
            selection: egui::Color32::from_rgba_premultiplied(60, 56, 54, 160),
            black: egui::Color32::from_rgb(40, 40, 40),
            red: egui::Color32::from_rgb(204, 36, 29),
            green: egui::Color32::from_rgb(152, 151, 26),
            yellow: egui::Color32::from_rgb(215, 153, 33),
            blue: egui::Color32::from_rgb(69, 133, 136),
            magenta: egui::Color32::from_rgb(177, 98, 134),
            cyan: egui::Color32::from_rgb(104, 157, 106),
            white: egui::Color32::from_rgb(168, 153, 132),
            bright_black: egui::Color32::from_rgb(146, 131, 116),
            bright_red: egui::Color32::from_rgb(251, 73, 52),
            bright_green: egui::Color32::from_rgb(184, 187, 38),
            bright_yellow: egui::Color32::from_rgb(250, 189, 47),
            bright_blue: egui::Color32::from_rgb(131, 165, 152),
            bright_magenta: egui::Color32::from_rgb(211, 134, 155),
            bright_cyan: egui::Color32::from_rgb(142, 192, 124),
            bright_white: egui::Color32::from_rgb(235, 219, 178),
        }
    }

    /// List of all built-in presets with their names.
    pub fn presets() -> [(&'static str, fn() -> Self); 7] {
        [
            ("Default", Self::default),
            ("Dracula", Self::dracula),
            ("Solarized Dark", Self::solarized_dark),
            ("Monokai", Self::monokai),
            ("Nord", Self::nord),
            ("Tokyo Night", Self::tokyo_night),
            ("Gruvbox Dark", Self::gruvbox_dark),
        ]
    }
}
