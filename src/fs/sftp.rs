use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, UNIX_EPOCH};

use ssh2::{Session, Sftp};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

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

        let client = Self {
            req_tx,
            _thread: thread,
        };
        Ok(client)
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
    let tcp = TcpStream::connect(format!("{host}:{port}")).map_err(|e| e.to_string())?;
    tcp.set_read_timeout(Some(Duration::from_secs(60)))
        .ok();
    tcp.set_write_timeout(Some(Duration::from_secs(60)))
        .ok();

    let mut sess = Session::new().map_err(|e| e.to_string())?;
    sess.set_tcp_stream(tcp);
    sess.handshake().map_err(|e| e.to_string())?;
    authenticate(&sess, user, password.as_deref())?;

    let sftp = Arc::new(Mutex::new(sess.sftp().map_err(|e| e.to_string())?));

    while let Ok(req) = req_rx.recv() {
        match req {
            SftpRequest::List { path, reply } => {
                let r = with_sftp(&sftp, |s| list_remote(s, &path));
                let _ = reply.send(r);
            }
            SftpRequest::Upload {
                local,
                remote,
                progress,
                label,
                reply,
            } => {
                let r = with_sftp(&sftp, |s| {
                    upload_file(s, &local, &remote, progress.as_deref(), &label)
                });
                let _ = reply.send(r);
            }
            SftpRequest::Download {
                remote,
                local,
                progress,
                label,
                reply,
            } => {
                let r = with_sftp(&sftp, |s| {
                    download_file(s, &remote, &local, progress.as_deref(), &label)
                });
                let _ = reply.send(r);
            }
            SftpRequest::Stat { path, reply } => {
                let r = with_sftp(&sftp, |s| remote_entry_info(s, &path));
                let _ = reply.send(r);
            }
            SftpRequest::PathBytes { path, reply } => {
                let r = with_sftp(&sftp, |s| remote_path_bytes(s, &path));
                let _ = reply.send(r);
            }
            SftpRequest::Remove { path, is_dir, reply } => {
                let r = with_sftp(&sftp, |s| {
                    if is_dir {
                        s.rmdir(Path::new(&path)).map_err(|e| e.to_string())
                    } else {
                        s.unlink(Path::new(&path)).map_err(|e| e.to_string())
                    }
                });
                let _ = reply.send(r);
            }
            SftpRequest::Mkdir { path, reply } => {
                let r = with_sftp(&sftp, |s| {
                    s.mkdir(Path::new(&path), 0o755).map_err(|e| e.to_string())
                });
                let _ = reply.send(r);
            }
            SftpRequest::Rename { from, to, reply } => {
                let r = with_sftp(&sftp, |s| {
                    s.rename(Path::new(&from), Path::new(&to), None)
                        .map_err(|e| e.to_string())
                });
                let _ = reply.send(r);
            }
            SftpRequest::Home { reply } => {
                let r = remote_home(&sftp);
                let _ = reply.send(r);
            }
            SftpRequest::Shutdown => break,
        }
    }
    Ok(())
}

fn with_sftp<T>(sftp: &Arc<Mutex<Sftp>>, f: impl FnOnce(&Sftp) -> Result<T, String>) -> Result<T, String> {
    let guard = sftp.lock().map_err(|_| "SFTP lock failed".to_string())?;
    f(&guard)
}

fn remote_home(sftp: &Arc<Mutex<Sftp>>) -> Result<String, String> {
    with_sftp(sftp, |s| {
        let cwd = s.realpath(Path::new(".")).map_err(|e| e.to_string())?;
        Ok(cwd.to_string_lossy().into_owned())
    })
}

fn list_remote(sftp: &Sftp, path: &str) -> Result<Vec<FileEntry>, String> {
    let p = Path::new(path);
    let mut entries = Vec::new();
    for (path, stat) in sftp.readdir(p).map_err(|e| e.to_string())? {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == "." || name == ".." {
            continue;
        }
        let is_dir = stat.is_dir();
        entries.push(FileEntry {
            name,
            is_dir,
            size: stat.size.unwrap_or(0),
            modified: stat
                .mtime
                .and_then(|t| UNIX_EPOCH.checked_add(Duration::new(t, 0))),
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

fn upload_file(
    sftp: &Sftp,
    local: &Path,
    remote: &str,
    progress: Option<&ByteProgress>,
    label: &str,
) -> Result<(), String> {
    use std::fs::File;
    use std::io::{Read, Write};

    check_cancel(progress)?;

    if local.is_dir() {
        sftp.mkdir(Path::new(remote), 0o755).ok();
        for item in std::fs::read_dir(local).map_err(|e| e.to_string())? {
            check_cancel(progress)?;
            let item = item.map_err(|e| e.to_string())?;
            let name = item.file_name().to_string_lossy().into_owned();
            let sub_local = local.join(&name);
            let sub_remote = format!("{remote}/{name}");
            let sub_label = format!("Uploading {name}");
            upload_file(sftp, &sub_local, &sub_remote, progress, &sub_label)?;
        }
        return Ok(());
    }

    if let Some(p) = progress {
        p.set_label(label);
    }

    let mut local_f = File::open(local).map_err(|e| e.to_string())?;
    let mut remote_f = sftp
        .create(Path::new(remote))
        .map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        check_cancel(progress)?;
        let n = local_f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        remote_f
            .write_all(&buf[..n])
            .map_err(|e| e.to_string())?;
        if let Some(p) = progress {
            p.add_bytes(n as u64, label);
        }
    }
    Ok(())
}

fn download_file(
    sftp: &Sftp,
    remote: &str,
    local: &Path,
    progress: Option<&ByteProgress>,
    label: &str,
) -> Result<(), String> {
    use std::fs::File;
    use std::io::{Read, Write};

    check_cancel(progress)?;

    let remote_path = Path::new(remote);
    let stat = sftp.stat(remote_path).map_err(|e| e.to_string())?;
    if stat.is_dir() {
        std::fs::create_dir_all(local).map_err(|e| e.to_string())?;
        for (ent_path, _) in sftp.readdir(remote_path).map_err(|e| e.to_string())? {
            check_cancel(progress)?;
            let name = ent_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name == "." || name == ".." {
                continue;
            }
            let sub_remote = format!("{remote}/{name}");
            let sub_local = local.join(&name);
            let sub_label = format!("Downloading {name}");
            download_file(sftp, &sub_remote, &sub_local, progress, &sub_label)?;
        }
        return Ok(());
    }

    if let Some(p) = progress {
        p.set_label(label);
    }

    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut remote_f = sftp.open(remote_path).map_err(|e| e.to_string())?;
    let mut local_f = File::create(local).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        check_cancel(progress)?;
        let n = remote_f.read(&mut buf).map_err(|e| e.to_string())?;
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
    Ok(())
}

fn remote_entry_info(sftp: &Sftp, path: &str) -> Result<EntryInfo, String> {
    let p = Path::new(path);
    let stat = sftp.stat(p).map_err(|e| e.to_string())?;
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let is_dir = stat.is_dir();
    let size = if is_dir {
        remote_path_bytes(sftp, path)?
    } else {
        stat.size.unwrap_or(0)
    };
    let mode = stat.perm.unwrap_or(0);
    Ok(EntryInfo {
        path: path.to_string(),
        name,
        kind: if is_dir { "Folder".into() } else { "File".into() },
        size,
        permissions: entry_info::format_unix_mode(mode),
        modified: entry_info::format_time(
            stat.mtime
                .and_then(|t| UNIX_EPOCH.checked_add(Duration::new(t, 0))),
        ),
    })
}

fn remote_path_bytes(sftp: &Sftp, path: &str) -> Result<u64, String> {
    let p = Path::new(path);
    let stat = sftp.stat(p).map_err(|e| e.to_string())?;
    if !stat.is_dir() {
        return Ok(stat.size.unwrap_or(0));
    }
    let mut total = 0u64;
    for (ent_path, ent_stat) in sftp.readdir(p).map_err(|e| e.to_string())? {
        let name = ent_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == "." || name == ".." {
            continue;
        }
        let sub = format!("{path}/{name}");
        total += remote_path_bytes(sftp, &sub)?;
    }
    Ok(total)
}

fn authenticate(sess: &Session, user: &str, password: Option<&str>) -> Result<(), String> {
    if try_public_key(sess, user) {
        return Ok(());
    }
    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        sess.userauth_password(user, pw).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if std::env::var("SSH_PASSWORD")
        .ok()
        .filter(|p| !p.is_empty())
        .is_some_and(|pw| sess.userauth_password(user, &pw).is_ok())
    {
        return Ok(());
    }
    sess.userauth_agent(user).ok();
    if sess.authenticated() {
        return Ok(());
    }
    Err("SSH authentication failed".into())
}

fn try_public_key(sess: &Session, user: &str) -> bool {
    let Some(home) = directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()) else {
        return false;
    };
    for name in ["id_ed25519", "id_rsa", "id_ecdsa"] {
        let path = home.join(".ssh").join(name);
        if !path.is_file() {
            continue;
        }
        if sess
            .userauth_pubkey_file(user, None, &path, None)
            .is_ok()
            && sess.authenticated()
        {
            return true;
        }
    }
    false
}

pub fn join_remote(base: &str, name: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}
