use std::sync::mpsc;

use crate::platform::{self, Capabilities};
use crate::storage::types::{ConnectionType, SavedConnection};
use crate::ui::devices::{enumerate_serial_ports, scan_ble_devices_blocking};

pub struct NewConnectionDialog {
    pub open: bool,
    pub name: String,
    pub conn_type: ConnectionType,
    // Local
    pub shell: String,
    // SSH
    pub ssh_host: String,
    pub ssh_port: String,
    pub ssh_user: String,
    pub ssh_password: String,
    // Serial
    pub serial_port: String,
    pub serial_baud: String,
    serial_devices: Vec<(String, String)>,
    // BLE
    pub ble_device: String,
    ble_devices: Vec<String>,
    ble_scanning: bool,
    ble_scan_rx: Option<mpsc::Receiver<Result<Vec<String>, String>>>,
}

impl Default for NewConnectionDialog {
    fn default() -> Self {
        Self {
            open: false,
            name: String::new(),
            conn_type: ConnectionType::Local,
            shell: String::new(),
            ssh_host: String::new(),
            ssh_port: "22".to_string(),
            ssh_user: String::new(),
            ssh_password: String::new(),
            serial_port: String::new(),
            serial_baud: "115200".to_string(),
            serial_devices: Vec::new(),
            ble_device: String::new(),
            ble_devices: Vec::new(),
            ble_scanning: false,
            ble_scan_rx: None,
        }
    }
}

impl NewConnectionDialog {
    fn available_types(caps: &Capabilities) -> Vec<ConnectionType> {
        let mut types = Vec::new();
        if caps.local_terminal {
            types.push(ConnectionType::Local);
        }
        if caps.ssh {
            types.push(ConnectionType::Ssh);
        }
        if caps.serial {
            types.push(ConnectionType::Serial);
        }
        if caps.ble {
            types.push(ConnectionType::Ble);
        }
        types
    }

    fn ensure_conn_type_supported(&mut self) {
        let types = Self::available_types(&platform::capabilities());
        if types.is_empty() {
            return;
        }
        if !types.contains(&self.conn_type) {
            self.conn_type = types[0].clone();
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<SavedConnection> {
        if !self.open {
            return None;
        }

        self.poll_ble_scan();
        self.ensure_conn_type_supported();
        if self.conn_type == ConnectionType::Serial && self.serial_devices.is_empty() {
            self.refresh_serial_devices();
        }

        let caps = platform::capabilities();
        let mut result = None;
        let mut close = false;

        egui::Window::new("New Connection")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.name);
                });

                ui.horizontal(|ui| {
                    ui.label("Type:");
                    for ct in Self::available_types(&caps) {
                        let selected = self.conn_type == ct;
                        let mut text = egui::RichText::new(ct.label());
                        if selected {
                            text = text.strong().color(egui::Color32::from_rgb(33, 150, 243));
                        }
                        if ui.selectable_label(selected, text).clicked() {
                            if self.conn_type != ct {
                                self.conn_type = ct.clone();
                                if ct == ConnectionType::Serial {
                                    self.refresh_serial_devices();
                                }
                            }
                        }
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(4.0);

                match self.conn_type {
                    ConnectionType::Local => {
                        ui.horizontal(|ui| {
                            ui.label("Shell (optional):");
                            ui.text_edit_singleline(&mut self.shell);
                        });
                    }
                    ConnectionType::Ssh => {
                        ui.horizontal(|ui| {
                            ui.label("Host:");
                            ui.text_edit_singleline(&mut self.ssh_host);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Port:");
                            ui.text_edit_singleline(&mut self.ssh_port);
                        });
                        ui.horizontal(|ui| {
                            ui.label("User:");
                            ui.text_edit_singleline(&mut self.ssh_user);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Password:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.ssh_password)
                                    .password(true),
                            );
                        });
                        ui.label(
                            egui::RichText::new(
                                "留空则使用 ~/.ssh 密钥，或环境变量 SSH_PASSWORD",
                            )
                            .small()
                            .weak(),
                        );
                    }
                    ConnectionType::Serial => {
                        ui.horizontal(|ui| {
                            if ui.button("刷新设备").clicked() {
                                self.refresh_serial_devices();
                            }
                        });
                        if self.serial_devices.is_empty() {
                            ui.label(egui::RichText::new(platform::serial_empty_hint()).weak());
                            ui.horizontal(|ui| {
                                ui.label("Device:");
                                ui.text_edit_singleline(&mut self.serial_port);
                            });
                        } else {
                            let selected_text = self
                                .serial_devices
                                .iter()
                                .find(|(path, _)| path == &self.serial_port)
                                .map(|(_, label)| label.as_str())
                                .unwrap_or(self.serial_port.as_str());
                            egui::ComboBox::from_label("Device")
                                .selected_text(selected_text)
                                .show_ui(ui, |ui| {
                                    for (path, label) in &self.serial_devices {
                                        ui.selectable_value(
                                            &mut self.serial_port,
                                            path.clone(),
                                            label,
                                        );
                                    }
                                });
                        }
                        ui.horizontal(|ui| {
                            ui.label("Baud rate:");
                            ui.text_edit_singleline(&mut self.serial_baud);
                        });
                        ui.horizontal(|ui| {
                            ui.label("或手动路径:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.serial_port)
                                    .hint_text(platform::serial_manual_placeholder()),
                            );
                        });
                    }
                    ConnectionType::Ble => {
                        ui.horizontal(|ui| {
                            let scan_label = if self.ble_scanning {
                                "扫描中…"
                            } else {
                                "扫描设备"
                            };
                            if ui
                                .add_enabled(!self.ble_scanning, egui::Button::new(scan_label))
                                .clicked()
                            {
                                self.start_ble_scan(ctx);
                            }
                        });
                        if self.ble_devices.is_empty() && !self.ble_scanning {
                            ui.label(
                                egui::RichText::new("点击「扫描设备」查找附近 BLE 设备")
                                    .weak(),
                            );
                            ui.horizontal(|ui| {
                                ui.label("设备名:");
                                ui.text_edit_singleline(&mut self.ble_device);
                            });
                        } else if !self.ble_devices.is_empty() {
                            let selected = if self.ble_device.is_empty() {
                                self.ble_devices[0].as_str()
                            } else {
                                self.ble_device.as_str()
                            };
                            egui::ComboBox::from_label("Device")
                                .selected_text(selected)
                                .show_ui(ui, |ui| {
                                    for name in &self.ble_devices {
                                        ui.selectable_value(
                                            &mut self.ble_device,
                                            name.clone(),
                                            name,
                                        );
                                    }
                                });
                        }
                    }
                }

                ui.add_space(16.0);

                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }

                    let can_create = !self.name.trim().is_empty()
                        && match self.conn_type {
                            ConnectionType::Ssh => !self.ssh_host.trim().is_empty(),
                            ConnectionType::Serial => !self.serial_port.trim().is_empty(),
                            ConnectionType::Ble => !self.ble_device.trim().is_empty(),
                            ConnectionType::Local => true,
                        };

                    if ui
                        .add_enabled(can_create, egui::Button::new("Create"))
                        .clicked()
                    {
                        let conn = match &self.conn_type {
                            ConnectionType::Local => {
                                let shell = if self.shell.is_empty() {
                                    None
                                } else {
                                    Some(self.shell.as_str())
                                };
                                SavedConnection::new_local(&self.name, shell)
                            }
                            ConnectionType::Ssh => {
                                let mut c = SavedConnection::new_ssh(
                                    &self.name,
                                    &self.ssh_host,
                                    self.ssh_port.parse().unwrap_or(22),
                                    &self.ssh_user,
                                );
                                if !self.ssh_password.is_empty() {
                                    c.ssh_password = Some(self.ssh_password.clone());
                                }
                                c
                            }
                            ConnectionType::Serial => SavedConnection::new_serial(
                                &self.name,
                                &self.serial_port,
                                self.serial_baud.parse().unwrap_or(115200),
                            ),
                            ConnectionType::Ble => {
                                SavedConnection::new_ble(&self.name, &self.ble_device)
                            }
                        };
                        result = Some(conn);
                        close = true;
                    }
                });
            });

        if close {
            self.open = false;
            *self = Self::default();
        }

        result
    }

    fn refresh_serial_devices(&mut self) {
        self.serial_devices = enumerate_serial_ports()
            .into_iter()
            .map(|d| (d.path, d.label))
            .collect();
        if self.serial_port.is_empty() {
            if let Some((path, _)) = self.serial_devices.first() {
                self.serial_port = path.clone();
            }
        }
    }

    fn start_ble_scan(&mut self, ctx: &egui::Context) {
        if self.ble_scanning {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.ble_scan_rx = Some(rx);
        self.ble_scanning = true;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = scan_ble_devices_blocking();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn poll_ble_scan(&mut self) {
        let Some(rx) = self.ble_scan_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(devices)) => {
                self.ble_scanning = false;
                self.ble_devices = devices;
                if self.ble_device.is_empty() {
                    if let Some(first) = self.ble_devices.first() {
                        self.ble_device = first.clone();
                    }
                }
            }
            Ok(Err(e)) => {
                self.ble_scanning = false;
                log::warn!("BLE scan failed: {e}");
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.ble_scan_rx = Some(rx);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.ble_scanning = false;
            }
        }
    }
}
