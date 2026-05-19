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
    directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn normalize_local_path(path: &Path) -> PathBuf {
    let p = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    p
}
