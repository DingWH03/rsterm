use egui::{FontId, Painter, Ui};

use crate::config::{CursorStyle, TerminalTheme};
use crate::terminal::screen::{cell_display_width, Cell, Color, Screen};

/// Paint the visible terminal grid into `rect`.
/// `scroll_offset` is how many lines scrolled up from the live view (0 = current screen).
pub fn paint_screen(
    painter: &Painter,
    ui: &Ui,
    screen: &Screen,
    theme: &TerminalTheme,
    rect: egui::Rect,
    font_size: f32,
    cell_w: f32,
    cell_h: f32,
    scroll_offset: usize,
    cursor_style: CursorStyle,
) {
    let rows = screen.rows;
    let cols = screen.cols;
    if rows == 0 || cols == 0 || cell_w <= 0.0 || cell_h <= 0.0 {
        return;
    }

    let font_id = FontId::monospace(font_size);
    let in_alt = screen.in_alternate_screen();
    let sb_lines = if in_alt { 0 } else { screen.scrollback_lines() };
    let effective_offset = if in_alt { 0 } else { scroll_offset };
    let virtual_start = sb_lines.saturating_sub(effective_offset);
    let show_cursor = effective_offset == 0 && screen.cursor_visible;
    let block_cursor = show_cursor && cursor_style == CursorStyle::Block;

    painter.rect_filled(rect, egui::CornerRadius::ZERO, theme.bg);

    for row in 0..rows {
        let y = rect.top() + row as f32 * cell_h;
        let row_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left(), y),
            egui::pos2(rect.left() + cols as f32 * cell_w, y + cell_h),
        );
        let (cells, highlight_col) = if in_alt {
            (
                screen.cells.get(row).map(|r| r.as_slice()),
                if block_cursor && row == screen.cursor_y {
                    Some(screen.cursor_x)
                } else {
                    None
                },
            )
        } else {
            let virtual_line = virtual_start + row;
            let cells: Option<&[Cell]> = if virtual_line < sb_lines {
                screen.scrollback_row(virtual_line)
            } else if virtual_line < sb_lines + rows {
                screen.cells.get(virtual_line - sb_lines).map(|r| r.as_slice())
            } else {
                None
            };
            let highlight_col = if block_cursor && virtual_line == sb_lines + screen.cursor_y {
                Some(screen.cursor_x)
            } else {
                None
            };
            (cells, highlight_col)
        };
        if let Some(cells) = cells {
            paint_row(
                painter,
                ui,
                cells,
                cols,
                theme,
                &font_id,
                row_rect,
                cell_w,
                highlight_col,
            );
        }
    }
}

pub fn paint_row(
    painter: &Painter,
    ui: &Ui,
    cells: &[Cell],
    cols: usize,
    theme: &TerminalTheme,
    font_id: &FontId,
    row_rect: egui::Rect,
    cell_w: f32,
    cursor_col: Option<usize>,
) {
    let col_count = cols.min(cells.len());

    for col in 0..col_count {
        let cell = &cells[col];
        if cell.wide_continuation {
            continue;
        }

        let span = cell_display_width(cells, col);
        let paint_w = cell_w * span as f32;

        let (mut fg, mut bg) = resolve_colors(theme, cell);
        let at_cursor = cursor_col == Some(col);
        if at_cursor {
            std::mem::swap(&mut fg, &mut bg);
        }
        let x = row_rect.left() + col as f32 * cell_w;
        let cell_rect = egui::Rect::from_min_size(
            egui::pos2(x, row_rect.top()),
            egui::vec2(paint_w, row_rect.height()),
        );

        if at_cursor || !matches!(cell.bg, Color::Default) {
            painter.rect_filled(cell_rect, egui::CornerRadius::ZERO, bg);
        }

        if cell.ch == ' ' {
            continue;
        }

        let mut utf8 = [0u8; 4];
        let ch_str = cell.ch.encode_utf8(&mut utf8);
        let galley = ui.fonts_mut(|f| {
            f.layout(
                ch_str.to_string(),
                font_id.clone(),
                fg,
                f32::INFINITY,
            )
        });
        painter.galley(
            egui::pos2(x, row_rect.top()),
            galley,
            egui::Color32::WHITE,
        );
    }
}

pub fn paint_cursor(
    painter: &Painter,
    screen: &Screen,
    theme: &TerminalTheme,
    rect: egui::Rect,
    cell_w: f32,
    cell_h: f32,
    style: CursorStyle,
) {
    let rows = screen.rows;
    let cols = screen.cols;
    if rows == 0 || cols == 0 || cell_w <= 0.0 || cell_h <= 0.0 {
        return;
    }
    if !screen.cursor_visible || screen.cursor_y >= rows || screen.cursor_x >= cols {
        return;
    }

    let row = screen
        .cells
        .get(screen.cursor_y)
        .map(|r| r.as_slice());
    let span = row
        .map(|r| cell_display_width(r, screen.cursor_x))
        .unwrap_or(1)
        .max(1);
    let cx = rect.left() + screen.cursor_x as f32 * cell_w;
    let cy = rect.top() + screen.cursor_y as f32 * cell_h;
    let cell_rect = egui::Rect::from_min_size(
        egui::pos2(cx, cy),
        egui::vec2(cell_w * span as f32, cell_h),
    );

    match style {
        CursorStyle::Bar => {
            const BAR_WIDTH: f32 = 2.0;
            let bar_w = BAR_WIDTH.min(cell_w);
            let bar_rect = egui::Rect::from_min_size(egui::pos2(cx, cy), egui::vec2(bar_w, cell_h));
            painter.rect_filled(bar_rect, egui::CornerRadius::ZERO, theme.cursor);
        }
        CursorStyle::Block => {
            painter.rect_stroke(
                cell_rect,
                egui::CornerRadius::ZERO,
                egui::Stroke::new(1.0, theme.cursor),
                egui::StrokeKind::Inside,
            );
        }
        CursorStyle::Underline => {
            const LINE_HEIGHT: f32 = 2.0;
            let line_h = LINE_HEIGHT.min(cell_h * 0.2);
            let line_rect = egui::Rect::from_min_max(
                egui::pos2(cx, cy + cell_h - line_h),
                egui::pos2(cx + cell_w, cy + cell_h),
            );
            painter.rect_filled(line_rect, egui::CornerRadius::ZERO, theme.cursor);
        }
    }
}

pub fn resolve_colors(theme: &TerminalTheme, cell: &Cell) -> (egui::Color32, egui::Color32) {
    let mut fg = match cell.fg {
        Color::Default => theme.fg,
        Color::Indexed(i) => {
            let idx = if cell.bold && i < 8 { i + 8 } else { i };
            theme.ansi_color(idx)
        }
        Color::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
    };
    let mut bg = match cell.bg {
        Color::Default => theme.bg,
        Color::Indexed(i) => theme.ansi_color(i),
        Color::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
    };
    if cell.dim {
        fg = egui::Color32::from_rgba_unmultiplied(fg.r() / 2, fg.g() / 2, fg.b() / 2, fg.a());
    }
    if cell.reverse {
        std::mem::swap(&mut fg, &mut bg);
    }
    (fg, bg)
}
