//! rsTerm library (desktop `bin` + Android `cdylib`).

#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

// Load all locale files from the `locales` directory at compile time.
rust_i18n::i18n!("locales", fallback = "en");

pub mod app;
pub mod config;
pub mod connection;
pub mod fs;
pub mod session;
pub mod fonts;
pub mod i18n;
pub mod platform;
pub mod settings;
pub mod storage;
pub mod terminal;
pub mod ui;

use app::RstermApp;
use log::info;

pub fn run_app(native_options: eframe::NativeOptions) {
    if let Err(e) = eframe::run_native(
        "rsTerm",
        native_options,
        Box::new(|cc| {
            let settings = crate::settings::load_settings();
            settings.language.apply();
            fonts::setup_fonts(
                &cc.egui_ctx,
                &settings.default_profile().terminal_font,
            );
            fonts::preload_monospace_catalog();
            fonts::tune_android_display(&cc.egui_ctx);
            Ok(Box::new(RstermApp::default()))
        }),
    ) {
        info!("Failed to start: {e}");
    }
}

/// Desktop entry.
#[cfg(not(target_os = "android"))]
pub fn run_desktop() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title(rust_i18n::t!("app_title")),
        centered: true,
        ..Default::default()
    };

    run_app(native_options);
}

/// Android entry (winit / eframe). Loaded via `NativeActivity` + `android.app.lib_name`.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );

    // -- Initialise platform services -----------------------------------
    platform::init_android_ime(app.clone());
    platform::ensure_storage_access(&app);
    platform::ensure_bluetooth_access(&app);

    // Initialise btleplug (Droidplug) for BLE scanning/connection.
    // Android NativeActivity threads are not guaranteed to already have a JNIEnv,
    // so the helper attaches the current thread when needed.
    //
    // IMPORTANT: cache the app ClassLoader from the Activity *before* any
    // FindClass call, because NativeActivity's main thread may not have the
    // app's class loader either.
    platform::cache_android_class_loader(&app);
    platform::init_android_btleplug(app.vm_as_ptr());

    // -- Initialise persistent config path ------------------------------
    // Android NativeActivity does NOT set $HOME / $XDG_CONFIG_HOME, so
    // the `directories` crate returns None and config is lost on restart.
    // Use the app-internal data path provided by the system instead.
    if let Some(data_dir) = app.internal_data_path() {
        storage::init_android_base_dir(data_dir);
        log::info!("Android config dir: {:?}", storage::config_dir());
    }

    let native_options = eframe::NativeOptions {
        android_app: Some(app),
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("rsTerm"),
        ..Default::default()
    };

    run_app(native_options);
}
