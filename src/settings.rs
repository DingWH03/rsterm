use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::{BellStyle, CursorStyle, TerminalTheme, TerminalType};
use crate::i18n::{Language, UiTheme};
use crate::ui::widget::keyboard::KeyboardMode;

/// A terminal profile — a complete set of terminal appearance and behavior settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    // ---- Identity ----
    pub name: String,
    #[serde(default)]
    pub description: String,

    // ---- Display ----
    /// Absolute path to a monospace font file; empty = pick first suitable system mono.
    #[serde(default)]
    pub terminal_font: String,
    pub font_size: f32,
    #[serde(default = "default_line_spacing")]
    pub line_spacing: f32,
    #[serde(default = "default_cell_width_scale")]
    pub cell_width_scale: f32,
    pub theme: TerminalTheme,
    pub cursor_style: CursorStyle,
    #[serde(default = "default_true")]
    pub bold_is_bright: bool,

    // ---- Scrollback ----
    pub scrollback_lines: usize,

    // ---- Terminal Behavior ----
    #[serde(default)]
    pub terminal_type: TerminalType,
    #[serde(default)]
    pub bell: BellStyle,
    #[serde(default = "default_true")]
    pub enable_bracketed_paste: bool,
    #[serde(default = "default_true")]
    pub enable_sgr_mouse: bool,
    #[serde(default = "default_true")]
    pub auto_wrap: bool,
    #[serde(default)]
    pub word_separators: String,

    // ---- Keyboard ----
    pub keyboard_mode: KeyboardMode,

    // ---- Environment ----
    pub env_vars: HashMap<String, String>,
}

fn default_line_spacing() -> f32 { 1.0 }
fn default_cell_width_scale() -> f32 { 1.0 }
fn default_true() -> bool { true }

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            description: String::new(),
            terminal_font: String::new(),
            font_size: 14.0,
            line_spacing: 1.0,
            cell_width_scale: 1.0,
            theme: TerminalTheme::default(),
            cursor_style: CursorStyle::default(),
            bold_is_bright: true,
            scrollback_lines: 5000,
            terminal_type: TerminalType::default(),
            bell: BellStyle::default(),
            enable_bracketed_paste: true,
            enable_sgr_mouse: true,
            auto_wrap: true,
            word_separators: " /\\()\"'-:,.;<>~!@#$%^&*|+=[]{}~?│".to_string(),
            keyboard_mode: KeyboardMode::Full,
            env_vars: HashMap::from([
                ("TERM".to_string(), std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string())),
                ("COLORTERM".to_string(), std::env::var("COLORTERM").unwrap_or_else(|_| "truecolor".to_string())),
                ("LC_ALL".to_string(), std::env::var("LC_ALL").unwrap_or_else(|_| "en_US.UTF-8".to_string())),
            ]),
        }
    }
}

impl Profile {
    /// Create a deep copy with a new name.
    pub fn duplicate(&self, new_name: &str) -> Self {
        let mut copy = self.clone();
        copy.name = new_name.to_string();
        copy
    }

    /// Export this profile as a JSON string (standalone file).
    pub fn export_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Import a profile from a JSON string.
    pub fn import_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub profiles: Vec<Profile>,
    pub default_profile_name: String,
    pub ssh_env_vars: HashMap<String, String>,
    /// Saved local connection profile used for quick「Open Local Terminal」.
    #[serde(default)]
    pub default_local_connection_id: Option<String>,
    /// UI language preference.
    #[serde(default)]
    pub language: Language,
    /// UI theme (light/dark/system).
    #[serde(default)]
    pub ui_theme: UiTheme,
}

impl Default for AppSettings {
    fn default() -> Self {
        let default_profile = Profile::default();
        Self {
            default_profile_name: default_profile.name.clone(),
            profiles: vec![default_profile],
            ssh_env_vars: HashMap::from([
                ("TERM".to_string(), "xterm-256color".to_string()),
                ("LANG".to_string(), "en_US.UTF-8".to_string()),
            ]),
            default_local_connection_id: None,
            language: Language::default(),
            ui_theme: UiTheme::default(),
        }
    }
}

impl AppSettings {
    pub fn default_profile(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.name == self.default_profile_name)
            .unwrap_or(&self.profiles[0])
    }

    pub fn find_profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn font_size(&self) -> f32 {
        self.default_profile().font_size
    }

    pub fn theme(&self) -> &TerminalTheme {
        &self.default_profile().theme
    }

    pub fn cursor_style(&self) -> CursorStyle {
        self.default_profile().cursor_style
    }
}

pub fn load_settings() -> AppSettings {
    let path = match settings_path() {
        Some(p) => p,
        None => return AppSettings::default(),
    };

    if !path.exists() {
        return AppSettings::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save_settings(settings: &AppSettings) {
    let path = match settings_path() {
        Some(p) => {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            p
        }
        None => return,
    };

    if let Ok(data) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(&path, data);
    }
}

fn settings_path() -> Option<std::path::PathBuf> {
    crate::storage::config_dir().map(|p| p.join("settings.json"))
}
