//! Re-export device enumeration from the platform layer.

pub use crate::platform::{enumerate_serial_ports, scan_ble_devices_blocking};
