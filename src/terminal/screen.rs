use std::collections::VecDeque;

use unicode_width::UnicodeWidthChar;

use crate::terminal::parser::{TermEvent, TermHandler};

/// Scrollback buffer size
const TAB_STOPS_EVERY: usize = 8;

/// Max scrollback lines (configurable per screen)
const MAX_SCROLLBACK: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    pub dim: bool,
    pub strikethrough: bool,
    /// Second column of a wcwidth=2 character (do not paint separately).
    pub wide_continuation: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            italic: false,
            underline: false,
            blink: false,
            reverse: false,
            dim: false,
            strikethrough: false,
            wide_continuation: false,
        }
    }
}

/// Terminal display width (0–2 columns).
pub fn char_display_width(c: char) -> usize {
    match c.width() {
        Some(0) => 0,
        Some(w) => w.min(2),
        None => 1,
    }
}

/// How many columns this cell occupies when painted (0 if continuation slot).
pub fn cell_display_width(cells: &[Cell], col: usize) -> usize {
    if col >= cells.len() {
        return 1;
    }
    if cells[col].wide_continuation {
        return 0;
    }
    if col + 1 < cells.len() && cells[col + 1].wide_continuation {
        return 2;
    }
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

pub struct Screen {
    /// Visible screen rows (row-major, index 0 = top line)
    pub cells: Vec<Vec<Cell>>,
    pub rows: usize,
    pub cols: usize,

    pub cursor_x: usize,
    pub cursor_y: usize,

    saved_cursor_x: usize,
    saved_cursor_y: usize,

    scroll_top: usize,
    scroll_bottom: usize,

    current_attrs: Cell,
    default_attrs: Cell,

    /// Tab stops per column
    tab_stops: Vec<bool>,

    /// Scrollback buffer (newer lines at end)
    scrollback: VecDeque<Vec<Cell>>,
    scrollback_limit: usize,

    /// Whether cursor is visible
    pub cursor_visible: bool,

    /// Main screen saved while alternate screen (vim, less, etc.) is active.
    saved_main: Option<MainScreenState>,

    /// Current character set designator
    charset: u8,
    g0_charset: Charset,
    g1_charset: Charset,

    /// Pending outgoing events (e.g. device reports)
    outgoing: Vec<TermEvent>,

    /// Origin mode (DECOM)
    origin_mode: bool,

    /// Auto-wrap mode
    auto_wrap: bool,

    /// Insert/replace mode
    insert_mode: bool,

    /// Application cursor keys
    app_cursor_keys: bool,

    /// Application keypad
    app_keypad: bool,

    /// Bracketed paste
    bracketed_paste: bool,

    /// xterm DECSET ?1000 — report mouse click/release.
    mouse_report_clicks: bool,
    /// xterm DECSET ?1002 — report mouse drag with button down.
    mouse_report_drag: bool,
    /// xterm DECSET ?1003 — report all mouse motion.
    mouse_report_motion: bool,
    /// xterm DECSET ?1006 — SGR extended mouse coordinates.
    mouse_sgr_encoding: bool,

    /// Title
    pub title: String,

    /// Deferred `\r` (applied on next output unless followed immediately by `\n`).
    pending_cr: bool,

    /// Skip the LF in a second consecutive `\r\n` (zsh: `\r\n\r\n` → one line break).
    suppress_extra_crlf_newline: bool,

    /// Absorbing zsh `%` + space padding before the prompt (do not wrap past EOL).
    in_zsh_line_pad: bool,

    /// zsh POSTDISPLAY: first padding space is painted but must not advance the cursor
    /// (zsh CUB count excludes it; advancing breaks the per-char `vim` redraw).
    postdisplay_leading_space: bool,

    /// Reused empty row for [`Self::scroll_up`] (avoids allocating every wrapped line).
    scroll_scratch: Vec<Cell>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Charset {
    UsAscii,
    DecGraphics,
}

struct MainScreenState {
    cells: Vec<Vec<Cell>>,
    cursor_x: usize,
    cursor_y: usize,
    current_attrs: Cell,
    saved_cursor_x: usize,
    saved_cursor_y: usize,
    scroll_top: usize,
    scroll_bottom: usize,
}

impl Screen {
    pub fn new(rows: usize, cols: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
        let mut tab_stops = vec![false; cols];
        for t in (0..cols).step_by(TAB_STOPS_EVERY) {
            tab_stops[t] = true;
        }

        Self {
            rows,
            cols,
            cells,
            cursor_x: 0,
            cursor_y: 0,
            saved_cursor_x: 0,
            saved_cursor_y: 0,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            current_attrs: Cell::default(),
            default_attrs: Cell::default(),
            tab_stops,
            scrollback: VecDeque::with_capacity(5000),
            scrollback_limit: 5000,
            cursor_visible: true,
            saved_main: None,
            charset: 0,
            g0_charset: Charset::UsAscii,
            g1_charset: Charset::UsAscii,
            outgoing: Vec::new(),
            origin_mode: false,
            auto_wrap: true,
            insert_mode: false,
            app_cursor_keys: false,
            app_keypad: false,
            bracketed_paste: false,
            mouse_report_clicks: false,
            mouse_report_drag: false,
            mouse_report_motion: false,
            mouse_sgr_encoding: false,
            title: String::new(),
            pending_cr: false,
            suppress_extra_crlf_newline: false,
            in_zsh_line_pad: false,
            postdisplay_leading_space: false,
            scroll_scratch: Vec::new(),
        }
    }

    fn mark_postdisplay_suggest_attrs(&mut self) {
        let suggest_fg = match self.current_attrs.fg {
            Color::Indexed(8) => true,
            Color::Indexed(i) if (232..=255).contains(&i) => true,
            _ => false,
        };
        self.postdisplay_leading_space = self.current_attrs.dim || suggest_fg;
    }

    fn blank_scroll_row(&mut self) -> Vec<Cell> {
        if self.scroll_scratch.len() != self.cols {
            self.scroll_scratch.resize(self.cols, Cell::default());
        } else {
            for cell in &mut self.scroll_scratch {
                *cell = Cell::default();
            }
        }
        std::mem::take(&mut self.scroll_scratch)
    }

    fn row_is_blank(&self, y: usize) -> bool {
        self.cells
            .get(y)
            .is_some_and(|row| row.iter().all(|c| c.ch == ' ' && !c.wide_continuation))
    }

    /// zsh preprompt: `%` then spaces until `\r`, used to clear the line before drawing the prompt.
    fn row_is_zsh_clear_pad(&self, y: usize) -> bool {
        let Some(row) = self.cells.get(y) else {
            return false;
        };
        if row.first().is_none_or(|c| c.ch != '%') {
            return false;
        }
        row[1..]
            .iter()
            .all(|c| c.ch == ' ' && !c.wide_continuation)
    }

    fn clear_row(&mut self, y: usize) {
        if let Some(row) = self.cells.get_mut(y) {
            for cell in row.iter_mut() {
                *cell = Cell::default();
            }
        }
    }

    /// Space cell using the active SGR (for EL/ED/clrtoeol in ncurses/htop).
    fn erased_cell_from(&self, attrs: Cell) -> Cell {
        Cell {
            ch: ' ',
            wide_continuation: false,
            ..attrs
        }
    }

    fn erased_cell(&self) -> Cell {
        self.erased_cell_from(self.current_attrs)
    }

    fn erased_cell_default(&self) -> Cell {
        self.erased_cell_from(self.default_attrs)
    }

    fn erase_cells_in_row(&mut self, y: usize, x_start: usize, x_end: usize, use_default: bool) {
        let blank = if use_default {
            self.erased_cell_default()
        } else {
            self.erased_cell()
        };
        if let Some(row) = self.cells.get_mut(y) {
            for x in x_start..x_end.min(row.len()) {
                row[x] = blank;
            }
        }
    }

    /// Handle LF from output or from the `\n` in `\r\n`.
    /// Only `\r\n\r\n` collapses to a single newline; bare `\n` from program output is never swallowed.
    fn newline_from_lf(&mut self, preceded_by_cr: bool) {
        if self.in_alternate_screen() {
            self.newline();
            return;
        }
        if preceded_by_cr {
            if self.suppress_extra_crlf_newline {
                self.suppress_extra_crlf_newline = false;
                return;
            }
            // zsh preprompt is `\r\n%<spaces>\r<prompt>`; skip the leading `\r\n` when we are
            // already on a fresh blank line after command output ending with `\n`.
            if self.cursor_x == 0 && self.row_is_blank(self.cursor_y) {
                return;
            }
            self.newline();
            self.suppress_extra_crlf_newline = true;
        } else {
            self.newline();
            self.suppress_extra_crlf_newline = false;
        }
    }

    /// IND / NEL and similar — not part of zsh's double-`\r\n` quirk.
    fn maybe_newline(&mut self) {
        self.pending_cr = false;
        if self.in_alternate_screen() {
            self.newline();
            return;
        }
        self.newline();
        self.suppress_extra_crlf_newline = false;
    }

    pub fn in_alternate_screen(&self) -> bool {
        self.saved_main.is_some()
    }

    fn enter_alternate_screen(&mut self, save_cursor: bool) {
        if self.saved_main.is_some() {
            // Nested smcup or recovery: clear alternate buffer again.
            self.cells = vec![vec![Cell::default(); self.cols]; self.rows];
            self.cursor_x = 0;
            self.cursor_y = 0;
            self.scroll_top = 0;
            self.scroll_bottom = self.rows.saturating_sub(1);
            self.origin_mode = false;
            self.pending_cr = false;
            self.suppress_extra_crlf_newline = false;
            return;
        }
        let snapshot = MainScreenState {
            cells: std::mem::replace(
                &mut self.cells,
                vec![vec![Cell::default(); self.cols]; self.rows],
            ),
            cursor_x: self.cursor_x,
            cursor_y: self.cursor_y,
            current_attrs: self.current_attrs,
            saved_cursor_x: self.saved_cursor_x,
            saved_cursor_y: self.saved_cursor_y,
            scroll_top: self.scroll_top,
            scroll_bottom: self.scroll_bottom,
        };
        if save_cursor {
            self.saved_cursor_x = snapshot.cursor_x;
            self.saved_cursor_y = snapshot.cursor_y;
        }
        self.saved_main = Some(snapshot);
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);
        self.origin_mode = false;
        self.current_attrs = self.default_attrs;
        self.pending_cr = false;
        self.suppress_extra_crlf_newline = false;
        self.in_zsh_line_pad = false;
    }

    fn leave_alternate_screen(&mut self, restore_cursor: bool) {
        let Some(main) = self.saved_main.take() else {
            return;
        };
        self.cells = main.cells;
        self.current_attrs = main.current_attrs;
        self.scroll_top = main.scroll_top;
        self.scroll_bottom = main.scroll_bottom;
        if restore_cursor {
            self.cursor_x = main.cursor_x;
            self.cursor_y = main.cursor_y;
            self.saved_cursor_x = main.saved_cursor_x;
            self.saved_cursor_y = main.saved_cursor_y;
        }
        self.pending_cr = false;
        self.suppress_extra_crlf_newline = false;
        self.in_zsh_line_pad = false;
    }

    fn flush_pending_cr(&mut self) {
        if self.pending_cr {
            if self.in_zsh_line_pad || self.row_is_zsh_clear_pad(self.cursor_y) {
                self.clear_row(self.cursor_y);
                self.in_zsh_line_pad = false;
            } else if !self.in_alternate_screen() {
                // Shell progress bars (apt, wget): full-line overwrite after '\r'.
                // Alternate-screen apps (htop, vim) often '\r' then patch columns via CUP;
                // clearing the row would erase cells they do not rewrite.
                self.clear_row(self.cursor_y);
            }
            self.cursor_x = 0;
            self.pending_cr = false;
        }
    }

    fn resize_grid(cells: &[Vec<Cell>], old_rows: usize, old_cols: usize, rows: usize, cols: usize) -> Vec<Vec<Cell>> {
        (0..rows)
            .map(|r| {
                let mut row_cells = vec![Cell::default(); cols];
                if r < old_rows {
                    let src_row = &cells[r];
                    let copy_len = cols.min(old_cols);
                    row_cells[..copy_len].copy_from_slice(&src_row[..copy_len]);
                }
                row_cells
            })
            .collect()
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        if rows == self.rows && cols == self.cols {
            return;
        }

        let old_rows = self.rows;
        let old_cols = self.cols;
        if self.in_alternate_screen() {
            // Full-screen TUIs repaint after SIGWINCH; keeping old cells causes ghost rows (btop).
            self.cells = vec![vec![Cell::default(); cols]; rows];
            self.cursor_x = 0;
            self.cursor_y = 0;
            self.scroll_top = 0;
            self.scroll_bottom = rows.saturating_sub(1);
            self.origin_mode = false;
        } else {
            self.cells = Self::resize_grid(&self.cells, old_rows, old_cols, rows, cols);
        }
        if let Some(main) = self.saved_main.as_mut() {
            main.cells = Self::resize_grid(&main.cells, old_rows, old_cols, rows, cols);
        }
        self.rows = rows;
        self.cols = cols;
        self.cursor_y = self.cursor_y.min(rows.saturating_sub(1));
        self.cursor_x = self.cursor_x.min(cols.saturating_sub(1));
        if !self.in_alternate_screen() {
            self.scroll_bottom = rows.saturating_sub(1);
            self.scroll_top = self.scroll_top.min(self.scroll_bottom);
        }

        let mut tab_stops = vec![false; cols];
        for t in (0..cols).step_by(TAB_STOPS_EVERY) {
            tab_stops[t] = true;
        }
        self.tab_stops = tab_stops;
        self.scroll_scratch.clear();
    }

    fn clear_wide_trailer(&mut self, row: usize, col: usize) {
        if col + 1 < self.cols && self.cells[row][col + 1].wide_continuation {
            self.cells[row][col + 1] = Cell::default();
        }
    }

    fn ensure_cursor_fits(&mut self, width: usize) {
        if width == 0 {
            return;
        }
        if self.cursor_x + width <= self.cols {
            return;
        }
        if self.auto_wrap {
            self.newline();
            self.cursor_x = 0;
        }
    }

    pub fn put_char(&mut self, c: char) {
        let width = char_display_width(c);
        if width == 0 {
            return;
        }

        if c == '%' && self.cursor_x == 0 {
            self.in_zsh_line_pad = true;
        }
        if self.in_zsh_line_pad && c == ' ' && self.cursor_x >= self.cols.saturating_sub(1) {
            return;
        }

        let attrs = self.current_attrs;

        if self.postdisplay_leading_space && c == ' ' && width == 1 {
            self.postdisplay_leading_space = false;
            if self.cursor_x >= self.cols {
                return;
            }
            let row = self.cursor_y;
            let col = self.cursor_x;
            self.clear_wide_trailer(row, col);
            let mut cell = attrs;
            cell.ch = ' ';
            cell.wide_continuation = false;
            self.cells[row][col] = cell;
            return;
        }

        if self.insert_mode {
            let row = &mut self.cells[self.cursor_y];
            for i in (self.cursor_x..self.cols.saturating_sub(width)).rev() {
                row[i + width] = row[i];
            }
        }

        if self.cursor_x >= self.cols {
            if self.auto_wrap {
                self.newline();
                self.cursor_x = 0;
            } else {
                return;
            }
        }

        self.ensure_cursor_fits(width);
        if self.cursor_x + width > self.cols {
            return;
        }

        let row = self.cursor_y;
        let col = self.cursor_x;
        self.clear_wide_trailer(row, col);

        let mut cell = attrs;
        cell.ch = c;
        cell.wide_continuation = false;
        self.cells[row][col] = cell;

        if width == 2 {
            self.cells[row][col + 1] = Cell {
                wide_continuation: true,
                ch: ' ',
                ..attrs
            };
        }

        self.cursor_x += width;
        if self.cursor_x >= self.cols && self.auto_wrap {
            self.newline();
            self.cursor_x = 0;
        }
    }

    pub fn newline(&mut self) {
        if self.cursor_y == self.scroll_bottom {
            self.scroll_up(1);
        } else {
            self.cursor_y = (self.cursor_y + 1).min(self.rows.saturating_sub(1));
        }
        self.cursor_x = 0;
    }

    /// BS (0x08): move cursor left without erasing (zsh uses this for line redraw).
    pub fn backspace(&mut self) {
        if self.cursor_x == 0 {
            return;
        }
        self.cursor_x -= 1;
        if self.cursor_x > 0 && self.cells[self.cursor_y][self.cursor_x].wide_continuation {
            self.cursor_x -= 1;
        }
    }

    /// DEL (0x7f): erase the cell before the cursor.
    pub fn erase_left(&mut self) {
        if self.cursor_x == 0 {
            return;
        }
        self.backspace();
        let y = self.cursor_y;
        let x = self.cursor_x;
        let erase_w = cell_display_width(&self.cells[y], x);
        for i in 0..erase_w {
            if x + i < self.cols {
                self.cells[y][x + i] = Cell::default();
            }
        }
    }

    /// Display-column index of the cursor (for CUB/CUF/CHA).
    fn cursor_display_col(&self) -> usize {
        let row = &self.cells[self.cursor_y];
        let mut display_col = 0usize;
        let mut col = 0usize;
        while col < self.cursor_x && col < self.cols {
            if row[col].wide_continuation {
                col += 1;
                continue;
            }
            display_col += cell_display_width(row, col).max(1);
            col += cell_display_width(row, col).max(1);
        }
        display_col
    }

    fn set_cursor_display_col(&mut self, target: usize) {
        let row = &self.cells[self.cursor_y];
        let mut display_col = 0usize;
        let mut col = 0usize;
        while col < self.cols {
            if row[col].wide_continuation {
                col += 1;
                continue;
            }
            let w = cell_display_width(row, col).max(1);
            if display_col + w > target {
                self.cursor_x = col;
                self.normalize_cursor_x();
                return;
            }
            display_col += w;
            col += w;
        }
        self.cursor_x = self.cols.saturating_sub(1);
        self.normalize_cursor_x();
    }

    fn cursor_step_left(&mut self, n: usize) {
        let target = self.cursor_display_col().saturating_sub(n);
        self.set_cursor_display_col(target);
    }

    fn cursor_step_right(&mut self, n: usize) {
        let target = self.cursor_display_col().saturating_add(n);
        self.set_cursor_display_col(target);
    }

    fn normalize_cursor_x(&mut self) {
        if self.cursor_x < self.cols
            && self.cells[self.cursor_y][self.cursor_x].wide_continuation
        {
            self.cursor_x = (self.cursor_x + 1).min(self.cols.saturating_sub(1));
        }
    }

    pub fn advance_tabs(&mut self) {
        for i in (self.cursor_x + 1)..self.cols {
            if self.tab_stops[i] {
                self.cursor_x = i;
                return;
            }
        }
        self.cursor_x = self.cols.saturating_sub(1);
    }

    fn scroll_up(&mut self, n: usize) {
        let n = n.min(self.scroll_bottom - self.scroll_top + 1);
        let start = self.scroll_top;
        let use_scrollback = !self.in_alternate_screen();

        for _ in 0..n {
            let removed = self.cells.remove(start);
            if use_scrollback {
                if self.scrollback.len() >= self.scrollback_limit {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(removed);
            }
            let blank = self.blank_scroll_row();
            self.cells.insert(self.scroll_bottom, blank);
        }
    }

    fn scroll_down(&mut self, n: usize) {
        let n = n.min(self.scroll_bottom - self.scroll_top + 1);
        for _ in 0..n {
            let _removed = self.cells.remove(self.scroll_bottom);
            self.cells
                .insert(self.scroll_top, vec![Cell::default(); self.cols]);
        }
    }

    fn set_cursor(&mut self, row: usize, col: usize) {
        let row = if self.origin_mode {
            (self.scroll_top + row).min(self.scroll_bottom)
        } else {
            row.min(self.rows.saturating_sub(1))
        };
        self.cursor_y = row;
        self.cursor_x = col.min(self.cols.saturating_sub(1));
        self.normalize_cursor_x();
    }

    /// Returns the effective scrollback viewable range
    pub fn set_scrollback_limit(&mut self, limit: usize) {
        self.scrollback_limit = limit.min(MAX_SCROLLBACK);
    }

    pub fn scrollback_lines(&self) -> usize {
        self.scrollback.len()
    }

    pub fn scrollback_row(&self, index: usize) -> Option<&[Cell]> {
        self.scrollback.get(index).map(|v| v.as_slice())
    }

    /// Line in scrollback + main buffer coordinates (used for selection and copy).
    pub fn line_at_virtual(&self, virtual_line: usize) -> Option<&[Cell]> {
        if self.in_alternate_screen() {
            return self.cells.get(virtual_line).map(|r| r.as_slice());
        }
        let sb = self.scrollback.len();
        if virtual_line < sb {
            self.scrollback.get(virtual_line).map(|v| v.as_slice())
        } else if virtual_line < sb + self.rows {
            self.cells
                .get(virtual_line - sb)
                .map(|r| r.as_slice())
        } else {
            None
        }
    }

    pub fn bracketed_paste_enabled(&self) -> bool {
        self.bracketed_paste
    }

    pub fn mouse_tracking_active(&self) -> bool {
        self.mouse_report_clicks || self.mouse_report_drag || self.mouse_report_motion
    }

    pub fn mouse_report_clicks(&self) -> bool {
        self.mouse_report_clicks
    }

    pub fn mouse_report_drag(&self) -> bool {
        self.mouse_report_drag
    }

    pub fn mouse_report_motion(&self) -> bool {
        self.mouse_report_motion
    }

    pub fn mouse_sgr_encoding(&self) -> bool {
        self.mouse_sgr_encoding
    }

    pub fn application_cursor_keys(&self) -> bool {
        self.app_cursor_keys
    }

    pub fn drain_outgoing(&mut self) -> Vec<TermEvent> {
        std::mem::take(&mut self.outgoing)
    }
}

impl TermHandler for Screen {
    fn print(&mut self, c: char) {
        self.flush_pending_cr();
        self.suppress_extra_crlf_newline = false;
        if self.in_zsh_line_pad && c != ' ' {
            self.in_zsh_line_pad = false;
        }
        let ch = match self.charset {
            0 => match self.g0_charset {
                Charset::UsAscii => c,
                Charset::DecGraphics => dec_graphics(c),
            },
            1 => match self.g1_charset {
                Charset::UsAscii => c,
                Charset::DecGraphics => dec_graphics(c),
            },
            _ => c,
        };
        self.put_char(ch);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => {}                           // BEL - ignore
            0x08 => {
                self.flush_pending_cr();
                self.backspace();
            }
            0x7F => {
                self.flush_pending_cr();
                self.erase_left();
            }
            0x09 => {
                self.flush_pending_cr();
                self.advance_tabs();
            }
            0x0A | 0x0B | 0x0C => {
                let preceded_by_cr = self.pending_cr;
                self.pending_cr = false;
                self.newline_from_lf(preceded_by_cr);
            }
            0x0D => self.pending_cr = true,
            0x0E => {
                self.flush_pending_cr();
                self.charset = 1;
            }
            0x0F => {
                self.flush_pending_cr();
                self.charset = 0;
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], final_byte: u8) {
        self.flush_pending_cr();
        // SS3 (ESC O x): same cursor motion as CSI ESC [ x
        if intermediates == [b'O'] {
            match final_byte {
                b'A' | b'B' | b'C' | b'D' => {
                    self.csi_dispatch(&[1], &[], final_byte);
                    return;
                }
                _ => {}
            }
        }

        match final_byte {
            b'D' => {
                // IND - index
                self.maybe_newline();
            }
            b'E' => {
                // NEL - next line
                self.cursor_x = 0;
                self.maybe_newline();
            }
            b'H' => {
                // HTS - horizontal tab set
                if self.cursor_x < self.cols {
                    self.tab_stops[self.cursor_x] = true;
                }
            }
            b'M' => {
                // RI - reverse index
                if self.cursor_y == self.scroll_top {
                    self.scroll_down(1);
                } else {
                    self.cursor_y = self.cursor_y.saturating_sub(1);
                }
            }
            b'7' => {
                // DECSC - save cursor
                self.saved_cursor_x = self.cursor_x;
                self.saved_cursor_y = self.cursor_y;
            }
            b'8' => {
                // DECRC - restore cursor
                self.cursor_x = self.saved_cursor_x.min(self.cols.saturating_sub(1));
                self.cursor_y = self.saved_cursor_y.min(self.rows.saturating_sub(1));
            }
            b'=' => {
                // DECKPAM - application keypad
                self.app_keypad = true;
            }
            b'>' => {
                // DECKPNM - normal keypad
                self.app_keypad = false;
            }
            b'c' => {
                // RIS - reset to initial state (drop alternate buffer snapshot)
                *self = Self::new(self.rows, self.cols);
            }
            b'(' => {
                // SCS G0
                if let Some(&b) = intermediates.first() {
                    self.g0_charset = match b {
                        b'0' => Charset::DecGraphics,
                        b'B' => Charset::UsAscii,
                        _ => Charset::UsAscii,
                    };
                }
            }
            b')' => {
                // SCS G1
                if let Some(&b) = intermediates.first() {
                    self.g1_charset = match b {
                        b'0' => Charset::DecGraphics,
                        b'B' => Charset::UsAscii,
                        _ => Charset::UsAscii,
                    };
                }
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], final_byte: u8) {
        self.flush_pending_cr();
        let def = |i: usize, d: i64| -> i64 {
            params.get(i).map(|&v| if v == 0 { d } else { v }).unwrap_or(d)
        };

        match final_byte {
            b'A' => {
                // CUU - cursor up
                let n = def(0, 1) as usize;
                self.cursor_y = self.cursor_y.saturating_sub(n);
            }
            b'B' => {
                // CUD - cursor down
                let n = def(0, 1) as usize;
                let limit = self.rows.saturating_sub(1);
                self.cursor_y = (self.cursor_y + n).min(limit);
            }
            b'C' => {
                // CUF - cursor forward (buffer columns in TUIs; display columns for zsh on main)
                let n = def(0, 1) as usize;
                if self.in_alternate_screen() {
                    self.cursor_x = (self.cursor_x + n).min(self.cols.saturating_sub(1));
                    self.normalize_cursor_x();
                } else {
                    self.cursor_step_right(n);
                    self.normalize_cursor_x();
                }
            }
            b'D' => {
                // CUB - cursor back
                let n = def(0, 1) as usize;
                if self.in_alternate_screen() {
                    self.cursor_x = self.cursor_x.saturating_sub(n);
                    self.normalize_cursor_x();
                } else {
                    self.cursor_step_left(n);
                }
            }
            b'E' => {
                // CNL - cursor next line
                let n = def(0, 1) as usize;
                self.cursor_x = 0;
                let limit = self.rows.saturating_sub(1);
                self.cursor_y = (self.cursor_y + n).min(limit);
            }
            b'F' => {
                // CPL - cursor previous line
                let n = def(0, 1) as usize;
                self.cursor_x = 0;
                self.cursor_y = self.cursor_y.saturating_sub(n);
            }
            b'G' => {
                // CHA - cursor horizontal absolute
                let n = def(0, 1) as usize;
                self.cursor_x = (n.saturating_sub(1)).min(self.cols.saturating_sub(1));
            }
            b'H' | b'f' => {
                // CUP / HVP - cursor position
                let row = def(0, 1) as usize;
                let col = def(1, 1) as usize;
                self.set_cursor(row.saturating_sub(1), col.saturating_sub(1));
            }
            b'J' => {
                // ED - erase in display
                let n = def(0, 0);
                match n {
                    0 => {
                        // Erase below (keep current SGR on erased cells)
                        self.erase_cells_in_row(self.cursor_y, self.cursor_x, self.cols, false);
                        for y in (self.cursor_y + 1)..self.rows {
                            self.erase_cells_in_row(y, 0, self.cols, false);
                        }
                    }
                    1 => {
                        // Erase above
                        for y in 0..self.cursor_y {
                            self.erase_cells_in_row(y, 0, self.cols, false);
                        }
                        self.erase_cells_in_row(self.cursor_y, 0, self.cursor_x.saturating_add(1), false);
                    }
                    2 | 3 => {
                        // Erase all — reset to default rendition
                        for y in 0..self.rows {
                            self.erase_cells_in_row(y, 0, self.cols, true);
                        }
                    }
                    _ => {}
                }
            }
            b'K' => {
                // EL - erase in line
                let n = def(0, 0);
                let y = self.cursor_y;
                match n {
                    0 => self.erase_cells_in_row(y, self.cursor_x, self.cols, false),
                    1 => self.erase_cells_in_row(y, 0, self.cursor_x.saturating_add(1), false),
                    2 => self.erase_cells_in_row(y, 0, self.cols, false),
                    _ => {}
                }
            }
            b'L' => {
                // IL - insert lines
                let n = (def(0, 1) as usize).max(1);
                for _ in 0..n {
                    self.cells.remove(self.scroll_bottom);
                    let blank = self.blank_scroll_row();
                    self.cells.insert(self.cursor_y, blank);
                }
            }
            b'M' => {
                // DL - delete lines
                let n = (def(0, 1) as usize).max(1);
                for _ in 0..n {
                    self.cells.remove(self.cursor_y);
                    self.cells
                        .insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
                }
            }
            b'P' => {
                // DCH - delete characters
                let n = (def(0, 1) as usize).max(1);
                let row = &mut self.cells[self.cursor_y];
                for _ in 0..n {
                    row.remove(self.cursor_x);
                    row.push(Cell::default());
                }
            }
            b'@' => {
                // ICH - insert characters
                let n = (def(0, 1) as usize).max(1);
                let row = &mut self.cells[self.cursor_y];
                for _ in 0..n {
                    row.insert(self.cursor_x, Cell::default());
                    row.pop();
                }
            }
            b'S' => {
                // SU - scroll up
                let n = (def(0, 1) as usize).max(1);
                self.scroll_up(n);
            }
            b'T' => {
                // SD - scroll down
                let n = (def(0, 1) as usize).max(1);
                self.scroll_down(n);
            }
            b'X' => {
                // ECH - erase characters
                let n = (def(0, 1) as usize).max(1);
                let end = (self.cursor_x + n).min(self.cols);
                self.erase_cells_in_row(self.cursor_y, self.cursor_x, end, false);
            }
            b'd' => {
                // VPA - vertical position absolute
                let row = def(0, 1) as usize;
                self.set_cursor(row.saturating_sub(1), self.cursor_x);
            }
            b'm' => {
                // SGR - select graphic rendition
                if params.is_empty() {
                    self.current_attrs = self.default_attrs;
                    self.postdisplay_leading_space = false;
                    return;
                }
                let mut i: usize = 0;
                while i < params.len() {
                    let p = params[i];
                    match p {
                        0 => {
                            self.current_attrs = self.default_attrs;
                            self.postdisplay_leading_space = false;
                        }
                        1 => self.current_attrs.bold = true,
                        2 => {
                            self.current_attrs.dim = true;
                            self.mark_postdisplay_suggest_attrs();
                        }
                        3 => self.current_attrs.italic = true,
                        4 => self.current_attrs.underline = true,
                        5 | 6 => self.current_attrs.blink = true,
                        7 => self.current_attrs.reverse = true,
                        9 => self.current_attrs.strikethrough = true,
                        22 => {
                            self.current_attrs.bold = false;
                            self.current_attrs.dim = false;
                        }
                        23 => self.current_attrs.italic = false,
                        24 => self.current_attrs.underline = false,
                        25 => self.current_attrs.blink = false,
                        27 => self.current_attrs.reverse = false,
                        29 => self.current_attrs.strikethrough = false,
                        30..=37 => {
                            self.current_attrs.fg = Color::Indexed((p - 30) as u8);
                        }
                        38 => {
                            if i + 2 < params.len() && params[i + 1] == 5 {
                                self.current_attrs.fg = Color::Indexed(params[i + 2] as u8);
                                self.mark_postdisplay_suggest_attrs();
                                i += 2;
                            } else if i + 4 < params.len() && params[i + 1] == 2 {
                                self.current_attrs.fg = Color::Rgb(
                                    params[i + 2] as u8,
                                    params[i + 3] as u8,
                                    params[i + 4] as u8,
                                );
                                i += 4;
                            }
                        }
                        39 => {
                            self.current_attrs.fg = Color::Default;
                            self.postdisplay_leading_space = false;
                        }
                        40..=47 => {
                            self.current_attrs.bg = Color::Indexed((p - 40) as u8);
                        }
                        48 => {
                            if i + 2 < params.len() && params[i + 1] == 5 {
                                self.current_attrs.bg = Color::Indexed(params[i + 2] as u8);
                                i += 2;
                            } else if i + 4 < params.len() && params[i + 1] == 2 {
                                self.current_attrs.bg = Color::Rgb(
                                    params[i + 2] as u8,
                                    params[i + 3] as u8,
                                    params[i + 4] as u8,
                                );
                                i += 4;
                            }
                        }
                        49 => self.current_attrs.bg = Color::Default,
                        90..=97 => {
                            self.current_attrs.fg = Color::Indexed((p - 90 + 8) as u8);
                        }
                        100..=107 => {
                            self.current_attrs.bg = Color::Indexed((p - 100 + 8) as u8);
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'n' => {
                // DSR - device status report
                let n = def(0, 0);
                match n {
                    5 => {
                        // Operating status - report OK
                        self.outgoing
                            .push(TermEvent::Response(b"\x1b[0n".to_vec()));
                    }
                    6 => {
                        // Cursor position report
                        let resp = format!("\x1b[{};{}R", self.cursor_y + 1, self.cursor_x + 1);
                        self.outgoing
                            .push(TermEvent::Response(resp.into_bytes()));
                    }
                    _ => {}
                }
            }
            b'r' => {
                // DECSTBM - set scroll region
                let top = def(0, 1) as usize;
                let bot = def(1, self.rows as i64) as usize;
                self.scroll_top = (top.saturating_sub(1)).min(self.rows.saturating_sub(1));
                self.scroll_bottom = (bot.saturating_sub(1)).min(self.rows.saturating_sub(1));
                self.cursor_x = 0;
                // Home position is the top margin line (xterm DECSTBM), not always screen row 0.
                self.cursor_y = self.scroll_top;
            }
            b'h' | b'l' => {
                let set = final_byte == b'h';
                let private = intermediates.contains(&b'?');
                for &p in params {
                    match (p, set) {
                        (47 | 1047, true) => self.enter_alternate_screen(false),
                        (47 | 1047, false) => self.leave_alternate_screen(false),
                        (1049, true) => self.enter_alternate_screen(true),
                        (1049, false) => self.leave_alternate_screen(true),
                        _ if private => match (p, set) {
                            (1, s) => self.app_cursor_keys = s,
                            // DECOM in the alternate buffer breaks full-screen TUIs (btop/htop):
                            // CUP 1;1 must target screen row 0, not scroll_top + 0.
                            (6, s) => {
                                self.origin_mode = s && !self.in_alternate_screen();
                            }
                            (7, s) => self.auto_wrap = s,
                            (12, _) => {}
                            (25, s) => self.cursor_visible = s,
                            (2004, s) => self.bracketed_paste = s,
                            (1000, s) => self.mouse_report_clicks = s,
                            (1002, s) => self.mouse_report_drag = s,
                            (1003, s) => {
                                self.mouse_report_motion = s;
                                if s {
                                    self.mouse_report_drag = true;
                                    self.mouse_report_clicks = true;
                                }
                            }
                            (1006, s) => self.mouse_sgr_encoding = s,
                            (2026, _) => {
                                // Synchronized updates: ignore; always paint the live buffer (vim/btop).
                            }
                            _ => {}
                        },
                        (25, _) if !private => self.cursor_visible = set,
                        _ => {}
                    }
                }
            }
            b'c' => {
                // DA - device attributes
                if def(0, 0) == 0 {
                    self.outgoing
                        .push(TermEvent::Response(b"\x1b[?1;2c".to_vec()));
                }
            }
            b's' => {
                // SCOSC - save cursor position (zsh autosuggest / completion)
                self.saved_cursor_x = self.cursor_x;
                self.saved_cursor_y = self.cursor_y;
            }
            b'u' => {
                // SCORC - restore cursor position
                self.cursor_x = self.saved_cursor_x.min(self.cols.saturating_sub(1));
                self.cursor_y = self.saved_cursor_y.min(self.rows.saturating_sub(1));
            }
            b't' => {
                // Window manipulation (xterm)
                let ps = def(0, 0);
                match ps {
                    8 => {
                        // xterm: application asks the *terminal emulator* to resize its window.
                        // rsterm owns window geometry from the egui layout; honoring CSI 8 here would
                        // let stale ncurses LINES/COLS snap the PTY back after a UI resize (htop 3.x).
                        // Size changes are delivered via kernel winsize + SIGWINCH instead (like Konsole).
                    }
                    14 => {
                        let px_h = (self.rows * 16).max(1);
                        let px_w = (self.cols * 8).max(1);
                        let resp = format!("\x1b[4;{px_h};{px_w}t");
                        self.outgoing
                            .push(TermEvent::Response(resp.into_bytes()));
                    }
                    18 => {
                        let resp = format!("\x1b[8;{};{}t", self.rows, self.cols);
                        self.outgoing
                            .push(TermEvent::Response(resp.into_bytes()));
                    }
                    22 | 23 => {} // save/restore window title on stack (smcup/rmcup)
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn dcs_dispatch(&mut self, data: &str) {
        self.flush_pending_cr();
        self.handle_xtgettcap(data);
    }

    fn osc_dispatch(&mut self, data: &str) {
        self.flush_pending_cr();
        if let Some(rest) = data.strip_prefix("0;") {
            self.title = rest.to_string();
        } else if let Some(rest) = data.strip_prefix("2;") {
            self.title = rest.to_string();
        } else if data == "0" {
            if let Some(rest) = data.get(1..) {
                self.title = rest.to_string();
            }
        }
    }
}

impl Screen {
    /// XTGETTCAP — ncurses/vim query termcap/terminfo via DCS.
    fn handle_xtgettcap(&mut self, data: &str) {
        let mut query = data;
        if let Some(rest) = query.strip_prefix('+') {
            query = rest;
        }
        if let Some(rest) = query.strip_prefix('$') {
            query = rest;
        }
        if let Some(rest) = query.strip_prefix('q') {
            query = rest;
        }
        if query.is_empty() {
            self.outgoing
                .push(TermEvent::Response(b"\x1bP0+r0\x1b\\".to_vec()));
            return;
        }

        // xterm XTGETTCAP: ESC P 1 + r {hex-name} = {hex-value} ESC \
        let mut reply = String::from("\x1bP");
        let mut replied = false;
        for cap_token in query.split(';').filter(|s| !s.is_empty()) {
            let cap = xt_hex_decode(cap_token).unwrap_or_else(|| cap_token.to_string());
            if let Some(value) = xt_cap_value(&cap) {
                reply.push_str("1+r");
                reply.push_str(&xt_hex_encode(&cap));
                reply.push('=');
                reply.push_str(&xt_hex_encode(value));
            } else {
                reply.push_str("0+r");
                reply.push_str(&xt_hex_encode(&cap));
            }
            replied = true;
        }
        if !replied {
            reply.push_str("0+r0");
        }
        reply.push_str("\x1b\\");
        self.outgoing.push(TermEvent::Response(reply.into_bytes()));
    }
}

fn xt_hex_encode(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn xt_hex_decode(s: &str) -> Option<String> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = String::new();
    for i in (0..s.len()).step_by(2) {
        let byte = u8::from_str_radix(&s[i..i + 2], 16).ok()?;
        out.push(byte as char);
    }
    Some(out)
}

fn xt_cap_value(cap: &str) -> Option<&'static str> {
    match cap {
        "Co" | "colors" => Some("256"),
        "RGB" | "Tc" => Some("1"),
        "TN" | "name" => Some("xterm-256color"),
        "am" | "xn" | "km" | "cc" | "mi" | "da" | "hs" => Some("1"),
        "cols" => None, // use window size report instead
        _ => None,
    }
}

fn dec_graphics(c: char) -> char {
    match c as u8 {
        b'+' => '\u{2192}',
        b',' => '\u{2190}',
        b'-' => '\u{2191}',
        b'.' => '\u{2193}',
        b'0' => '\u{2588}',
        b'`' => '\u{25C6}',
        b'a' => '\u{2592}',
        b'f' => '\u{00B0}',
        b'g' => '\u{00B1}',
        b'h' => '\u{2424}',
        b'j' => '\u{2518}',
        b'k' => '\u{2510}',
        b'l' => '\u{250C}',
        b'm' => '\u{2514}',
        b'n' => '\u{253C}',
        b'o' => '\u{23BA}',
        b'p' => '\u{23BB}',
        b'q' => '\u{2500}',
        b'r' => '\u{23BC}',
        b's' => '\u{23BD}',
        b't' => '\u{251C}',
        b'u' => '\u{2524}',
        b'v' => '\u{2534}',
        b'w' => '\u{252C}',
        b'x' => '\u{2502}',
        b'y' => '\u{2264}',
        b'z' => '\u{2265}',
        b'~' => '\u{00B7}',
        _ => c,
    }
}
