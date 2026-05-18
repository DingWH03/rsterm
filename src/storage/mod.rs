pub mod types;

use std::path::PathBuf;

use directories::ProjectDirs;
use log::info;
use types::SavedConnection;

fn storage_path() -> Option<PathBuf> {
    ProjectDirs::from("io", "rsterm", "rsTerm")
        .map(|d| d.config_dir().to_path_buf())
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
