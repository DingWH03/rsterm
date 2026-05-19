use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::{CursorStyle, TerminalTheme};
use crate::ui::keyboard::KeyboardMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub font_size: f32,
    pub theme: TerminalTheme,
    pub env_vars: HashMap<String, String>,
    pub scrollback_lines: usize,
    pub keyboard_mode: KeyboardMode,
    pub cursor_style: CursorStyle,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            font_size: 14.0,
            theme: TerminalTheme::default(),
            scrollback_lines: 5000,
            keyboard_mode: KeyboardMode::Full,
            cursor_style: CursorStyle::Bar,
            env_vars: HashMap::from([
                ("TERM".to_string(), std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string())),
                ("COLORTERM".to_string(), std::env::var("COLORTERM").unwrap_or_else(|_| "truecolor".to_string())),
                ("LC_ALL".to_string(), std::env::var("LC_ALL").unwrap_or_else(|_| "en_US.UTF-8".to_string())),
            ]),
        }
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
    directories::ProjectDirs::from("io", "rsterm", "rsTerm")
        .map(|d| d.config_dir().join("settings.json"))
}
