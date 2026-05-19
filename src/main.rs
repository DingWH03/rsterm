mod app;
mod config;
mod connection;
mod fs;
mod session;
mod fonts;
mod platform;
mod settings;
mod storage;
mod terminal;
mod ui;

use app::RstermApp;
use log::info;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("rsTerm - Multi Terminal Emulator"),
        centered: true,
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "rsTerm",
        native_options,
        Box::new(|cc| {
            fonts::setup_fonts(&cc.egui_ctx);
            Ok(Box::new(RstermApp::default()))
        }),
    ) {
        info!("Failed to start: {e}");
    }
}
