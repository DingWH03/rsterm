//! Android back‑button interception.
//!
//! `RsTerminalActivity` (custom NativeActivity) overrides `onBackPressed()`
//! and calls the JNI function to signal Rust.  We set an atomic flag; the
//! next egui frame runs `handle_back_navigation()`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use jni_0_19::objects::GlobalRef;

static BACK_PRESSED: AtomicBool = AtomicBool::new(false);
static JVM: Mutex<Option<jni_0_19::JavaVM>> = Mutex::new(None);
static ACTIVITY: Mutex<Option<GlobalRef>> = Mutex::new(None);

/// Init from `android_main` (Java main thread).
/// Can be called again on Activity restart (same process).
pub fn init(app: &winit::platform::android::activity::AndroidApp) {
    use jni_0_19::objects::JObject;
    if let Ok(jvm) = unsafe {
        jni_0_19::JavaVM::from_raw(app.vm_as_ptr() as *mut jni_0_19::sys::JavaVM)
    } {
        *JVM.lock().unwrap() = Some(jvm);
    }
    if let Some(ref jvm) = *JVM.lock().unwrap() {
        if let Ok(env) = jvm.attach_current_thread() {
            let activity = JObject::from(app.activity_as_ptr() as jni_0_19::sys::jobject);
            if let Ok(g) = env.new_global_ref(activity) {
                *ACTIVITY.lock().unwrap() = Some(g);
            }
        }
    }
}

/// JNI callback from `RsTerminalActivity.nativeOnBackPressed()`.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_rsTerminal_app_RsTerminalActivity_nativeOnBackPressed(
    _env: jni_0_19::sys::JNIEnv,
    _class: jni_0_19::sys::jclass,
) {
    BACK_PRESSED.store(true, Ordering::SeqCst);
}

/// Call from `ui()` each frame.  Runs `handle` when back was pressed.
/// Finishes the Activity when `handle` returns `false`.
pub fn consume_back_pressed(handle: impl FnOnce() -> bool) -> bool {
    if !BACK_PRESSED.swap(false, Ordering::SeqCst) {
        return false;
    }
    if handle() {
        return true;
    }
    // Don't call finish_activity() here – the caller (app.rs) should
    // send ViewportCommand::Close instead, so eframe exits gracefully.
    // (Navigation handler doesn't have the egui ctx in this scope.)
    true
}

/// Call `finish()` on the Activity via JNI (public so app.rs can call it).
pub fn finish_activity() {
    let jvm_guard = JVM.lock().unwrap();
    let act_guard = ACTIVITY.lock().unwrap();
    if let Some(ref jvm) = *jvm_guard {
        if let Some(ref act) = *act_guard {
            if let Ok(env) = jvm.attach_current_thread() {
                let _ = env.call_method(act.as_obj(), "finish", "()V", &[]);
            }
        }
    }
}
