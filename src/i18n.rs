//! Internationalization (i18n) module for rsTerm.
//!
//! Uses `rust-i18n` for translation loading and `sys-locale` for system locale detection.
//! Supports runtime language switching and persists the choice in settings.
//!
//! The `rust_i18n::i18n!("locales")` macro is invoked in `lib.rs` (the crate root).

use rust_i18n::t;
use serde::{Deserialize, Serialize};

// ─── Language ─────────────────────────────────────────────────────────────────

/// Supported languages for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// Follow the system locale (auto-detect).
    System,
    /// Simplified Chinese.
    ZhCN,
    /// English.
    En,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::System, Self::ZhCN, Self::En];

    /// Human-readable label for the language selector.
    pub fn label(self) -> String {
        match self {
            Self::System => t!("language_system").into_owned(),
            Self::ZhCN => t!("language_zh").into_owned(),
            Self::En => t!("language_en").into_owned(),
        }
    }

    /// The locale code used by `rust-i18n`.
    fn locale_code(self) -> &'static str {
        match self {
            Self::System => detect_system_locale(),
            Self::ZhCN => "zh-CN",
            Self::En => "en",
        }
    }

    /// Apply this language setting, making all subsequent `t!()` calls use it.
    pub fn apply(self) {
        let code = self.locale_code();
        rust_i18n::set_locale(code);
    }
}

impl Default for Language {
    fn default() -> Self {
        Self::System
    }
}

// ─── UI Theme ─────────────────────────────────────────────────────────────────

/// UI appearance theme (separate from terminal themes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiTheme {
    System,
    Light,
    Dark,
}

impl UiTheme {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn label(self) -> String {
        match self {
            Self::System => t!("ui_theme_system").into_owned(),
            Self::Light => t!("ui_theme_light").into_owned(),
            Self::Dark => t!("ui_theme_dark").into_owned(),
        }
    }

    /// Apply this theme to the egui context.
    pub fn apply(self, ctx: &egui::Context) {
        let theme = match self {
            Self::System => {
                let dark = std::env::var("COLORFGBG")
                    .ok()
                    .and_then(|v| v.split(';').last().map(|s| s.trim() == "0"))
                    .unwrap_or(false)
                    || std::env::var("GTK_THEME")
                        .ok()
                        .map(|t| t.contains("dark") || t.contains("Dark"))
                        .unwrap_or(false);
                if dark { egui::Visuals::dark() } else { egui::Visuals::light() }
            }
            Self::Light => egui::Visuals::light(),
            Self::Dark => egui::Visuals::dark(),
        };
        ctx.set_visuals(theme);
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self::System
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn detect_system_locale() -> &'static str {
    let locale = sys_locale::get_locale().unwrap_or_else(|| String::from("en"));
    if locale.starts_with("zh") {
        "zh-CN"
    } else {
        "en"
    }
}

/// Convenience wrapper: translate a key, returning the translated string.
/// This is equivalent to `rust_i18n::t!(key)` but can be used as a function.
#[macro_export]
macro_rules! tr {
    ($key:tt) => {
        rust_i18n::t!($key)
    };
    ($key:tt, $($arg:tt)*) => {
        rust_i18n::t!($key, $($arg)*)
    };
}
