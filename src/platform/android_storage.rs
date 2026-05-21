//! Android external storage: manifest permissions + runtime prompts.
//! Also handles BLE runtime permission requests (Android 12+).

use std::sync::OnceLock;

use jni::errors::Error;
use jni::objects::{JObject, JObjectArray, JString, JValue};
use jni::{jni_sig, jni_str, Env, JavaVM};
use winit::platform::android::activity::AndroidApp;

const PERM_READ: &str = "android.permission.READ_EXTERNAL_STORAGE";
const PERM_WRITE: &str = "android.permission.WRITE_EXTERNAL_STORAGE";
const PERMISSION_GRANTED: i32 = 0;
const REQUEST_CODE_STORAGE: i32 = 42;
const REQUEST_CODE_BLE: i32 = 43;

static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();

/// BLE permissions needed on Android 12+ (API 31+).
const PERM_BLE_SCAN: &str = "android.permission.BLUETOOTH_SCAN";
const PERM_BLE_CONNECT: &str = "android.permission.BLUETOOTH_CONNECT";

/// Request legacy read/write (API 23–32) and all-files access settings (API 30+) when needed.
pub fn ensure_storage_access(app: &AndroidApp) {
    let worker = app.clone();
    app.clone().run_on_java_main_thread(Box::new(move || {
        if let Err(e) = run_on_main_thread(&worker) {
            log::warn!("storage permission setup failed: {e}");
        }
    }));
}

fn run_on_main_thread(app: &AndroidApp) -> Result<(), String> {
    let jvm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) };
    jvm.attach_current_thread(|env| -> Result<(), Error> {
        let activity = activity_jobject(env, app);
        let sdk = sdk_int(env)?;

        if sdk >= 30 {
            ensure_all_files_access(env, &activity)?;
        }
        if (23..33).contains(&sdk) {
            request_legacy_permissions(env, &activity)?;
        }
        Ok(())
    })
    .map_err(|e| format!("JNI: {e}"))?;
    Ok(())
}

fn activity_jobject<'local>(env: &Env<'local>, app: &AndroidApp) -> JObject<'local> {
    unsafe { JObject::from_raw(env, app.activity_as_ptr() as jni::sys::jobject) }
}

fn sdk_int(env: &mut Env<'_>) -> Result<i32, Error> {
    let version = env.find_class(jni_str!("android/os/Build$VERSION"))?;
    env.get_static_field(version, jni_str!("SDK_INT"), jni_sig!("I"))?
        .i()
}

fn has_permission(env: &mut Env<'_>, activity: &JObject, perm: &str) -> Result<bool, Error> {
    let perm = env.new_string(perm)?;
    let granted = env
        .call_method(
            activity,
            jni_str!("checkSelfPermission"),
            jni_sig!("(Ljava/lang/String;)I"),
            &[JValue::Object(&perm)],
        )?
        .i()?;
    Ok(granted == PERMISSION_GRANTED)
}

fn request_legacy_permissions(env: &mut Env<'_>, activity: &JObject) -> Result<(), Error> {
    let mut perms = Vec::new();
    if !has_permission(env, activity, PERM_READ)? {
        perms.push(PERM_READ);
    }
    if !has_permission(env, activity, PERM_WRITE)? {
        perms.push(PERM_WRITE);
    }
    if perms.is_empty() {
        return Ok(());
    }

    let placeholder = env.new_string("")?;
    let array = JObjectArray::<JString>::new(env, perms.len(), &placeholder)?;
    for (i, p) in perms.iter().enumerate() {
        let s = env.new_string(p)?;
        array.set_element(env, i, &s)?;
    }
    env.call_method(
        activity,
        jni_str!("requestPermissions"),
        jni_sig!("([Ljava/lang/String;I)V"),
        &[
            JValue::Object(&array),
            JValue::Int(REQUEST_CODE_STORAGE),
        ],
    )?;
    log::info!("requested storage permissions: {:?}", perms);
    Ok(())
}

fn ensure_all_files_access(env: &mut Env<'_>, activity: &JObject) -> Result<(), Error> {
    let environment = env.find_class(jni_str!("android/os/Environment"))?;
    let is_manager = env
        .call_static_method(
            environment,
            jni_str!("isExternalStorageManager"),
            jni_sig!("()Z"),
            &[],
        )?
        .z()?;
    if is_manager {
        return Ok(());
    }

    let settings = env.find_class(jni_str!("android/provider/Settings"))?;
    let action = env.get_static_field(
        settings,
        jni_str!("ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION"),
        jni_sig!("Ljava/lang/String;"),
    )?;

    let intent = env.new_object(
        jni_str!("android/content/Intent"),
        jni_sig!("(Ljava/lang/String;)V"),
        &[JValue::from(&action)],
    )?;

    let package_obj: JObject = env
        .call_method(activity, jni_str!("getPackageName"), jni_sig!("()Ljava/lang/String;"), &[])?
        .try_into()?;
    let package_jstr = env.as_cast::<JString>(&package_obj)?;
    let package = env.get_string(&package_jstr)?;
    let uri_str = env.new_string(format!("package:{package}"))?;

    let uri_class = env.find_class(jni_str!("android/net/Uri"))?;
    let uri = env.call_static_method(
        uri_class,
        jni_str!("parse"),
        jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
        &[JValue::Object(&uri_str)],
    )?;

    env.call_method(
        &intent,
        jni_str!("setData"),
        jni_sig!("(Landroid/net/Uri;)Landroid/content/Intent;"),
        &[JValue::from(&uri)],
    )?;

    env.call_method(
        activity,
        jni_str!("startActivity"),
        jni_sig!("(Landroid/content/Intent;)V"),
        &[JValue::Object(&intent)],
    )?;
    log::info!("opened all-files access settings (grant storage in system UI)");
    Ok(())
}

// ---------------------------------------------------------------------------
// Bluetooth LE runtime permissions (Android 12+)
// ---------------------------------------------------------------------------

/// Request BLE + location permissions needed for Bluetooth scanning.
///
/// - API 31+ : `BLUETOOTH_SCAN`, `BLUETOOTH_CONNECT`
/// - API 29-30: `ACCESS_FINE_LOCATION`
/// - API ≤ 28 : `ACCESS_COARSE_LOCATION`
pub fn ensure_bluetooth_access(app: &AndroidApp) {
    let _ = ANDROID_APP.set(app.clone());
    request_bluetooth_access();
}

/// Re-request BLE permissions from UI code before starting a scan.
///
/// Android permission prompts are asynchronous; callers should start scanning only after
/// [`has_bluetooth_access`] returns true.
pub fn request_bluetooth_access() {
    let Some(app) = ANDROID_APP.get().cloned() else {
        log::warn!("BLE permission request skipped: Android app is not initialized");
        return;
    };

    let worker = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        if let Err(e) = request_ble_permissions(&worker) {
            log::warn!("BLE permission request failed: {e}");
        }
    }));
}

/// Whether every Android runtime permission needed for BLE scanning is currently granted.
pub fn has_bluetooth_access() -> bool {
    let Some(app) = ANDROID_APP.get() else {
        return false;
    };

    match check_ble_permissions(app) {
        Ok(granted) => granted,
        Err(e) => {
            log::warn!("BLE permission check failed: {e}");
            false
        }
    }
}

const PERM_BLE_FINE_LOC: &str = "android.permission.ACCESS_FINE_LOCATION";
const PERM_BLE_COARSE_LOC: &str = "android.permission.ACCESS_COARSE_LOCATION";

fn check_ble_permissions(app: &AndroidApp) -> Result<bool, String> {
    let jvm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) };
    jvm.attach_current_thread(|env| -> Result<bool, Error> {
        let sdk = sdk_int(env)?;
        let activity = activity_jobject(env, app);

        if sdk >= 31 {
            return Ok(
                has_permission(env, &activity, PERM_BLE_SCAN)?
                    && has_permission(env, &activity, PERM_BLE_CONNECT)?,
            );
        }

        if (29..=30).contains(&sdk) {
            return has_permission(env, &activity, PERM_BLE_FINE_LOC);
        }

        if sdk <= 28 {
            return has_permission(env, &activity, PERM_BLE_COARSE_LOC);
        }

        Ok(true)
    })
    .map_err(|e| format!("JNI: {e}"))
}

fn request_ble_permissions(app: &AndroidApp) -> Result<(), String> {
    let jvm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) };
    jvm.attach_current_thread(|env| -> Result<(), Error> {
        let sdk = sdk_int(env)?;
        let activity = activity_jobject(env, app);

        let mut perms = Vec::new();

        if sdk >= 31 {
            // Android 12+ : granular BLE permissions
            if !has_permission(env, &activity, PERM_BLE_SCAN)? {
                perms.push(PERM_BLE_SCAN);
            }
            if !has_permission(env, &activity, PERM_BLE_CONNECT)? {
                perms.push(PERM_BLE_CONNECT);
            }
        }

        if (29..=30).contains(&sdk) {
            // Android 10-11 : BLE scan requires fine location
            if !has_permission(env, &activity, PERM_BLE_FINE_LOC)? {
                perms.push(PERM_BLE_FINE_LOC);
            }
        } else if sdk <= 28 {
            // Android 6-9 : BLE scan requires coarse location
            if !has_permission(env, &activity, PERM_BLE_COARSE_LOC)? {
                perms.push(PERM_BLE_COARSE_LOC);
            }
        }

        if perms.is_empty() {
            return Ok(());
        }

        let placeholder = env.new_string("")?;
        let array = JObjectArray::<JString>::new(env, perms.len(), &placeholder)?;
        for (i, p) in perms.iter().enumerate() {
            let s = env.new_string(p)?;
            array.set_element(env, i, &s)?;
        }
        env.call_method(
            &activity,
            jni_str!("requestPermissions"),
            jni_sig!("([Ljava/lang/String;I)V"),
            &[JValue::Object(&array), JValue::Int(REQUEST_CODE_BLE)],
        )?;
        log::info!("requested BLE permissions: {:?}", perms);
        Ok(())
    })
    .map_err(|e| format!("JNI: {e}"))?;
    Ok(())
}
