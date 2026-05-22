//! Notify the foreground process group on a PTY that the terminal size changed.

#[cfg(unix)]
pub fn signal_winch(master_fd: i32, shell_pid: Option<u32>) {
    if master_fd < 0 {
        return;
    }
    // SAFETY: tcgetpgrp/kill with a valid PTY master fd.
    unsafe {
        let pgid = libc::tcgetpgrp(master_fd);
        if pgid > 0 {
            // Negative pid: whole foreground process group (htop, shell job control).
            libc::kill(-pgid, libc::SIGWINCH);
        } else {
            // tcgetpgrp failed or returned no foreground group – try the shell itself.
            if let Some(shell) = shell_pid {
                libc::kill(shell as i32, libc::SIGWINCH);
            }
        }
    }
    #[cfg(target_os = "linux")]
    if let Some(shell) = shell_pid {
        if let Some(fg) = crate::platform::get().foreground_process_pid(shell) {
            // Belt-and-suspenders: some setups only deliver SIGWINCH to the leaf process.
            unsafe {
                libc::kill(fg as i32, libc::SIGWINCH);
            }
        }
        // On Linux, also signal the shell itself so it updates COLUMNS/LINES.
        unsafe {
            libc::kill(shell as i32, libc::SIGWINCH);
        }
    }
}

#[cfg(not(unix))]
pub fn signal_winch(_master_fd: i32, _shell_pid: Option<u32>) {}
