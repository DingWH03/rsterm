//! Android IME / system keyboard insets (`content_rect` shrinks when soft input is visible).
//! Also provides the status-bar inset so content is not drawn under the system UI.

use std::sync::{Mutex, OnceLock};

use egui::Context;
use jni::{jni_sig, jni_str, JavaVM};
use winit::platform::android::activity::AndroidApp;

/// Updated on each Activity restart so inset helpers always have a valid
/// (non-finished) Activity reference.
static ANDROID_APP: Mutex<Option<AndroidApp>> = Mutex::new(None);
static STATUS_BAR_HEIGHT_PX: OnceLock<i32> = OnceLock::new();

/// Called from `android_main` (once per Activity start).
pub fn init(app: AndroidApp) {
    *ANDROID_APP.lock().unwrap() = Some(app.clone());
    // Compute the status-bar height once at startup.
    let _ = STATUS_BAR_HEIGHT_PX.set(get_status_bar_height_px(&app));
}

// ---------------------------------------------------------------------------
// Status‑bar inset
// ---------------------------------------------------------------------------

/// Height of the system status bar in egui points.
pub fn top_inset_points(ctx: &Context) -> f32 {
    let px = STATUS_BAR_HEIGHT_PX.get().copied().unwrap_or(0).max(0);
    if px == 0 {
        return 0.0;
    }
    px as f32 / ctx.pixels_per_point()
}

fn get_status_bar_height_px(app: &AndroidApp) -> i32 {
    let jvm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) };
    jvm.attach_current_thread(|env| -> Result<i32, jni::errors::Error> {
        let activity = unsafe { jni::objects::JObject::from_raw(env, app.activity_as_ptr() as jni::sys::jobject) };

        // Get Resources via activity.getResources()
        let resources = env.call_method(
            &activity,
            jni_str!("getResources"),
            jni_sig!("()Landroid/content/res/Resources;"),
            &[],
        )?.l()?;

        // Get the resource ID for "status_bar_height" in the "dimen" category
        let name = env.new_string("status_bar_height")?;
        let def_type = env.new_string("dimen")?;
        let def_pkg = env.new_string("android")?;
        let res_id = env.call_method(
            &resources,
            jni_str!("getIdentifier"),
            jni_sig!("(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I"),
            &[
                jni::objects::JValue::Object(&name),
                jni::objects::JValue::Object(&def_type),
                jni::objects::JValue::Object(&def_pkg),
            ],
        )?.i()?;

        if res_id <= 0 {
            return Ok(0);
        }

        // resources.getDimensionPixelSize(res_id)
        let px = env.call_method(
            &resources,
            jni_str!("getDimensionPixelSize"),
            jni_sig!("(I)I"),
            &[jni::objects::JValue::Int(res_id)],
        )?.i()?;

        Ok(px)
    })
    .unwrap_or(0)
}


/// Explicitly ask the Activity to show rsTerminal's Android IME bridge.
///
/// This is only used by the terminal canvas after a user tap. Standard egui
/// `TextEdit` widgets keep using the normal egui/winit IME path.
pub fn show_soft_input() {
    // SHOW_FORCED is intentional here: after Android Back hides the keyboard,
    // some IMEs ignore SHOW_IMPLICIT for an already-focused custom view. The
    // Activity also retries once on the UI thread to handle focus timing.
    call_activity_ime_method(jni_str!("showIme"), 2);
}

/// Explicitly hide the Android soft keyboard through the Activity bridge.
pub fn hide_soft_input() {
    call_activity_ime_method(jni_str!("hideIme"), 0);
}

fn call_activity_ime_method(method: &jni::strings::JNIStr, mode: i32) {
    let guard = ANDROID_APP.lock().unwrap();
    let Some(ref app) = *guard else {
        return;
    };

    let jvm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) };

    let _ = jvm.attach_current_thread(|env| -> Result<(), jni::errors::Error> {
        let activity = unsafe {
            jni::objects::JObject::from_raw(env, app.activity_as_ptr() as jni::sys::jobject)
        };
        env.call_method(
            &activity,
            method,
            jni_sig!("(I)V"),
            &[jni::objects::JValue::Int(mode)],
        )?;
        Ok(())
    });
}

// ---------------------------------------------------------------------------
// Soft‑keyboard (IME) inset
// ---------------------------------------------------------------------------

/// Space occupied by the system soft keyboard below the usable content area, in egui points.
pub fn bottom_inset_points(ctx: &Context) -> f32 {
    let guard = ANDROID_APP.lock().unwrap();
    let Some(ref app) = *guard else {
        return 0.0;
    };
    let rect = app.content_rect();
    let window_bottom = app
        .native_window()
        .map(|w| w.height() as i32)
        .unwrap_or(rect.bottom);
    let inset_px = (window_bottom - rect.bottom).max(0);
    if inset_px == 0 {
        return 0.0;
    }
    inset_px as f32 / ctx.pixels_per_point()
}
