use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::fs::entry_info;
use crate::fs::local;
use crate::fs::sftp::{join_remote, SftpClient};
use crate::fs::transfer_progress::ByteProgress;
use crate::session::{FileClipboard, FileClipboardMode, FileTransferState, TransferSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteTarget {
    LocalRight,
    LocalLeft,
    Remote,
}

impl FileTransferState {
    pub fn is_active(&self) -> bool {
        self.snapshot
            .lock()
            .map(|s| s.active)
            .unwrap_or(false)
    }

    pub fn read_ui(&self) -> TransferSnapshot {
        self.snapshot
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn start_paste(
        &mut self,
        target: PasteTarget,
        clip: FileClipboard,
        dest_local: Option<PathBuf>,
        remote_cwd: Option<String>,
        remote_client: Option<Arc<SftpClient>>,
    ) {
        if self.is_active() {
            return;
        }
        self.cancel.store(false, Ordering::Relaxed);
        {
            let mut snap = self.snapshot.lock().expect("transfer snapshot");
            *snap = TransferSnapshot {
                active: true,
                progress: 0.0,
                label: "Calculating size…".into(),
                ..Default::default()
            };
        }

        let cancel = Arc::clone(&self.cancel);
        let snapshot = Arc::clone(&self.snapshot);
        let handle = thread::spawn(move || {
            run_paste_job(
                cancel,
                snapshot,
                target,
                clip,
                dest_local,
                remote_cwd,
                remote_client,
            );
        });
        self.join = Some(handle);
    }

    pub fn poll(&mut self, ctx: &egui::Context) -> Option<TransferDone> {
        let finished = self
            .snapshot
            .lock()
            .map(|s| s.finished)
            .unwrap_or(false);
        if !finished {
            if self.is_active() {
                ctx.request_repaint();
            }
            return None;
        }

        let done = {
            let mut snap = self.snapshot.lock().expect("transfer snapshot");
            let result = TransferDone {
                status: snap.status_message.take(),
                clear_clipboard: snap.clear_clipboard,
                refresh_remote: snap.refresh_remote,
                refresh_local_right: snap.refresh_local_right,
                refresh_local_left: snap.refresh_local_left,
            };
            *snap = TransferSnapshot::default();
            result
        };

        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }

        Some(done)
    }
}

#[derive(Default)]
pub struct TransferDone {
    pub status: Option<String>,
    pub clear_clipboard: bool,
    pub refresh_remote: bool,
    pub refresh_local_right: bool,
    pub refresh_local_left: bool,
}

fn run_paste_job(
    cancel: Arc<AtomicBool>,
    snapshot: Arc<Mutex<TransferSnapshot>>,
    target: PasteTarget,
    clip: FileClipboard,
    dest_local: Option<PathBuf>,
    remote_cwd: Option<String>,
    remote_client: Option<Arc<SftpClient>>,
) {
    if cancelled(&cancel) {
        finish_cancelled(&snapshot, vec![]);
        return;
    }

    let total_bytes = match compute_total_bytes(&clip, remote_client.as_ref()) {
        Ok(n) => n.max(1),
        Err(e) => {
            finish_error(&snapshot, e);
            return;
        }
    };

    let progress = ByteProgress::new(cancel.clone(), snapshot.clone(), total_bytes);
    let mut errors = Vec::new();

    match target {
        PasteTarget::LocalRight | PasteTarget::LocalLeft => {
            let Some(dest_dir) = dest_local else {
                finish_error(&snapshot, "No destination folder".into());
                return;
            };
            if clip.from_remote {
                let Some(client) = remote_client else {
                    finish_error(&snapshot, "No remote connection".into());
                    return;
                };
                for remote_path in &clip.paths {
                    if cancelled(&cancel) {
                        finish_cancelled(&snapshot, errors);
                        return;
                    }
                    let name = file_name_from_path(remote_path);
                    let label = format!("Downloading {name}");
                    let local_path = dest_dir.join(&name);
                    if let Err(e) =
                        client.download_with_progress(remote_path, &local_path, Some(progress.clone()), &label)
                    {
                        errors.push(e);
                    }
                }
                if clip.mode == FileClipboardMode::Cut && !cancelled(&cancel) {
                    for path in &clip.paths {
                        if client.remove(path, false).is_err() {
                            let _ = client.remove(path, true);
                        }
                    }
                }
            } else {
                for src_str in &clip.paths {
                    if cancelled(&cancel) {
                        finish_cancelled(&snapshot, errors);
                        return;
                    }
                    let src = PathBuf::from(src_str);
                    let name = src
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("untitled");
                    let label = format!("Copying {name}");
                    let dst = dest_dir.join(name);
                    let r = match clip.mode {
                        FileClipboardMode::Copy => {
                            local::copy_file_with_progress(&src, &dst, Some(&progress), &label)
                        }
                        FileClipboardMode::Cut => {
                            local::move_path_with_progress(&src, &dst, Some(&progress), &label)
                        }
                    };
                    if let Err(e) = r {
                        errors.push(e);
                    }
                }
            }
            finish_ok(
                &snapshot,
                errors,
                true,
                matches!(target, PasteTarget::LocalRight),
                matches!(target, PasteTarget::LocalLeft),
                false,
            );
        }
        PasteTarget::Remote => {
            let Some(cwd) = remote_cwd else {
                finish_error(&snapshot, "No remote folder".into());
                return;
            };
            let Some(client) = remote_client else {
                finish_error(&snapshot, "No remote connection".into());
                return;
            };
            if clip.from_remote {
                for from in &clip.paths {
                    if cancelled(&cancel) {
                        finish_cancelled(&snapshot, errors);
                        return;
                    }
                    let name = file_name_from_path(from);
                    let label = format!("Moving {name}");
                    let to = join_remote(&cwd, &name);
                    let r = match clip.mode {
                        FileClipboardMode::Cut => client.rename(from, &to),
                        FileClipboardMode::Copy => {
                            let tmp = std::env::temp_dir().join(&name);
                            client
                                .download_with_progress(from, &tmp, Some(progress.clone()), &label)
                                .and_then(|_| {
                                    client.upload_with_progress(&tmp, &to, Some(progress.clone()), &label)
                                })
                                .map(|_| {
                                    if tmp.is_file() {
                                        let _ = std::fs::remove_file(&tmp);
                                    }
                                })
                        }
                    };
                    if let Err(e) = r {
                        errors.push(e);
                    }
                }
            } else {
                for src_str in &clip.paths {
                    if cancelled(&cancel) {
                        finish_cancelled(&snapshot, errors);
                        return;
                    }
                    let src = PathBuf::from(src_str);
                    let name = src
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("untitled");
                    let label = format!("Uploading {name}");
                    let remote_path = join_remote(&cwd, name);
                    if let Err(e) =
                        client.upload_with_progress(&src, &remote_path, Some(progress.clone()), &label)
                    {
                        errors.push(e);
                    } else if clip.mode == FileClipboardMode::Cut {
                        let _ = local::remove_path(&src);
                    }
                }
            }
            finish_ok(&snapshot, errors, true, false, false, true);
        }
    }
}

fn compute_total_bytes(
    clip: &FileClipboard,
    remote_client: Option<&Arc<SftpClient>>,
) -> Result<u64, String> {
    if clip.from_remote {
        let client = remote_client.ok_or("No remote connection")?;
        let mut total = 0u64;
        for path in &clip.paths {
            total += client.path_bytes(path)?;
        }
        Ok(total)
    } else {
        Ok(entry_info::local_paths_total_bytes(&clip.paths))
    }
}

fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "untitled".into())
}

fn cancelled(cancel: &Arc<AtomicBool>) -> bool {
    cancel.load(Ordering::Relaxed)
}

fn finish_ok(
    snapshot: &Arc<Mutex<TransferSnapshot>>,
    errors: Vec<String>,
    clear_clipboard: bool,
    refresh_local_right: bool,
    refresh_local_left: bool,
    refresh_remote: bool,
) {
    let (msg, clear) = if errors.is_empty() {
        ("Transfer complete".into(), clear_clipboard)
    } else {
        (errors.join("; "), false)
    };
    if let Ok(mut s) = snapshot.lock() {
        s.active = false;
        s.progress = 1.0;
        s.finished = true;
        s.status_message = Some(msg);
        s.clear_clipboard = clear;
        s.refresh_local_right = refresh_local_right;
        s.refresh_local_left = refresh_local_left;
        s.refresh_remote = refresh_remote;
    }
}

fn finish_cancelled(snapshot: &Arc<Mutex<TransferSnapshot>>, errors: Vec<String>) {
    let mut msg = "Transfer stopped".to_string();
    if !errors.is_empty() {
        msg = format!("{msg}: {}", errors.join("; "));
    }
    if let Ok(mut s) = snapshot.lock() {
        s.active = false;
        s.finished = true;
        s.status_message = Some(msg);
    }
}

fn finish_error(snapshot: &Arc<Mutex<TransferSnapshot>>, msg: String) {
    if let Ok(mut s) = snapshot.lock() {
        s.active = false;
        s.finished = true;
        s.status_message = Some(msg);
    }
}

pub fn apply_transfer_done(session: &mut crate::session::FileManagerSession, done: TransferDone) {
    if let Some(msg) = done.status {
        session.status = Some(msg);
    }
    if done.clear_clipboard {
        session.clipboard = None;
    }
    if done.refresh_remote {
        if let Some(remote) = session.remote.as_mut() {
            remote.loading = true;
        }
    }
    if done.refresh_local_right {
        session.right.loading = true;
    }
    if done.refresh_local_left {
        if let Some(left) = session.left_local.as_mut() {
            left.loading = true;
        }
    }
}
