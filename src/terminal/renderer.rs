use crate::config::TerminalTheme;
use crate::terminal::screen::{Cell, Color, Screen};

pub struct TerminalRenderer {
    pub rows: usize,
    pub cols: usize,
    pub font_size: f32,
    cell_w: f32,
    cell_h: f32,
    scroll_viewport: f32,
}

impl TerminalRenderer {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            font_size: 14.0,
            cell_w: 8.0,
            cell_h: 16.0,
            scroll_viewport: 0.0,
        }
    }

    /// Measure cell size once per frame; values are cached on the renderer.
    pub fn cell_metrics(&mut self, ui: &egui::Ui) -> (f32, f32) {
        let (w, h) = Self::measure_cell(ui, self.font_size);
        self.cell_w = w;
        self.cell_h = h;
        (w, h)
    }

    pub fn set_font_size(&mut self, size: f32) {
        self.font_size = size;
    }

    pub fn measure_cell(ui: &egui::Ui, font_size: f32) -> (f32, f32) {
        let font_id = egui::FontId::monospace(font_size);
        ui.fonts(|f| {
            let width_galley = f.layout(
                "M".to_string(),
                font_id.clone(),
                egui::Color32::WHITE,
                f32::INFINITY,
            );
            // Include descenders so the last screen row is not clipped by the paint rect.
            let height_galley = f.layout(
                "Wg".to_string(),
                font_id,
                egui::Color32::WHITE,
                f32::INFINITY,
            );
            let cell_h = height_galley.rect.height().max(font_size * 1.2);
            (width_galley.rect.width(), cell_h)
        })
    }

    pub fn rendered_size(&self, cell_w: f32, cell_h: f32) -> (f32, f32) {
        (self.cols as f32 * cell_w, self.rows as f32 * cell_h)
    }

    pub fn update_size(&mut self, available: egui::Vec2, cell_w: f32, cell_h: f32) {
        let cols = (available.x / cell_w).floor() as usize;
        let rows = (available.y / cell_h).floor() as usize;
        self.cols = cols.max(1);
        self.rows = rows.max(1);
    }

    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        screen: &Screen,
        theme: &TerminalTheme,
        cursor_visible: bool,
        _scrollback_offset: usize,
    ) -> egui::Response {
        let font_id = egui::FontId::monospace(self.font_size);
        let (cell_w, cell_h) = Self::measure_cell(ui, self.font_size);

        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(self.cols as f32 * cell_w, self.rows as f32 * cell_h),
            egui::Sense::click_and_drag(),
        );

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);

            painter.rect_filled(rect, egui::CornerRadius::ZERO, theme.bg);

            for row in 0..self.rows {
                let y = rect.top() + row as f32 * cell_h;

                let cells: &[Cell] = if row < screen.rows {
                    &screen.cells[row]
                } else {
                    continue;
                };

                let mut run_start: usize = 0;

                for col in 0..self.cols {
                    if col + 1 < self.cols
                        && cells[col].fg == cells[col + 1].fg
                        && cells[col].bg == cells[col + 1].bg
                        && cells[col].bold == cells[col + 1].bold
                        && cells[col].italic == cells[col + 1].italic
                        && cells[col].underline == cells[col + 1].underline
                        && cells[col].dim == cells[col + 1].dim
                        && cells[col].reverse == cells[col + 1].reverse
                        && cells[col].strikethrough == cells[col + 1].strikethrough
                    {
                        continue;
                    }

                    let x_offset = rect.left() + run_start as f32 * cell_w;

                    self.render_text_run(
                        &painter,
                        ui,
                        &font_id,
                        theme,
                        cells,
                        run_start,
                        col + 1,
                        x_offset,
                        y,
                        cell_w,
                        cell_h,
                    );

                    run_start = col + 1;
                }
            }

            if cursor_visible && screen.cursor_visible {
                let cx = rect.left() + screen.cursor_x as f32 * cell_w;
                let cy = rect.top() + screen.cursor_y as f32 * cell_h;
                let cursor_rect = egui::Rect::from_min_size(
                    egui::pos2(cx, cy),
                    egui::vec2(cell_w, cell_h),
                );
                painter.rect_stroke(
                    cursor_rect,
                    egui::CornerRadius::ZERO,
                    egui::Stroke::new(1.0, theme.cursor),
                    egui::StrokeKind::Inside,
                );
            }
        }

        response
    }

    fn render_text_run(
        &self,
        painter: &egui::Painter,
        ui: &egui::Ui,
        font_id: &egui::FontId,
        theme: &TerminalTheme,
        cells: &[Cell],
        start: usize,
        end: usize,
        x: f32,
        y: f32,
        cell_w: f32,
        cell_h: f32,
    ) {
        let mut text = String::with_capacity(end - start);
        for i in start..end {
            if i < cells.len() {
                let cell = &cells[i];
                text.push(cell.ch);
            }
        }

        if text.is_empty() || start >= cells.len() {
            return;
        }

        let cell = &cells[start];
        let (mut fg, mut bg) = resolve_colors(theme, cell);

        if cell.reverse {
            std::mem::swap(&mut fg, &mut bg);
        }

        let run_rect = egui::Rect::from_min_size(
            egui::pos2(x, y),
            egui::vec2(cell_w * (end - start) as f32, cell_h),
        );
        if !matches!(cell.bg, Color::Default) {
            painter.rect_filled(run_rect, egui::CornerRadius::ZERO, bg);
        }

        let _rich = egui::RichText::new(&text)
            .font(font_id.clone())
            .color(fg);

        let galley = ui.fonts(|f| {
            f.layout(
                text,
                font_id.clone(),
                fg,
                f32::INFINITY,
            )
        });

        let text_pos = egui::pos2(x, y);
        painter.galley(text_pos, galley, fg);

        if cell.underline {
            let ul_y = y + cell_h - 1.0;
            painter.line_segment(
                [egui::pos2(x, ul_y), egui::pos2(x + run_rect.width(), ul_y)],
                egui::Stroke::new(1.0, fg),
            );
        }
    }
}

fn resolve_colors(theme: &TerminalTheme, cell: &Cell) -> (egui::Color32, egui::Color32) {
    let fg = match cell.fg {
        Color::Default => theme.fg,
        Color::Indexed(i) => theme.ansi_color(i),
        Color::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
    };
    let bg = match cell.bg {
        Color::Default => theme.bg,
        Color::Indexed(i) => theme.ansi_color(i),
        Color::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
    };
    (fg, bg)
}
