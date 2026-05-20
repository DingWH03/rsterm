use crate::ui::connection_view::{ActiveSession, ConnectionViewAction};

pub fn sync_emulator_grid(session: &mut ActiveSession, rows: usize, cols: usize, font_size: f32) {
    let rows = rows.max(1);
    let cols = cols.max(1);
    if session.grid_rows == rows
        && session.grid_cols == cols
        && (session.layout_font_size - font_size).abs() <= f32::EPSILON
    {
        return;
    }
    session.grid_rows = rows;
    session.grid_cols = cols;
    session.terminal.resize(rows, cols);
    session.layout_font_size = font_size;
    session.row_galley_cache.clear();
    session.scroll_offset = 0;
}

pub fn sync_pty_size(session: &mut ActiveSession, rows: usize, cols: usize) {
    let rows = rows.max(1) as u16;
    let cols = cols.max(1) as u16;
    if session.last_pty_rows == rows && session.last_pty_cols == cols {
        return;
    }
    session.last_pty_rows = rows;
    session.last_pty_cols = cols;
    session.handle.resize(rows, cols);
}

pub fn apply_resize(
    session: &mut ActiveSession,
    rows: usize,
    cols: usize,
    font_size: f32,
    in_alt: bool,
) {
    if in_alt {
        sync_emulator_grid(session, rows, cols, font_size);
        sync_pty_size(session, rows, cols);
    } else {
        sync_pty_size(session, rows, cols);
        sync_emulator_grid(session, rows, cols, font_size);
    }
}

pub fn drain_after_resize(
    session: &mut ActiveSession,
    action: &mut ConnectionViewAction,
    in_alt: bool,
    drain: fn(&mut ActiveSession, &mut ConnectionViewAction) -> bool,
) {
    for _ in 0..256 {
        if !drain(session, action) {
            break;
        }
    }
    if in_alt {
        session.handle.signal_winch();
        for _ in 0..128 {
            if !drain(session, action) {
                break;
            }
        }
    }
}
