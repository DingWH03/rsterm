use std::io::{Read, Write};
use std::sync::mpsc;

use portable_pty::{CommandBuilder, PtySize};

use crate::connection::{
    emit_conn_data, pty_burst, winchg, ConnIn, ConnOut, ConnectionHandle, ConnectionState,
    RepaintNotifier,
};
use crate::settings::Profile;
use crate::storage::types::SavedConnection;

pub fn connect_local(
    config: &SavedConnection,
    profile: &Profile,
    rows: u16,
    cols: u16,
) -> Result<ConnectionHandle, String> {
    let (to_conn_tx, to_conn_rx) = mpsc::channel::<ConnOut>();
    let (from_conn_tx, from_conn_rx) = mpsc::channel::<ConnIn>();

    let shell = config
        .shell
        .clone()
        .unwrap_or_else(crate::platform::default_shell);

    let sys = portable_pty::native_pty_system();
    let pair = sys
        .openpty(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to open PTY: {e}"))?;

    let mut cmd = CommandBuilder::new(&shell);
    // Login/interactive shell so zsh/bash emit prompts and echo input correctly.
    cmd.arg("-l");
    cmd.env("COLUMNS", cols.to_string());
    cmd.env("LINES", rows.to_string());

    // Inherit system environment variables
    for (key, value) in std::env::vars() {
        cmd.env(&key, &value);
    }
    // Override with profile settings
    for (key, value) in &profile.env_vars {
        cmd.env(key, value);
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn command: {e}"))?;

    drop(pair.slave);

    let master = pair.master;
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
                    let _ = reader_from_tx.send(ConnIn::StateChanged(ConnectionState::Disconnected));
                    return;
                }
                Ok(n) => {
                    let mut chunk = buf[..n].to_vec();
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
            match to_conn_rx.recv() {
                Ok(ConnOut::Data(data)) => {
                    if writer.write_all(&data).is_err() {
                        let _ = writer_from_tx.send(ConnIn::StateChanged(ConnectionState::Disconnected));
                        return;
                    }
                    let _ = writer.flush();
                }
                Ok(ConnOut::Resize(rows, cols)) => {
                    let size = PtySize {
                        rows: rows.max(1),
                        cols: cols.max(1),
                        // Non-zero pixels help some ncurses apps pick a sane geometry on SIGWINCH.
                        pixel_width: cols.max(1).saturating_mul(8),
                        pixel_height: rows.max(1).saturating_mul(16),
                    };
                    if master.resize(size).is_ok() {
                        if let Some(fd) = master.as_raw_fd() {
                            winchg::signal_winch_to_pty_foreground(fd);
                        }
                    }
                }
                Ok(ConnOut::Close) => {
                    let _ = writer_from_tx.send(ConnIn::StateChanged(ConnectionState::Disconnected));
                    return;
                }
                Err(_) => return,
            }
        }
    });

    Ok(
        ConnectionHandle::new(to_conn_tx, from_conn_rx, reader_thread, writer_thread, repaint)
            .with_pty_child(child),
    )
}
