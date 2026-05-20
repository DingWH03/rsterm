//! Cell-grid terminal painting: one glyph per column (stable grid), suggestion erase.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, FontId, Galley, Painter, Stroke, Ui};

use crate::config::TerminalTheme;
use crate::fonts::terminal_font_id_for_char;
use crate::terminal::screen::{cell_display_width, Cell, Color};

/// Cache shaped single-glyph layouts keyed by character + visual attributes.
#[derive(Default)]
pub struct RowGalleyCache {
    font_size: f32,
    entries: HashMap<u64, Arc<Galley>>,
}

impl RowGalleyCache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn ensure_font(&mut self, font_size: f32) {
        if (self.font_size - font_size).abs() > f32::EPSILON {
            self.font_size = font_size;
            self.entries.clear();
        }
    }

    fn get(&self, key: u64) -> Option<Arc<Galley>> {
        self.entries.get(&key).cloned()
    }

    fn insert(&mut self, key: u64, galley: Arc<Galley>) {
        if self.entries.len() > 4096 {
            self.entries.clear();
        }
        self.entries.insert(key, galley);
    }
}

/// Visual attributes that affect how a glyph is shaped (SGR / vim / zsh suggest).
#[derive(Clone, Copy, PartialEq, Eq)]
struct RunAttrs {
    fg: Color,
    bg: Color,
    bold: bool,
    italic: bool,
    underline: bool,
    reverse: bool,
    dim: bool,
    strikethrough: bool,
}

impl RunAttrs {
    fn from_cell(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            bg: cell.bg,
            bold: cell.bold,
            italic: cell.italic,
            underline: cell.underline,
            reverse: cell.reverse,
            dim: cell.dim,
            strikethrough: cell.strikethrough,
        }
    }

    /// zsh-autosuggestions / completion ghost text (not normal prompt colors).
    fn is_suggestion_style(self) -> bool {
        if self.dim {
            return true;
        }
        match self.fg {
            Color::Indexed(8) => true,
            Color::Indexed(i) if (232..=255).contains(&i) => true,
            _ => false,
        }
    }
}

pub fn paint_row(
    painter: &Painter,
    ui: &Ui,
    cache: &mut RowGalleyCache,
    font_size: f32,
    theme: &TerminalTheme,
    cells: &[Cell],
    cols: usize,
    x_start: f32,
    y: f32,
    cell_w: f32,
    cell_h: f32,
    tui_surface: bool,
) {
    let col_count = cols.min(cells.len());
    if col_count == 0 || cell_w <= 0.0 || cell_h <= 0.0 {
        return;
    }

    cache.ensure_font(font_size);

    // Coalesce adjacent cells with the same background into one fill (htop header bars).
    let mut col = 0usize;
    while col < col_count {
        if cells[col].wide_continuation {
            col += 1;
            continue;
        }
        let span = cell_display_width(cells, col).max(1);
        let attrs = RunAttrs::from_cell(&cells[col]);
        if !matches!(attrs.bg, Color::Default) || attrs.reverse {
            let (_, bg) = resolve_colors(theme, attrs);
            let start_col = col;
            col += span;
            while col < col_count {
                if cells[col].wide_continuation {
                    col += 1;
                    continue;
                }
                let next_span = cell_display_width(cells, col).max(1);
                let next_attrs = RunAttrs::from_cell(&cells[col]);
                if next_attrs != attrs {
                    break;
                }
                col += next_span;
            }
            let x0 = x_start + start_col as f32 * cell_w;
            let x1 = x_start + col as f32 * cell_w;
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(x0, y), egui::pos2(x1, y + cell_h)),
                egui::CornerRadius::ZERO,
                bg,
            );
        } else {
            col += span;
        }
    }

    col = 0;
    while col < col_count {
        if cells[col].wide_continuation {
            col += 1;
            continue;
        }

        let span = cell_display_width(cells, col).max(1);
        let cell = &cells[col];
        let attrs = RunAttrs::from_cell(cell);
        let x = x_start + col as f32 * cell_w;
        let cell_rect = egui::Rect::from_min_size(
            egui::pos2(x, y),
            egui::vec2(cell_w * span as f32, cell_h),
        );

        if !tui_surface && cell.ch == ' ' && attrs.is_suggestion_style() {
            // zsh clears suggestion with dim/gray spaces — erase stale glyphs underneath.
            painter.rect_filled(cell_rect, egui::CornerRadius::ZERO, theme.bg);
        }

        if cell.ch != ' ' {
            let galley = layout_glyph(ui, cache, font_size, theme, cell.ch, attrs);
            let (fg, _) = resolve_colors(theme, attrs);
            paint_glyph_at(&painter, galley, cell_rect, fg);
        } else if attrs.underline {
            // Highlight plugins may underline spaces in a command span.
            let (_, bg) = resolve_colors(theme, attrs);
            if !matches!(cell.bg, Color::Default) || attrs.reverse {
                painter.rect_filled(cell_rect, egui::CornerRadius::ZERO, bg);
            }
        }

        if attrs.underline {
            let (fg, _) = resolve_colors(theme, attrs);
            let y_line = cell_rect.bottom() - 1.0;
            painter.line_segment(
                [egui::pos2(cell_rect.left(), y_line), egui::pos2(cell_rect.right(), y_line)],
                Stroke::new(1.0, fg),
            );
        }

        col += span;
    }
}

fn hash_color(h: u64, color: Color) -> u64 {
    match color {
        Color::Default => h.wrapping_mul(31),
        Color::Indexed(i) => h.wrapping_mul(31).wrapping_add(i as u64),
        Color::Rgb(r, g, b) => h
            .wrapping_mul(31)
            .wrapping_add(r as u64)
            .wrapping_add((g as u64) << 8)
            .wrapping_add((b as u64) << 16),
    }
}

fn hash_glyph(ch: char, attrs: RunAttrs, font_size: f32) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    font_size.to_bits().hash(&mut h);
    ch.hash(&mut h);
    crate::fonts::needs_braille_font(ch).hash(&mut h);
    let mut n = h.finish();
    n = hash_color(n, attrs.fg);
    n = hash_color(n, attrs.bg);
    n = n
        .wrapping_mul(17)
        .wrapping_add(attrs.bold as u64)
        .wrapping_add(attrs.italic as u64)
        .wrapping_add(attrs.underline as u64)
        .wrapping_add(attrs.reverse as u64)
        .wrapping_add(attrs.dim as u64)
        .wrapping_add(attrs.strikethrough as u64);
    n
}

fn blend_dim(fg: egui::Color32, bg: egui::Color32) -> egui::Color32 {
    const T: f32 = 0.55;
    egui::Color32::from_rgb(
        (fg.r() as f32 * T + bg.r() as f32 * (1.0 - T)) as u8,
        (fg.g() as f32 * T + bg.g() as f32 * (1.0 - T)) as u8,
        (fg.b() as f32 * T + bg.b() as f32 * (1.0 - T)) as u8,
    )
}

fn resolve_colors(theme: &TerminalTheme, attrs: RunAttrs) -> (egui::Color32, egui::Color32) {
    let mut fg = match attrs.fg {
        Color::Default => theme.fg,
        Color::Indexed(i) => {
            let idx = if attrs.bold && i < 8 { i + 8 } else { i };
            theme.indexed_color(idx)
        }
        Color::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
    };
    let mut bg = match attrs.bg {
        Color::Default => theme.bg,
        Color::Indexed(i) => theme.indexed_color(i),
        Color::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
    };
    if attrs.reverse {
        std::mem::swap(&mut fg, &mut bg);
    }
    if attrs.dim {
        fg = blend_dim(fg, bg);
    }
    (fg, bg)
}

fn text_format(font_id: FontId, fg: egui::Color32, bg: egui::Color32, attrs: RunAttrs) -> TextFormat {
    let stroke = Stroke::new(1.0, fg);
    TextFormat {
        font_id,
        color: fg,
        background: if matches!(attrs.bg, Color::Default) && !attrs.reverse {
            egui::Color32::TRANSPARENT
        } else {
            bg
        },
        italics: attrs.italic,
        underline: Stroke::NONE,
        strikethrough: if attrs.strikethrough { stroke } else { Stroke::NONE },
        valign: Align::Min,
        ..Default::default()
    }
}

fn paint_glyph_at(
    painter: &Painter,
    galley: std::sync::Arc<Galley>,
    cell_rect: egui::Rect,
    fg: egui::Color32,
) {
    let pos = egui::pos2(cell_rect.left(), cell_rect.top());
    painter
        .with_clip_rect(cell_rect)
        .galley(pos, galley, fg);
}

fn layout_glyph(
    ui: &Ui,
    cache: &mut RowGalleyCache,
    font_size: f32,
    theme: &TerminalTheme,
    ch: char,
    attrs: RunAttrs,
) -> Arc<Galley> {
    let key = hash_glyph(ch, attrs, font_size);
    if let Some(g) = cache.get(key) {
        return g;
    }

    let (fg, bg) = resolve_colors(theme, attrs);
    let font_id = terminal_font_id_for_char(ch, font_size);
    let format = text_format(font_id, fg, bg, attrs);
    let mut utf8 = [0u8; 4];
    let ch_str = ch.encode_utf8(&mut utf8);
    let job = LayoutJob::single_section(ch_str.to_owned(), format);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    cache.insert(key, galley.clone());
    galley
}
