//! Notify the foreground process group on a PTY that the terminal size changed.

#[cfg(unix)]
pub fn signal_winch_to_pty_foreground(master_fd: i32) {
    if master_fd < 0 {
        return;
    }
    // SAFETY: tcgetpgrp/kill with a valid PTY master fd.
    unsafe {
        let pgid = libc::tcgetpgrp(master_fd);
        if pgid > 0 {
            // Negative pid: whole foreground process group (htop, shell job control).
            libc::kill(-pgid, libc::SIGWINCH);
        }
    }
}

#[cfg(not(unix))]
pub fn signal_winch_to_pty_foreground(_master_fd: i32) {}
