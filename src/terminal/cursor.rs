use egui::Painter;

use crate::config::{CursorStyle, TerminalTheme};
use crate::terminal::screen::{cell_display_width, Screen};

pub fn paint_cursor(
    painter: &Painter,
    screen: &Screen,
    theme: &TerminalTheme,
    rect: egui::Rect,
    cell_w: f32,
    cell_h: f32,
    style: CursorStyle,
    now: Option<std::time::Instant>,
) {
    let rows = screen.rows;
    let cols = screen.cols;
    if rows == 0 || cols == 0 || cell_w <= 0.0 || cell_h <= 0.0 {
        return;
    }
    if !screen.cursor_visible || screen.cursor_y >= rows || screen.cursor_x >= cols {
        return;
    }

    let is_blink = matches!(
        style,
        CursorStyle::BarBlink | CursorStyle::BlockBlink | CursorStyle::UnderlineBlink
    );
    if is_blink {
        if let Some(now) = now {
            if (now.elapsed().as_millis() / 530) % 2 == 1 {
                return;
            }
        }
    }

    let row = screen.cells.get(screen.cursor_y).map(|r| r.as_slice());
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
        CursorStyle::Bar | CursorStyle::BarBlink => {
            const BAR_WIDTH: f32 = 2.0;
            let bar_w = BAR_WIDTH.min(cell_w);
            let bar_rect = egui::Rect::from_min_size(egui::pos2(cx, cy), egui::vec2(bar_w, cell_h));
            painter.rect_filled(bar_rect, egui::CornerRadius::ZERO, theme.cursor);
        }
        CursorStyle::Block | CursorStyle::BlockBlink => {
            painter.rect_stroke(
                cell_rect,
                egui::CornerRadius::ZERO,
                egui::Stroke::new(1.0, theme.cursor),
                egui::StrokeKind::Inside,
            );
        }
        CursorStyle::Underline | CursorStyle::UnderlineBlink => {
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
