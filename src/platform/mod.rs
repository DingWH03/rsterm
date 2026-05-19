//! Cross-platform helpers (Linux, macOS, Windows, Android).

mod ble;
mod process;
mod serial;
mod shell;

pub use process::{
    foreground_command, foreground_process_pid, local_user_at_host, ssh_user_at_host,
    title_is_idle_host, truncate_label,
};

pub use ble::scan_ble_devices_blocking;
pub use serial::{enumerate_serial_ports, SerialDevice};
pub use shell::default_shell;

/// Which connection kinds are supported on this build target.
#[derive(Clone, Copy, Debug)]
pub struct Capabilities {
    pub local_terminal: bool,
    pub ssh: bool,
    pub serial: bool,
    pub ble: bool,
}

pub fn capabilities() -> Capabilities {
    Capabilities {
        local_terminal: cfg!(any(windows, unix)),
        ssh: true,
        serial: true,
        ble: cfg!(any(
            target_os = "linux",
            target_os = "windows",
            target_os = "macos",
            target_os = "android",
            target_os = "ios",
        )),
    }
}
