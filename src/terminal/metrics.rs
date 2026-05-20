use egui::Ui;

use crate::fonts::terminal_font_id;

/// Measure one terminal cell in points (Latin, CJK, and TUI symbol glyphs must all fit).
pub fn measure_cell(ui: &Ui, font_size: f32, cell_width_scale: f32) -> (f32, f32) {
    let font_id = terminal_font_id(font_size);
    ui.fonts_mut(|f| {
        let mut layout = |text: &str| {
            f.layout(
                text.to_string(),
                font_id.clone(),
                egui::Color32::WHITE,
                f32::INFINITY,
            )
            .rect
        };

        let latin_w = layout("M").width();
        let cjk_w = layout("汉").width() / 2.0;
        let tui_w = layout("⣿")
            .width()
            .max(layout("█").width())
            .max(layout("─").width())
            .max(layout("│").width());
        let cell_w = (latin_w.max(cjk_w).max(tui_w).max(1.0) * cell_width_scale).max(1.0);

        let cell_h = layout("Wg")
            .height()
            .max(layout("汉").height())
            .max(layout("Mg汉_⣿").height())
            .max(font_size * 1.25);

        #[cfg(target_os = "android")]
        let (cell_w, cell_h) = (cell_w.ceil(), (cell_h * 1.2).ceil());

        (cell_w, cell_h)
    })
}
