use std::collections::BTreeSet;
use std::sync::mpsc;
use std::time::Duration;

use btleplug::api::{
    Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _, ScanFilter, Service,
    WriteType,
};
use btleplug::platform::Manager;
use futures::StreamExt;
use uuid::Uuid;

use crate::connection::{
    emit_conn_port_data, emit_conn_ports_changed, ConnIn, ConnOut, ConnectionHandle,
    ConnectionPort, ConnectionPortKind, ConnectionState, RepaintNotifier,
};
use crate::storage::types::SavedConnection;

// ---------------------------------------------------------------------------
// Built-in BLE terminal profiles
// ---------------------------------------------------------------------------

/// Nordic UART Service (NUS).
const NUS_SERVICE_UUID: &str = "6E400001-B5A3-F393-E0A9-E50E24DCCA9E";
/// NUS RX: central -> peripheral, write here.
const NUS_RX_UUID: &str = "6E400002-B5A3-F393-E0A9-E50E24DCCA9E";
/// NUS TX: peripheral -> central, subscribe here.
const NUS_TX_UUID: &str = "6E400003-B5A3-F393-E0A9-E50E24DCCA9E";

/// RSTerm example multi-UART service used by the ESP32-C3 sketch.
const RSTERM_MULTI_UART_SERVICE_UUID: &str = "B7E40001-B5A3-F393-E0A9-E50E24DCCA9E";
const RSTERM_UART0_TX_UUID: &str = "B7E40002-B5A3-F393-E0A9-E50E24DCCA9E";
const RSTERM_UART0_RX_UUID: &str = "B7E40003-B5A3-F393-E0A9-E50E24DCCA9E";
const RSTERM_UART1_TX_UUID: &str = "B7E40004-B5A3-F393-E0A9-E50E24DCCA9E";
const RSTERM_UART1_RX_UUID: &str = "B7E40005-B5A3-F393-E0A9-E50E24DCCA9E";

/// Optional future-proof mux profile: one TX/RX pair carries framed logical ports.
/// Frame format: port:u8, type:u8, len:u16 little-endian, payload[len].
const RSTERM_MUX_SERVICE_UUID: &str = "7A6E0001-B5A3-F393-E0A9-E50E24DCCA9E";
const RSTERM_MUX_RX_UUID: &str = "7A6E0002-B5A3-F393-E0A9-E50E24DCCA9E";
const RSTERM_MUX_TX_UUID: &str = "7A6E0003-B5A3-F393-E0A9-E50E24DCCA9E";
const MUX_FRAME_DATA: u8 = 0x01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BleProtocol {
    MultiCharacteristic,
    MuxFrames,
}

#[derive(Clone, Copy, Debug)]
struct KnownPortPair {
    port: u8,
    name: &'static str,
    kind: ConnectionPortKind,
    tx_uuid: &'static str,
    rx_uuid: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct KnownBleProfile {
    service_uuid: &'static str,
    protocol: BleProtocol,
    ports: &'static [KnownPortPair],
}

static RSTERM_MULTI_UART_PORTS: &[KnownPortPair] = &[
    KnownPortPair {
        port: 0,
        name: "UART0",
        kind: ConnectionPortKind::Serial,
        tx_uuid: RSTERM_UART0_TX_UUID,
        rx_uuid: RSTERM_UART0_RX_UUID,
    },
    KnownPortPair {
        port: 1,
        name: "UART1",
        kind: ConnectionPortKind::Serial,
        tx_uuid: RSTERM_UART1_TX_UUID,
        rx_uuid: RSTERM_UART1_RX_UUID,
    },
];

static RSTERM_MUX_PORTS: &[KnownPortPair] = &[KnownPortPair {
    port: 0,
    name: "MUX",
    kind: ConnectionPortKind::Terminal,
    tx_uuid: RSTERM_MUX_TX_UUID,
    rx_uuid: RSTERM_MUX_RX_UUID,
}];

static NUS_PORTS: &[KnownPortPair] = &[KnownPortPair {
    port: 0,
    name: "NUS",
    kind: ConnectionPortKind::Serial,
    tx_uuid: NUS_TX_UUID,
    rx_uuid: NUS_RX_UUID,
}];

static TI_PORTS: &[KnownPortPair] = &[KnownPortPair {
    port: 0,
    name: "TI UART",
    kind: ConnectionPortKind::Serial,
    tx_uuid: "FFF1",
    rx_uuid: "FFF2",
}];

static HM10_PORTS: &[KnownPortPair] = &[KnownPortPair {
    port: 0,
    name: "HM-10",
    kind: ConnectionPortKind::Serial,
    tx_uuid: "FFE1",
    rx_uuid: "FFE1",
}];

static ADAFRUIT_PORTS: &[KnownPortPair] = &[KnownPortPair {
    port: 0,
    name: "Bluefruit UART",
    kind: ConnectionPortKind::Serial,
    tx_uuid: "ADAFAF01-C464-4BDA-B3B2-2241514ADF3C",
    rx_uuid: "ADAFAF02-C464-4BDA-B3B2-2241514ADF3C",
}];

static DFB_PORTS: &[KnownPortPair] = &[KnownPortPair {
    port: 0,
    name: "DFB UART",
    kind: ConnectionPortKind::Serial,
    tx_uuid: "0000DFB1-0000-1000-8000-00805F9B34FB",
    rx_uuid: "0000DFB2-0000-1000-8000-00805F9B34FB",
}];

static KNOWN_BLE_PROFILES: &[KnownBleProfile] = &[
    KnownBleProfile {
        service_uuid: RSTERM_MULTI_UART_SERVICE_UUID,
        protocol: BleProtocol::MultiCharacteristic,
        ports: RSTERM_MULTI_UART_PORTS,
    },
    KnownBleProfile {
        service_uuid: RSTERM_MUX_SERVICE_UUID,
        protocol: BleProtocol::MuxFrames,
        ports: RSTERM_MUX_PORTS,
    },
    KnownBleProfile {
        service_uuid: NUS_SERVICE_UUID,
        protocol: BleProtocol::MultiCharacteristic,
        ports: NUS_PORTS,
    },
    KnownBleProfile {
        service_uuid: "FFF0",
        protocol: BleProtocol::MultiCharacteristic,
        ports: TI_PORTS,
    },
    KnownBleProfile {
        service_uuid: "FFE0",
        protocol: BleProtocol::MultiCharacteristic,
        ports: HM10_PORTS,
    },
    KnownBleProfile {
        service_uuid: "ADAFAF00-C464-4BDA-B3B2-2241514ADF3C",
        protocol: BleProtocol::MultiCharacteristic,
        ports: ADAFRUIT_PORTS,
    },
    KnownBleProfile {
        service_uuid: "0000DFB0-0000-1000-8000-00805F9B34FB",
        protocol: BleProtocol::MultiCharacteristic,
        ports: DFB_PORTS,
    },
];

#[derive(Clone, Debug)]
struct BlePort {
    info: ConnectionPort,
    tx: Characteristic,
    rx: Characteristic,
}

#[derive(Clone, Debug)]
struct BleDiscovery {
    service_uuid: Uuid,
    protocol: BleProtocol,
    ports: Vec<BlePort>,
}

// ---------------------------------------------------------------------------
// UUID / characteristic helpers
// ---------------------------------------------------------------------------

fn parse_ble_uuid(s: &str) -> Option<Uuid> {
    let s = s.trim();
    match s.len() {
        4 => Uuid::parse_str(&format!("0000{s}-0000-1000-8000-00805F9B34FB")).ok(),
        8 => Uuid::parse_str(&format!("{s}-0000-1000-8000-00805F9B34FB")).ok(),
        _ => Uuid::parse_str(s).ok(),
    }
}

fn is_notifiable(c: &Characteristic) -> bool {
    c.properties
        .intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE)
}

fn is_writable(c: &Characteristic) -> bool {
    c.properties
        .intersects(CharPropFlags::WRITE | CharPropFlags::WRITE_WITHOUT_RESPONSE)
}

fn prefer_notify_score(c: &Characteristic) -> u8 {
    if c.properties.contains(CharPropFlags::NOTIFY) {
        0
    } else if c.properties.contains(CharPropFlags::INDICATE) {
        1
    } else {
        2
    }
}

fn prefer_write_score(c: &Characteristic) -> u8 {
    if c.properties.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE) {
        0
    } else if c.properties.contains(CharPropFlags::WRITE) {
        1
    } else {
        2
    }
}

fn write_type_for(rx: &Characteristic) -> WriteType {
    if rx
        .properties
        .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE)
    {
        WriteType::WithoutResponse
    } else {
        WriteType::WithResponse
    }
}

fn port_for_notification_uuid(ports: &[BlePort], uuid: Uuid) -> Option<u8> {
    ports
        .iter()
        .find(|port| port.tx.uuid == uuid)
        .map(|port| port.info.port)
}

fn find_port(ports: &[BlePort], port: u8) -> Option<&BlePort> {
    ports.iter().find(|p| p.info.port == port)
}

fn advertised_ports(ports: &[BlePort]) -> Vec<ConnectionPort> {
    ports.iter().map(|p| p.info.clone()).collect()
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

fn build_from_known_profile(service: &Service, profile: KnownBleProfile) -> Option<BleDiscovery> {
    let mut ports = Vec::new();

    for pair in profile.ports {
        let Some(tx_uuid) = parse_ble_uuid(pair.tx_uuid) else {
            continue;
        };
        let Some(rx_uuid) = parse_ble_uuid(pair.rx_uuid) else {
            continue;
        };

        let tx = service
            .characteristics
            .iter()
            .find(|c| c.uuid == tx_uuid && is_notifiable(c))
            .cloned();
        let rx = service
            .characteristics
            .iter()
            .find(|c| c.uuid == rx_uuid && is_writable(c))
            .cloned();

        if let (Some(tx), Some(rx)) = (tx, rx) {
            ports.push(BlePort {
                info: ConnectionPort {
                    port: pair.port,
                    name: pair.name.to_string(),
                    kind: pair.kind,
                    read_only: false,
                    write_only: false,
                },
                tx,
                rx,
            });
        }
    }

    if ports.is_empty() {
        None
    } else {
        Some(BleDiscovery {
            service_uuid: service.uuid,
            protocol: profile.protocol,
            ports,
        })
    }
}

fn build_from_properties(service: &Service, max_ports: usize) -> Option<BleDiscovery> {
    let mut tx_candidates: Vec<Characteristic> = service
        .characteristics
        .iter()
        .filter(|c| is_notifiable(c))
        .cloned()
        .collect();
    let mut rx_candidates: Vec<Characteristic> = service
        .characteristics
        .iter()
        .filter(|c| is_writable(c))
        .cloned()
        .collect();

    if tx_candidates.is_empty() || rx_candidates.is_empty() {
        return None;
    }

    tx_candidates.sort_by_key(|c| (prefer_notify_score(c), c.uuid.as_u128()));
    rx_candidates.sort_by_key(|c| (prefer_write_score(c), c.uuid.as_u128()));

    let mut used_tx = BTreeSet::new();
    let mut used_rx = BTreeSet::new();
    let mut ports = Vec::new();

    for tx in tx_candidates.iter() {
        if ports.len() >= max_ports {
            break;
        }
        if used_tx.contains(&tx.uuid) {
            continue;
        }

        let rx = rx_candidates
            .iter()
            .find(|rx| !used_rx.contains(&rx.uuid) && rx.uuid != tx.uuid)
            .or_else(|| {
                rx_candidates
                    .iter()
                    .find(|rx| !used_rx.contains(&rx.uuid) && rx.uuid == tx.uuid)
            });

        let Some(rx) = rx else {
            continue;
        };

        used_tx.insert(tx.uuid);
        used_rx.insert(rx.uuid);

        let port = ports.len() as u8;
        ports.push(BlePort {
            info: ConnectionPort::serial(port, format!("BLE port {port}")),
            tx: tx.clone(),
            rx: rx.clone(),
        });
    }

    if ports.is_empty() {
        None
    } else {
        Some(BleDiscovery {
            service_uuid: service.uuid,
            protocol: BleProtocol::MultiCharacteristic,
            ports,
        })
    }
}

fn discover_uart(services: &BTreeSet<Service>) -> Option<BleDiscovery> {
    for profile in KNOWN_BLE_PROFILES {
        let Some(service_uuid) = parse_ble_uuid(profile.service_uuid) else {
            continue;
        };
        let Some(service) = services.iter().find(|s| s.uuid == service_uuid) else {
            continue;
        };

        if let Some(discovery) = build_from_known_profile(service, *profile) {
            return Some(discovery);
        }

        if profile.protocol == BleProtocol::MultiCharacteristic {
            if let Some(discovery) = build_from_properties(service, 8) {
                return Some(discovery);
            }
        }
    }

    let mut candidates = Vec::new();
    for service in services {
        if let Some(discovery) = build_from_properties(service, 8) {
            candidates.push(discovery);
        }
    }

    candidates.sort_by_key(|d| {
        let first = &d.ports[0];
        (
            d.ports.len() == 1,
            prefer_write_score(&first.rx),
            prefer_notify_score(&first.tx),
            d.service_uuid.as_u128(),
        )
    });

    candidates.into_iter().next()
}

// ---------------------------------------------------------------------------
// Optional RSTerm mux frame support
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MuxDecoder {
    buffer: Vec<u8>,
}

impl MuxDecoder {
    fn push(&mut self, bytes: &[u8]) -> Vec<(u8, u8, Vec<u8>)> {
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();

        loop {
            if self.buffer.len() < 4 {
                break;
            }
            let port = self.buffer[0];
            let frame_type = self.buffer[1];
            let len = u16::from_le_bytes([self.buffer[2], self.buffer[3]]) as usize;
            if self.buffer.len() < 4 + len {
                break;
            }
            let payload = self.buffer[4..4 + len].to_vec();
            self.buffer.drain(0..4 + len);
            frames.push((port, frame_type, payload));
        }

        frames
    }
}

fn encode_mux_frame(port: u8, frame_type: u8, payload: &[u8]) -> Vec<u8> {
    let len = payload.len().min(u16::MAX as usize);
    let mut out = Vec::with_capacity(4 + len);
    out.push(port);
    out.push(frame_type);
    out.extend_from_slice(&(len as u16).to_le_bytes());
    out.extend_from_slice(&payload[..len]);
    out
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

async fn find_peripheral_by_name(
    peripherals: &[btleplug::platform::Peripheral],
    target_name: &str,
) -> Option<btleplug::platform::Peripheral> {
    for p in peripherals {
        if let Ok(Some(props)) = p.properties().await {
            if let Some(ref name) = props.local_name {
                if name == target_name || name.contains(target_name) {
                    return Some(p.clone());
                }
            }
        }
    }
    None
}

async fn close_ble_connection(peripheral: &btleplug::platform::Peripheral, ports: &[BlePort]) {
    let mut unsubscribed = BTreeSet::new();
    for port in ports {
        if unsubscribed.insert(port.tx.uuid) {
            let _ = peripheral.unsubscribe(&port.tx).await;
        }
    }
    let _ = peripheral.disconnect().await;
}

async fn write_raw_port(
    peripheral: &btleplug::platform::Peripheral,
    port: &BlePort,
    data: &[u8],
    max_payload_len: usize,
) -> Result<(), String> {
    let write_type = write_type_for(&port.rx);
    for chunk in data.chunks(max_payload_len) {
        peripheral
            .write(&port.rx, chunk, write_type)
            .await
            .map_err(|e| e.to_string())?;
        if write_type == WriteType::WithoutResponse {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    Ok(())
}

async fn write_mux_port(
    peripheral: &btleplug::platform::Peripheral,
    mux_port: &BlePort,
    logical_port: u8,
    data: &[u8],
    max_payload_len: usize,
) -> Result<(), String> {
    let payload_len = max_payload_len.saturating_sub(4).max(1);
    for chunk in data.chunks(payload_len) {
        let frame = encode_mux_frame(logical_port, MUX_FRAME_DATA, chunk);
        write_raw_port(peripheral, mux_port, &frame, max_payload_len).await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public entry — connect BLE terminal transport
// ---------------------------------------------------------------------------

pub fn connect_ble(config: &SavedConnection) -> Result<ConnectionHandle, String> {
    let device_name = config
        .ble_device
        .clone()
        .ok_or_else(|| "BLE device not configured".to_string())?;

    let (to_conn_tx, to_conn_rx) = mpsc::channel::<ConnOut>();
    let (from_conn_tx, from_conn_rx) = mpsc::channel::<ConnIn>();

    let from_tx = from_conn_tx.clone();
    let repaint = RepaintNotifier::default();
    let repaint_reader = repaint.clone();

    let reader_thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        rt.block_on(async move {
            #[cfg(target_os = "android")]
            if let Err(e) = crate::platform::ensure_android_btleplug_initialized() {
                let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(e)));
                return;
            }

            let manager = match Manager::new().await {
                Ok(m) => m,
                Err(e) => {
                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(e.to_string())));
                    return;
                }
            };

            let adapters = match manager.adapters().await {
                Ok(a) => a,
                Err(e) => {
                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(e.to_string())));
                    return;
                }
            };

            let adapter = match adapters.into_iter().next() {
                Some(a) => a,
                None => {
                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(
                        "No BLE adapter found".to_string(),
                    )));
                    return;
                }
            };

            let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Connecting));

            if let Err(e) = adapter.start_scan(ScanFilter::default()).await {
                let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(e.to_string())));
                return;
            }

            tokio::time::sleep(Duration::from_secs(4)).await;

            let peripherals = match adapter.peripherals().await {
                Ok(p) => p,
                Err(e) => {
                    let _ = adapter.stop_scan().await;
                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(e.to_string())));
                    return;
                }
            };

            let _ = adapter.stop_scan().await;

            let peripheral = match find_peripheral_by_name(&peripherals, &device_name).await {
                Some(p) => p,
                None => {
                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(format!(
                        "BLE device '{device_name}' not found. Ensure it is powered on and advertising."
                    ))));
                    return;
                }
            };

            if let Err(e) = peripheral.connect().await {
                let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(format!(
                    "Failed to connect to '{device_name}': {e}"
                ))));
                return;
            }

            if let Err(e) = peripheral.discover_services().await {
                let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(e.to_string())));
                let _ = peripheral.disconnect().await;
                return;
            }

            let services = peripheral.services();
            let discovery = match discover_uart(&services) {
                Some(d) => d,
                None => {
                    let msg = "Could not find a suitable BLE terminal/UART service on this device.\n\
                               A compatible device must expose Notify/Indicate + Write characteristics,\n\
                               or the RSTerm BLE MUX profile."
                        .to_string();
                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(msg)));
                    let _ = peripheral.disconnect().await;
                    return;
                }
            };

            log::info!(
                "BLE transport discovered: service={}, protocol={:?}, ports={}",
                discovery.service_uuid,
                discovery.protocol,
                discovery.ports.len(),
            );
            for port in &discovery.ports {
                log::info!(
                    "BLE port {}: {} tx={} rx={} write={:?}",
                    port.info.port,
                    port.info.name,
                    port.tx.uuid,
                    port.rx.uuid,
                    write_type_for(&port.rx),
                );
            }

            let mut notifications = match peripheral.notifications().await {
                Ok(s) => s,
                Err(e) => {
                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(e.to_string())));
                    let _ = peripheral.disconnect().await;
                    return;
                }
            };

            for port in &discovery.ports {
                if let Err(e) = peripheral.subscribe(&port.tx).await {
                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(format!(
                        "Failed to subscribe to BLE port {} notifications: {e}",
                        port.info.port,
                    ))));
                    let _ = peripheral.disconnect().await;
                    return;
                }
            }

            emit_conn_ports_changed(&from_tx, &repaint_reader, advertised_ports(&discovery.ports));
            let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Connected));

            let (internal_write_tx, mut internal_write_rx) =
                tokio::sync::mpsc::channel::<ConnOut>(256);
            let write_tx_for_task = internal_write_tx.clone();
            tokio::task::spawn_blocking(move || {
                while let Ok(msg) = to_conn_rx.recv() {
                    if write_tx_for_task.blocking_send(msg).is_err() {
                        break;
                    }
                }
            });
            drop(internal_write_tx);

            let protocol = discovery.protocol;
            let ports = discovery.ports.clone();
            let max_payload_len = peripheral.mtu().saturating_sub(3).max(20) as usize;
            let mut mux_decoder = MuxDecoder::default();

            loop {
                tokio::select! {
                    notification = notifications.next() => {
                        let Some(notification) = notification else {
                            let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Lost(
                                "BLE notification stream ended".to_string(),
                            )));
                            let _ = peripheral.disconnect().await;
                            return;
                        };

                        match protocol {
                            BleProtocol::MultiCharacteristic => {
                                if let Some(port) = port_for_notification_uuid(&ports, notification.uuid) {
                                    emit_conn_port_data(
                                        &from_tx,
                                        &repaint_reader,
                                        port,
                                        notification.value,
                                    );
                                }
                            }
                            BleProtocol::MuxFrames => {
                                for (port, frame_type, payload) in mux_decoder.push(&notification.value) {
                                    if frame_type == MUX_FRAME_DATA {
                                        emit_conn_port_data(
                                            &from_tx,
                                            &repaint_reader,
                                            port,
                                            payload,
                                        );
                                    }
                                }
                            }
                        }
                    }

                    msg = internal_write_rx.recv() => {
                        match msg {
                            Some(ConnOut::Data(data)) => {
                                let port = find_port(&ports, 0);
                                let Some(port0) = port else {
                                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(
                                        "BLE port 0 is not available".to_string(),
                                    )));
                                    continue;
                                };

                                let result = match protocol {
                                    BleProtocol::MultiCharacteristic => {
                                        write_raw_port(&peripheral, port0, &data, max_payload_len).await
                                    }
                                    BleProtocol::MuxFrames => {
                                        write_mux_port(&peripheral, port0, 0, &data, max_payload_len).await
                                    }
                                };

                                if let Err(e) = result {
                                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Lost(e)));
                                    let _ = peripheral.disconnect().await;
                                    return;
                                }
                            }

                            Some(ConnOut::PortData { port, data }) => {
                                let result = match protocol {
                                    BleProtocol::MultiCharacteristic => {
                                        let Some(uart_port) = find_port(&ports, port) else {
                                            let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(format!(
                                                "BLE port {port} is not available"
                                            ))));
                                            continue;
                                        };
                                        write_raw_port(&peripheral, uart_port, &data, max_payload_len).await
                                    }
                                    BleProtocol::MuxFrames => {
                                        let Some(mux_port) = find_port(&ports, 0) else {
                                            let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(
                                                "BLE MUX write port is not available".to_string(),
                                            )));
                                            continue;
                                        };
                                        write_mux_port(&peripheral, mux_port, port, &data, max_payload_len).await
                                    }
                                };

                                if let Err(e) = result {
                                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Lost(e)));
                                    let _ = peripheral.disconnect().await;
                                    return;
                                }
                            }

                            Some(ConnOut::Close) | None => {
                                close_ble_connection(&peripheral, &ports).await;
                                let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Closed));
                                return;
                            }

                            Some(ConnOut::Resize(_, _)) | Some(ConnOut::Winch) => {
                                // BLE terminal transports do not have a PTY window size by default.
                            }
                        }
                    }
                }
            }
        });
    });

    let writer_thread = std::thread::spawn(|| {});

    Ok(ConnectionHandle::new(
        to_conn_tx,
        from_conn_rx,
        reader_thread,
        writer_thread,
        repaint,
    ))
}
