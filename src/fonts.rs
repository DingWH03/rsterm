use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use egui::{FontData, FontFamily, FontId, FontTweak};

static FONT_GENERATION: AtomicU32 = AtomicU32::new(0);

/// Bumped whenever [`apply_terminal_fonts`] runs; UI clears glyph caches when this changes.
pub fn font_generation() -> u32 {
    FONT_GENERATION.load(Ordering::Relaxed)
}

const USER_MONO_FAMILY: &str = "term_user_mono";
const FALLBACK_LATIN_FAMILY: &str = "term_fallback_latin";
const FALLBACK_BRAILLE_FAMILY: &str = "term_fallback_braille";

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
    "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansMono-Regular.otf",
];

#[cfg(target_os = "android")]
const MONO_FONT_PATHS: &[&str] = &[
    "/system/fonts/RobotoMono-Regular.ttf",
    "/system/fonts/DroidSansMono.ttf",
];

#[cfg(windows)]
const CJK_FONT_PATHS: &[&str] = &[
    r"C:\Windows\Fonts\msyh.ttc",
    r"C:\Windows\Fonts\simhei.ttf",
];

#[cfg(all(not(windows), not(target_os = "android")))]
const CJK_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
];

#[cfg(target_os = "android")]
const CJK_FONT_PATHS: &[&str] = &[
    "/system/fonts/NotoSansCJK-Regular.ttc",
    "/system/fonts/NotoSansSC-Regular.otf",
    "/system/fonts/DroidSansFallback.ttf",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonospaceFontEntry {
    pub path: String,
    pub label: String,
}

fn mono_tweak() -> FontTweak {
    FontTweak::default()
}

fn insert_font(fonts: &mut egui::FontDefinitions, name: &str, bytes: Vec<u8>) {
    let mut data = FontData::from_owned(bytes);
    data.tweak = mono_tweak();
    fonts.font_data.insert(name.to_owned(), Arc::new(data));
}

fn read_file(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok().filter(|b| !b.is_empty())
}

fn read_first_existing(paths: &[&str]) -> Option<Vec<u8>> {
    for path in paths {
        if let Some(bytes) = read_file(path) {
            log::info!("loaded font: {path}");
            return Some(bytes);
        }
    }
    None
}

pub fn needs_braille_font(ch: char) -> bool {
    matches!(ch as u32, 0x2800..=0x28FF)
}

pub fn terminal_font_id(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn terminal_font_id_for_char(ch: char, size: f32) -> FontId {
    if needs_braille_font(ch) {
        FontId::new(size, FontFamily::Name(FALLBACK_BRAILLE_FAMILY.into()))
    } else {
        terminal_font_id(size)
    }
}

enum MonospaceCatalog {
    Idle,
    Loading,
    Ready(Arc<Vec<MonospaceFontEntry>>),
}

static MONO_CATALOG: OnceLock<Mutex<MonospaceCatalog>> = OnceLock::new();

fn mono_catalog() -> &'static Mutex<MonospaceCatalog> {
    MONO_CATALOG.get_or_init(|| Mutex::new(MonospaceCatalog::Idle))
}

/// Start scanning system monospace fonts on a background thread (no-op if already started).
pub fn preload_monospace_catalog() {
    let mut guard = mono_catalog().lock().expect("mono catalog lock");
    match *guard {
        MonospaceCatalog::Loading | MonospaceCatalog::Ready(_) => return,
        MonospaceCatalog::Idle => *guard = MonospaceCatalog::Loading,
    }
    drop(guard);
    std::thread::spawn(|| {
        let list = build_monospace_font_list();
        *mono_catalog().lock().expect("mono catalog lock") =
            MonospaceCatalog::Ready(Arc::new(list));
    });
}

pub enum MonospaceCatalogStatus {
    Loading,
    Ready(Arc<Vec<MonospaceFontEntry>>),
}

pub fn monospace_catalog_status() -> MonospaceCatalogStatus {
    preload_monospace_catalog();
    let guard = mono_catalog().lock().expect("mono catalog lock");
    match &*guard {
        MonospaceCatalog::Idle | MonospaceCatalog::Loading => MonospaceCatalogStatus::Loading,
        MonospaceCatalog::Ready(entries) => MonospaceCatalogStatus::Ready(Arc::clone(entries)),
    }
}

fn build_monospace_font_list() -> Vec<MonospaceFontEntry> {
    let mut out = vec![MonospaceFontEntry {
        path: String::new(),
        label: rust_i18n::t!("settings_terminal_font_auto").into_owned(),
    }];

    #[cfg(not(target_os = "android"))]
    {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let mut seen = std::collections::BTreeSet::new();
        let mut faces: Vec<_> = db.faces().collect();
        faces.sort_by(|a, b| {
            let na = a
                .families
                .first()
                .map(|(n, _)| n.as_str())
                .unwrap_or("");
            let nb = b
                .families
                .first()
                .map(|(n, _)| n.as_str())
                .unwrap_or("");
            na.cmp(nb)
        });
        for face in faces {
            if !face.monospaced {
                continue;
            }
            let fontdb::Source::File(path_buf) = &face.source else {
                continue;
            };
            let path = path_buf.to_string_lossy().into_owned();
            if !seen.insert(path.clone()) {
                continue;
            }
            let family = face
                .families
                .first()
                .map(|(n, _)| n.as_str())
                .unwrap_or("Monospace");
            let style = match face.style {
                fontdb::Style::Normal => "",
                fontdb::Style::Italic => " Italic",
                fontdb::Style::Oblique => " Oblique",
            };
            out.push(MonospaceFontEntry {
                path,
                label: format!("{family}{style}"),
            });
        }
    }

    #[cfg(target_os = "android")]
    {
        for path in MONO_FONT_PATHS {
            if std::path::Path::new(path).is_file() {
                out.push(MonospaceFontEntry {
                    path: path.to_string(),
                    label: path.to_string(),
                });
            }
        }
    }

    out
}

/// Build the terminal font stack: user/system mono → bundled Latin → CJK; Braille per-glyph.
pub fn apply_terminal_fonts(ctx: &egui::Context, terminal_font_path: &str) {
    let mut fonts = egui::FontDefinitions::default();

    insert_font(
        &mut fonts,
        FALLBACK_LATIN_FAMILY,
        include_bytes!("../assets/fonts/DejaVuSansMono.ttf").to_vec(),
    );
    insert_font(
        &mut fonts,
        FALLBACK_BRAILLE_FAMILY,
        include_bytes!("../assets/fonts/FreeMono.ttf").to_vec(),
    );

    let user_path = terminal_font_path.trim();
    let mut has_user = false;
    if !user_path.is_empty() {
        if let Some(bytes) = read_file(user_path) {
            insert_font(&mut fonts, USER_MONO_FAMILY, bytes);
            has_user = true;
            log::info!("terminal primary font: {user_path}");
        } else {
            log::warn!("terminal font not found: {user_path}");
        }
    }
    if !has_user {
        if let Some(bytes) = read_first_existing(MONO_FONT_PATHS) {
            insert_font(&mut fonts, USER_MONO_FAMILY, bytes);
            has_user = true;
        }
    }

    if let Some(bytes) = read_first_existing(CJK_FONT_PATHS) {
        insert_font(&mut fonts, "mono_cjk", bytes);
    }

    let mono = fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default();
    mono.clear();
    if has_user {
        mono.push(USER_MONO_FAMILY.to_owned());
    }
    mono.push(FALLBACK_LATIN_FAMILY.to_owned());
    if fonts.font_data.contains_key("mono_cjk") {
        mono.push("mono_cjk".to_owned());
    }

    fonts.families.insert(
        FontFamily::Name(FALLBACK_BRAILLE_FAMILY.into()),
        vec![FALLBACK_BRAILLE_FAMILY.to_owned()],
    );

    if fonts.font_data.contains_key("mono_cjk") {
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "mono_cjk".to_owned());
    }

    ctx.set_fonts(fonts);
    FONT_GENERATION.fetch_add(1, Ordering::Relaxed);
}

pub fn setup_fonts(ctx: &egui::Context, terminal_font_path: &str) {
    apply_terminal_fonts(ctx, terminal_font_path);
}

#[cfg(target_os = "android")]
pub fn tune_android_display(ctx: &egui::Context) {
    let ppp = ctx.pixels_per_point();
    let ppp = if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 };
    let ppp = (ppp * 4.0).round() / 4.0;
    ctx.set_pixels_per_point(ppp);
    ctx.options_mut(|o| {
        o.tessellation_options.feathering = false;
    });
}

#[cfg(not(target_os = "android"))]
pub fn tune_android_display(_ctx: &egui::Context) {}
