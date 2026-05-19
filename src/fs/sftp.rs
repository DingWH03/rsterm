use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, UNIX_EPOCH};

use russh::client::{self, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};
use russh_sftp::client::SftpSession;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::fs::entry_info::{self, EntryInfo};
use crate::fs::transfer_progress::ByteProgress;
use crate::fs::FileEntry;
use crate::storage::types::SavedConnection;

pub struct SftpClient {
    req_tx: mpsc::Sender<SftpRequest>,
    _thread: JoinHandle<()>,
}

enum SftpRequest {
    List {
        path: String,
        reply: mpsc::SyncSender<Result<Vec<FileEntry>, String>>,
    },
    Upload {
        local: PathBuf,
        remote: String,
        progress: Option<Arc<ByteProgress>>,
        label: String,
        reply: mpsc::SyncSender<Result<(), String>>,
    },
    Download {
        remote: String,
        local: PathBuf,
        progress: Option<Arc<ByteProgress>>,
        label: String,
        reply: mpsc::SyncSender<Result<(), String>>,
    },
    Stat {
        path: String,
        reply: mpsc::SyncSender<Result<EntryInfo, String>>,
    },
    PathBytes {
        path: String,
        reply: mpsc::SyncSender<Result<u64, String>>,
    },
    Remove {
        path: String,
        is_dir: bool,
        reply: mpsc::SyncSender<Result<(), String>>,
    },
    Mkdir {
        path: String,
        reply: mpsc::SyncSender<Result<(), String>>,
    },
    Rename {
        from: String,
        to: String,
        reply: mpsc::SyncSender<Result<(), String>>,
    },
    Home {
        reply: mpsc::SyncSender<Result<String, String>>,
    },
    Shutdown,
}

struct SftpSshClient;

impl client::Handler for SftpSshClient {
    type Error = russh::Error;

    fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        async { Ok(true) }
    }
}

impl SftpClient {
    pub fn connect(conn: &SavedConnection) -> Result<Self, String> {
        let host = conn
            .ssh_host
            .clone()
            .ok_or_else(|| "SSH host not configured".to_string())?;
        let port = conn.ssh_port.unwrap_or(22);
        let user = conn
            .ssh_user
            .clone()
            .ok_or_else(|| "SSH user not configured".to_string())?;
        let password = conn.ssh_password.clone();

        let (req_tx, req_rx) = mpsc::channel();
        let thread = thread::spawn(move || {
            if let Err(e) = sftp_worker(&host, port, &user, password, req_rx) {
                log::error!("SFTP worker ended: {e}");
            }
        });

        Ok(Self {
            req_tx,
            _thread: thread,
        })
    }

    fn call<T>(&self, build: impl FnOnce(mpsc::SyncSender<Result<T, String>>) -> SftpRequest) -> Result<T, String> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.req_tx
            .send(build(tx))
            .map_err(|_| "SFTP thread stopped".to_string())?;
        match rx.recv_timeout(Duration::from_secs(120)) {
            Ok(r) => r,
            Err(_) => Err("SFTP operation timed out".to_string()),
        }
    }

    pub fn home_dir(&self) -> Result<String, String> {
        self.call(|reply| SftpRequest::Home { reply })
    }

    pub fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>, String> {
        self.call(|reply| SftpRequest::List {
            path: path.to_string(),
            reply,
        })
    }

    pub fn upload(&self, local: &Path, remote: &str) -> Result<(), String> {
        self.upload_with_progress(local, remote, None, "")
    }

    pub fn upload_with_progress(
        &self,
        local: &Path,
        remote: &str,
        progress: Option<Arc<ByteProgress>>,
        label: &str,
    ) -> Result<(), String> {
        self.call(|reply| SftpRequest::Upload {
            local: local.to_path_buf(),
            remote: remote.to_string(),
            progress,
            label: label.to_string(),
            reply,
        })
    }

    pub fn download(&self, remote: &str, local: &Path) -> Result<(), String> {
        self.download_with_progress(remote, local, None, "")
    }

    pub fn download_with_progress(
        &self,
        remote: &str,
        local: &Path,
        progress: Option<Arc<ByteProgress>>,
        label: &str,
    ) -> Result<(), String> {
        self.call(|reply| SftpRequest::Download {
            remote: remote.to_string(),
            local: local.to_path_buf(),
            progress,
            label: label.to_string(),
            reply,
        })
    }

    pub fn entry_info(&self, path: &str) -> Result<EntryInfo, String> {
        self.call(|reply| SftpRequest::Stat {
            path: path.to_string(),
            reply,
        })
    }

    pub fn path_bytes(&self, path: &str) -> Result<u64, String> {
        self.call(|reply| SftpRequest::PathBytes {
            path: path.to_string(),
            reply,
        })
    }

    pub fn remove(&self, path: &str, is_dir: bool) -> Result<(), String> {
        self.call(|reply| SftpRequest::Remove {
            path: path.to_string(),
            is_dir,
            reply,
        })
    }

    pub fn mkdir(&self, path: &str) -> Result<(), String> {
        self.call(|reply| SftpRequest::Mkdir {
            path: path.to_string(),
            reply,
        })
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        self.call(|reply| SftpRequest::Rename {
            from: from.to_string(),
            to: to.to_string(),
            reply,
        })
    }
}

impl Drop for SftpClient {
    fn drop(&mut self) {
        let _ = self.req_tx.send(SftpRequest::Shutdown);
    }
}

fn sftp_worker(
    host: &str,
    port: u16,
    user: &str,
    password: Option<String>,
    req_rx: mpsc::Receiver<SftpRequest>,
) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;

    let sftp = rt.block_on(connect_sftp(host, port, user, password.as_deref()))?;
    sftp.set_timeout(120);

    while let Ok(req) = req_rx.recv() {
        match req {
            SftpRequest::List { path, reply } => {
                let r = rt.block_on(list_remote(&sftp, &path));
                let _ = reply.send(r);
            }
            SftpRequest::Upload {
                local,
                remote,
                progress,
                label,
                reply,
            } => {
                let r = rt.block_on(upload_file(
                    &sftp,
                    &local,
                    &remote,
                    progress.as_deref(),
                    &label,
                ));
                let _ = reply.send(r);
            }
            SftpRequest::Download {
                remote,
                local,
                progress,
                label,
                reply,
            } => {
                let r = rt.block_on(download_file(
                    &sftp,
                    &remote,
                    &local,
                    progress.as_deref(),
                    &label,
                ));
                let _ = reply.send(r);
            }
            SftpRequest::Stat { path, reply } => {
                let r = rt.block_on(remote_entry_info(&sftp, &path));
                let _ = reply.send(r);
            }
            SftpRequest::PathBytes { path, reply } => {
                let r = rt.block_on(remote_path_bytes(&sftp, &path));
                let _ = reply.send(r);
            }
            SftpRequest::Remove { path, is_dir, reply } => {
                let r = rt.block_on(async {
                    if is_dir {
                        sftp.remove_dir(&path).await
                    } else {
                        sftp.remove_file(&path).await
                    }
                    .map_err(|e| e.to_string())
                });
                let _ = reply.send(r);
            }
            SftpRequest::Mkdir { path, reply } => {
                let r = rt.block_on(async {
                    sftp.create_dir(&path).await.map_err(|e| e.to_string())
                });
                let _ = reply.send(r);
            }
            SftpRequest::Rename { from, to, reply } => {
                let r = rt.block_on(async {
                    sftp.rename(&from, &to).await.map_err(|e| e.to_string())
                });
                let _ = reply.send(r);
            }
            SftpRequest::Home { reply } => {
                let r = rt.block_on(async {
                    sftp.canonicalize(".")
                        .await
                        .map_err(|e| e.to_string())
                });
                let _ = reply.send(r);
            }
            SftpRequest::Shutdown => break,
        }
    }
    Ok(())
}

async fn connect_sftp(
    host: &str,
    port: u16,
    user: &str,
    password: Option<&str>,
) -> Result<SftpSession, String> {
    let ssh_config = Arc::new(client::Config::default());
    let mut handle =
        client::connect(ssh_config, (host, port), SftpSshClient)
            .await
            .map_err(|e| e.to_string())?;

    authenticate(&mut handle, user, password).await?;

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| e.to_string())?;

    SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| e.to_string())
}

async fn authenticate(
    handle: &mut Handle<SftpSshClient>,
    user: &str,
    password: Option<&str>,
) -> Result<(), String> {
    for path in default_key_paths() {
        if !path.is_file() {
            continue;
        }
        let Ok(key) = load_secret_key(&path, None) else {
            continue;
        };
        let hash = handle
            .best_supported_rsa_hash()
            .await
            .map_err(|e| e.to_string())?
            .flatten();
        let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
        if handle
            .authenticate_publickey(user, key)
            .await
            .map(|r| r.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
    }

    let env_pw = std::env::var("SSH_PASSWORD")
        .ok()
        .filter(|p| !p.is_empty());
    let password = password
        .filter(|p| !p.is_empty())
        .or(env_pw.as_deref());

    if let Some(pw) = password {
        if handle
            .authenticate_password(user, pw)
            .await
            .map(|r| r.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        if try_keyboard_interactive(handle, user, pw).await {
            return Ok(());
        }
    }

    if handle
        .authenticate_none(user)
        .await
        .map(|r| r.success())
        .unwrap_or(false)
    {
        return Ok(());
    }

    Err("SSH authentication failed (tried public keys, password, and keyboard-interactive)".into())
}

async fn try_keyboard_interactive(
    handle: &mut Handle<SftpSshClient>,
    user: &str,
    password: &str,
) -> bool {
    let mut resp = match handle
        .authenticate_keyboard_interactive_start(user, None::<String>)
        .await
    {
        Ok(r) => r,
        Err(_) => return false,
    };

    loop {
        match resp {
            KeyboardInteractiveAuthResponse::Success => return true,
            KeyboardInteractiveAuthResponse::Failure { .. } => return false,
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let answers: Vec<String> = prompts
                    .iter()
                    .map(|_| password.to_string())
                    .collect();
                resp = match handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                {
                    Ok(r) => r,
                    Err(_) => return false,
                };
            }
        }
    }
}

fn default_key_paths() -> Vec<PathBuf> {
    let Some(home) = directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()) else {
        return Vec::new();
    };
    ["id_ed25519", "id_rsa", "id_ecdsa"]
        .into_iter()
        .map(|name| home.join(".ssh").join(name))
        .collect()
}

fn metadata_mtime(meta: &russh_sftp::client::fs::Metadata) -> Option<std::time::SystemTime> {
    meta.mtime
        .and_then(|t| UNIX_EPOCH.checked_add(Duration::new(t as u64, 0)))
        .or_else(|| meta.modified().ok())
}

async fn list_remote(sftp: &SftpSession, path: &str) -> Result<Vec<FileEntry>, String> {
    let read_dir = sftp.read_dir(path).await.map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for entry in read_dir {
        let name = entry.file_name();
        let meta = entry.metadata();
        let is_dir = meta.is_dir();
        entries.push(FileEntry {
            name,
            is_dir,
            size: meta.len(),
            modified: metadata_mtime(&meta),
        });
    }
    entries.sort_by_key(|e| e.sort_key());
    Ok(entries)
}

fn check_cancel(progress: Option<&ByteProgress>) -> Result<(), String> {
    if progress.is_some_and(|p| p.is_cancelled()) {
        Err("Transfer stopped".into())
    } else {
        Ok(())
    }
}

async fn upload_file(
    sftp: &SftpSession,
    local: &Path,
    remote: &str,
    progress: Option<&ByteProgress>,
    label: &str,
) -> Result<(), String> {
    use std::fs::File;
    use std::io::Read;

    check_cancel(progress)?;

    if local.is_dir() {
        let _ = sftp.create_dir(remote).await;
        for item in std::fs::read_dir(local).map_err(|e| e.to_string())? {
            check_cancel(progress)?;
            let item = item.map_err(|e| e.to_string())?;
            let name = item.file_name().to_string_lossy().into_owned();
            let sub_local = local.join(&name);
            let sub_remote = format!("{remote}/{name}");
            let sub_label = format!("Uploading {name}");
            Box::pin(upload_file(
                sftp,
                &sub_local,
                &sub_remote,
                progress,
                &sub_label,
            ))
            .await?;
        }
        return Ok(());
    }

    if let Some(p) = progress {
        p.set_label(label);
    }

    let mut local_f = File::open(local).map_err(|e| e.to_string())?;
    let mut remote_f = sftp.create(remote).await.map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        check_cancel(progress)?;
        let n = local_f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        remote_f
            .write_all(&buf[..n])
            .await
            .map_err(|e| e.to_string())?;
        if let Some(p) = progress {
            p.add_bytes(n as u64, label);
        }
    }
    let _ = remote_f.shutdown().await;
    Ok(())
}

async fn download_file(
    sftp: &SftpSession,
    remote: &str,
    local: &Path,
    progress: Option<&ByteProgress>,
    label: &str,
) -> Result<(), String> {
    use std::fs::File;
    use std::io::Write;

    check_cancel(progress)?;

    let meta = sftp.metadata(remote).await.map_err(|e| e.to_string())?;
    if meta.is_dir() {
        std::fs::create_dir_all(local).map_err(|e| e.to_string())?;
        let read_dir = sftp.read_dir(remote).await.map_err(|e| e.to_string())?;
        for entry in read_dir {
            check_cancel(progress)?;
            let name = entry.file_name();
            let sub_remote = format!("{remote}/{name}");
            let sub_local = local.join(&name);
            let sub_label = format!("Downloading {name}");
            Box::pin(download_file(
                sftp,
                &sub_remote,
                &sub_local,
                progress,
                &sub_label,
            ))
            .await?;
        }
        return Ok(());
    }

    if let Some(p) = progress {
        p.set_label(label);
    }

    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut remote_f = sftp.open(remote).await.map_err(|e| e.to_string())?;
    let mut local_f = File::create(local).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        check_cancel(progress)?;
        let n = remote_f
            .read(&mut buf)
            .await
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        local_f
            .write_all(&buf[..n])
            .map_err(|e| e.to_string())?;
        if let Some(p) = progress {
            p.add_bytes(n as u64, label);
        }
    }
    let _ = remote_f.shutdown().await;
    Ok(())
}

async fn remote_entry_info(sftp: &SftpSession, path: &str) -> Result<EntryInfo, String> {
    let stat = sftp.metadata(path).await.map_err(|e| e.to_string())?;
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let is_dir = stat.is_dir();
    let size = if is_dir {
        remote_path_bytes(sftp, path).await?
    } else {
        stat.len()
    };
    let mode = stat.permissions.unwrap_or(0);
    Ok(EntryInfo {
        path: path.to_string(),
        name,
        kind: if is_dir { "Folder".into() } else { "File".into() },
        size,
        permissions: entry_info::format_unix_mode(mode),
        modified: entry_info::format_time(metadata_mtime(&stat)),
    })
}

async fn remote_path_bytes(sftp: &SftpSession, path: &str) -> Result<u64, String> {
    let stat = sftp.metadata(path).await.map_err(|e| e.to_string())?;
    if !stat.is_dir() {
        return Ok(stat.len());
    }
    let mut total = 0u64;
    let read_dir = sftp.read_dir(path).await.map_err(|e| e.to_string())?;
    for entry in read_dir {
        let name = entry.file_name();
        let sub = format!("{path}/{name}");
        total += Box::pin(remote_path_bytes(sftp, &sub)).await?;
    }
    Ok(total)
}

pub fn join_remote(base: &str, name: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}
