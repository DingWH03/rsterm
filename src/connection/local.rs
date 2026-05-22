use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use portable_pty::{CommandBuilder, MasterPty, PtySize};

use crate::connection::{
    emit_conn_data, pty_burst, winchg, ConnIn, ConnOut, ConnectionHandle, ConnectionState,
    RepaintNotifier,
};
use crate::settings::Profile;
use crate::storage::types::SavedConnection;

fn pty_size(rows: u16, cols: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: cols.max(1).saturating_mul(8),
        pixel_height: rows.max(1).saturating_mul(16),
    }
}

/// Unix: SIGWINCH to the foreground process group. Windows ConPTY: resize on the handle is enough.
fn signal_winch_if_needed(master: &dyn MasterPty, shell_pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(fd) = master.as_raw_fd() {
        winchg::signal_winch(fd, shell_pid);
    }
}

fn apply_pty_resize(
    master: &dyn MasterPty,
    rows: u16,
    cols: u16,
    shell_pid: Option<u32>,
) {
    // Try TIOCSWINSZ first – the kernel will broadcast SIGWINCH to the foreground
    // process group on success.  Always send our own SIGWINCH as well because:
    //   - Some kernels/pty layers skip the automatic SIGWINCH when the size is the
    //     same (harmless extra signal).
    //   - portable-pty might fail TIOCSWINSZ (e.g. Windows ConPTY fallback).
    let _ = master.resize(pty_size(rows, cols));
    signal_winch_if_needed(master, shell_pid);
}

pub fn connect_local(
    config: &SavedConnection,
    profile: &Profile,
    rows: u16,
    cols: u16,
) -> Result<ConnectionHandle, String> {
    let (to_conn_tx, to_conn_rx) = mpsc::channel::<ConnOut>();
    let (from_conn_tx, from_conn_rx) = mpsc::channel::<ConnIn>();
    let (blocking_resize_tx, blocking_resize_rx) = mpsc::sync_channel::<(u16, u16)>(0);

    let shell = config
        .shell
        .clone()
        .unwrap_or_else(|| crate::platform::get().default_shell());

    let sys = portable_pty::native_pty_system();
    let pair = sys
        .openpty(pty_size(rows, cols))
        .map_err(|e| format!("Failed to open PTY: {e}"))?;

    let mut cmd = CommandBuilder::new(&shell);
    // Login/interactive shell (Unix). Windows uses ConPTY + cmd/powershell without -l.
    #[cfg(unix)]
    cmd.arg("-l");

    // Inherit system environment variables
    for (key, value) in std::env::vars() {
        cmd.env(&key, &value);
    }
    // Override with profile settings
    for (key, value) in &profile.env_vars {
        cmd.env(key, value);
    }

    if let Some(dir) = config.working_dir.as_deref() {
        let path = std::path::Path::new(dir);
        if path.is_dir() {
            cmd.cwd(path);
        }
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn command: {e}"))?;

    let shell_pid = child.process_id();

    drop(pair.slave);

    let master = pair.master;
    #[cfg(unix)]
    let poll_fd = master.as_raw_fd();
    let mut reader = master.try_clone_reader().map_err(|e| e.to_string())?;
    let mut writer = master.take_writer().map_err(|e| e.to_string())?;

    let _ = from_conn_tx.send(ConnIn::StateChanged(ConnectionState::Connected));

    let repaint = RepaintNotifier::default();
    let reader_from_tx = from_conn_tx.clone();
    let reader_repaint = repaint.clone();
    let reader_thread = std::thread::spawn(move || {
        let mut buf = [0u8; 32 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    let _ = reader_from_tx
                        .send(ConnIn::StateChanged(ConnectionState::Closed));
                    return;
                }
                Ok(n) => {
                    let mut chunk = buf[..n].to_vec();
                    #[cfg(unix)]
                    if let Some(fd) = poll_fd {
                        let _ = pty_burst::append_until_idle(fd, &mut *reader, &mut buf, &mut chunk);
                    }
                    emit_conn_data(&reader_from_tx, &reader_repaint, chunk);
                }
                Err(e) => {
                    let _ = reader_from_tx.send(ConnIn::StateChanged(ConnectionState::Error(e.to_string())));
                    return;
                }
            }
        }
    });

    let writer_from_tx = from_conn_tx;
    let writer_thread = std::thread::spawn(move || {
        loop {
            match blocking_resize_rx.recv_timeout(Duration::from_millis(10)) {
                Ok((rows, cols)) => {
                    apply_pty_resize(&*master, rows, cols, shell_pid);
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
            match to_conn_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(ConnOut::Data(data)) | Ok(ConnOut::PortData { port: 0, data }) => {
                    if writer.write_all(&data).is_err() {
                        let _ = writer_from_tx.send(ConnIn::StateChanged(ConnectionState::Lost(
                            "Connection lost.".into(),
                        )));
                        return;
                    }
                    let _ = writer.flush();
                }
                Ok(ConnOut::PortData { .. }) => {
                    // Non-multiplexed PTY connections only expose port 0.
                }
                Ok(ConnOut::Resize(rows, cols)) => {
                    apply_pty_resize(&*master, rows, cols, shell_pid);
                }
                Ok(ConnOut::Winch) => {
                    signal_winch_if_needed(&*master, shell_pid);
                }
                Ok(ConnOut::Close) => {
                    let _ = writer_from_tx
                        .send(ConnIn::StateChanged(ConnectionState::Closed));
                    return;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });

    Ok(
        ConnectionHandle::new(to_conn_tx, from_conn_rx, reader_thread, writer_thread, repaint)
            .with_pty_child(child)
            .with_blocking_resize(blocking_resize_tx),
    )
}
