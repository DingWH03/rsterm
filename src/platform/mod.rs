//! Cross-platform helpers (Linux, macOS, Windows, Android).
//!
//! Uses a `Platform` trait with per-platform implementations so that
//! unsupported features (e.g. local terminal on Android) are never compiled
//! on targets that cannot use them.
//!
//! # Initialisation
//!
//! Call [`init_platform`] early in `main` / `android_main` before any
//! other platform function is used.

use std::sync::OnceLock;

use egui::Context;

pub use serial::SerialDevice;

// ── Internal implementation modules ─────────────────────────────────────────

// These modules contain the actual per-platform logic and are used by
// the concrete platform structs below.
pub(crate) mod ble;
pub(crate) mod process;
pub(crate) mod serial;
pub(crate) mod shell;

#[cfg(target_os = "android")]
pub(crate) mod android_btleplug;
#[cfg(target_os = "android")]
pub(crate) mod android_ime;
#[cfg(target_os = "android")]
pub(crate) mod android_storage;

// ── Platform selection ──────────────────────────────────────────────────────

#[cfg(not(target_os = "android"))]
mod desktop;
#[cfg(target_os = "android")]
mod android;

// ── Platform trait ──────────────────────────────────────────────────────────

/// Each method has a **default** implementation returning a safe / unsupported
/// value, so platforms only override what they actually support.
pub trait Platform: Send + Sync + std::fmt::Debug {
    // ── Host identity (unconditional) ───────────────────────────────────
    fn local_user_at_host(&self) -> String;
    fn ssh_user_at_host(&self, user: &str, host: &str) -> String;
    fn title_is_idle_host(&self, title: &str, user_at_host: &str) -> bool;
    fn truncate_label(&self, s: &str, max_chars: usize) -> String;

    // ── Local terminal / PTY (desktop only) ─────────────────────────────
    fn default_shell(&self) -> String {
        "sh".into()
    }
    fn foreground_command(&self, _shell_pid: Option<u32>) -> Option<String> {
        None
    }
    #[allow(unused_variables)]
    fn foreground_process_pid(&self, shell_pid: u32) -> Option<u32> {
        None
    }

    // ── Device enumeration ──────────────────────────────────────────────
    fn enumerate_serial_ports(&self) -> Vec<SerialDevice>;
    fn scan_ble_devices(&self) -> Result<Vec<String>, String>;

    // ── Capability queries ──────────────────────────────────────────────
    fn supports_local_terminal(&self) -> bool;
    fn supports_ble(&self) -> bool;
    fn supports_serial(&self) -> bool;

    // ── Android UI insets (stubs on desktop) ────────────────────────────
    fn top_inset_points(&self, _ctx: &Context) -> f32 {
        0.0
    }
    fn bottom_inset_points(&self, _ctx: &Context) -> f32 {
        0.0
    }

    // ── Android BLE permissions & init (no-ops on desktop) ──────────────
    fn has_bluetooth_access(&self) -> bool {
        true
    }
    fn request_bluetooth_access(&self) {}
    fn ensure_btleplug_ready(&self) -> Result<(), String> {
        Ok(())
    }
}

// ── Global accessor ─────────────────────────────────────────────────────────

static PLATFORM: OnceLock<Box<dyn Platform>> = OnceLock::new();

/// Call once at startup before any other platform function.
pub fn init_platform() {
    #[cfg(not(target_os = "android"))]
    {
        let p: Box<dyn Platform> = Box::new(desktop::DesktopPlatform);
        PLATFORM.set(p).expect("platform already initialised");
    }
    #[cfg(target_os = "android")]
    {
        // Android requires the `AndroidApp` passed from `android_main`.
        panic!("use init_android_platform(app) on Android");
    }
}

/// Android-specific initialisation (called from `lib.rs`).
#[cfg(target_os = "android")]
pub fn init_android_platform(app: &winit::platform::android::activity::AndroidApp) {
    android::init(app);
    let p: Box<dyn Platform> = Box::new(android::AndroidPlatform);
    PLATFORM.set(p).expect("platform already initialised");
}



/// Retrieve the current platform implementation.
///
/// Panics if [`init_platform`] or [`init_android_platform`] was not called.
pub fn get() -> &'static dyn Platform {
    match PLATFORM.get() {
        Some(b) => &**b,
        None => panic!("platform not initialised – call platform::init_platform() early in main"),
    }
}


