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

    // Android may keep the same process after the Activity is recreated.
    // Clear any stale signal before the new UI starts polling input.
    BACK_PRESSED.store(false, Ordering::SeqCst);

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

/// Call from `ui()` each frame. Runs `handle` when back was pressed.
///
/// The closure returns whether the press was consumed by in-app navigation.
pub fn consume_back_pressed(handle: impl FnOnce() -> bool) -> bool {
    if !BACK_PRESSED.swap(false, Ordering::SeqCst) {
        return false;
    }
    if handle() {
        return true;
    }
    true
}

fn with_activity<R>(f: impl FnOnce(&jni_0_19::JNIEnv<'_>, &GlobalRef) -> R) -> Option<R> {
    let jvm_guard = JVM.lock().unwrap();
    let act_guard = ACTIVITY.lock().unwrap();
    let jvm = jvm_guard.as_ref()?;
    let act = act_guard.as_ref()?;
    let env = jvm.attach_current_thread().ok()?;
    Some(f(&env, act))
}

/// Move the task to the background without destroying the NativeActivity.
///
/// This is the safest Android "exit" path for winit/eframe: destroying the
/// Activity can recreate `android_main` in the same process, which may collide
/// with the previous event loop while it is still shutting down.
pub fn move_task_to_back() -> bool {
    with_activity(|env, act| {
        env.call_method(
            act.as_obj(),
            "moveTaskToBack",
            "(Z)Z",
            &[jni_0_19::objects::JValue::Bool(jni_0_19::sys::JNI_TRUE)],
        )
            .and_then(|v| v.z())
            .unwrap_or(false)
    })
    .unwrap_or(false)
}

/// Call `finish()` on the Activity via JNI. Kept as a fallback for non-back
/// lifecycle paths, but normal Android back navigation should prefer
/// [`move_task_to_back`] to avoid same-process event-loop recreation crashes.
pub fn finish_activity() -> bool {
    with_activity(|env, act| {
        env.call_method(act.as_obj(), "finish", "()V", &[]).is_ok()
    })
    .unwrap_or(false)
}
