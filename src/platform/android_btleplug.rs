//! Android btleplug / Droidplug initialisation.
//!
//! btleplug's Android backend keeps its own global JavaVM/JNI state. If it is
//! not initialised before `Manager::new()` runs, Android reports:
//!
//! "Droidplug has not been initialized. Please initialize it with
//! btleplug::platform::init()".
//!
//! We initialise once during `android_main`, and also retry lazily from BLE
//! worker threads before scanning/connecting. The lazy retry matters because BLE
//! work is done on spawned Rust threads with their own Tokio runtimes.

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use winit::platform::android::activity::AndroidApp;

static JVM_PTR: AtomicPtr<jni_0_19::sys::JavaVM> = AtomicPtr::new(std::ptr::null_mut());
static BTLEPLUG_READY: AtomicBool = AtomicBool::new(false);

/// Record the Android JavaVM and initialise btleplug immediately.
pub fn init_btleplug(app: &AndroidApp) {
    JVM_PTR.store(app.vm_as_ptr() as *mut jni_0_19::sys::JavaVM, Ordering::SeqCst);
    if let Err(e) = ensure_btleplug_initialized() {
        log::warn!("btleplug init failed: {e}");
    }
}

/// Ensure Droidplug is initialised on Android before using btleplug.
///
/// Safe to call repeatedly. On non-initialised Android worker threads this will
/// attach the thread to the JVM long enough to obtain a `JNIEnv` and call
/// `btleplug::platform::init(&env)`.
pub fn ensure_btleplug_initialized() -> Result<(), String> {
    if BTLEPLUG_READY.load(Ordering::SeqCst) {
        return Ok(());
    }

    let ptr = JVM_PTR.load(Ordering::SeqCst);
    if ptr.is_null() {
        return Err("Android JavaVM 尚未记录，无法初始化 btleplug/Droidplug".to_string());
    }

    let jvm = unsafe { jni_0_19::JavaVM::from_raw(ptr) }
        .map_err(|e| format!("创建 JavaVM 句柄失败：{e}"))?;

    // BLE scanning/connection often happens on spawned Rust worker threads. They
    // are not guaranteed to already be attached to the JVM, so `get_env()` is not
    // enough here. A permanent attach is appropriate for these long-lived Rust
    // threads and avoids immediate detach while btleplug registers callbacks.
    let env = jvm
        .attach_current_thread_permanently()
        .map_err(|e| format!("附加当前线程到 JVM 失败：{e}"))?;

    match btleplug::platform::init(&env) {
        Ok(()) => {
            BTLEPLUG_READY.store(true, Ordering::SeqCst);
            log::info!("btleplug/Droidplug initialized");
            Ok(())
        }
        Err(e) => {
            // Some btleplug versions return an error if native methods were
            // already registered. Treat messages containing "already" as a
            // successful idempotent initialisation, otherwise surface the error.
            let msg = e.to_string();
            if msg.to_lowercase().contains("already") {
                BTLEPLUG_READY.store(true, Ordering::SeqCst);
                log::info!("btleplug/Droidplug was already initialized: {msg}");
                Ok(())
            } else {
                Err(format!("初始化 btleplug/Droidplug 失败：{msg}"))
            }
        }
    }
}
