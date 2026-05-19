//! Android IME / system keyboard insets (`content_rect` shrinks when soft input is visible).

use std::sync::OnceLock;

use egui::Context;
use winit::platform::android::activity::AndroidApp;

static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();

/// Called once from `android_main` before `eframe::run_native`.
pub fn init(app: AndroidApp) {
    let _ = ANDROID_APP.set(app);
}

/// Space occupied by the system soft keyboard below the usable content area, in egui points.
pub fn bottom_inset_points(ctx: &Context) -> f32 {
    let Some(app) = ANDROID_APP.get() else {
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
