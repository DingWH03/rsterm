use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionType {
    Local,
    Ssh,
    Serial,
    Ble,
}

impl ConnectionType {
    pub fn label(&self) -> &str {
        match self {
            ConnectionType::Local => "Local Terminal",
            ConnectionType::Ssh => "SSH",
            ConnectionType::Serial => "Serial Port",
            ConnectionType::Ble => "BLE Serial",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            ConnectionType::Local => "💻",
            ConnectionType::Ssh => "🌐",
            ConnectionType::Serial => "🔌",
            ConnectionType::Ble => "📶",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    pub conn_type: ConnectionType,
    #[serde(default)]
    pub favorite: bool,
    pub last_connected: Option<String>,
    /// Local: shell path
    pub shell: Option<String>,
    /// Local: initial working directory
    #[serde(default)]
    pub working_dir: Option<String>,
    /// SSH
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_user: Option<String>,
    /// Optional password (stored locally in settings JSON).
    pub ssh_password: Option<String>,
    /// Serial
    pub serial_port: Option<String>,
    pub serial_baud: Option<u32>,
    /// BLE
    pub ble_device: Option<String>,
}

impl SavedConnection {
    pub fn new_local(name: &str, shell: Option<&str>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            conn_type: ConnectionType::Local,
            favorite: false,
            last_connected: None,
            shell: shell.map(|s| s.to_string()),
            working_dir: None,
            ssh_host: None,
            ssh_port: None,
            ssh_user: None,
            ssh_password: None,
            serial_port: None,
            serial_baud: None,
            ble_device: None,
        }
    }

    pub fn new_ssh(name: &str, host: &str, port: u16, user: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            conn_type: ConnectionType::Ssh,
            favorite: false,
            last_connected: None,
            shell: None,
            working_dir: None,
            ssh_host: Some(host.to_string()),
            ssh_port: Some(port),
            ssh_user: Some(user.to_string()),
            ssh_password: None,
            serial_port: None,
            serial_baud: None,
            ble_device: None,
        }
    }

    pub fn new_serial(name: &str, port: &str, baud: u32) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            conn_type: ConnectionType::Serial,
            favorite: false,
            last_connected: None,
            shell: None,
            working_dir: None,
            ssh_host: None,
            ssh_port: None,
            ssh_user: None,
            ssh_password: None,
            serial_port: Some(port.to_string()),
            serial_baud: Some(baud),
            ble_device: None,
        }
    }

    pub fn new_ble(name: &str, device: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            conn_type: ConnectionType::Ble,
            favorite: false,
            last_connected: None,
            shell: None,
            working_dir: None,
            ssh_host: None,
            ssh_port: None,
            ssh_user: None,
            ssh_password: None,
            serial_port: None,
            serial_baud: None,
            ble_device: Some(device.to_string()),
        }
    }
}
