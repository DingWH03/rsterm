//! Android platform implementation.
//! Supports BLE, serial, and Android-specific UI insets / permissions.
//! Does NOT support local terminal (portable-pty has no Android backend).

use std::sync::OnceLock;

use egui::Context;

use crate::platform::process;
use crate::platform::serial::{self, SerialDevice};
use crate::platform::Platform;

pub(super) static PLATFORM: OnceLock<AndroidPlatform> = OnceLock::new();

pub(super) fn init(
    app: &winit::platform::android::activity::AndroidApp,
) {
    // Run Android-specific init before storing the platform.
    crate::platform::android_ime::init(app.clone());
    crate::platform::android_storage::ensure_storage_access(app);
    crate::platform::android_storage::ensure_bluetooth_access(app);
    crate::platform::android_btleplug::cache_class_loader_from_activity(app);
    crate::platform::android_btleplug::init_from_android_app(app.vm_as_ptr());

    let _ = PLATFORM.set(AndroidPlatform);
}

#[derive(Debug)]
pub struct AndroidPlatform;

impl Platform for AndroidPlatform {
    // ── Identity ────────────────────────────────────────────────────────
    fn local_user_at_host(&self) -> String {
        process::local_user_at_host()
    }

    fn ssh_user_at_host(&self, user: &str, host: &str) -> String {
        process::ssh_user_at_host(user, host)
    }

    fn title_is_idle_host(&self, title: &str, user_at_host: &str) -> bool {
        process::title_is_idle_host(title, user_at_host)
    }

    fn truncate_label(&self, s: &str, max_chars: usize) -> String {
        process::truncate_label(s, max_chars)
    }

    // ── Local terminal — NOT supported on Android ───────────────────────
    fn supports_local_terminal(&self) -> bool {
        false
    }

    // ── Device enumeration ──────────────────────────────────────────────
    fn enumerate_serial_ports(&self) -> Vec<SerialDevice> {
        serial::enumerate_serial_ports()
    }

    fn scan_ble_devices(&self) -> Result<Vec<String>, String> {
        // BLE scan on Android needs btleplug initialised first.
        if let Err(e) = crate::platform::android_btleplug::ensure_initialized() {
            return Err(e);
        }
        crate::platform::ble::scan_ble_devices_blocking()
    }

    fn supports_ble(&self) -> bool {
        true
    }

    fn supports_serial(&self) -> bool {
        true
    }

    // ── Android UI insets ───────────────────────────────────────────────
    fn top_inset_points(&self, ctx: &Context) -> f32 {
        crate::platform::android_ime::top_inset_points(ctx)
    }

    fn bottom_inset_points(&self, ctx: &Context) -> f32 {
        crate::platform::android_ime::bottom_inset_points(ctx)
    }

    // ── Android BLE permissions ─────────────────────────────────────────
    fn has_bluetooth_access(&self) -> bool {
        crate::platform::android_storage::has_bluetooth_access()
    }

    fn request_bluetooth_access(&self) {
        // This is best-effort; the actual dialog is triggered by the JNI call.
        crate::platform::android_storage::request_bluetooth_access();
    }

    fn ensure_btleplug_ready(&self) -> Result<(), String> {
        crate::platform::android_btleplug::ensure_initialized()
    }
}
