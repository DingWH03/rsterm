use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::fs::sftp::SftpClient;
use crate::fs::{home_dir, FileEntry};
use crate::storage::types::ConnectionType;

/// Session types that bridge terminal and file-manager sessions.
/// The enum itself lives in the terminal page module because it wraps
/// [`ActiveSession`]; we re-export it here for convenience.
pub use crate::ui::page::terminal::WorkspaceSession;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PaneSide {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileClipboardMode {
    Copy,
    Cut,
}

#[derive(Clone)]
pub struct FileClipboard {
    pub mode: FileClipboardMode,
    pub from_remote: bool,
    pub paths: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileManagerMode {
    /// SSH: left remote SFTP, right local disk.
    SshSftp,
    /// Local: both panes use the local filesystem.
    LocalDual,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum FileActivePane {
    #[default]
    Right,
    Remote,
    LeftLocal,
}

pub struct PaneState {
    pub cwd: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected: HashSet<usize>,
    pub select_mode: bool,
    pub focus_index: Option<usize>,
    pub error: Option<String>,
    pub loading: bool,
}

impl PaneState {
    pub fn new_local(start: PathBuf) -> Self {
        Self {
            cwd: start,
            entries: Vec::new(),
            selected: HashSet::new(),
            select_mode: false,
            focus_index: None,
            error: None,
            loading: true,
        }
    }
}

#[derive(Clone, Default)]
pub struct TransferSnapshot {
    pub active: bool,
    pub progress: f32,
    pub label: String,
    pub finished: bool,
    pub status_message: Option<String>,
    pub clear_clipboard: bool,
    pub refresh_remote: bool,
    pub refresh_local_right: bool,
    pub refresh_local_left: bool,
}

pub struct FileTransferState {
    pub cancel: Arc<AtomicBool>,
    pub snapshot: Arc<Mutex<TransferSnapshot>>,
    pub join: Option<JoinHandle<()>>,
}

impl Default for FileTransferState {
    fn default() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            snapshot: Arc::new(Mutex::new(TransferSnapshot::default())),
            join: None,
        }
    }
}

pub struct RemotePane {
    pub client: Arc<SftpClient>,
    pub cwd: String,
    pub entries: Vec<FileEntry>,
    pub selected: HashSet<usize>,
    pub select_mode: bool,
    pub focus_index: Option<usize>,
    pub error: Option<String>,
    pub loading: bool,
}

#[derive(Default)]
pub struct RenameDialog {
    pub open: bool,
    pub pane: FileActivePane,
    pub new_name: String,
    old_name: String,
}

impl RenameDialog {
    pub fn open_for(&mut self, pane: FileActivePane, name: &str) {
        self.open = true;
        self.pane = pane;
        self.old_name = name.to_string();
        self.new_name = name.to_string();
    }

    pub fn old_name(&self) -> &str {
        &self.old_name
    }
}

#[derive(Clone)]
pub struct InfoLine(pub String, pub String);

#[derive(Default)]
pub struct InfoDialog {
    pub open: bool,
    pub lines: Vec<InfoLine>,
}

impl InfoDialog {
    pub fn show(&mut self, info: crate::fs::entry_info::EntryInfo) {
        use crate::fs::transfer_progress::format_bytes;
        self.open = true;
        self.lines = vec![
            InfoLine("Name".into(), info.name),
            InfoLine("Type".into(), info.kind),
            InfoLine("Size".into(), format_bytes(info.size)),
            InfoLine("Permissions".into(), info.permissions),
            InfoLine("Modified".into(), info.modified),
            InfoLine("Path".into(), info.path),
        ];
    }
}

pub struct FileManagerSession {
    pub id: String,
    pub title: String,
    /// Saved SSH profile id (for sidebar「新窗口」).
    pub saved_conn_id: Option<String>,
    pub mode: FileManagerMode,
    pub remote: Option<RemotePane>,
    /// Left pane when `mode == LocalDual`.
    pub left_local: Option<PaneState>,
    pub right: PaneState,
    pub clipboard: Option<FileClipboard>,
    pub status: Option<String>,
    pub rename_dialog: RenameDialog,
    pub info_dialog: InfoDialog,
    pub transfer: FileTransferState,
    /// Anchor index for shift-range selection per pane.
    pub local_anchor: Option<usize>,
    pub right_anchor: Option<usize>,
    pub remote_anchor: Option<usize>,
    pub active_pane: FileActivePane,
}

impl FileManagerSession {
    /// Sidebar label: left pane path only (remote for SFTP, left local for dual-local).
    pub fn tab_label(&self) -> String {
        match self.mode {
            FileManagerMode::SshSftp => {
                let host = self
                    .title
                    .strip_prefix("Remote: ")
                    .unwrap_or(self.title.as_str());
                self.remote
                    .as_ref()
                    .map(|r| format!("{host}:{}", r.cwd))
                    .unwrap_or_else(|| self.title.clone())
            }
            FileManagerMode::LocalDual => self
                .left_local
                .as_ref()
                .map(|p| p.cwd.display().to_string())
                .unwrap_or_else(|| "File Manager".to_string()),
        }
    }

    pub fn open_ssh(config: &crate::storage::types::SavedConnection) -> Result<Self, String> {
        let client = SftpClient::connect(config)?;
        let host = config.ssh_host.as_deref().unwrap_or("host");
        let title = format!("Remote: {host}");
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            saved_conn_id: Some(config.id.clone()),
            mode: FileManagerMode::SshSftp,
            remote: Some(RemotePane {
                client: Arc::new(client),
                // Start at "/"; the first refresh will load it. The real home
                // will be resolved once the SFTP connection is ready.
                cwd: "/".to_string(),
                entries: Vec::new(),
                selected: HashSet::new(),
                select_mode: false,
                focus_index: None,
                error: None,
                loading: true,
            }),
            left_local: None,
            right: PaneState::new_local(home_dir()),
            clipboard: None,
            status: None,
            rename_dialog: RenameDialog::default(),
            info_dialog: InfoDialog::default(),
            transfer: FileTransferState::default(),
            local_anchor: None,
            right_anchor: None,
            remote_anchor: None,
            active_pane: FileActivePane::Remote,
        })
    }

    pub fn open_local() -> Self {
        let home = home_dir();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: "File Manager".to_string(),
            saved_conn_id: None,
            mode: FileManagerMode::LocalDual,
            remote: None,
            left_local: Some(PaneState::new_local(home.clone())),
            right: PaneState::new_local(home),
            clipboard: None,
            status: None,
            rename_dialog: RenameDialog::default(),
            info_dialog: InfoDialog::default(),
            transfer: FileTransferState::default(),
            local_anchor: None,
            right_anchor: None,
            remote_anchor: None,
            active_pane: FileActivePane::LeftLocal,
        }
    }
}

pub fn terminal_conn_type(session: &WorkspaceSession) -> Option<&ConnectionType> {
    match session {
        WorkspaceSession::Terminal(s) => Some(&s.conn_type),
        _ => None,
    }
}
