//! Serial port enumeration per platform.

/// Port path and human-readable label for UI.
#[derive(Clone, Debug)]
pub struct SerialDevice {
    pub path: String,
    pub label: String,
}

pub fn enumerate_serial_ports() -> Vec<SerialDevice> {
    #[cfg(target_os = "android")]
    {
        return enumerate_android_dev_tty();
    }

    #[cfg(not(target_os = "android"))]
    enumerate_via_serialport()
}

/// Desktop / server: use the `serialport` crate (Windows COMx, Linux /dev, macOS /dev).
fn enumerate_via_serialport() -> Vec<SerialDevice> {
    let mut ports = match serialport::available_ports() {
        Ok(p) => p,
        Err(e) => {
            log::warn!("serialport::available_ports: {e}");
            return Vec::new();
        }
    };
    ports.sort_by_key(|p| p.port_name.clone());

    ports
        .into_iter()
        .map(|info| {
            let kind = serial_port_kind_label(&info.port_type);
            let label = format!("{} ({kind})", info.port_name);
            SerialDevice {
                path: info.port_name,
                label,
            }
        })
        .collect()
}

#[cfg(target_os = "android")]
fn enumerate_android_dev_tty() -> Vec<SerialDevice> {
    let mut found = Vec::new();
    let dev = std::path::Path::new("/dev");
    let Ok(entries) = std::fs::read_dir(dev) else {
        return found;
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if android_serial_name(&name) {
            let path = format!("/dev/{name}");
            let label = format!("{path} (USB serial)");
            found.push(SerialDevice { path, label });
        }
    }

    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

#[cfg(target_os = "android")]
fn android_serial_name(name: &str) -> bool {
    name.starts_with("ttyUSB")
        || name.starts_with("ttyACM")
        || name.starts_with("ttyGS")
        || name.starts_with("ttyS")
}

fn serial_port_kind_label(port_type: &serialport::SerialPortType) -> String {
    match port_type {
        serialport::SerialPortType::UsbPort(u) => {
            let product = u.product.as_deref().unwrap_or("");
            let mfr = u.manufacturer.as_deref().unwrap_or("");
            if !product.is_empty() {
                format!("USB · {product}")
            } else if !mfr.is_empty() {
                format!("USB · {mfr}")
            } else {
                "USB".to_string()
            }
        }
        serialport::SerialPortType::BluetoothPort => "Bluetooth".to_string(),
        serialport::SerialPortType::PciPort => "PCI".to_string(),
        serialport::SerialPortType::Unknown => {
            if cfg!(windows) {
                "Serial".to_string()
            } else {
                "Unknown".to_string()
            }
        }
    }
}
