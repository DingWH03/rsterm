pub mod ble;
pub mod local;
pub mod pty_burst;
pub mod ssh_keys;
pub mod winchg;
pub mod serial;
pub mod ssh;

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
    Error(String),
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionState::Connecting => write!(f, "Connecting..."),
            ConnectionState::Connected => write!(f, "Connected"),
            ConnectionState::Disconnected => write!(f, "Disconnected"),
            ConnectionState::Error(e) => write!(f, "Error: {e}"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConnIn {
    Data(Vec<u8>),
    StateChanged(ConnectionState),
}

#[derive(Debug, Clone)]
pub enum ConnOut {
    Data(Vec<u8>),
    Resize(u16, u16),
    /// Re-deliver SIGWINCH without changing winsize (ncurses full refresh after resize).
    Winch,
    Close,
}

/// Wakes the egui event loop when connection I/O threads receive terminal output.
#[derive(Clone, Default)]
pub struct RepaintNotifier(Arc<RepaintNotifierInner>);

#[derive(Default)]
struct RepaintNotifierInner {
    ctx: Mutex<Option<egui::Context>>,
    /// Coalesce bursty PTY output (e.g. long shell history redraw) into one repaint per frame.
    repaint_pending: AtomicBool,
}

impl RepaintNotifier {
    pub fn set_context(&self, ctx: egui::Context) {
        if let Ok(mut guard) = self.0.ctx.lock() {
            *guard = Some(ctx);
        }
    }

    pub fn notify(&self) {
        if self
            .0
            .repaint_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        if let Ok(guard) = self.0.ctx.lock() {
            if let Some(ctx) = guard.as_ref() {
                ctx.request_repaint();
            } else {
                self.0.repaint_pending.store(false, Ordering::Release);
            }
        }
    }

    /// Allow another repaint after the UI has drained pending PTY data.
    pub fn clear_repaint_pending(&self) {
        self.0.repaint_pending.store(false, Ordering::Release);
    }
}

pub fn emit_conn_data(
    from: &std::sync::mpsc::Sender<ConnIn>,
    repaint: &RepaintNotifier,
    data: Vec<u8>,
) {
    if from.send(ConnIn::Data(data)).is_ok() {
        repaint.notify();
    }
}

pub struct ConnectionHandle {
    pub sender: std::sync::mpsc::Sender<ConnOut>,
    pub receiver: std::sync::mpsc::Receiver<ConnIn>,
    pub state: ConnectionState,
    pub repaint: RepaintNotifier,
    /// Local PTY shell PID (for tab foreground process label).
    pub shell_pid: Option<u32>,
    /// Local PTY: blocks until the writer thread applies `TIOCSWINSZ` (keeps SIGWINCH before emulator resize).
    blocking_resize: Option<std::sync::mpsc::SyncSender<(u16, u16)>>,
    _reader_thread: Option<std::thread::JoinHandle<()>>,
    _writer_thread: Option<std::thread::JoinHandle<()>>,
    _pty_child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
}

impl ConnectionHandle {
    pub fn new(
        sender: std::sync::mpsc::Sender<ConnOut>,
        receiver: std::sync::mpsc::Receiver<ConnIn>,
        reader_thread: std::thread::JoinHandle<()>,
        writer_thread: std::thread::JoinHandle<()>,
        repaint: RepaintNotifier,
    ) -> Self {
        Self {
            sender,
            receiver,
            state: ConnectionState::Connecting,
            repaint,
            shell_pid: None,
            blocking_resize: None,
            _reader_thread: Some(reader_thread),
            _writer_thread: Some(writer_thread),
            _pty_child: None,
        }
    }

    pub fn with_pty_child(mut self, child: Box<dyn portable_pty::Child + Send + Sync>) -> Self {
        self.shell_pid = child.process_id();
        self._pty_child = Some(child);
        self
    }

    pub fn with_blocking_resize(
        mut self,
        tx: std::sync::mpsc::SyncSender<(u16, u16)>,
    ) -> Self {
        self.blocking_resize = Some(tx);
        self
    }

    pub fn send(&self, data: Vec<u8>) {
        let _ = self.sender.send(ConnOut::Data(data));
    }

    pub fn resize(&self, rows: u16, cols: u16) {
        if let Some(tx) = &self.blocking_resize {
            let _ = tx.send((rows, cols));
        } else {
            let _ = self.sender.send(ConnOut::Resize(rows, cols));
        }
    }

    /// Ask the PTY writer to signal SIGWINCH again (htop/ncurses may need this after the grid catches up).
    pub fn signal_winch(&self) {
        let _ = self.sender.send(ConnOut::Winch);
    }

    pub fn close(&self) {
        let _ = self.sender.send(ConnOut::Close);
    }

    pub fn drain(&mut self) -> Vec<ConnIn> {
        let mut events = Vec::new();
        while let Ok(event) = self.receiver.try_recv() {
            match &event {
                ConnIn::StateChanged(s) => self.state = s.clone(),
                _ => {}
            }
            events.push(event);
        }
        events
    }
}

impl Drop for ConnectionHandle {
    fn drop(&mut self) {
        let _ = self.sender.send(ConnOut::Close);
        if let Some(j) = self._writer_thread.take() {
            let _ = j.join();
        }
        if let Some(j) = self._reader_thread.take() {
            let _ = j.join();
        }
    }
}
