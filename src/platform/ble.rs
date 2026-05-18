//! BLE scan — btleplug supports Linux, Windows, macOS, Android, iOS.

use std::time::Duration;

pub fn scan_ble_devices_blocking() -> Result<Vec<String>, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;

    rt.block_on(async { scan_ble_async().await })
}

async fn scan_ble_async() -> Result<Vec<String>, String> {
    use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
    use btleplug::platform::Manager;

    let manager = Manager::new().await.map_err(|e| e.to_string())?;
    let adapters = manager.adapters().await.map_err(|e| e.to_string())?;
    let adapter = adapters.first().ok_or_else(no_adapter_error)?;

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
        "未找到蓝牙适配器。请在 Windows 设置中打开蓝牙。".to_string()
    } else if cfg!(target_os = "android") {
        "未找到蓝牙适配器。请打开蓝牙并授予应用蓝牙/定位权限。".to_string()
    } else if cfg!(target_os = "macos") {
        "未找到蓝牙适配器。请在系统设置中打开蓝牙。".to_string()
    } else {
        "未找到蓝牙适配器。".to_string()
    }
}
