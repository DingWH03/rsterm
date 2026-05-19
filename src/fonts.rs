use std::sync::Arc;

use egui::{FontData, FontTweak};

#[cfg(windows)]
const MONO_FONT_PATHS: &[&str] = &[
    r"C:\Windows\Fonts\consola.ttf",
    r"C:\Windows\Fonts\cour.ttf",
    r"C:\Windows\Fonts\lucon.ttf",
];

#[cfg(all(not(windows), not(target_os = "android")))]
const MONO_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansMono-Regular.otf",
];

#[cfg(target_os = "android")]
const MONO_FONT_PATHS: &[&str] = &[
    "/system/fonts/RobotoMono-Regular.ttf",
    "/product/fonts/RobotoMono-Regular.ttf",
    "/system/fonts/DroidSansMono.ttf",
    "/system/fonts/CutiveMono.ttf",
];

#[cfg(windows)]
const CJK_FONT_PATHS: &[&str] = &[
    r"C:\Windows\Fonts\msyh.ttc",
    r"C:\Windows\Fonts\msyhbd.ttc",
    r"C:\Windows\Fonts\simhei.ttf",
    r"C:\Windows\Fonts\simsun.ttc",
    r"C:\Windows\Fonts\mingliu.ttc",
];

#[cfg(all(not(windows), not(target_os = "android")))]
const CJK_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
];

#[cfg(target_os = "android")]
const CJK_FONT_PATHS: &[&str] = &[
    "/system/fonts/NotoSansCJK-Regular.ttc",
    "/system/fonts/NotoSansSC-Regular.otf",
    "/system/fonts/NotoSansHans-Regular.otf",
    "/system/fonts/DroidSansFallback.ttf",
    "/system/fonts/DroidSansFallbackBBK.ttf",
    "/product/fonts/NotoSansCJK-Regular.ttc",
    "/product/fonts/NotoSansSC-Regular.otf",
    "/product/fonts/NotoSansHans-Regular.otf",
    "/system_ext/fonts/NotoSansCJK-Regular.ttc",
    "/system_ext/fonts/NotoSansSC-Regular.otf",
    "/vendor/fonts/NotoSansCJK-Regular.ttc",
    "/vendor/fonts/NotoSansSC-Regular.otf",
];

fn mono_tweak() -> FontTweak {
    FontTweak::default()
}

fn insert_font(fonts: &mut egui::FontDefinitions, name: &str, bytes: Vec<u8>) {
    let mut data = FontData::from_owned(bytes);
    data.tweak = mono_tweak();
    fonts.font_data.insert(name.to_owned(), Arc::new(data));
}

/// Tune HiDPI rendering on Android (crisper glyph rasterization).
#[cfg(target_os = "android")]
pub fn tune_android_display(ctx: &egui::Context) {
    let ppp = ctx.pixels_per_point();
    let ppp = if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 };
    // Snap to quarter steps so layout sizes land closer to physical pixels.
    let ppp = (ppp * 4.0).round() / 4.0;
    ctx.set_pixels_per_point(ppp);
    ctx.options_mut(|o| {
        // Heavy feathering + per-cell clips on GLES can look like torn horizontal bands.
        o.tessellation_options.feathering = false;
    });
}

#[cfg(not(target_os = "android"))]
pub fn tune_android_display(_ctx: &egui::Context) {}

/// System fonts per platform. Android uses `/system/fonts` only (no bundled fonts).
pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    if let Some(bytes) = read_first_existing(MONO_FONT_PATHS) {
        insert_font(&mut fonts, "mono_latin", bytes);
    }

    if let Some(bytes) = read_first_existing(CJK_FONT_PATHS) {
        insert_font(&mut fonts, "mono_cjk", bytes);
    }

    let mono = fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default();

    if fonts.font_data.contains_key("mono_latin") {
        mono.insert(0, "mono_latin".to_owned());
    }
    if fonts.font_data.contains_key("mono_cjk") {
        let at = if fonts.font_data.contains_key("mono_latin") {
            1
        } else {
            0
        };
        mono.insert(at, "mono_cjk".to_owned());
    }

    if fonts.font_data.contains_key("mono_cjk") {
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "mono_cjk".to_owned());
    }

    #[cfg(target_os = "android")]
    let has_cjk = fonts.font_data.contains_key("mono_cjk");

    ctx.set_fonts(fonts);

    #[cfg(target_os = "android")]
    if !has_cjk {
        log::warn!(
            "no CJK system font found; Chinese may not render (check logcat for loaded paths)"
        );
    }
}

fn read_first_existing(paths: &[&str]) -> Option<Vec<u8>> {
    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            log::info!("loaded font: {path}");
            return Some(bytes);
        }
    }
    None
}
