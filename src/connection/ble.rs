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
    emit_conn_data, ConnIn, ConnOut, ConnectionHandle, ConnectionState, RepaintNotifier,
};
use crate::storage::types::SavedConnection;

// ---------------------------------------------------------------------------
// 已知的 BLE UART 服务配置清单
// ---------------------------------------------------------------------------

/// Nordic UART Service (NUS)
const NUS_SERVICE_UUID: &str = "6E400001-B5A3-F393-E0A9-E50E24DCCA9E";

/// NUS RX: central -> peripheral，也就是本程序写入的特征
const NUS_RX_UUID: &str = "6E400002-B5A3-F393-E0A9-E50E24DCCA9E";

/// NUS TX: peripheral -> central，也就是本程序订阅通知的特征
const NUS_TX_UUID: &str = "6E400003-B5A3-F393-E0A9-E50E24DCCA9E";

#[derive(Clone, Debug)]
struct KnownUartService {
    service_uuid: &'static str,
    characteristic_uuids: &'static [&'static str],
}

/// 已知的 UART 服务列表。
///
/// 注意：这里不强行假设 characteristic 的方向，而是用属性判断：
/// - NOTIFY / INDICATE => 设备到电脑，作为 TX
/// - WRITE / WRITE_WITHOUT_RESPONSE => 电脑到设备，作为 RX
const KNOWN_UART_SERVICES: &[KnownUartService] = &[
    // 1) Nordic UART Service
    KnownUartService {
        service_uuid: NUS_SERVICE_UUID,
        characteristic_uuids: &[NUS_RX_UUID, NUS_TX_UUID],
    },
    // 2) Texas Instruments / SimpleLink 风格
    KnownUartService {
        service_uuid: "FFF0",
        characteristic_uuids: &["FFF1", "FFF2"],
    },
    // 3) HM-10 / JDY 等常见单特征串口
    KnownUartService {
        service_uuid: "FFE0",
        characteristic_uuids: &["FFE1"],
    },
    // 4) Adafruit Bluefruit LE
    KnownUartService {
        service_uuid: "ADAFAF00-C464-4BDA-B3B2-2241514ADF3C",
        characteristic_uuids: &[
            "ADAFAF01-C464-4BDA-B3B2-2241514ADF3C",
            "ADAFAF02-C464-4BDA-B3B2-2241514ADF3C",
        ],
    },
    // 5) Serial Bluetooth Terminal 常见 DFBx 服务
    KnownUartService {
        service_uuid: "0000DFB0-0000-1000-8000-00805F9B34FB",
        characteristic_uuids: &[
            "0000DFB1-0000-1000-8000-00805F9B34FB",
            "0000DFB2-0000-1000-8000-00805F9B34FB",
        ],
    },
];

/// 自动发现的 UART 特征集合
#[derive(Clone, Debug)]
struct UartChars {
    service_uuid: Uuid,

    /// 设备 -> 本程序，Notify / Indicate
    tx: Characteristic,

    /// 本程序 -> 设备，Write / WriteWithoutResponse
    rx: Characteristic,

    /// 可选的第二组通知特征
    tx2: Option<Characteristic>,
}

// ---------------------------------------------------------------------------
// 通用发现逻辑
// ---------------------------------------------------------------------------

fn parse_ble_uuid(s: &str) -> Option<Uuid> {
    let s = s.trim();

    match s.len() {
        // 16-bit UUID，例如 FFF0 => 0000FFF0-0000-1000-8000-00805F9B34FB
        4 => Uuid::parse_str(&format!(
            "0000{s}-0000-1000-8000-00805F9B34FB"
        ))
        .ok(),

        // 32-bit UUID，例如 12345678 => 12345678-0000-1000-8000-00805F9B34FB
        8 => Uuid::parse_str(&format!(
            "{s}-0000-1000-8000-00805F9B34FB"
        ))
        .ok(),

        // 完整 UUID
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

/// 从一组 characteristic 中推断 UART 的 TX/RX。
///
/// 优先选择：
/// - TX: NOTIFY 优先于 INDICATE
/// - RX: WRITE_WITHOUT_RESPONSE 优先于 WRITE
/// - 如果存在独立的 TX/RX 特征，优先使用两个不同特征
/// - 如果只有一个同时支持 Notify + Write 的特征，也允许复用同一个特征
fn build_uart_from_chars(service_uuid: Uuid, chars: &[Characteristic]) -> Option<UartChars> {
    let mut tx_candidates: Vec<Characteristic> = chars
        .iter()
        .filter(|c| is_notifiable(*c))
        .cloned()
        .collect();

    let mut rx_candidates: Vec<Characteristic> = chars
        .iter()
        .filter(|c| is_writable(*c))
        .cloned()
        .collect();

    if tx_candidates.is_empty() || rx_candidates.is_empty() {
        return None;
    }

    tx_candidates.sort_by_key(prefer_notify_score);
    rx_candidates.sort_by_key(prefer_write_score);

    // 优先找两个不同的 characteristic
    for tx in &tx_candidates {
        for rx in &rx_candidates {
            if tx.uuid != rx.uuid {
                let tx2 = tx_candidates
                    .iter()
                    .find(|other| other.uuid != tx.uuid)
                    .cloned();

                return Some(UartChars {
                    service_uuid,
                    tx: tx.clone(),
                    rx: rx.clone(),
                    tx2,
                });
            }
        }
    }

    // 如果只有一个同时支持 Notify + Write 的 characteristic，也允许使用
    let tx = tx_candidates[0].clone();
    let rx = rx_candidates[0].clone();

    Some(UartChars {
        service_uuid,
        tx,
        rx,
        tx2: None,
    })
}

/// 尝试按已知 UUID 表匹配 UART 服务。
fn match_known_service(services: &BTreeSet<Service>) -> Option<UartChars> {
    for known in KNOWN_UART_SERVICES {
        let Some(service_uuid) = parse_ble_uuid(known.service_uuid) else {
            continue;
        };

        let Some(service) = services.iter().find(|s| s.uuid == service_uuid) else {
            continue;
        };

        let known_char_uuids: Vec<Uuid> = known
            .characteristic_uuids
            .iter()
            .filter_map(|s| parse_ble_uuid(s))
            .collect();

        let known_chars: Vec<Characteristic> = service
            .characteristics
            .iter()
            .filter(|c| known_char_uuids.contains(&c.uuid))
            .cloned()
            .collect();

        // 先只用已知 characteristic UUID 匹配
        if let Some(uart) = build_uart_from_chars(service.uuid, &known_chars) {
            return Some(uart);
        }

        // 如果服务 UUID 是已知的，但 characteristic UUID 表不完整，
        // 则在该服务内按属性自动猜测一次。
        let all_chars: Vec<Characteristic> =
            service.characteristics.iter().cloned().collect();

        if let Some(uart) = build_uart_from_chars(service.uuid, &all_chars) {
            return Some(uart);
        }
    }

    None
}

/// 遍历所有服务，自动猜测哪一组特征是 UART 串口。
fn auto_discover_uart(services: &BTreeSet<Service>) -> Option<UartChars> {
    let mut candidates: Vec<UartChars> = Vec::new();

    for service in services {
        let chars: Vec<Characteristic> = service.characteristics.iter().cloned().collect();

        if let Some(uart) = build_uart_from_chars(service.uuid, &chars) {
            candidates.push(uart);
        }
    }

    candidates.sort_by_key(|c| {
        (
            prefer_write_score(&c.rx),
            prefer_notify_score(&c.tx),
        )
    });

    candidates.into_iter().next()
}

/// 综合入口：先查已知表，失败则自动发现。
fn discover_uart(services: &BTreeSet<Service>) -> Option<UartChars> {
    match_known_service(services).or_else(|| auto_discover_uart(services))
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
                if name == target_name {
                    return Some(p.clone());
                }
            }
        }
    }

    None
}

async fn close_ble_connection(
    peripheral: &btleplug::platform::Peripheral,
    uart_tx: &Characteristic,
    uart_tx2: &Option<Characteristic>,
) {
    let _ = peripheral.unsubscribe(uart_tx).await;

    if let Some(tx2) = uart_tx2 {
        let _ = peripheral.unsubscribe(tx2).await;
    }

    let _ = peripheral.disconnect().await;
}

// ---------------------------------------------------------------------------
// 公共入口 — 建立 BLE 串口连接
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

    // ---- 主 I/O 线程：运行 Tokio runtime，同时处理读写 ----
    let reader_thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        rt.block_on(async move {
            // 1. Manager & Adapter -------------------------------------------
            let manager = match Manager::new().await {
                Ok(m) => m,
                Err(e) => {
                    let _ = from_tx.send(ConnIn::StateChanged(
                        ConnectionState::Error(e.to_string()),
                    ));
                    return;
                }
            };

            let adapters = match manager.adapters().await {
                Ok(a) => a,
                Err(e) => {
                    let _ = from_tx.send(ConnIn::StateChanged(
                        ConnectionState::Error(e.to_string()),
                    ));
                    return;
                }
            };

            let adapter = match adapters.into_iter().next() {
                Some(a) => a,
                None => {
                    let _ = from_tx.send(ConnIn::StateChanged(
                        ConnectionState::Error("No BLE adapter found".to_string()),
                    ));
                    return;
                }
            };

            let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Connecting));

            // 2. 扫描 -------------------------------------------------------
            if let Err(e) = adapter.start_scan(ScanFilter::default()).await {
                let _ = from_tx.send(ConnIn::StateChanged(
                    ConnectionState::Error(e.to_string()),
                ));
                return;
            }

            tokio::time::sleep(Duration::from_secs(4)).await;

            let peripherals = match adapter.peripherals().await {
                Ok(p) => p,
                Err(e) => {
                    let _ = adapter.stop_scan().await;
                    let _ = from_tx.send(ConnIn::StateChanged(
                        ConnectionState::Error(e.to_string()),
                    ));
                    return;
                }
            };

            let _ = adapter.stop_scan().await;

            // 3. 按名称查找目标设备 ------------------------------------------
            let peripheral = match find_peripheral_by_name(&peripherals, &device_name).await {
                Some(p) => p,
                None => {
                    let _ = from_tx.send(ConnIn::StateChanged(
                        ConnectionState::Error(format!(
                            "BLE device '{device_name}' not found. \
                             Ensure it is powered on and advertising."
                        )),
                    ));
                    return;
                }
            };

            // 4. 连接 -------------------------------------------------------
            if let Err(e) = peripheral.connect().await {
                let _ = from_tx.send(ConnIn::StateChanged(
                    ConnectionState::Error(format!(
                        "Failed to connect to '{device_name}': {e}"
                    )),
                ));
                return;
            }

            // 5. 发现服务 ---------------------------------------------------
            if let Err(e) = peripheral.discover_services().await {
                let _ = from_tx.send(ConnIn::StateChanged(
                    ConnectionState::Error(e.to_string()),
                ));
                let _ = peripheral.disconnect().await;
                return;
            }

            let services = peripheral.services();

            let uart = match discover_uart(&services) {
                Some(u) => u,
                None => {
                    let msg = "Could not find a suitable UART service on this device.\n\
                               The device must expose at least one characteristic with \
                               NOTIFY/INDICATE and one with WRITE/WRITE_WITHOUT_RESPONSE."
                        .to_string();

                    let _ = from_tx.send(ConnIn::StateChanged(
                        ConnectionState::Error(msg),
                    ));
                    let _ = peripheral.disconnect().await;
                    return;
                }
            };

            log::info!(
                "BLE UART discovered: service={}, tx={}, rx={}, tx2={:?}",
                uart.service_uuid,
                uart.tx.uuid,
                uart.rx.uuid,
                uart.tx2.as_ref().map(|c| c.uuid),
            );

            // 6. 先获取通知流，再订阅通知，减少漏首包风险 -----------------------
            let mut notifications = match peripheral.notifications().await {
                Ok(s) => s,
                Err(e) => {
                    let _ = from_tx.send(ConnIn::StateChanged(
                        ConnectionState::Error(e.to_string()),
                    ));
                    let _ = peripheral.disconnect().await;
                    return;
                }
            };

            if let Err(e) = peripheral.subscribe(&uart.tx).await {
                let _ = from_tx.send(ConnIn::StateChanged(
                    ConnectionState::Error(format!(
                        "Failed to subscribe to TX notifications: {e}"
                    )),
                ));
                let _ = peripheral.disconnect().await;
                return;
            }

            if let Some(ref tx2) = uart.tx2 {
                if let Err(e) = peripheral.subscribe(tx2).await {
                    log::warn!(
                        "Failed to subscribe to secondary TX notifications: {e}"
                    );
                }
            }

            let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Connected));

            // 7. 写通道桥接：std::mpsc -> tokio::mpsc ------------------------
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

            // 丢掉当前 async 任务里的 sender。
            // 这样当外部 to_conn_rx 关闭、桥接任务退出后，
            // internal_write_rx.recv() 能正确返回 None。
            drop(internal_write_tx);

            // 8. 写模式：优先 WithoutResponse 以获得更高吞吐 ------------------
            let write_type = if uart
                .rx
                .properties
                .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE)
            {
                WriteType::WithoutResponse
            } else {
                WriteType::WithResponse
            };

            let uart_rx = uart.rx.clone();
            let uart_tx = uart.tx.clone();
            let uart_tx2 = uart.tx2.clone();

            // BLE ATT 有 3 字节头部，单包 payload 通常为 MTU - 3。
            // 如果平台暂时返回很小的 MTU，至少按 20 字节保守分片。
            let max_payload_len = peripheral.mtu().saturating_sub(3).max(20);

            log::info!(
                "BLE write mode={:?}, mtu={}, max_payload_len={}",
                write_type,
                peripheral.mtu(),
                max_payload_len,
            );

            // 9. 主事件循环 --------------------------------------------------
            loop {
                tokio::select! {
                    // ---- 读路径：BLE 通知 -> ConnIn::Data ----
                    notification = notifications.next() => {
                        let Some(notification) = notification else {
                            let _ = from_tx.send(
                                ConnIn::StateChanged(ConnectionState::Lost(
                                    "BLE notification stream ended".to_string(),
                                )),
                            );
                            let _ = peripheral.disconnect().await;
                            return;
                        };

                        let is_primary = notification.uuid == uart_tx.uuid;
                        let is_secondary = uart_tx2
                            .as_ref()
                            .map_or(false, |tx2| notification.uuid == tx2.uuid);

                        if is_primary || is_secondary {
                            emit_conn_data(
                                &from_tx,
                                &repaint_reader,
                                notification.value,
                            );
                        }
                    }

                    // ---- 写路径：ConnOut::Data -> BLE RX 特征 ----
                    msg = internal_write_rx.recv() => {
                        match msg {
                            Some(ConnOut::Data(data)) => {
                                for chunk in data.chunks(max_payload_len.into()) {
                                    if let Err(e) = peripheral
                                        .write(&uart_rx, chunk, write_type)
                                        .await
                                    {
                                        log::warn!("BLE write error: {e}");

                                        let _ = from_tx.send(
                                            ConnIn::StateChanged(
                                                ConnectionState::Lost(e.to_string()),
                                            ),
                                        );

                                        let _ = peripheral.disconnect().await;
                                        return;
                                    }

                                    // WithoutResponse 没有确认/背压。
                                    // 给设备一点处理时间，避免大粘贴时压爆外围设备缓冲区。
                                    if write_type == WriteType::WithoutResponse {
                                        tokio::time::sleep(
                                            Duration::from_millis(2)
                                        ).await;
                                    }
                                }
                            }

                            Some(ConnOut::Close) | None => {
                                close_ble_connection(
                                    &peripheral,
                                    &uart_tx,
                                    &uart_tx2,
                                ).await;

                                let _ = from_tx.send(
                                    ConnIn::StateChanged(ConnectionState::Closed),
                                );

                                return;
                            }

                            Some(ConnOut::Resize(_, _)) | Some(ConnOut::Winch) => {
                                // BLE 串口无需处理窗口尺寸变化
                            }
                        }
                    }
                }
            }
        });
    });

    // writer_thread 是虚设的：所有写入在 reader_thread 的异步事件循环中完成。
    let writer_thread = std::thread::spawn(|| {});

    Ok(ConnectionHandle::new(
        to_conn_tx,
        from_conn_rx,
        reader_thread,
        writer_thread,
        repaint,
    ))
}