use std::time::{Duration, Instant};

use crate::config::TerminalTheme;
use crate::connection::{ConnIn, ConnectionState};
use crate::platform::{foreground_command, title_is_idle_host, truncate_label};
use crate::storage::types::ConnectionType;
use crate::terminal::parser::TermEvent;
use crate::terminal::renderer::TerminalRenderer;
use crate::ui::clipboard::read_text;
use crate::ui::keyboard::VirtualKeyboard;
use crate::ui::sidebar::{Sidebar, SidebarPage};
use crate::ui::terminal_input::{
    allocate_terminal_surface, lock_terminal_focus, process_keyboard_input, terminal_widget_id,
};
use crate::ui::terminal_paint::{paint_row, RowGalleyCache};
use crate::ui::terminal_mouse::{process_terminal_mouse, process_terminal_wheel};
use crate::ui::terminal_selection::{
    CellPos, TerminalSelection, paint_selection, paste_payload, update_terminal_selection,
};

pub struct ActiveSession {
    pub id: String,
    pub conn_type: ConnectionType,
    /// Source saved connection (for SSH「新窗口」); local may be absent.
    pub saved_conn_id: Option<String>,
    /// Saved connection display name (serial/BLE tab title).
    pub name: String,
    /// Idle tab label for local / SSH (`user@host`).
    pub user_at_host: String,
    pub handle: crate::connection::ConnectionHandle,
    pub terminal: crate::terminal::Terminal,
    pub scroll_offset: usize,
    pub selection: Option<TerminalSelection>,
    pub selection_pointer: Option<CellPos>,
    /// Request keyboard focus on the terminal surface once after connect / click.
    pub want_terminal_focus: bool,
    /// Previous frame: terminal area had keyboard focus (for shortcut routing).
    pub terminal_had_focus: bool,
    pub row_galley_cache: RowGalleyCache,
    /// Last font size used for grid layout (detect A+/A− and reflow immediately).
    pub layout_font_size: f32,
    /// Last size pushed to the PTY (skip redundant resizes).
    pub last_pty_rows: u16,
    pub last_pty_cols: u16,
    /// Grid size shown in the transient resize overlay (`cols×rows`).
    pub size_label_dims: (usize, usize),
    /// Hide overlay at this time after dimensions stop changing.
    pub size_label_hide_at: Option<Instant>,
    /// True after the user has resized at least once (suppress overlay on connect).
    pub size_label_active: bool,
    /// Frames left to aggressively drain PTY output after an alternate-screen resize.
    pub alt_resize_drain_frames: u8,
    /// Last cell reported for xterm mouse motion (dedupe).
    pub mouse_motion_last: Option<(usize, usize)>,
}

pub enum ConnectionViewAction {
    None,
    /// Close the session currently shown in the terminal panel.
    CloseSession,
}

impl ActiveSession {
    /// Sidebar tab: serial/BLE → connection name; local/SSH → running command or `user@host`.
    pub fn tab_label(&self) -> String {
        match self.conn_type {
            ConnectionType::Serial | ConnectionType::Ble => self.name.clone(),
            ConnectionType::Local | ConnectionType::Ssh => {
                if let Some(cmd) = foreground_command(self.handle.shell_pid) {
                    return truncate_label(&cmd, 32);
                }
                let title = self.terminal.screen.title.trim();
                if !title.is_empty() && !title_is_idle_host(title, &self.user_at_host) {
                    return truncate_label(title, 32);
                }
                self.user_at_host.clone()
            }
        }
    }

    /// Sidebar row: local / SSH get「新窗口」; serial / BLE only close.
    pub fn sidebar_has_new_window(&self) -> bool {
        matches!(self.conn_type, ConnectionType::Local | ConnectionType::Ssh)
    }
}

pub fn connection_view(
    ui: &mut egui::Ui,
    mut session: Option<&mut ActiveSession>,
    renderer: &mut TerminalRenderer,
    keyboard: &mut VirtualKeyboard,
    theme: &TerminalTheme,
    font_size: &mut f32,
    sidebar: &mut Sidebar,
    settings_open: &mut bool,
) -> ConnectionViewAction {
    let ctx = ui.ctx().clone();
    let mut action = ConnectionViewAction::None;

    if let Some(session) = session.as_ref() {
        session.handle.repaint.set_context(ctx.clone());
    }

    let mut copy_requested = false;
    let mut pending_input: Vec<Vec<u8>> = Vec::new();
    let mut paste_texts: Vec<String> = Vec::new();
    let mut context_menu_paste = false;

    // 1. Header bar — ☰ + title + toolbar (click-only, not in Tab focus chain)
    ui.horizontal(|ui| {
        if sidebar.show_content_hamburger(SidebarPage::Workspace)
            && sidebar.hamburger(ui).clicked()
        {
            sidebar.hamburger_click(SidebarPage::Workspace);
        }

        let title = session.as_ref().map(|s| s.tab_label()).unwrap_or_default();
        ui.label(egui::RichText::new(title).size(14.0).strong());

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let settings_active = *settings_open;
            let settings_label = if settings_active {
                "\u{2699}\u{FE0F}"
            } else {
                "\u{2699}"
            };
            if toolbar_button(ui, settings_label).clicked() {
                *settings_open = !*settings_open;
            }

            // Font size quick controls
            if toolbar_button(ui, "A-").clicked() {
                *font_size = (*font_size - 1.0).max(8.0);
            }
            if toolbar_button(ui, "A+").clicked() {
                *font_size = (*font_size + 1.0).min(32.0);
            }
            ui.separator();

            // Keyboard toggle and mode
            let kb_icon = if keyboard.visible { "⌨✓" } else { "⌨" };
            if toolbar_button(ui, kb_icon).clicked() {
                keyboard.toggle();
            }
            let mode_label = match keyboard.mode {
                crate::ui::keyboard::KeyboardMode::Special => "Sp",
                crate::ui::keyboard::KeyboardMode::Full => "Full",
            };
            if toolbar_button(ui, mode_label).clicked() {
                keyboard.toggle_mode();
            }

            if toolbar_button(ui, egui::RichText::new("✕").size(14.0).color(egui::Color32::RED)).clicked() {
                action = ConnectionViewAction::CloseSession;
            }
        });
    });
    ui.separator();

    // 2. Measure and resize terminal
    renderer.font_size = *font_size;
    let available = ui.available_size();
    let kb_total = keyboard.reserved_height(available.x);
    let term_w = available.x;
    let term_h = (available.y - kb_total).max(1.0);

    let (cell_w, cell_h) = TerminalRenderer::measure_cell(ui, *font_size);
    let desired_cols = (term_w / cell_w).floor().max(1.0) as usize;
    let desired_rows = (term_h / cell_h).floor().max(1.0) as usize;
    let grid_cols = renderer.cols;
    let grid_rows = renderer.rows;
    let mut resize_applied = false;

    if let Some(session) = session.as_mut() {
        let font_changed = (session.layout_font_size - *font_size).abs() > f32::EPSILON;
        let in_alt = session.terminal.screen.in_alternate_screen();

        let pty_rows = session.last_pty_rows as usize;
        let pty_cols = session.last_pty_cols as usize;
        let size_changed = desired_rows != grid_rows
            || desired_cols != grid_cols
            || desired_rows != pty_rows
            || desired_cols != pty_cols
            || font_changed;

        if size_changed {
            if in_alt {
                // Emulator grid must match before ncurses paints the post-SIGWINCH refresh (~10kB).
                sync_display_grid(session, renderer, desired_rows, desired_cols, *font_size);
                sync_pty_size(session, desired_rows, desired_cols);
                session.alt_resize_drain_frames = 120;
            } else {
                sync_pty_size(session, desired_rows, desired_cols);
                sync_display_grid(session, renderer, desired_rows, desired_cols, *font_size);
            }
            drain_after_resize(session, &mut action, in_alt);
            ctx.request_repaint();
            resize_applied = true;
        }
    }

    let grid_cols = renderer.cols;
    let grid_rows = renderer.rows;

    // 3. Process connection data (keep draining after alt-screen resize; ncurses redraw is bursty)
    if let Some(session) = session.as_mut() {
        if session.alt_resize_drain_frames > 0 {
            session.alt_resize_drain_frames -= 1;
            drain_after_resize(session, &mut action, true);
        }
        while drain_connection(session, &mut action) {}
    }

    // 4. Terminal surface (keyboard focus target; stable id for focus-lock filter)
    let panel_size = egui::vec2(term_w, term_h);
    let grid_size = egui::vec2(grid_cols as f32 * cell_w, grid_rows as f32 * cell_h);
    let (panel_rect, grid_rect, mut term_resp) = allocate_terminal_surface(
        ui,
        panel_size,
        grid_size,
        egui::Sense::click_and_drag() | egui::Sense::FOCUSABLE,
    );
    if resize_applied {
        term_resp.mark_changed();
    }
    term_resp = term_resp.on_hover_cursor(egui::CursorIcon::Text);
    if term_resp.clicked() {
        term_resp.request_focus();
    }
    if session.as_ref().is_some_and(|s| s.want_terminal_focus) {
        ui.ctx().memory_mut(|mem| mem.request_focus(terminal_widget_id()));
    }
    // Reclaim focus if navigation stole it (only the terminal should be keyboard-focusable here).
    if session.as_ref().is_some_and(|s| s.terminal_had_focus) && !term_resp.has_focus() {
        term_resp.request_focus();
    }
    let term_focused = term_resp.has_focus()
        || session.as_ref().is_some_and(|s| s.terminal_had_focus);

    let has_selection = session
        .as_ref()
        .and_then(|s| s.selection.as_ref())
        .is_some();
    let app_cursor_keys = session
        .as_ref()
        .map(|s| s.terminal.screen.application_cursor_keys())
        .unwrap_or(false);
    let modifiers = ctx.input(|i| i.modifiers);
    process_keyboard_input(
        &ctx,
        term_focused,
        has_selection,
        modifiers,
        app_cursor_keys,
        &mut copy_requested,
        &mut pending_input,
        &mut paste_texts,
    );

    if let Some(session) = session.as_mut() {
        if copy_requested {
            if let Some(ref sel) = session.selection {
                let text = sel.text(&session.terminal.screen);
                if !text.is_empty() {
                    ctx.copy_text(text);
                }
                session.selection = None;
                session.selection_pointer = None;
            }
        }
        for text in paste_texts {
            paste_to_session(session, &text, &ctx, &mut action);
        }
        for bytes in pending_input {
            session.handle.send(bytes);
        }
    }

    // Right-click context menu for copy/paste (paste handled after menu closes)
    term_resp.context_menu(|ui| {
        let has_sel = session.as_ref().and_then(|s| s.selection.as_ref()).is_some();
        if ui.add_enabled(has_sel, egui::Button::new("Copy")).clicked() {
            if let Some(session) = session.as_mut() {
                if let Some(ref sel) = session.selection {
                    let text = sel.text(&session.terminal.screen);
                    if !text.is_empty() {
                        ctx.copy_text(text);
                    }
                    session.selection = None;
                    session.selection_pointer = None;
                }
            }
            ui.close_menu();
        }
        if ui.button("Paste").clicked() {
            context_menu_paste = true;
            ui.close_menu();
        }
    });

    if context_menu_paste {
        if let Some(session) = session.as_mut() {
            if let Some(text) = read_text() {
                paste_to_session(session, &text, &ctx, &mut action);
            }
        }
    }

    if let Some(session) = session.as_mut() {
        if session.want_terminal_focus && term_resp.has_focus() {
            session.want_terminal_focus = false;
        }
    }

    // Drain all pending PTY chunks before painting (avoids half-colored history frames).
    let mut terminal_dirty = false;
    if let Some(session) = session.as_mut() {
        while drain_connection(session, &mut action) {
            terminal_dirty = true;
        }
    }
    if terminal_dirty {
        if let Some(session) = session.as_mut() {
            session.row_galley_cache.clear();
        }
        term_resp.mark_changed();
    }

    if ui.is_rect_visible(panel_rect) {
        let painter = ui.painter_at(panel_rect);
        painter.rect_filled(panel_rect, egui::CornerRadius::ZERO, theme.bg);

        let show_size_label = session
            .as_mut()
            .map(|s| {
                let label_cols = if desired_cols != grid_cols {
                    desired_cols
                } else {
                    grid_cols
                };
                let label_rows = if desired_rows != grid_rows {
                    desired_rows
                } else {
                    grid_rows
                };
                size_label_visible(s, label_cols, label_rows, &ctx)
            })
            .unwrap_or(false);

        if let Some(session) = session.as_mut() {
            let screen = &session.terminal.screen;
            let font_id = egui::FontId::monospace(*font_size);

            let in_alt = screen.in_alternate_screen();
            if in_alt {
                // vim/htop: do not scroll the shell scrollback behind the alternate buffer.
                session.scroll_offset = 0;
            }

            let sb_lines = screen.scrollback_lines();
            let mouse_to_pty = screen.mouse_tracking_active() && !modifiers.shift;
            let mut wheel_input: Vec<Vec<u8>> = Vec::new();
            process_terminal_wheel(
                &term_resp,
                grid_rect,
                cell_w,
                cell_h,
                grid_rows,
                grid_cols,
                screen,
                in_alt,
                sb_lines,
                &mut session.scroll_offset,
                &mut wheel_input,
            );
            for bytes in wheel_input {
                session.handle.send(bytes);
            }

            let mut mouse_input: Vec<Vec<u8>> = Vec::new();
            if mouse_to_pty {
                process_terminal_mouse(
                    ui,
                    &term_resp,
                    grid_rect,
                    cell_w,
                    cell_h,
                    grid_rows,
                    grid_cols,
                    screen,
                    &mut mouse_input,
                    &mut session.mouse_motion_last,
                );
            }
            for bytes in mouse_input {
                session.handle.send(bytes);
            }

            let offset = session.scroll_offset;

            for row in 0..grid_rows {
                if row < offset {
                    let line_index = offset - 1 - row;
                    if let Some(cells) = screen.scrollback_row(line_index) {
                        let y = grid_rect.top() + row as f32 * cell_h;
                        paint_row(
                            &painter,
                            ui,
                            &mut session.row_galley_cache,
                            &font_id,
                            *font_size,
                            theme,
                            cells,
                            grid_cols,
                            grid_rect.left(),
                            y,
                            cell_w,
                            cell_h,
                        );
                    }
                } else {
                    let screen_row = row - offset;
                    if screen_row < screen.rows {
                        let y = grid_rect.top() + row as f32 * cell_h;
                        let cells = &screen.cells[screen_row];
                        paint_row(
                            &painter,
                            ui,
                            &mut session.row_galley_cache,
                            &font_id,
                            *font_size,
                            theme,
                            cells,
                            grid_cols,
                            grid_rect.left(),
                            y,
                            cell_w,
                            cell_h,
                        );
                    }
                }
            }

            // Cursor (only at bottom)
            if offset == 0
                && screen.cursor_visible
                && screen.cursor_y < grid_rows
                && screen.cursor_x < grid_cols
            {
                let cx = grid_rect.left() + screen.cursor_x as f32 * cell_w;
                let cy = grid_rect.top() + screen.cursor_y as f32 * cell_h;
                painter.rect_stroke(
                    egui::Rect::from_min_size(egui::pos2(cx, cy), egui::vec2(cell_w, cell_h)),
                    egui::CornerRadius::ZERO,
                    egui::Stroke::new(1.0, theme.cursor),
                    egui::StrokeKind::Inside,
                );
            }

            // Selection highlight
            if let Some(ref sel) = session.selection {
                paint_selection(&painter, screen, theme, grid_rect, cell_w, cell_h, offset, sel);
            }

            // Selection from mouse/touch (disabled while mouse reporting unless Shift).
            if !mouse_to_pty {
                update_terminal_selection(
                    &mut session.selection,
                    &mut session.selection_pointer,
                    screen,
                    offset,
                    ui,
                    &term_resp,
                    grid_rect,
                    cell_w,
                    cell_h,
                    grid_rows,
                    grid_cols,
                );
            }

            if show_size_label {
                let (label_cols, label_rows) = if desired_cols != grid_cols || desired_rows != grid_rows {
                    (desired_cols, desired_rows)
                } else {
                    (grid_cols, grid_rows)
                };
                let dim_label = format!("{label_cols}×{label_rows}");
                let dim_color = egui::Color32::from_rgba_premultiplied(
                    theme.fg.r(),
                    theme.fg.g(),
                    theme.fg.b(),
                    140,
                );
                painter.text(
                    panel_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    dim_label,
                    egui::FontId::monospace(13.0),
                    dim_color,
                );
            }

            // Scrollbar (thumb at bottom when viewing the live tail / offset == 0)
            if sb_lines > 0 && !in_alt {
                let sb_total = sb_lines + screen.rows;
                let sb_pos = 1.0 - (offset as f32 / sb_total as f32);
                let sb_visible = grid_rows as f32 / sb_total as f32;
                let bar_x = grid_rect.right() - 6.0;
                let bar_h = (grid_size.y * sb_visible).max(8.0);
                let bar_y = grid_rect.top() + grid_size.y * (sb_pos - sb_visible).max(0.0);
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(bar_x, bar_y), egui::vec2(4.0, bar_h)),
                    egui::CornerRadius::same(2),
                    egui::Color32::from_rgba_premultiplied(255, 255, 255, 60),
                );
            }
        }
    }

    // 6. Virtual keyboard — fixed-height bottom strip so rows are not pushed/clipped
    if keyboard.visible {
        ui.separator();
        let kb_h = keyboard.content_height(ui.available_width());
        let kbd_output = ui
            .allocate_ui_with_layout(
                egui::vec2(ui.available_width(), kb_h),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| keyboard.show(ui),
            )
            .inner;
        if let Some(session) = session.as_mut() {
            let mut sent = false;
            for data in &kbd_output {
                if !data.is_empty() {
                    session.handle.send(data.clone());
                    sent = true;
                }
            }
            let _ = sent;
        }
    }

    if let Some(session) = session.as_mut() {
        session.terminal_had_focus = term_resp.has_focus();
    }
    if term_resp.has_focus() {
        lock_terminal_focus(ui.ctx());
    }

    action
}

/// Show `cols×rows` while the grid is changing, then for one second after it stabilizes.
fn size_label_visible(
    session: &mut ActiveSession,
    cols: usize,
    rows: usize,
    ctx: &egui::Context,
) -> bool {
    let dims = (cols, rows);
    let now = Instant::now();

    if dims != session.size_label_dims {
        session.size_label_dims = dims;
        session.size_label_active = true;
        session.size_label_hide_at = None;
        return true;
    }

    if !session.size_label_active {
        return false;
    }

    if session.size_label_hide_at.is_none() {
        session.size_label_hide_at = Some(now + Duration::from_secs(1));
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    session.size_label_hide_at.is_some_and(|deadline| now < deadline)
}

/// Pull as much post-resize output as possible (htop sends a full-screen refresh via ncurses).
fn drain_after_resize(
    session: &mut ActiveSession,
    action: &mut ConnectionViewAction,
    in_alt: bool,
) {
    for _ in 0..256 {
        if !drain_connection(session, action) {
            break;
        }
    }
    if in_alt {
        session.handle.signal_winch();
        for _ in 0..128 {
            if !drain_connection(session, action) {
                break;
            }
        }
    }
}

/// Resize the on-screen cell grid to match the window (immediate, centered layout).
fn sync_display_grid(
    session: &mut ActiveSession,
    renderer: &mut TerminalRenderer,
    rows: usize,
    cols: usize,
    font_size: f32,
) {
    let rows = rows.max(1);
    let cols = cols.max(1);
    if renderer.rows == rows && renderer.cols == cols && session.layout_font_size == font_size {
        return;
    }
    renderer.rows = rows;
    renderer.cols = cols;
    session.terminal.resize(rows, cols);
    session.layout_font_size = font_size;
    session.row_galley_cache.clear();
    session.scroll_offset = 0;
}

/// Resize the PTY (SIGWINCH). Debounced for normal shell; immediate for alt-screen / shrink.
fn sync_pty_size(session: &mut ActiveSession, rows: usize, cols: usize) {
    let rows = rows.max(1) as u16;
    let cols = cols.max(1) as u16;
    if session.last_pty_rows == rows && session.last_pty_cols == cols {
        return;
    }
    session.last_pty_rows = rows;
    session.last_pty_cols = cols;
    session.handle.resize(rows, cols);
}

/// Header control: clickable but not in keyboard focus navigation (Tab/arrows).
fn toolbar_button(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(egui::Button::new(label.into()).sense(egui::Sense::CLICK))
}

/// Paste into the PTY. Use raw bytes at the shell prompt (immediate echo); bracketed only in alt-screen apps.
pub fn paste_to_session(
    session: &mut ActiveSession,
    text: &str,
    ctx: &egui::Context,
    action: &mut ConnectionViewAction,
) {
    let bracketed =
        session.terminal.screen.bracketed_paste_enabled() && session.terminal.screen.in_alternate_screen();
    session.handle.send(paste_payload(text, bracketed));
    let _ = drain_connection(session, action);
    ctx.request_repaint();
}

/// Read pending bytes from the connection and apply them to the terminal emulator.
pub(crate) fn drain_connection(session: &mut ActiveSession, action: &mut ConnectionViewAction) -> bool {
    let mut updated = false;
    let mut pty_data = Vec::new();
    for ev in session.handle.drain() {
        match ev {
            ConnIn::Data(data) => pty_data.extend(data),
            ConnIn::StateChanged(s) => {
                if matches!(s, ConnectionState::Disconnected | ConnectionState::Error(_)) {
                    *action = ConnectionViewAction::CloseSession;
                }
            }
        }
    }
    if !pty_data.is_empty() {
        session.terminal.write(&pty_data);
        updated = true;
    }
    session.handle.repaint.clear_repaint_pending();
    for resp in session.terminal.drain_pending() {
        match resp {
            TermEvent::Response(data) => session.handle.send(data),
            TermEvent::PtyResize { rows: _, cols: _ } => {
                // CSI 8 no longer emits this; keep arm so older tests / sequences do not resize the PTY.
            }
        }
    }
    updated
}

