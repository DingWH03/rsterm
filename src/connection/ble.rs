use std::sync::mpsc;

use btleplug::api::Manager as _;
use btleplug::platform::Manager;

use crate::connection::{ConnIn, ConnOut, ConnectionHandle, ConnectionState, RepaintNotifier};
use crate::storage::types::SavedConnection;

pub fn connect_ble(config: &SavedConnection) -> Result<ConnectionHandle, String> {
    let _device_name = config
        .ble_device
        .clone()
        .ok_or_else(|| "BLE device not configured".to_string())?;

    let (to_conn_tx, _to_conn_rx) = mpsc::channel::<ConnOut>();
    let (from_conn_tx, from_conn_rx) = mpsc::channel::<ConnIn>();

    let from_tx = from_conn_tx.clone();

    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        let _ = rt.block_on(async move {
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

            let _adapter = match adapters.first() {
                Some(a) => a,
                None => {
                    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Error(
                        "No BLE adapter found".to_string(),
                    )));
                    return;
                }
            };

            let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Connecting));

            let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Connected));
        });
    });

    Ok(ConnectionHandle::new(
        to_conn_tx,
        from_conn_rx,
        thread,
        std::thread::spawn(|| {}),
        RepaintNotifier::default(),
    ))
}
