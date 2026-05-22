//! Device enumeration — delegates to the active [`Platform`] implementation.
//!
//! These are thin wrappers so callers don't need to import the platform
//! trait directly for simple device scans.

use crate::platform;

/// Enumerate available serial ports.
pub fn enumerate_serial_ports() -> Vec<platform::SerialDevice> {
    platform::get().enumerate_serial_ports()
}

/// Scan for nearby BLE devices (blocking, may take several seconds).
pub fn scan_ble_devices_blocking() -> Result<Vec<String>, String> {
    platform::get().scan_ble_devices()
}
