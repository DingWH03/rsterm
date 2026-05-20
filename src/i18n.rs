//! Internationalization (i18n) module for rsTerm.
//!
//! Uses `rust-i18n` for translation loading and `sys-locale` for system locale detection.
//! Supports runtime language switching and persists the choice in settings.
//!
//! The `rust_i18n::i18n!("locales")` macro is invoked in `lib.rs` (the crate root).

use rust_i18n::t;
use serde::{Deserialize, Serialize};

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

/// Detect the system locale and map it to a supported locale code.
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
