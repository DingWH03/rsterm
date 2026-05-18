use std::sync::Arc;

use egui::FontData;

const MONO_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansMono-Regular.ttf",
];

/// CJK fallback (Noto CJK / WenQuanYi). Latin mono fonts above do not cover Chinese.
const CJK_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
];

pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    if let Some(bytes) = read_first_existing(MONO_FONT_PATHS) {
        fonts
            .font_data
            .insert("mono_latin".to_owned(), Arc::new(FontData::from_owned(bytes)));
    }

    if let Some(bytes) = read_first_existing(CJK_FONT_PATHS) {
        fonts
            .font_data
            .insert("mono_cjk".to_owned(), Arc::new(FontData::from_owned(bytes)));
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

    // Proportional text (UI) can fall back to CJK as well.
    if fonts.font_data.contains_key("mono_cjk") {
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push("mono_cjk".to_owned());
    }

    ctx.set_fonts(fonts);
}

fn read_first_existing(paths: &[&str]) -> Option<Vec<u8>> {
    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            return Some(bytes);
        }
    }
    None
}
