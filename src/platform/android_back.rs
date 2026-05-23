//! Android back‑button interception.
//!
//! `RsTerminalActivity` (custom NativeActivity) overrides `onBackPressed()`
//! and calls the JNI function to signal Rust.  We set an atomic flag; the
//! next egui frame runs `handle_back_navigation()`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use jni_0_19::objects::GlobalRef;

static BACK_PRESSED: AtomicBool = AtomicBool::new(false);
static JVM: OnceLock<jni_0_19::JavaVM> = OnceLock::new();
static ACTIVITY: OnceLock<GlobalRef> = OnceLock::new();

/// Init from `android_main` (Java main thread).
pub fn init(app: &winit::platform::android::activity::AndroidApp) {
    use jni_0_19::objects::JObject;
    if let Ok(jvm) = unsafe {
        jni_0_19::JavaVM::from_raw(app.vm_as_ptr() as *mut jni_0_19::sys::JavaVM)
    } {
        let _ = JVM.set(jvm);
    }
    if let Ok(env) = JVM.get().unwrap().attach_current_thread() {
        let activity = JObject::from(app.activity_as_ptr() as jni_0_19::sys::jobject);
        if let Ok(g) = env.new_global_ref(activity) {
            let _ = ACTIVITY.set(g);
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
    if let (Some(jvm), Some(act)) = (JVM.get(), ACTIVITY.get()) {
        if let Ok(env) = jvm.attach_current_thread() {
            let _ = env.call_method(act.as_obj(), "finish", "()V", &[]);
        }
    }
    true
}
