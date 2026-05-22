//! Android btleplug / Droidplug initialization.
//!
//! btleplug's Android backend keeps a process-global Droidplug singleton.
//! `Manager::adapters()` will panic if `btleplug::platform::init(&JNIEnv)` has
//! not completed successfully.  Be careful not to call `init()` too early: the
//! function stores JavaVM before it registers/caches all Java classes, so a
//! failed class registration can otherwise leave btleplug half-initialized.
//!
//! # Android class-loader caveat
//!
//! On Android, `JNI FindClass()` called from a **native** thread (one not
//! attached by the Android framework, e.g. a Rust `std::thread`) uses the
//! *system* class loader, which only sees `java.*` / `android.*` framework
//! classes — **not** the app's own classes.
//!
//! To work around this we cache the app's `ClassLoader` during startup (when
//! `android_main` runs on the main Java thread) and use `ClassLoader.loadClass()`
//! in the preflight check whenever `FindClass` fails.  The same ClassLoader is
//! also passed down to `btleplug::platform::init()` so its internal class
//! registration also works from native worker threads.

use std::any::Any;
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use jni_0_19::objects::{GlobalRef, JValue};
use jni_0_19::sys::{jint, JNI_VERSION_1_6};
use jni_0_19::{JNIEnv, JavaVM};

static JVM_PTR: OnceLock<usize> = OnceLock::new();
static INIT_LOCK: Mutex<()> = Mutex::new(());
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static FATAL_INIT_ERROR: OnceLock<String> = OnceLock::new();

/// Cached application ClassLoader (global JNI ref) so native threads can load
/// app classes without relying on `FindClass` (which uses the system loader).
static APP_CLASS_LOADER: OnceLock<GlobalRef> = OnceLock::new();

/// Cache the app `ClassLoader` from the `AndroidApp` Activity.
///
/// Must be called early in `android_main`, **before** `init_from_android_app`,
/// because on NativeActivity even the main thread's `FindClass` may not see
/// app-specific classes.  We obtain the ClassLoader from the Activity object
/// instead, which always works.
#[cfg(target_os = "android")]
pub fn cache_class_loader_from_activity(
    app: &winit::platform::android::activity::AndroidApp,
) {
    use jni_0_19::objects::JObject;

    if APP_CLASS_LOADER.get().is_some() {
        return;
    }

    let jvm = match unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni_0_19::sys::JavaVM) } {
        Ok(jvm) => jvm,
        Err(e) => {
            log::warn!("btleplug cache_class_loader: JavaVM::from_raw failed: {e}");
            return;
        }
    };

    let env = match jvm.get_env() {
        Ok(env) => env,
        Err(e) => {
            log::warn!("btleplug cache_class_loader: get_env failed: {e}");
            return;
        }
    };

    // Build a JObject from the raw Activity pointer.
    let activity = JObject::from(app.activity_as_ptr() as jni_0_19::sys::jobject);

    // activity.getClassLoader()
    let class_loader = match env.call_method(
        activity,
        "getClassLoader",
        "()Ljava/lang/ClassLoader;",
        &[],
    ) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("btleplug cache_class_loader: getClassLoader call failed: {e}");
            return;
        }
    };

    let class_loader_obj = match class_loader.l() {
        Ok(obj) => obj,
        Err(e) => {
            log::warn!("btleplug cache_class_loader: extract ClassLoader failed: {e}");
            return;
        }
    };

    let global = match env.new_global_ref(class_loader_obj) {
        Ok(g) => g,
        Err(e) => {
            log::warn!("btleplug cache_class_loader: new_global_ref failed: {e}");
            return;
        }
    };

    let _ = APP_CLASS_LOADER.set(global);
    log::info!("btleplug: cached app ClassLoader from Activity");
}

/// Called from `android_main` with `winit::AndroidApp::vm_as_ptr()`.
///
/// We capture the JavaVM pointer early, then try a safe warm-up.  A failed
/// preflight does not poison the process: scanning will simply retry later and
/// report the real reason to the UI.
///
/// Note: `cache_class_loader_from_activity` should be called **before** this
/// function so the ClassLoader is available for native-thread preflight checks.
pub fn init_from_android_app(vm_ptr: *mut c_void) {
    log::info!("btleplug init_from_android_app called");
    capture_java_vm(vm_ptr);

    if let Err(e) = ensure_initialized() {
        log::warn!("btleplug warm-up from android_main did not complete: {e}");
    }
}

fn capture_java_vm(vm_ptr: *mut c_void) {
    if vm_ptr.is_null() {
        log::warn!("btleplug init: Android JavaVM pointer is null");
        return;
    }

    let _ = JVM_PTR.set(vm_ptr as usize);
}

/// Ensure Droidplug is initialized before any `Manager::adapters()` call.
///
/// Important: btleplug's Android backend calls `JavaVM::get_env()` on every
/// operation. That means each Rust worker thread that uses btleplug must remain
/// attached to the JVM, not just attach briefly during initialization.
pub fn ensure_initialized() -> Result<(), String> {
    if let Some(e) = FATAL_INIT_ERROR.get() {
        return Err(format!(
            "Android btleplug initialization previously failed; restart the app after fixing packaging/configuration: {e}"
        ));
    }

    let ptr = *JVM_PTR
        .get()
        .ok_or_else(|| "Android JavaVM was not captured before BLE initialization".to_string())?
        as *mut jni_0_19::sys::JavaVM;

    let jvm = unsafe { JavaVM::from_raw(ptr) }
        .map_err(|e| format!("failed to wrap Android JavaVM for btleplug: {e}"))?;

    let is_main_thread = jvm.get_env().is_ok();
    let env = match jvm.get_env() {
        Ok(env) => env,
        Err(get_env_err) => {
            // Android native Rust worker threads are often not attached to the
            // JVM. Use a permanent attachment so later btleplug calls on this
            // same worker thread can also call Java via JavaVM::get_env().
            log::info!(
                "btleplug init: current thread had no JNIEnv ({get_env_err}); attaching permanently"
            );
            jvm.attach_current_thread_permanently().map_err(|e| {
                format!("failed to permanently attach current thread to JVM for btleplug: {e}")
            })?
        }
    };

    if INITIALIZED.load(Ordering::Acquire) {
        log::debug!("btleplug ensure_initialized: already initialized, skipping");
        return Ok(());
    }

    let _guard = INIT_LOCK
        .lock()
        .map_err(|_| "btleplug init lock was poisoned".to_string())?;

    if INITIALIZED.load(Ordering::Acquire) {
        log::debug!("btleplug ensure_initialized: already initialized (after lock)");
        return Ok(());
    }

    if let Some(e) = FATAL_INIT_ERROR.get() {
        return Err(format!(
            "Android btleplug initialization previously failed; restart the app after fixing packaging/configuration: {e}"
        ));
    }

    log::info!(
        "btleplug ensure_initialized: proceeding with init (main_thread={is_main_thread})"
    );
    init_with_env(&env)
}

fn init_with_env(env: &JNIEnv<'_>) -> Result<(), String> {
    preflight_btleplug_java_classes(env)?;

    // btleplug::platform::init() internally uses FindClass to cache 20+ Java
    // classes.  On Android NativeActivity, even the main thread's FindClass
    // may not see app-specific classes.  We work around this by registering
    // a native method on the Adapter class (loaded via ClassLoader, so no
    // FindClass needed) and invoking it via JNI.
    // The native implementation of initBtleplug() calls btleplug::platform::init
    // with a JNIEnv that has the Adapter class's class loader, making FindClass
    // resolve app classes correctly.
    if let Some(global_ref) = APP_CLASS_LOADER.get() {
        let result = call_btleplug_init_via_native(env, global_ref);
        match result {
            Ok(()) => {
                INITIALIZED.store(true, Ordering::Release);
                log::info!("btleplug / Droidplug initialized");
                return Ok(());
            }
            Err(e) => {
                clear_pending_exception(env);
                let msg = format!("btleplug::platform::init failed (native method): {e}");
                let _ = FATAL_INIT_ERROR.set(msg.clone());
                return Err(msg);
            }
        }
    }

    // Fallback: call init directly (desktop or when no ClassLoader is cached).
    let result = catch_unwind(AssertUnwindSafe(|| btleplug::platform::init(env)));
    match result {
        Ok(Ok(())) => {
            INITIALIZED.store(true, Ordering::Release);
            log::info!("btleplug / Droidplug initialized (direct)");
            Ok(())
        }
        Ok(Err(e)) => {
            clear_pending_exception(env);
            let msg = format!("btleplug::platform::init failed: {e}");
            let _ = FATAL_INIT_ERROR.set(msg.clone());
            Err(msg)
        }
        Err(panic) => {
            clear_pending_exception(env);
            let msg = format!(
                "btleplug::platform::init panicked: {}",
                panic_message(&panic)
            );
            let _ = FATAL_INIT_ERROR.set(msg.clone());
            Err(msg)
        }
    }
}

/// Register our initBtleplug native method on the Adapter class (loaded
/// via ClassLoader, bypassing FindClass), then invoke it.  The native
/// implementation runs in Adapter's class-loader context, making FindClass
/// work for all btleplug classes.
fn call_btleplug_init_via_native(env: &JNIEnv<'_>, app_loader: &GlobalRef) -> Result<(), String> {
    use jni_0_19::objects::JClass;
    use jni_0_19::NativeMethod;

    // 1. Load the Adapter class via ClassLoader (no FindClass needed).
    let dot_name = "com.nonpolynomial.btleplug.android.impl.Adapter";
    let jname = env
        .new_string(dot_name)
        .map_err(|e| format!("new_string: {e}"))?;
    let cls_jobj = env
        .call_method(
            app_loader.as_obj(),
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(*jname)],
        )
        .map_err(|e| format!("loadClass: {e}"))?
        .l()
        .map_err(|e| format!("extract class: {e}"))?;
    let cls = JClass::from(cls_jobj);

    // 2. Register initBtleplug native method on the Adapter class.
    // JClass: Deref<Target = JObject>, and Desc is implemented for JObject.
    env.register_native_methods(
        cls,
        &[NativeMethod {
            name: "initBtleplug".into(),
            sig: "()Z".into(),
            fn_ptr: btleplug_init_native as *mut std::ffi::c_void,
        }],
    )
    .map_err(|e| format!("register_native_methods: {e}"))?;

    // 3. Call the static method.  Because step 2 registered it on the
    // Adapter class, JNI will use Adapter's class loader when invoking
    // the native function and for any FindClass calls within it.
    env.call_static_method(cls, "initBtleplug", "()Z", &[])
        .map_err(|e| format!("initBtleplug: {e}"))?;

    Ok(())
}

/// Native implementation of Adapter.initBtleplug().
///
/// Registered dynamically via register_native_methods (not by naming
/// convention), so the JNIEnv provided by the JNI invocation carries the
/// Adapter class's class-loader context.  All FindClass calls inside
/// btleplug::platform::init thus resolve app classes.
unsafe extern "system" fn btleplug_init_native(
    env: jni_0_19::JNIEnv<'_>,
    _class: jni_0_19::objects::JClass<'_>,
) -> jni_0_19::sys::jboolean {
    log::info!("btleplug: initBtleplug native called (class-loader context)");
    match btleplug::platform::init(&env) {
        Ok(()) => {
            log::info!("btleplug: initBtleplug succeeded");
            jni_0_19::sys::JNI_TRUE
        }
        Err(e) => {
            let _ = env.exception_clear();
            log::error!("btleplug: initBtleplug failed: {e}");
            jni_0_19::sys::JNI_FALSE
        }
    }
}

fn preflight_btleplug_java_classes(env: &JNIEnv<'_>) -> Result<(), String> {
    // These are the classes btleplug 0.12 registers/caches during
    // btleplug::platform::init(). Check them first so a missing Java/AAR or
    // ProGuard/R8 removal is reported cleanly instead of half-initializing
    // btleplug and crashing when scanning starts.
    const CLASSES: &[&str] = &[
        "com/nonpolynomial/btleplug/android/impl/Adapter",
        "com/nonpolynomial/btleplug/android/impl/Peripheral",
        "com/nonpolynomial/btleplug/android/impl/ScanFilter",
        "com/nonpolynomial/btleplug/android/impl/NotConnectedException",
        "com/nonpolynomial/btleplug/android/impl/PermissionDeniedException",
        "com/nonpolynomial/btleplug/android/impl/UnexpectedCallbackException",
        "com/nonpolynomial/btleplug/android/impl/UnexpectedCharacteristicException",
        "com/nonpolynomial/btleplug/android/impl/NoSuchCharacteristicException",
        "com/nonpolynomial/btleplug/android/impl/NoBluetoothAdapterException",
        "io/github/gedgygedgy/rust/future/Future",
        "io/github/gedgygedgy/rust/future/FutureException",
        "io/github/gedgygedgy/rust/ops/FnAdapter",
        "io/github/gedgygedgy/rust/ops/FnRunnableImpl",
        "io/github/gedgygedgy/rust/ops/FnBiFunctionImpl",
        "io/github/gedgygedgy/rust/ops/FnFunctionImpl",
        "io/github/gedgygedgy/rust/stream/Stream",
        "io/github/gedgygedgy/rust/stream/StreamPoll",
        "io/github/gedgygedgy/rust/task/Waker",
        "io/github/gedgygedgy/rust/task/PollResult",
        "android/bluetooth/le/ScanResult",
    ];

    let class_loader = APP_CLASS_LOADER.get();

    for class_name in CLASSES {
        let result = if let Some(loader) = class_loader {
            // Use the cached app ClassLoader which works even from native
            // threads where FindClass would use the system loader.
            find_class_via_loader(env, loader, class_name)
        } else {
            env.find_class(class_name).map(|_| ())
        };

        match result {
            Ok(_) => {
                log::debug!("btleplug preflight OK: {class_name}");
            }
            Err(e) => {
                // Log the full JNI exception description *before* clearing it so
                // the developer can see the root cause in logcat (e.g. a pending
                // exception from an earlier call, or a class-loading error).
                let _ = env.exception_describe();
                clear_pending_exception(env);
                let msg = format!(
                    "btleplug Android Java class not found: {class_name}. \
                     Make sure the btleplug droidplug Java/AAR and jni-utils-rs \
                     Java classes are packaged, and keep them from R8/ProGuard. \
                     JNI error: {e}"
                );
                log::error!("{msg}");
                return Err(msg);
            }
        }
    }

    Ok(())
}

/// Try to find a class using the cached app ClassLoader.
///
/// `ClassLoader.loadClass()` expects the class name in dotted form
/// (`com.foo.Bar`), unlike JNI `FindClass` which uses slashes.
fn find_class_via_loader(
    env: &JNIEnv<'_>,
    loader: &GlobalRef,
    slash_name: &str,
) -> std::result::Result<(), jni_0_19::errors::Error> {
    let dot_name = slash_name.replace('/', ".");
    let jname = env.new_string(&dot_name)?;
    // JString derefs to JObject (both are Copy in jni 0.19)
    let loader_obj = loader.as_obj();
    let _result = env.call_method(
        loader_obj,
        "loadClass",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        &[JValue::Object(*jname)],
    )?;
    Ok(())
}

fn clear_pending_exception(env: &JNIEnv<'_>) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
}

fn panic_message(panic: &Box<dyn Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Do not call `btleplug::platform::init()` here.
///
/// In some NativeActivity/eframe packaging paths `JNI_OnLoad` runs before the
/// btleplug Java classes are visible through `FindClass`. Calling init at that
/// point can poison btleplug's internal OnceCell and later cause a scan-time
/// panic. We only capture JavaVM here; `android_main`/scan will perform the
/// guarded initialization.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn JNI_OnLoad(
    vm: *mut jni_0_19::sys::JavaVM,
    _reserved: *mut c_void,
) -> jint {
    capture_java_vm(vm.cast());
    JNI_VERSION_1_6
}



