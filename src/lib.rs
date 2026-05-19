//! rsTerm library (desktop `bin` + Android `cdylib`).

#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

pub mod app;
pub mod config;
pub mod connection;
pub mod fs;
pub mod session;
pub mod fonts;
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
            fonts::setup_fonts(&cc.egui_ctx);
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
            .with_title("rsTerm - Multi Terminal Emulator"),
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

    platform::init_android_ime(app.clone());
    platform::ensure_storage_access(&app);

    let native_options = eframe::NativeOptions {
        android_app: Some(app),
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("rsTerm"),
        ..Default::default()
    };

    run_app(native_options);
}
