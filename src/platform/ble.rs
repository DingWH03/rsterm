//! BLE scan — btleplug supports Linux, Windows, macOS, Android, iOS.

use std::time::Duration;

pub fn scan_ble_devices_blocking() -> Result<Vec<String>, String> {
    log::info!("BLE scan: starting");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            log::error!("BLE scan: failed to create tokio runtime: {e}");
            e.to_string()
        })?;

    let result = rt.block_on(async { scan_ble_async().await });
    match &result {
        Ok(devices) => log::info!("BLE scan: completed, {} device(s)", devices.len()),
        Err(e) => log::error!("BLE scan: failed: {e}"),
    }
    result
}

async fn scan_ble_async() -> Result<Vec<String>, String> {
    // BLE initialisation is handled by the Platform trait impl
    // (AndroidPlatform::scan_ble_devices calls ensure_initialized beforehand).

    use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
    use btleplug::platform::Manager;

    log::info!("BLE scan: creating Manager");
    let manager = Manager::new().await.map_err(|e| {
        log::error!("BLE scan: Manager::new failed: {e}");
        e.to_string()
    })?;
    log::info!("BLE scan: getting adapters");
    let adapters = manager.adapters().await.map_err(|e| {
        log::error!("BLE scan: manager.adapters failed: {e}");
        e.to_string()
    })?;
    let adapter = adapters.first().ok_or_else(|| {
        log::error!("BLE scan: no adapter found");
        no_adapter_error()
    })?;

    adapter
        .start_scan(ScanFilter::default())
        .await
        .map_err(|e| e.to_string())?;

    let scan_secs = if cfg!(target_os = "android") { 4 } else { 3 };
    tokio::time::sleep(Duration::from_secs(scan_secs)).await;

    let peripherals = adapter.peripherals().await.map_err(|e| e.to_string())?;
    let mut names = Vec::new();

    for p in peripherals {
        let Ok(props) = p.properties().await else {
            continue;
        };
        let Some(props) = props else {
            continue;
        };
        let id = p.id().to_string();
        let label = props
            .local_name
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| id.clone());
        if !names.iter().any(|n| n == &label) {
            names.push(label);
        }
    }

    names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    Ok(names)
}

fn no_adapter_error() -> String {
    if cfg!(windows) {
        rust_i18n::t!("ble_no_adapter_windows").to_string()
    } else if cfg!(target_os = "android") {
        rust_i18n::t!("ble_no_adapter_android").to_string()
    } else if cfg!(target_os = "macos") {
        rust_i18n::t!("ble_no_adapter_macos").to_string()
    } else {
        rust_i18n::t!("ble_no_adapter").to_string()
    }
}
