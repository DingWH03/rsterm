pub mod entry_info;
pub mod local;
pub mod sftp;
pub mod transfer_progress;

use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

impl FileEntry {
    pub fn sort_key(&self) -> (u8, String) {
        (if self.is_dir { 0 } else { 1 }, self.name.to_lowercase())
    }
}

pub fn home_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        return android_external_home();
    }
    #[cfg(not(target_os = "android"))]
    {
        directories::UserDirs::new()
            .map(|u| u.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"))
    }
}

/// Primary shared storage root on Android (needs storage / all-files permission).
#[cfg(target_os = "android")]
fn android_external_home() -> PathBuf {
    for candidate in [
        "/storage/emulated/0",
        "/sdcard",
        "/storage/self/primary",
    ] {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return p;
        }
    }
    PathBuf::from("/storage/emulated/0")
}

pub fn normalize_local_path(path: &Path) -> PathBuf {
    let p = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    p
}
