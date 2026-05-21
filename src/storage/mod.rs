pub mod types;

use std::path::PathBuf;

use directories::ProjectDirs;
use log::info;
use types::SavedConnection;

// ---------------------------------------------------------------------------
// Android config directory override
// ---------------------------------------------------------------------------
/// On Android, `ProjectDirs` may fail because environment vars like `$HOME`
/// are not set in a `NativeActivity`.  We store the path from
/// `AndroidApp::internal_data_path()` here at startup.
#[cfg(target_os = "android")]
static ANDROID_BASE_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Initialise the config directory from a platform-provided path
/// (called from `android_main()`).
#[cfg(target_os = "android")]
pub fn init_android_base_dir(path: PathBuf) {
    let _ = ANDROID_BASE_DIR.set(path);
}

/// Resolve the application config directory.
///
/// * **Desktop** – uses [`directories::ProjectDirs`] (XDG on Linux,
///   `~/Library/Application Support` on macOS, `AppData` on Windows).
/// * **Android** – uses [`AndroidApp::internal_data_path`] which resolves to
///   `/data/data/<package>/files/config/`.
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        if let Some(dir) = ANDROID_BASE_DIR.get() {
            return Some(dir.join("config"));
        }
    }
    ProjectDirs::from("io", "rsterm", "rsTerm")
        .map(|d| d.config_dir().to_path_buf())
}

fn storage_path() -> Option<PathBuf> {
    config_dir()
}

pub fn load_connections() -> Vec<SavedConnection> {
    let path = match storage_path() {
        Some(p) => p.join("connections.json"),
        None => return Vec::new(),
    };

    if !path.exists() {
        return Vec::new();
    }

    match std::fs::read_to_string(&path) {
        Ok(data) => {
            let conns: Vec<SavedConnection> = match serde_json::from_str(&data) {
                Ok(c) => c,
                Err(e) => {
                    info!("Failed to parse connections: {e}");
                    return Vec::new();
                }
            };
            info!("Loaded {} saved connections", conns.len());
            conns
        },
        Err(e) => {
            info!("Failed to read connections file: {e}");
            Vec::new()
        }
    }
}

pub fn save_connections(connections: &[SavedConnection]) {
    let path = match storage_path() {
        Some(p) => {
            std::fs::create_dir_all(&p).ok();
            p.join("connections.json")
        }
        None => return,
    };

    let data = match serde_json::to_string_pretty(connections) {
        Ok(d) => d,
        Err(e) => {
            info!("Failed to serialize connections: {e}");
            return;
        }
    };

    if let Err(e) = std::fs::write(&path, data) {
        info!("Failed to write connections: {e}");
    }
}
