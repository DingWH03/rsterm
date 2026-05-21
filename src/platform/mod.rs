//! Cross-platform helpers (Linux, macOS, Windows, Android).

#[cfg(target_os = "android")]
mod android_ime;
#[cfg(target_os = "android")]
mod android_storage;

mod ble;
mod process;
mod serial;
mod shell;

#[cfg(target_os = "android")]
pub use android_ime::{bottom_inset_points, init as init_android_ime, top_inset_points};
#[cfg(target_os = "android")]
pub use android_storage::{ensure_bluetooth_access, ensure_storage_access};

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
        // portable-pty has no Android backend; local shell is desktop-only.
        local_terminal: cfg!(any(windows, all(unix, not(target_os = "android")))),
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
