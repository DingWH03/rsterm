use std::collections::VecDeque;

use unicode_width::UnicodeWidthChar;

use crate::terminal::parser::{TermEvent, TermHandler};

/// Scrollback buffer size
const TAB_STOPS_EVERY: usize = 8;

/// Max scrollback lines (configurable per screen)
const MAX_SCROLLBACK: usize = 100_000;

/// A logical character stored in the scrollback (without screen layout info).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LogicalCell {
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
    /// Display width (1 or 2 columns)
    pub width: usize,
}

/// A logical line stored in the scrollback buffer (application-output logical line).
/// This represents one logical output line (e.g. one echo/ls output),
/// which may be wider than the terminal and will be wrapped to visual rows during rendering.
#[derive(Debug, Clone)]
pub(crate) struct LogicalLine {
    pub cells: Vec<LogicalCell>,
}

/// One visual row produced by laying out a logical line at a given width.
/// `start..end` is the range of LogicalCell entries represented by this row.
#[derive(Debug, Clone)]
struct VisualSegment {
    row: Vec<Cell>,
    start: usize,
    end: usize,
    wrapped: bool,
}
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

/// Number of cells in a row, counting wide characters as 1 (i.e. the logical
/// length ignoring continuation slots).
#[allow(dead_code)]
pub(crate) fn count_cells_in_row(cells: &[Cell]) -> usize {
    cells.iter().filter(|c| !c.wide_continuation).count()
}

/// Visible content length of a terminal row.  This deliberately drops trailing
/// blank padding cells that exist only because terminal rows are fixed-width.
/// Keeping those cells during resize reflow makes short prompt/history lines
/// wrap into artificial blank continuation rows.
#[allow(dead_code)]
pub(crate) fn trimmed_row_len(cells: &[Cell]) -> usize {
    let mut end = cells.len();
    while end > 0 {
        let c = cells[end - 1];
        if c.ch == ' ' && !c.wide_continuation {
            end -= 1;
        } else {
            break;
        }
    }
    end
}

/// Convert a physical screen row (with wide_continuation) to logical cells.
/// Trailing spaces are stripped; wide_continuation slots are skipped.
pub(crate) fn row_to_logical_cells(row: &[Cell]) -> Vec<LogicalCell> {
    row_to_logical_cells_with_trim(row, true)
}

/// Convert a physical row to logical cells, optionally preserving trailing spaces.
///
/// This is important for resize reflow: if the next physical row is marked as a
/// soft-wrap continuation, spaces at the end of the current physical row are not
/// padding; they are part of the logical line and must be preserved.  Dropping
/// them is what makes columnar output collapse into strings like
/// `common_config_content.jsonkey` or `Pictures视频` after repeated resizes.
fn row_to_logical_cells_with_trim(row: &[Cell], trim_trailing_spaces: bool) -> Vec<LogicalCell> {
    let mut out = Vec::new();

    let mut end = row.len();
    if trim_trailing_spaces {
        while end > 0 {
            let c = row[end - 1];
            if c.ch == ' ' && !c.wide_continuation {
                end -= 1;
            } else {
                break;
            }
        }
    }

    // Do not leave a dangling wide-character leader at the logical boundary.
    if end < row.len() && end > 0 && row[end].wide_continuation {
        end -= 1;
    }

    let mut x = 0;
    while x < end {
        let c = row[x];

        // Skip continuation slots.
        if c.wide_continuation {
            x += 1;
            continue;
        }

        let width = if x + 1 < end && row[x + 1].wide_continuation {
            2
        } else {
            1
        };

        out.push(LogicalCell {
            ch: c.ch,
            fg: c.fg,
            bg: c.bg,
            bold: c.bold,
            italic: c.italic,
            underline: c.underline,
            blink: c.blink,
            reverse: c.reverse,
            dim: c.dim,
            strikethrough: c.strikethrough,
            width,
        });

        x += width;
    }

    out
}

/// Layout a logical line into visual screen rows for the given terminal width.
/// This handles character wrapping and adds wide_continuation markers.
pub(crate) fn layout_logical_line(line: &LogicalLine, cols: usize) -> Vec<Vec<Cell>> {
    if cols == 0 {
        return vec![vec![]];
    }

    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut row = vec![Cell::default(); cols];
    let mut x = 0usize;

    for lc in &line.cells {
        if lc.width == 0 {
            continue;
        }

        // Need to wrap?
        if x + lc.width > cols {
            rows.push(row);
            row = vec![Cell::default(); cols];
            x = 0;
        }

        // Safety: if width > cols, push a row and start fresh.
        if x >= cols {
            rows.push(row);
            row = vec![Cell::default(); cols];
            x = 0;
        }

        let cell = Cell {
            ch: lc.ch,
            fg: lc.fg,
            bg: lc.bg,
            bold: lc.bold,
            italic: lc.italic,
            underline: lc.underline,
            blink: lc.blink,
            reverse: lc.reverse,
            dim: lc.dim,
            strikethrough: lc.strikethrough,
            wide_continuation: false,
        };

        row[x] = cell;

        if lc.width == 2 && x + 1 < cols {
            row[x + 1] = Cell {
                ch: ' ',
                fg: lc.fg,
                bg: lc.bg,
                bold: lc.bold,
                italic: lc.italic,
                underline: lc.underline,
                blink: lc.blink,
                reverse: lc.reverse,
                dim: lc.dim,
                strikethrough: lc.strikethrough,
                wide_continuation: true,
            };
        }

        x += lc.width;
    }

    rows.push(row);
    rows
}

/// Layout a logical line into visual rows, keeping enough metadata to split the
/// current visible grid during resize without losing data.
fn layout_logical_line_segments(
    line: &LogicalLine,
    cols: usize,
    first_row_is_wrapped: bool,
) -> Vec<VisualSegment> {
    if cols == 0 {
        return vec![VisualSegment {
            row: Vec::new(),
            start: 0,
            end: 0,
            wrapped: first_row_is_wrapped,
        }];
    }

    if line.cells.is_empty() {
        return vec![VisualSegment {
            row: vec![Cell::default(); cols],
            start: 0,
            end: 0,
            wrapped: first_row_is_wrapped,
        }];
    }

    let mut rows: Vec<VisualSegment> = Vec::new();
    let mut row = vec![Cell::default(); cols];
    let mut x = 0usize;
    let mut start = 0usize;
    let mut first = true;

    for (i, lc) in line.cells.iter().enumerate() {
        if lc.width == 0 {
            continue;
        }

        let width = lc.width.max(1);
        if x > 0 && x + width > cols {
            rows.push(VisualSegment {
                row,
                start,
                end: i,
                wrapped: if first { first_row_is_wrapped } else { true },
            });
            row = vec![Cell::default(); cols];
            x = 0;
            start = i;
            first = false;
        }

        let cell = Cell {
            ch: lc.ch,
            fg: lc.fg,
            bg: lc.bg,
            bold: lc.bold,
            italic: lc.italic,
            underline: lc.underline,
            blink: lc.blink,
            reverse: lc.reverse,
            dim: lc.dim,
            strikethrough: lc.strikethrough,
            wide_continuation: false,
        };

        row[x] = cell;

        if lc.width == 2 && x + 1 < cols {
            row[x + 1] = Cell {
                ch: ' ',
                fg: lc.fg,
                bg: lc.bg,
                bold: lc.bold,
                italic: lc.italic,
                underline: lc.underline,
                blink: lc.blink,
                reverse: lc.reverse,
                dim: lc.dim,
                strikethrough: lc.strikethrough,
                wide_continuation: true,
            };
            x += 2;
        } else {
            // If cols == 1 and the logical cell is wide, render its leading cell
            // in the only available column and advance by one to avoid overflow.
            x += 1;
        }
    }

    rows.push(VisualSegment {
        row,
        start,
        end: line.cells.len(),
        wrapped: if first { first_row_is_wrapped } else { true },
    });

    rows
}

/// Logical-cell offset corresponding to a display column inside a physical row.
fn row_logical_offset_for_display_col(row: &[Cell], target_col: usize) -> usize {
    let mut display_col = 0usize;
    let mut x = 0usize;
    let mut logical = 0usize;

    while x < row.len() && display_col < target_col {
        if row[x].wide_continuation {
            x += 1;
            continue;
        }
        let width = cell_display_width(row, x).max(1);
        display_col += width;
        x += width;
        logical += 1;
    }

    logical
}

fn logical_display_col(cells: &[LogicalCell], start: usize, end: usize) -> usize {
    cells[start.min(cells.len())..end.min(cells.len())]
        .iter()
        .map(|c| c.width.max(1))
        .sum()
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

    /// Scrollback buffer (newer lines at end) as logical lines
    scrollback: VecDeque<LogicalLine>,
    scrollback_limit: usize,

    /// Visual layout cache derived from scrollback + current cols.
    /// Rebuilt when cols changes in resize, appended incrementally in scroll_up.
    visual_cache: Vec<Vec<Cell>>,
    /// visual_starts[i] = index in visual_cache where logical line i's visual rows begin.
    visual_starts: Vec<usize>,
    /// Whether each row in visual_cache is a soft-wrap continuation of the previous visual row.
    visual_wrapped: Vec<bool>,

    /// Whether cursor is visible
    pub cursor_visible: bool,

    /// Whether each visible row is a soft-wrapped continuation of the row above it.
    /// Used by [`Self::reflow_content`] to re-wrap text when the terminal width changes.
    wrapped: Vec<bool>,

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

    /// Delayed auto-wrap state: after printing in the last column, xterm keeps the
    /// cursor in the last column and wraps only before the next printable cell.
    /// Immediate wrapping here makes full-screen TUIs such as btop scroll/tear.
    pending_wrap: bool,

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
    wrapped: Vec<bool>,
    cursor_x: usize,
    cursor_y: usize,
    current_attrs: Cell,
    saved_cursor_x: usize,
    saved_cursor_y: usize,
    scroll_top: usize,
    scroll_bottom: usize,
    origin_mode: bool,
    pending_wrap: bool,
}

impl Screen {
    pub fn new(rows: usize, cols: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
        let mut tab_stops = vec![false; cols];
        for t in (0..cols).step_by(TAB_STOPS_EVERY) {
            tab_stops[t] = true;
        }

        let wrapped = vec![false; rows];

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
            visual_cache: Vec::new(),
            visual_starts: Vec::new(),
            visual_wrapped: Vec::new(),
            cursor_visible: true,
            saved_main: None,
            wrapped,
            charset: 0,
            g0_charset: Charset::UsAscii,
            g1_charset: Charset::UsAscii,
            outgoing: Vec::new(),
            origin_mode: false,
            auto_wrap: true,
            pending_wrap: false,
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

    /// Rebuild the visual cache from all logical lines in scrollback.
    /// Called when `cols` changes (resize) to re-layout all history.
    fn rebuild_visual_cache(&mut self) {
        self.visual_cache.clear();
        self.visual_starts.clear();
        self.visual_wrapped.clear();
        for line in &self.scrollback {
            self.visual_starts.push(self.visual_cache.len());
            for segment in layout_logical_line_segments(line, self.cols, false) {
                self.visual_cache.push(segment.row);
                self.visual_wrapped.push(segment.wrapped);
            }
        }
    }

    /// Append a new logical line to scrollback and its visual rows to the cache.
    fn append_logical_with_visuals(&mut self, cells: Vec<LogicalCell>) {
        self.visual_starts.push(self.visual_cache.len());
        self.scrollback.push_back(LogicalLine { cells });
        let last = self.scrollback.back().unwrap();
        for segment in layout_logical_line_segments(last, self.cols, false) {
            self.visual_cache.push(segment.row);
            self.visual_wrapped.push(segment.wrapped);
        }
    }

    /// Extend the last logical line's cells and recompute its visual rows in the cache.
    fn extend_last_logical_with_visuals(&mut self, cells: Vec<LogicalCell>) {
        if self.scrollback.is_empty() || self.visual_starts.is_empty() {
            // If the first row we ever see is marked as a continuation, there is
            // no previous logical line to attach to.  Preserve it as a new line
            // instead of silently dropping it.
            self.append_logical_with_visuals(cells);
            return;
        }

        if let Some(last) = self.scrollback.back_mut() {
            let old_start = self.visual_starts.pop().unwrap();
            self.visual_cache.truncate(old_start);
            self.visual_wrapped.truncate(old_start);
            last.cells.extend(cells);
            let new_start = self.visual_cache.len();
            self.visual_starts.push(new_start);
            for segment in layout_logical_line_segments(last, self.cols, false) {
                self.visual_cache.push(segment.row);
                self.visual_wrapped.push(segment.wrapped);
            }
        }
    }

    /// Pop the newest logical scrollback line and remove its cached visual rows.
    /// Used during resize when the first active row is a continuation of that
    /// scrollback line; temporarily recombining them gives correct cross-boundary
    /// reflow.
    fn pop_last_logical_with_visuals(&mut self) -> Option<LogicalLine> {
        let line = self.scrollback.pop_back()?;
        if let Some(start) = self.visual_starts.pop() {
            self.visual_cache.truncate(start);
            self.visual_wrapped.truncate(start);
        } else {
            self.visual_cache.clear();
            self.visual_wrapped.clear();
        }
        Some(line)
    }

    /// Trim scrollback (and visual cache) front when exceeding the limit.
    fn trim_scrollback_front(&mut self) {
        let excess = self.scrollback.len().saturating_sub(self.scrollback_limit);
        for _ in 0..excess {
            self.scrollback.pop_front();
            let start = self.visual_starts.remove(0);
            let end = self
                .visual_starts
                .first()
                .copied()
                .unwrap_or(self.visual_cache.len());
            let removed = end - start;
            self.visual_cache.drain(0..removed);
            self.visual_wrapped.drain(0..removed);
            for s in &mut self.visual_starts {
                *s -= removed;
            }
        }
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
            // This is an explicit terminal clear, not a resize.  Hidden tails kept
            // only for resize preservation must be discarded here; otherwise old
            // invisible text can reappear when the window grows later.
            row.resize(self.cols, Cell::default());
            for cell in row.iter_mut() {
                *cell = Cell::default();
            }
        }
        // A row that has been explicitly cleared is no longer a continuation
        // of the previous soft-wrapped row.  Leaving this flag set causes
        // resize reflow to concatenate unrelated prompts/history lines.
        if let Some(w) = self.wrapped.get_mut(y) {
            *w = false;
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
        let mut erased_whole_visible_row = false;
        if let Some(row) = self.cells.get_mut(y) {
            let visible_end = x_end.min(self.cols).min(row.len());
            for x in x_start.min(row.len())..visible_end {
                row[x] = blank;
            }
            erased_whole_visible_row = x_start == 0 && x_end >= self.cols;
            if erased_whole_visible_row {
                // Full-row clears are real output operations, so any resize-only
                // hidden tail must be discarded instead of preserved.
                row.resize(self.cols, blank);
                for cell in row.iter_mut() {
                    *cell = blank;
                }
            }
        }
        if erased_whole_visible_row {
            // EL 2 / ED / full-row clears break any previous soft-wrap chain.
            if let Some(w) = self.wrapped.get_mut(y) {
                *w = false;
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
            self.wrapped = vec![false; self.rows];
            self.cursor_x = 0;
            self.cursor_y = 0;
            self.scroll_top = 0;
            self.scroll_bottom = self.rows.saturating_sub(1);
            self.origin_mode = false;
            self.current_attrs = self.default_attrs;
            self.pending_cr = false;
            self.pending_wrap = false;
            self.suppress_extra_crlf_newline = false;
            self.in_zsh_line_pad = false;
            return;
        }
        let snapshot = MainScreenState {
            cells: std::mem::replace(
                &mut self.cells,
                vec![vec![Cell::default(); self.cols]; self.rows],
            ),
            wrapped: std::mem::take(&mut self.wrapped),
            cursor_x: self.cursor_x,
            cursor_y: self.cursor_y,
            current_attrs: self.current_attrs,
            saved_cursor_x: self.saved_cursor_x,
            saved_cursor_y: self.saved_cursor_y,
            scroll_top: self.scroll_top,
            scroll_bottom: self.scroll_bottom,
            origin_mode: self.origin_mode,
            pending_wrap: self.pending_wrap,
        };
        if save_cursor {
            self.saved_cursor_x = snapshot.cursor_x;
            self.saved_cursor_y = snapshot.cursor_y;
        }
        self.saved_main = Some(snapshot);
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.wrapped = vec![false; self.rows];
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);
        self.origin_mode = false;
        self.current_attrs = self.default_attrs;
        self.pending_cr = false;
        self.pending_wrap = false;
        self.suppress_extra_crlf_newline = false;
        self.in_zsh_line_pad = false;
    }

    fn leave_alternate_screen(&mut self, restore_cursor: bool) {
        let Some(main) = self.saved_main.take() else {
            return;
        };
        self.cells = main.cells;
        self.wrapped = main.wrapped;
        self.current_attrs = main.current_attrs;
        self.scroll_top = main.scroll_top;
        self.scroll_bottom = main.scroll_bottom;
        self.origin_mode = main.origin_mode;
        self.pending_wrap = main.pending_wrap;
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
            self.pending_wrap = false;
        }
    }

    /// Reflow the main-screen visible grid itself when the terminal width changes.
    ///
    /// Scrollback is already stored as logical lines and rebuilt through
    /// `visual_cache`; this function performs the same logical-line conversion
    /// for rows that are still on the active main screen.  Hard-newline rows stay
    /// as independent logical lines, while rows marked `wrapped=true` are merged
    /// with the preceding logical line.
    fn reflow_visible_main_grid(&mut self, rows: usize, cols: usize) {
        let old_cells = std::mem::take(&mut self.cells);
        let old_wrapped = std::mem::take(&mut self.wrapped);
        let old_cursor_y = self.cursor_y;
        let old_cursor_x = self.cursor_x;

        let mut logical_lines: Vec<LogicalLine> = Vec::new();
        let mut first_wrapped: Vec<bool> = Vec::new();
        let mut row_to_line: Vec<usize> = vec![0; old_cells.len()];
        let mut row_base_offset: Vec<usize> = vec![0; old_cells.len()];

        for (r, row) in old_cells.iter().enumerate() {
            let is_cont = old_wrapped.get(r).copied().unwrap_or(false);
            let next_is_cont = old_wrapped.get(r + 1).copied().unwrap_or(false);
            // If the next row is a soft-wrap continuation, this row is an
            // interior segment of the same logical line.  Preserve trailing
            // spaces at the segment boundary; they may be real column padding.
            let cells = row_to_logical_cells_with_trim(row, !next_is_cont);

            if is_cont && !logical_lines.is_empty() {
                let li = logical_lines.len() - 1;
                row_to_line[r] = li;
                row_base_offset[r] = logical_lines[li].cells.len();
                logical_lines[li].cells.extend(cells);
            } else {
                let li = logical_lines.len();
                row_to_line[r] = li;
                row_base_offset[r] = 0;
                logical_lines.push(LogicalLine { cells });
                // If the first active row is already a continuation, it belongs
                // to a logical line whose prefix is above the viewport. Preserve
                // that continuation flag so a later scroll_up can reconnect it
                // with the previous scrollback line.
                first_wrapped.push(is_cont);
            }
        }

        // If the first active row is a continuation, its prefix is the newest
        // logical line in scrollback.  Recombine them before reflow so growing the
        // terminal can join the line cleanly across the scrollback/screen boundary.
        if first_wrapped.first().copied().unwrap_or(false) {
            if let Some(prefix) = self.pop_last_logical_with_visuals() {
                if !logical_lines.is_empty() {
                    let prefix_len = prefix.cells.len();
                    let mut combined = prefix.cells;
                    combined.extend(std::mem::take(&mut logical_lines[0].cells));
                    logical_lines[0].cells = combined;
                    first_wrapped[0] = false;
                    for (r, li) in row_to_line.iter().copied().enumerate() {
                        if li == 0 {
                            row_base_offset[r] += prefix_len;
                        }
                    }
                } else {
                    logical_lines.push(prefix);
                    first_wrapped.push(false);
                }
            }
        }

        let cursor_line_and_offset = if old_cursor_y < old_cells.len() {
            let li = row_to_line[old_cursor_y];
            let offset = row_base_offset[old_cursor_y]
                + row_logical_offset_for_display_col(&old_cells[old_cursor_y], old_cursor_x);
            Some((li, offset.min(logical_lines[li].cells.len())))
        } else {
            None
        };

        let mut visual_rows: Vec<(usize, VisualSegment)> = Vec::new();
        for (li, line) in logical_lines.iter().enumerate() {
            for segment in layout_logical_line_segments(line, cols, first_wrapped[li]) {
                visual_rows.push((li, segment));
            }
        }

        let visible_start = visual_rows.len().saturating_sub(rows);

        // Rows that move above the viewport because of reflow become scrollback.
        // If only a prefix of a logical line moves into scrollback, the first
        // visible segment remains `wrapped=true`; when it later scrolls up,
        // `scroll_up` will extend this same logical line instead of creating a
        // duplicate hard line.
        for i in 0..visible_start {
            let (li, segment) = &visual_rows[i];
            let line = &logical_lines[*li];
            let cells = line.cells[segment.start.min(line.cells.len())..segment.end.min(line.cells.len())]
                .to_vec();
            if segment.wrapped {
                self.extend_last_logical_with_visuals(cells);
            } else {
                self.append_logical_with_visuals(cells);
            }
        }
        if visible_start > 0 {
            self.trim_scrollback_front();
        }

        self.cells = Vec::with_capacity(rows);
        self.wrapped = Vec::with_capacity(rows);
        for (_, segment) in visual_rows.iter().skip(visible_start) {
            self.cells.push(segment.row.clone());
            self.wrapped.push(segment.wrapped);
        }

        while self.cells.len() < rows {
            self.cells.push(vec![Cell::default(); cols]);
            self.wrapped.push(false);
        }

        let mut new_cursor_y = self.cells.len().saturating_sub(1);
        let mut new_cursor_x = 0usize;

        if let Some((cursor_line, cursor_offset)) = cursor_line_and_offset {
            for (global_row, (li, segment)) in visual_rows.iter().enumerate() {
                if *li != cursor_line {
                    continue;
                }

                let contains_cursor = if segment.start == segment.end {
                    cursor_offset == segment.start
                } else {
                    cursor_offset >= segment.start && cursor_offset <= segment.end
                };

                if contains_cursor {
                    if global_row >= visible_start {
                        let line = &logical_lines[*li];
                        new_cursor_y = global_row - visible_start;
                        new_cursor_x = logical_display_col(
                            &line.cells,
                            segment.start,
                            cursor_offset.min(segment.end),
                        )
                        .min(cols.saturating_sub(1));
                    } else {
                        new_cursor_y = 0;
                        new_cursor_x = 0;
                    }
                    break;
                }
            }
        }

        self.cursor_y = new_cursor_y.min(rows.saturating_sub(1));
        self.cursor_x = new_cursor_x.min(cols.saturating_sub(1));
    }

    /// Reflow the saved main-screen snapshot while the alternate screen is active.
    ///
    /// The alternate buffer itself should remain an exact rows×cols grid and will be
    /// repainted by the TUI after SIGWINCH.  The saved main buffer, however, is normal
    /// shell history; if the window is resized while htop/vim is open, it must be
    /// reflowed with the same logical-line model as the live main screen.  Otherwise
    /// leaving the alternate screen restores stale physical rows from the old size.
    fn reflow_saved_main_for_resize(
        &mut self,
        mut main: MainScreenState,
        rows: usize,
        cols: usize,
    ) -> MainScreenState {
        let alt_cells = std::mem::replace(&mut self.cells, main.cells);
        let alt_wrapped = std::mem::replace(&mut self.wrapped, main.wrapped);
        let alt_cursor_x = self.cursor_x;
        let alt_cursor_y = self.cursor_y;
        let alt_saved_cursor_x = self.saved_cursor_x;
        let alt_saved_cursor_y = self.saved_cursor_y;
        let alt_scroll_top = self.scroll_top;
        let alt_scroll_bottom = self.scroll_bottom;
        let alt_origin_mode = self.origin_mode;
        let alt_pending_wrap = self.pending_wrap;

        self.cursor_x = main.cursor_x;
        self.cursor_y = main.cursor_y;
        self.saved_cursor_x = main.saved_cursor_x;
        self.saved_cursor_y = main.saved_cursor_y;
        self.scroll_top = main.scroll_top;
        self.scroll_bottom = main.scroll_bottom;
        self.origin_mode = main.origin_mode;
        self.pending_wrap = main.pending_wrap;

        self.reflow_visible_main_grid(rows, cols);

        main.cells = std::mem::replace(&mut self.cells, alt_cells);
        main.wrapped = std::mem::replace(&mut self.wrapped, alt_wrapped);
        main.cursor_x = self.cursor_x.min(cols.saturating_sub(1));
        main.cursor_y = self.cursor_y.min(rows.saturating_sub(1));
        main.saved_cursor_x = self.saved_cursor_x.min(cols.saturating_sub(1));
        main.saved_cursor_y = self.saved_cursor_y.min(rows.saturating_sub(1));
        main.scroll_top = 0;
        main.scroll_bottom = rows.saturating_sub(1);
        main.origin_mode = false;
        main.pending_wrap = false;

        self.cursor_x = alt_cursor_x;
        self.cursor_y = alt_cursor_y;
        self.saved_cursor_x = alt_saved_cursor_x;
        self.saved_cursor_y = alt_saved_cursor_y;
        self.scroll_top = alt_scroll_top;
        self.scroll_bottom = alt_scroll_bottom;
        self.origin_mode = alt_origin_mode;
        self.pending_wrap = alt_pending_wrap;

        main
    }

    /// Resize a row to exactly `cols` cells.  Use this for alternate-screen
    /// buffers where the application is expected to repaint after SIGWINCH.
    fn resize_row_exact_preserving_wide(src: &[Cell], cols: usize) -> Vec<Cell> {
        let mut row_cells = vec![Cell::default(); cols];
        if cols == 0 {
            return row_cells;
        }

        let copy_len = cols.min(src.len());
        row_cells[..copy_len].copy_from_slice(&src[..copy_len]);

        // If the resize boundary cuts a wcwidth=2 character between its leading
        // cell and continuation cell, drop the dangling leading cell.  Otherwise
        // the renderer later treats it as a normal-width glyph and columns drift.
        if copy_len < src.len() && copy_len > 0 && src[copy_len].wide_continuation {
            row_cells[copy_len - 1] = Cell::default();
        }

        if row_cells[0].wide_continuation {
            row_cells[0] = Cell::default();
        }

        row_cells
    }

    /// Resize a main-screen row without discarding the right-hand tail when the
    /// terminal becomes narrower.  The renderer must only paint columns
    /// `0..self.cols`; cells beyond `self.cols` are hidden preservation data and
    /// will become visible again if the terminal grows.
    fn resize_row_keep_tail(src: &[Cell], cols: usize) -> Vec<Cell> {
        let target_len = src.len().max(cols);
        let mut row_cells = vec![Cell::default(); target_len];
        if !src.is_empty() {
            row_cells[..src.len()].copy_from_slice(src);
        }

        if !row_cells.is_empty() && row_cells[0].wide_continuation {
            row_cells[0] = Cell::default();
        }

        row_cells
    }

    fn resize_grid_exact(cells: &[Vec<Cell>], old_rows: usize, rows: usize, cols: usize) -> Vec<Vec<Cell>> {
        (0..rows)
            .map(|r| {
                if r < old_rows {
                    Self::resize_row_exact_preserving_wide(&cells[r], cols)
                } else {
                    vec![Cell::default(); cols]
                }
            })
            .collect()
    }

    fn resize_grid_keep_tails(cells: &[Vec<Cell>], old_rows: usize, rows: usize, cols: usize) -> Vec<Vec<Cell>> {
        (0..rows)
            .map(|r| {
                if r < old_rows {
                    Self::resize_row_keep_tail(&cells[r], cols)
                } else {
                    vec![Cell::default(); cols]
                }
            })
            .collect()
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        if rows == self.rows && cols == self.cols {
            return;
        }

        let old_rows = self.rows;
        let old_cols = self.cols;
        let col_changed = cols != old_cols;
        let in_alt = self.in_alternate_screen();

        // If the alternate screen is active, resize/reflow the saved main-screen
        // snapshot as main-screen history.  This keeps the shell buffer sane after
        // leaving htop/vim when the window was resized inside the alternate screen.
        if in_alt {
            if let Some(main) = self.saved_main.take() {
                self.saved_main = Some(self.reflow_saved_main_for_resize(main, rows, cols));
            }
        }

        if in_alt {
            // Alternate-screen TUIs repaint after SIGWINCH.  Keep the alternate
            // buffer exact-width to avoid stale hidden tails in htop/btop/vim.
            self.cells = Self::resize_grid_exact(&self.cells, old_rows, rows, cols);
            self.wrapped.resize(rows, false);
        } else {
            // Main-screen rows are active history too. Convert them to logical
            // lines, wrap at the new width, and keep the bottom of the resulting
            // visual stream visible. Overflow above the viewport is appended to
            // logical scrollback, so shrinking does not destroy buffer content.
            self.reflow_visible_main_grid(rows, cols);
        }

        self.rows = rows;
        self.cols = cols;

        self.cursor_y = self.cursor_y.min(rows.saturating_sub(1));
        self.cursor_x = self.cursor_x.min(cols.saturating_sub(1));

        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);

        if in_alt {
            self.origin_mode = false;
        }

        let mut tab_stops = vec![false; cols];
        for t in (0..cols).step_by(TAB_STOPS_EVERY) {
            tab_stops[t] = true;
        }
        self.tab_stops = tab_stops;

        self.scroll_scratch.clear();
        self.pending_wrap = false;

        // Re-layout all logical lines in scrollback when width changes.  Do this
        // even while in the alternate screen because saved-main reflow may have
        // appended logical lines using the old cache width.
        if col_changed {
            self.rebuild_visual_cache();
        }
    }

    fn clear_wide_trailer(&mut self, row: usize, col: usize) {
        if col + 1 < self.cols && self.cells[row][col + 1].wide_continuation {
            self.cells[row][col + 1] = Cell::default();
        }
    }

    fn apply_pending_wrap_before_print(&mut self) {
        if self.pending_wrap {
            self.pending_wrap = false;
            if self.auto_wrap {
                self.newline_with_wrap(true);
                self.cursor_x = 0;
            }
        }
    }

    fn ensure_cursor_fits(&mut self, width: usize) -> bool {
        if width == 0 || width > self.cols {
            return false;
        }
        if self.cursor_x + width <= self.cols {
            return true;
        }
        if self.auto_wrap {
            self.newline_with_wrap(true);
            self.cursor_x = 0;
            self.cursor_x + width <= self.cols
        } else {
            false
        }
    }

    pub fn put_char(&mut self, c: char) {
        let width = char_display_width(c);
        if width == 0 || self.cols == 0 || self.rows == 0 {
            return;
        }

        if c == '%' && self.cursor_x == 0 {
            self.in_zsh_line_pad = true;
        }
        if self.in_zsh_line_pad && c == ' ' && self.cursor_x >= self.cols.saturating_sub(1) {
            // zsh pads the line to the right edge before a CR.  Do not let those
            // padding spaces consume the delayed-wrap state and scroll the screen.
            return;
        }

        self.apply_pending_wrap_before_print();

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
            self.pending_wrap = false;
            return;
        }

        if !self.ensure_cursor_fits(width) {
            return;
        }

        if self.insert_mode {
            let row = &mut self.cells[self.cursor_y];
            for i in (self.cursor_x..self.cols.saturating_sub(width)).rev() {
                row[i + width] = row[i];
            }
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

        if col + width >= self.cols {
            self.cursor_x = self.cols.saturating_sub(1);
            self.pending_wrap = self.auto_wrap;
        } else {
            self.cursor_x = col + width;
            self.pending_wrap = false;
        }
    }

    pub fn newline(&mut self) {
        self.newline_with_wrap(false);
    }

    /// Move cursor to the next line.  `wrapped` should be `true` when the cursor
    /// moves because a printable character overflowed the right margin (soft wrap);
    /// these rows can be merged back together on width increase (reflow).
    ///
    /// IMPORTANT: when `wrapped` is `false` (hard newline such as LF/CR), the
    /// destination row's wrapped flag is **cleared**.  Previously-set wrapped
    /// flags on stale rows (e.g. from a prior wrap that was overtaken by a hard
    /// newline) would otherwise cause reflow to merge unrelated content.
    fn newline_with_wrap(&mut self, wrapped: bool) {
        self.pending_wrap = false;
        if self.cursor_y == self.scroll_bottom {
            self.scroll_up(1);
        } else {
            self.cursor_y = (self.cursor_y + 1).min(self.rows.saturating_sub(1));
        }
        self.cursor_x = 0;
        if self.cursor_y < self.rows {
            self.wrapped[self.cursor_y] = wrapped;
        }
    }

    /// BS (0x08): move cursor left without erasing (zsh uses this for line redraw).
    pub fn backspace(&mut self) {
        self.pending_wrap = false;
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
        self.pending_wrap = false;
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
        self.pending_wrap = false;
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
            let next_is_wrapped = self.wrapped.get(start + 1).copied().unwrap_or(false);
            let removed = self.cells.remove(start);
            let was_wrapped = self.wrapped.remove(start);

            if use_scrollback {
                // If the following row is a continuation, the removed row is an
                // interior segment of a logical line.  Keep trailing spaces so
                // re-layout can later reconstruct the exact text stream.
                let logical_cells = row_to_logical_cells_with_trim(&removed, !next_is_wrapped);

                if was_wrapped {
                    // Continuation of the previous logical line: extend it.
                    self.extend_last_logical_with_visuals(logical_cells);
                } else {
                    // New logical line.
                    self.append_logical_with_visuals(logical_cells);
                }

                self.trim_scrollback_front();
            }

            let blank = self.blank_scroll_row();
            self.cells.insert(self.scroll_bottom, blank);
            self.wrapped.insert(self.scroll_bottom, false);
        }
    }

    fn scroll_down(&mut self, n: usize) {
        let n = n.min(self.scroll_bottom - self.scroll_top + 1);
        for _ in 0..n {
            let _removed = self.cells.remove(self.scroll_bottom);
            let _w = self.wrapped.remove(self.scroll_bottom);
            self.cells
                .insert(self.scroll_top, vec![Cell::default(); self.cols]);
            self.wrapped.insert(self.scroll_top, false);
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
        self.pending_wrap = false;
        self.normalize_cursor_x();
    }

    /// Returns the effective scrollback viewable range
    pub fn set_scrollback_limit(&mut self, limit: usize) {
        self.scrollback_limit = limit.min(MAX_SCROLLBACK);
    }

    /// Number of visual rows in the scrollback (derived from logical lines + current cols).
    pub fn scrollback_lines(&self) -> usize {
        self.visual_cache.len()
    }

    /// Get the visual row at `index` in scrollback (0 = oldest visible in scrollback).
    pub fn scrollback_row(&self, index: usize) -> Option<&[Cell]> {
        self.visual_cache.get(index).map(|v| v.as_slice())
    }

    /// Whether the visual scrollback row at `index` is a soft-wrap continuation.
    pub fn scrollback_row_wrapped(&self, index: usize) -> Option<bool> {
        self.visual_wrapped.get(index).copied()
    }

    /// Whether the visible row at `y` was soft-wrapped from the row above it.
    pub fn row_wrapped(&self, y: usize) -> bool {
        self.wrapped.get(y).copied().unwrap_or(false)
    }

    /// Number of active main-screen rows that should participate in the live-tail
    /// viewport.  Rows below the cursor/last nonblank line are just terminal blank
    /// space; treating them as part of the live tail makes resize growth appear to
    /// insert a large gap between scrollback and the prompt.
    pub fn active_main_rows(&self) -> usize {
        if self.in_alternate_screen() {
            return self.rows;
        }
        if self.rows == 0 || self.cells.is_empty() {
            return 0;
        }

        let visible_rows = self.rows.min(self.cells.len());
        let mut last = self.cursor_y.min(visible_rows.saturating_sub(1));
        for y in 0..visible_rows {
            if self.cells[y]
                .iter()
                .take(self.cols.min(self.cells[y].len()))
                .any(|c| (c.ch != ' ' || c.wide_continuation))
            {
                last = last.max(y);
            }
        }
        (last + 1).min(visible_rows)
    }

    /// First virtual line to paint for the live tail.  Unlike the old
    /// `scrollback_lines() - offset` scheme, this ignores blank rows below the
    /// cursor, so expanding the window pulls scrollback lines into view above the
    /// prompt instead of leaving a huge blank gap.
    pub fn live_virtual_start(&self, viewport_rows: usize) -> usize {
        if self.in_alternate_screen() {
            0
        } else {
            let total = self.scrollback_lines() + self.active_main_rows();
            total.saturating_sub(viewport_rows)
        }
    }

    /// Maximum scrollback offset for a viewport of `viewport_rows` rows.
    pub fn max_scroll_offset(&self, viewport_rows: usize) -> usize {
        self.live_virtual_start(viewport_rows)
    }

    /// Viewport start after applying a user scroll offset.  Offset 0 means the
    /// live tail; increasing offset moves toward older history.
    pub fn viewport_virtual_start(&self, viewport_rows: usize, scroll_offset: usize) -> usize {
        self.live_virtual_start(viewport_rows)
            .saturating_sub(scroll_offset.min(self.max_scroll_offset(viewport_rows)))
    }

    /// Whether the cursor is visible in the current viewport, and if so which
    /// viewport row it occupies.
    pub fn cursor_viewport_row(&self, viewport_rows: usize, scroll_offset: usize) -> Option<usize> {
        if self.in_alternate_screen() {
            return (self.cursor_y < viewport_rows).then_some(self.cursor_y);
        }
        if scroll_offset != 0 {
            return None;
        }
        let cursor_line = self.scrollback_lines() + self.cursor_y;
        let start = self.viewport_virtual_start(viewport_rows, scroll_offset);
        (cursor_line >= start && cursor_line < start + viewport_rows).then_some(cursor_line - start)
    }

    /// Whether a virtual row (scrollback + visible screen) is a soft-wrap continuation.
    /// Use this for copy/selection so it follows the same visual model as rendering.
    pub fn virtual_line_wrapped(&self, virtual_line: usize) -> bool {
        if self.in_alternate_screen() {
            return self.wrapped.get(virtual_line).copied().unwrap_or(false);
        }
        let sb = self.visual_cache.len();
        if virtual_line < sb {
            self.visual_wrapped.get(virtual_line).copied().unwrap_or(false)
        } else {
            self.wrapped
                .get(virtual_line - sb)
                .copied()
                .unwrap_or(false)
        }
    }

    /// Line in scrollback + main buffer coordinates (used for selection and copy).
    ///
    /// Virtual lines 0..scrollback_lines() are visual rows from scrollback (logical lines
    /// laid out at the current terminal width).  The remaining lines are visible screen rows.
    pub fn line_at_virtual(&self, virtual_line: usize) -> Option<&[Cell]> {
        if self.in_alternate_screen() {
            return self.cells.get(virtual_line).map(|r| r.as_slice());
        }
        let sb = self.visual_cache.len();
        if virtual_line < sb {
            self.visual_cache.get(virtual_line).map(|v| v.as_slice())
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
                self.pending_wrap = false;
                self.newline_from_lf(preceded_by_cr);
            }
            0x0D => {
                self.pending_wrap = false;
                self.pending_cr = true;
            }
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
                self.pending_wrap = false;
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
                self.pending_wrap = false;
                let n = def(0, 1) as usize;
                self.cursor_y = self.cursor_y.saturating_sub(n);
            }
            b'B' => {
                // CUD - cursor down
                self.pending_wrap = false;
                let n = def(0, 1) as usize;
                let limit = self.rows.saturating_sub(1);
                self.cursor_y = (self.cursor_y + n).min(limit);
            }
            b'C' => {
                // CUF - cursor forward (buffer columns in TUIs; display columns for zsh on main)
                self.pending_wrap = false;
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
                self.pending_wrap = false;
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
                self.pending_wrap = false;
                let n = def(0, 1) as usize;
                self.cursor_x = 0;
                let limit = self.rows.saturating_sub(1);
                self.cursor_y = (self.cursor_y + n).min(limit);
            }
            b'F' => {
                // CPL - cursor previous line
                self.pending_wrap = false;
                let n = def(0, 1) as usize;
                self.cursor_x = 0;
                self.cursor_y = self.cursor_y.saturating_sub(n);
            }
            b'G' => {
                // CHA - cursor horizontal absolute
                self.pending_wrap = false;
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
                    self.wrapped.remove(self.scroll_bottom);
                    let blank = self.blank_scroll_row();
                    self.cells.insert(self.cursor_y, blank);
                    self.wrapped.insert(self.cursor_y, false);
                }
            }
            b'M' => {
                // DL - delete lines
                let n = (def(0, 1) as usize).max(1);
                for _ in 0..n {
                    self.cells.remove(self.cursor_y);
                    self.wrapped.remove(self.cursor_y);
                    self.cells
                        .insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
                    self.wrapped.insert(self.scroll_bottom, false);
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
                self.pending_wrap = false;
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
                self.pending_wrap = false;
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
                            (7, s) => {
                                self.auto_wrap = s;
                                if !s {
                                    self.pending_wrap = false;
                                }
                            }
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
                self.pending_wrap = false;
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
