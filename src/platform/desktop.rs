//! Desktop / non-Android platform implementation.
//! Supports local terminal, serial, BLE, and standard host identity.

use crate::platform::ble;
use crate::platform::process;
use crate::platform::serial::{self, SerialDevice};
use crate::platform::shell;
use crate::platform::Platform;

#[derive(Debug)]
pub struct DesktopPlatform;

impl Platform for DesktopPlatform {
    // ── Identity ────────────────────────────────────────────────────────
    fn local_user_at_host(&self) -> String {
        process::local_user_at_host()
    }

    fn ssh_user_at_host(&self, user: &str, host: &str) -> String {
        process::ssh_user_at_host(user, host)
    }

    fn title_is_idle_host(&self, title: &str, user_at_host: &str) -> bool {
        process::title_is_idle_host(title, user_at_host)
    }

    fn truncate_label(&self, s: &str, max_chars: usize) -> String {
        process::truncate_label(s, max_chars)
    }

    // ── Local terminal ──────────────────────────────────────────────────
    fn default_shell(&self) -> String {
        shell::default_shell()
    }

    fn foreground_command(&self, shell_pid: Option<u32>) -> Option<String> {
        process::foreground_command(shell_pid)
    }

    fn foreground_process_pid(&self, shell_pid: u32) -> Option<u32> {
        process::foreground_process_pid(shell_pid)
    }

    fn supports_local_terminal(&self) -> bool {
        true
    }

    // ── Device enumeration ──────────────────────────────────────────────
    fn enumerate_serial_ports(&self) -> Vec<SerialDevice> {
        serial::enumerate_serial_ports()
    }

    fn scan_ble_devices(&self) -> Result<Vec<String>, String> {
        ble::scan_ble_devices_blocking()
    }

    fn supports_ble(&self) -> bool {
        true
    }

    fn supports_serial(&self) -> bool {
        true
    }
}
