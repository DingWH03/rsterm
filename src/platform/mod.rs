//! Cross-platform helpers (Linux, macOS, Windows, Android).

mod ble;
mod process;
mod serial;
mod shell;

pub use process::{foreground_command, local_user_at_host, ssh_user_at_host, title_is_idle_host, truncate_label};

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

/// Hint shown when no serial devices are found.
pub fn serial_empty_hint() -> &'static str {
    if cfg!(windows) {
        "未检测到 COM 口。请检查设备管理器中的端口 (COMx)，或手动输入 COM3 等。"
    } else if cfg!(target_os = "android") {
        "未检测到 USB 串口。请确认 OTG 已连接并授予 USB 权限，或手动输入 /dev/ttyUSB0 等。"
    } else {
        "未检测到串口。请检查 USB 连接与 dialout 权限，或手动输入 /dev/ttyUSB0 等。"
    }
}

/// Hint for manual serial path placeholder.
pub fn serial_manual_placeholder() -> &'static str {
    if cfg!(windows) {
        "COM3"
    } else if cfg!(target_os = "android") {
        "/dev/ttyUSB0"
    } else {
        "/dev/ttyUSB0"
    }
}
