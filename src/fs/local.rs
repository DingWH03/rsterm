use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::transfer_progress::ByteProgress;
use super::FileEntry;

pub fn list_dir(path: &Path) -> Result<Vec<FileEntry>, String> {
    let mut entries = Vec::new();
    let read = fs::read_dir(path).map_err(|e| format!("Failed to read directory: {e}"))?;
    for item in read {
        let item = item.map_err(|e| e.to_string())?;
        let meta = item.metadata().map_err(|e| e.to_string())?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name == "." || name == ".." {
            continue;
        }
        entries.push(FileEntry {
            name,
            is_dir: meta.is_dir(),
            size: meta.len(),
            modified: meta.modified().ok(),
        });
    }
    entries.sort_by_key(|e| e.sort_key());
    Ok(entries)
}

pub fn copy_file(src: &Path, dst: &Path) -> Result<(), String> {
    copy_file_with_progress(src, dst, None, "")
}

pub fn copy_file_with_progress(
    src: &Path,
    dst: &Path,
    progress: Option<&Arc<ByteProgress>>,
    label: &str,
) -> Result<(), String> {
    if let Some(p) = progress {
        if p.is_cancelled() {
            return Err("Transfer stopped".into());
        }
    }
    if src.is_dir() {
        copy_dir_recursive_progress(src, dst, progress)?;
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        copy_file_bytes_progress(src, dst, progress, label)?;
    }
    Ok(())
}

fn copy_file_bytes_progress(
    src: &Path,
    dst: &Path,
    progress: Option<&Arc<ByteProgress>>,
    label: &str,
) -> Result<(), String> {
    if let Some(p) = progress {
        p.set_label(label);
    }
    let mut src_f = File::open(src).map_err(|e| e.to_string())?;
    let mut dst_f = File::create(dst).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        if progress.is_some_and(|p| p.is_cancelled()) {
            return Err("Transfer stopped".into());
        }
        let n = src_f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        dst_f
            .write_all(&buf[..n])
            .map_err(|e| e.to_string())?;
        if let Some(p) = progress {
            p.add_bytes(n as u64, label);
        }
    }
    Ok(())
}

fn copy_dir_recursive_progress(
    src: &Path,
    dst: &Path,
    progress: Option<&Arc<ByteProgress>>,
) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for item in fs::read_dir(src).map_err(|e| e.to_string())? {
        if progress.is_some_and(|p| p.is_cancelled()) {
            return Err("Transfer stopped".into());
        }
        let item = item.map_err(|e| e.to_string())?;
        let name = item.file_name();
        let from = src.join(&name);
        let to = dst.join(&name);
        let label = format!("Copying {}", name.to_string_lossy());
        if item.metadata().map_err(|e| e.to_string())?.is_dir() {
            copy_dir_recursive_progress(&from, &to, progress)?;
        } else {
            copy_file_bytes_progress(&from, &to, progress, &label)?;
        }
    }
    Ok(())
}

pub fn move_path(src: &Path, dst: &Path) -> Result<(), String> {
    move_path_with_progress(src, dst, None, "")
}

pub fn move_path_with_progress(
    src: &Path,
    dst: &Path,
    progress: Option<&Arc<ByteProgress>>,
    label: &str,
) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_file_with_progress(src, dst, progress, label)?;
            remove_path(src)
        }
    }
}

pub fn remove_path(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())
    }
}

pub fn mkdir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

pub fn join_path(base: &Path, name: &str) -> PathBuf {
    base.join(name)
}

pub fn rename_entry(dir: &Path, old_name: &str, new_name: &str) -> Result<(), String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    if new_name.contains('/') || new_name.contains('\\') || new_name == "." || new_name == ".." {
        return Err("Invalid name".into());
    }
    if old_name == new_name {
        return Ok(());
    }
    let src = dir.join(old_name);
    let dst = dir.join(new_name);
    if !src.exists() {
        return Err("Source not found".into());
    }
    if dst.exists() {
        return Err("A file with that name already exists".into());
    }
    move_path(&src, &dst)
}
