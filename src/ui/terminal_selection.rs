use egui::{Painter, Pos2, Rect, Response, TouchPhase, Ui};

use crate::config::TerminalTheme;
use crate::terminal::screen::{cell_display_width, Screen};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPos {
    /// Scrollback-aware line index (see [`Screen::line_at_virtual`]).
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct TerminalSelection {
    pub anchor: CellPos,
    pub cursor: CellPos,
}

impl TerminalSelection {
    pub fn new(anchor: CellPos) -> Self {
        Self {
            anchor,
            cursor: anchor,
        }
    }

    pub fn range(&self) -> (CellPos, CellPos) {
        if self.anchor.line < self.cursor.line
            || (self.anchor.line == self.cursor.line && self.anchor.col <= self.cursor.col)
        {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }

    pub fn text(&self, screen: &Screen) -> String {
        extract_range_text(screen, self.range())
    }
}

pub fn cursor_virtual_pos(screen: &Screen) -> CellPos {
    if screen.in_alternate_screen() {
        CellPos {
            line: screen.cursor_y,
            col: screen.cursor_x,
        }
    } else {
        CellPos {
            line: screen.scrollback_lines() + screen.cursor_y,
            col: screen.cursor_x,
        }
    }
}

/// Update text selection from mouse or touch pointer (press / drag / release).
pub fn update_terminal_selection(
    selection: &mut Option<TerminalSelection>,
    selection_pointer: &mut Option<CellPos>,
    screen: &Screen,
    scroll_offset: usize,
    ui: &Ui,
    term_resp: &Response,
    rect: Rect,
    cell_w: f32,
    cell_h: f32,
    grid_rows: usize,
    grid_cols: usize,
) {
    let has_touch = ui.input(|i| i.has_touch_screen());

    let cell_at = |pos: Pos2| -> Option<CellPos> {
        pointer_to_cell(
            pos,
            rect,
            cell_w,
            cell_h,
            grid_rows,
            grid_cols,
            screen,
            scroll_offset,
        )
    };

    let start_selection = |selection: &mut Option<TerminalSelection>,
                           selection_pointer: &mut Option<CellPos>,
                           cell: CellPos| {
        *selection_pointer = Some(cell);
        *selection = Some(TerminalSelection::new(cell));
    };

    let extend_to = |selection: &mut Option<TerminalSelection>,
                     selection_pointer: &mut Option<CellPos>,
                     cell: CellPos| {
        if let Some(sel) = selection {
            sel.cursor = cell;
        } else if let Some(anchor) = *selection_pointer {
            *selection = Some(TerminalSelection {
                anchor,
                cursor: cell,
            });
        } else {
            start_selection(selection, selection_pointer, cell);
        }
    };

    let mut touch_ended = false;
    if has_touch {
        for event in ui.input(|i| i.events.clone()) {
            let egui::Event::Touch { pos, phase, .. } = event else {
                continue;
            };
            if !rect.contains(pos) {
                continue;
            }
            match phase {
                TouchPhase::Start => {
                    if let Some(cell) = cell_at(pos) {
                        start_selection(selection, selection_pointer, cell);
                    }
                }
                TouchPhase::Move => {
                    if let Some(cell) = cell_at(pos) {
                        extend_to(selection, selection_pointer, cell);
                    }
                }
                TouchPhase::End | TouchPhase::Cancel => touch_ended = true,
            }
        }
    }

    let contains = term_resp.contains_pointer();
    let shift = ui.input(|i| i.modifiers.shift);
    let primary_pressed = ui.input(|i| i.pointer.primary_pressed()) && contains;
    let primary_down = ui.input(|i| i.pointer.primary_down()) && contains;
    let primary_released = ui.input(|i| i.pointer.primary_released());

    if let Some(pos) = term_resp.interact_pointer_pos() {
        if primary_pressed {
            if let Some(cell) = cell_at(pos) {
                if shift {
                    if let Some(sel) = selection {
                        sel.cursor = cell;
                        *selection_pointer = Some(sel.anchor);
                    } else {
                        let anchor = cursor_virtual_pos(screen);
                        *selection = Some(TerminalSelection {
                            anchor,
                            cursor: cell,
                        });
                        *selection_pointer = Some(anchor);
                    }
                } else {
                    start_selection(selection, selection_pointer, cell);
                }
            }
        } else if primary_down || term_resp.dragged() {
            if selection_pointer.is_some() {
                if let Some(cell) = cell_at(pos) {
                    extend_to(selection, selection_pointer, cell);
                }
            }
        } else if term_resp.drag_started() {
            if let Some(cell) = cell_at(pos) {
                start_selection(selection, selection_pointer, cell);
            }
        }

        if !has_touch && term_resp.clicked() && shift {
            if let Some(cell) = cell_at(pos) {
                extend_to(selection, selection_pointer, cell);
            }
        }
    }

    if primary_released || touch_ended {
        finish_pointer_selection(selection, selection_pointer, term_resp);
    }
}

fn finish_pointer_selection(
    selection: &mut Option<TerminalSelection>,
    selection_pointer: &mut Option<CellPos>,
    term_resp: &Response,
) {
    let Some(anchor) = selection_pointer.take() else {
        return;
    };
    let tap = selection
        .as_ref()
        .is_none_or(|s| s.anchor == anchor && s.cursor == anchor);
    if tap && !term_resp.long_touched() && !term_resp.secondary_clicked() {
        *selection = None;
    }
}

pub fn pointer_to_cell(
    pos: Pos2,
    rect: Rect,
    cell_w: f32,
    cell_h: f32,
    rows: usize,
    cols: usize,
    screen: &Screen,
    scroll_offset: usize,
) -> Option<CellPos> {
    if !rect.contains(pos) || cell_w <= 0.0 || cell_h <= 0.0 || rows == 0 || cols == 0 {
        return None;
    }
    let rel = pos - rect.min;
    let row = (rel.y / cell_h).floor() as usize;
    let col = (rel.x / cell_w).floor() as usize;
    let row = row.min(rows.saturating_sub(1));
    let col = col.min(cols.saturating_sub(1));

    let line = if screen.in_alternate_screen() {
        row
    } else {
        screen.viewport_virtual_start(rows, scroll_offset) + row
    };
    Some(CellPos { line, col })
}

pub fn extract_range_text(screen: &Screen, (start, end): (CellPos, CellPos)) -> String {
    let mut out = String::new();
    for line in start.line..=end.line {
        let Some(cells) = screen.line_at_virtual(line) else {
            continue;
        };
        let cols = screen.cols.min(cells.len());
        let col_start = if line == start.line { start.col } else { 0 };
        let col_end = if line == end.line {
            end.col.min(cols.saturating_sub(1))
        } else {
            cols.saturating_sub(1)
        };
        if line > start.line && !screen.virtual_line_wrapped(line) {
            out.push('\n');
        }
        if col_start <= col_end {
            out.push_str(&line_slice_text(cells, col_start, col_end));
        }
    }
    out
}

fn line_slice_text(cells: &[crate::terminal::screen::Cell], start_col: usize, end_col: usize) -> String {
    let end_col = end_col.min(cells.len().saturating_sub(1));
    let mut out = String::new();
    let mut col = start_col;
    while col <= end_col {
        if col >= cells.len() {
            break;
        }
        if cells[col].wide_continuation {
            col += 1;
            continue;
        }
        let ch = cells[col].ch;
        if ch != '\0' {
            out.push(ch);
        }
        col += cell_display_width(cells, col).max(1);
    }
    out.trim_end().to_string()
}

pub fn paint_selection(
    painter: &Painter,
    screen: &Screen,
    theme: &TerminalTheme,
    rect: Rect,
    cell_w: f32,
    cell_h: f32,
    scroll_offset: usize,
    selection: &TerminalSelection,
) {
    let rows = screen.rows;
    let cols = screen.cols;
    if rows == 0 || cols == 0 {
        return;
    }

    let (start, end) = selection.range();
    let in_alt = screen.in_alternate_screen();
    let virtual_start = if in_alt {
        0
    } else {
        screen.viewport_virtual_start(rows, scroll_offset)
    };

    for row in 0..rows {
        let virtual_line = if in_alt { row } else { virtual_start + row };
        if virtual_line < start.line || virtual_line > end.line {
            continue;
        }
        let col_start = if virtual_line == start.line {
            start.col
        } else {
            0
        };
        let col_end = if virtual_line == end.line {
            end.col
        } else {
            cols.saturating_sub(1)
        };

        let y = rect.top() + row as f32 * cell_h;
        let mut col = col_start;
        while col <= col_end && col < cols {
            let span = screen
                .line_at_virtual(virtual_line)
                .map(|cells| cell_display_width(cells, col))
                .unwrap_or(1)
                .max(1);
            let x = rect.left() + col as f32 * cell_w;
            let cell_rect = Rect::from_min_size(
                Pos2::new(x, y),
                egui::vec2(cell_w * span as f32, cell_h),
            );
            painter.rect_filled(cell_rect, egui::CornerRadius::ZERO, theme.selection);
            col += span;
        }
    }
}

pub fn normalize_paste_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn paste_payload(text: &str, bracketed: bool) -> Vec<u8> {
    let normalized = normalize_paste_text(text);
    if bracketed {
        let mut bytes = Vec::with_capacity(normalized.len() + 14);
        bytes.extend_from_slice(b"\x1b[200~");
        bytes.extend_from_slice(normalized.as_bytes());
        bytes.extend_from_slice(b"\x1b[201~");
        bytes
    } else {
        normalized.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_multiline_with_newlines() {
        let mut screen = Screen::new(2, 5);
        screen.cells[0][0].ch = 'a';
        screen.cells[0][1].ch = 'b';
        screen.cells[1][0].ch = 'c';
        let text = extract_range_text(
            &screen,
            (
                CellPos { line: 0, col: 0 },
                CellPos { line: 1, col: 0 },
            ),
        );
        assert_eq!(text, "ab\nc");
    }

    #[test]
    fn trims_trailing_spaces_on_line() {
        let mut screen = Screen::new(1, 4);
        screen.cells[0][0].ch = 'x';
        screen.cells[0][2].ch = 'y';
        let text = extract_range_text(
            &screen,
            (
                CellPos { line: 0, col: 0 },
                CellPos { line: 0, col: 3 },
            ),
        );
        assert_eq!(text, "x y");
    }
}
