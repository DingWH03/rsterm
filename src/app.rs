use crate::connection::{ble, serial, ssh};
#[cfg(not(target_os = "android"))]
use crate::connection::local;
use crate::fonts;
use crate::settings::{AppSettings, save_settings};
use crate::storage;
use crate::storage::types::{ConnectionType, SavedConnection};
use crate::terminal::{DEFAULT_GRID_COLS, DEFAULT_GRID_ROWS};
use crate::terminal::Terminal;
use crate::session::{FileManagerMode, FileManagerSession, WorkspaceSession};
use crate::ui::page::terminal::{
    drain_connection, ActiveSession, ConnectionViewAction, connection_view,
};
use crate::ui::page::file_manager::{file_manager_view, FileManagerAction};
use crate::ui::widget::dialogs::{LocalTerminalSettingsDialog, NewConnectionDialog};
use crate::ui::widget::keyboard::VirtualKeyboard;
use crate::ui::page::settings::{settings_page, settings_side_panel};
use crate::ui::widget::sidebar::{Sidebar, DOCK_WIDTH};
use crate::ui::widget::style;
use crate::ui::widget::sidebar::sidebars::{main_sidebar, connections_sidebar, MainSidebarAction, ConnectionsSidebarAction};
use log::info;

#[derive(Clone, Copy, PartialEq)]
enum SidebarMode {
    Main,
    Connections,
}

pub struct RsTerminalApp {
    settings: AppSettings,
    saved_connections: Vec<SavedConnection>,
    sessions: Vec<WorkspaceSession>,
    active_session_id: Option<String>,
    virtual_keyboard: VirtualKeyboard,
    new_conn_dialog: NewConnectionDialog,
    local_term_dialog: LocalTerminalSettingsDialog,
    live_font_size: f32,
    sidebar: Sidebar,
    sidebar_mode: SidebarMode,
    /// Central panel: full-page settings (narrow layout).
    workspace_settings: bool,
    /// Right panel: settings (wide layout only).
    settings_open: bool,
    /// Immediate connect failure (serial open, SSH config, etc.) before a session is opened.
    connection_notice: Option<String>,
    /// User confirmed exit while sessions were still open.
    quit_after_close: bool,
    /// Show「仍有会话，是否退出」dialog.
    show_quit_dialog: bool,
}

impl Default for RsTerminalApp {
    fn default() -> Self {
        let settings = crate::settings::load_settings();
        // Apply the saved language preference on startup.
        settings.language.apply();
        let live_font_size = settings.font_size();
        let kbd_mode = settings.default_profile().keyboard_mode;
        let saved = storage::load_connections();
        Self {
            settings,
            saved_connections: saved,
            sessions: Vec::new(),
            active_session_id: None,
            virtual_keyboard: VirtualKeyboard::new(kbd_mode),
            new_conn_dialog: NewConnectionDialog::default(),
            local_term_dialog: LocalTerminalSettingsDialog::default(),
            live_font_size,
            sidebar: Sidebar::new(),
            sidebar_mode: SidebarMode::Main,
            workspace_settings: false,
            settings_open: false,
            connection_notice: None,
            quit_after_close: false,
            show_quit_dialog: false,
        }
    }
}

fn show_quit_confirm_dialog(
    ctx: &egui::Context,
    open: &mut bool,
    session_count: usize,
) -> bool {
    if !*open {
        return false;
    }
    let mut confirmed = false;
    egui::Window::new(rust_i18n::t!("quit_with_sessions_title"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_max_width(400.0);
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(rust_i18n::t!("quit_with_sessions_body", count = session_count))
                    .size(14.0)
                    .color(ui.visuals().text_color()),
            );
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                let cancel_btn = egui::Button::new(
                    egui::RichText::new(rust_i18n::t!("cancel"))
                        .size(14.0)
                        .color(ui.visuals().weak_text_color()),
                )
                .fill(ui.visuals().panel_fill)
                .corner_radius(style::CORNER_RADIUS_SM)
                .min_size(egui::vec2(90.0, 34.0));
                if ui.add(cancel_btn).clicked() {
                    *open = false;
                }

                let confirm_btn = egui::Button::new(
                    egui::RichText::new(rust_i18n::t!("quit_with_sessions_confirm"))
                        .size(14.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(style::RED)
                .corner_radius(style::CORNER_RADIUS_SM)
                .min_size(egui::vec2(100.0, 34.0));
                if ui.add(confirm_btn).clicked() {
                    confirmed = true;
                    *open = false;
                }
            });
        });
    confirmed
}

fn show_connection_notice(ctx: &egui::Context, notice: &mut Option<String>) {
    let Some(msg) = notice.clone() else {
        return;
    };
    let mut dismiss = false;
    egui::Window::new(rust_i18n::t!("connection_failed"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(&msg)
                    .size(14.0)
                    .color(ui.visuals().text_color()),
            );
            ui.add_space(16.0);
            let ok_btn = egui::Button::new(
                egui::RichText::new(rust_i18n::t!("ok"))
                    .size(14.0)
                    .color(egui::Color32::WHITE),
            )
            .fill(style::ACCENT)
            .corner_radius(style::CORNER_RADIUS_SM)
            .min_size(egui::vec2(80.0, 34.0));
            if ui.add(ok_btn).clicked() {
                dismiss = true;
            }
        });
    if dismiss {
        *notice = None;
    }
}

impl RsTerminalApp {
    fn reload_terminal_fonts(&mut self, ctx: &egui::Context) {
        fonts::apply_terminal_fonts(ctx, &self.settings.default_profile().terminal_font);
        let font_gen = fonts::font_generation();
        for session in &mut self.sessions {
            if let WorkspaceSession::Terminal(term) = session {
                term.clear_all_galley_caches();
                term.font_generation = font_gen;
            }
        }
    }

    fn push_session(&mut self, session: WorkspaceSession) {
        let id = session.id().to_string();
        self.active_session_id = Some(id);
        self.sessions.push(session);
    }

    fn open_file_manager_ssh(&mut self, conn_id: &str) {
        let config = match self.saved_connections.iter().find(|c| c.id == conn_id) {
            Some(c) => c.clone(),
            None => return,
        };
        match FileManagerSession::open_ssh(&config) {
            Ok(fm) => self.push_session(WorkspaceSession::FileManager(fm)),
            Err(e) => info!("SFTP failed: {e}"),
        }
    }

    fn open_file_manager_local(&mut self) {
        self.push_session(WorkspaceSession::FileManager(FileManagerSession::open_local()));
    }

    #[cfg(not(target_os = "android"))]
    fn connect_local(&mut self) {
        let Some(config) = self
            .saved_connections
            .iter()
            .find(|c| c.conn_type == ConnectionType::Local)
            .cloned()
        else {
            self.connection_notice = Some(
                "No saved Local Terminal connection. Add one via the + button.".into(),
            );
            return;
        };
        let profile = self.settings.default_profile().clone();
        match local::connect_local(&config, &profile, 24, 80) {
            Ok(handle) => self.open_session(handle, &config, profile.scrollback_lines),
            Err(e) => self.connection_notice = Some(e),
        }
    }

    #[cfg(not(target_os = "android"))]
    fn reconnect_local_session(&mut self, session_id: &str, config: &SavedConnection) {
        let Some(idx) = self.sessions.iter().position(|s| s.id() == session_id) else {
            return;
        };
        let WorkspaceSession::Terminal(term) = &mut self.sessions[idx] else {
            return;
        };
        if term.conn_type != ConnectionType::Local {
            return;
        }
        term.handle.close();
        let profile = self.settings.default_profile().clone();
        let rows = term.last_pty_rows.max(1);
        let cols = term.last_pty_cols.max(1);
        match local::connect_local(config, &profile, rows, cols) {
            Ok(handle) => {
                term.handle = handle;
                term.saved_conn_id = Some(config.id.clone());
                term.name = config.name.clone();
                term.user_at_host = crate::platform::get().local_user_at_host();
                term.want_terminal_focus = true;
                term.selection = None;
                term.selection_pointer = None;
            }
            Err(e) => term.disconnect_message = Some(e),
        }
    }

    fn apply_local_terminal_settings(
        &mut self,
        apply: crate::ui::widget::dialogs::LocalTerminalSettingsApply,
    ) {
        if self
            .saved_connections
            .iter()
            .any(|c| c.id == apply.config.id)
        {
            if let Some(pos) = self
                .saved_connections
                .iter()
                .position(|c| c.id == apply.config.id)
            {
                self.saved_connections[pos] = apply.config.clone();
            }
            storage::save_connections(&self.saved_connections);
            self.settings.default_local_connection_id = Some(apply.config.id.clone());
            save_settings(&self.settings);
        }
        #[cfg(not(target_os = "android"))]
        if let Some(session_id) = &apply.session_id {
            self.reconnect_local_session(session_id, &apply.config);
        }
    }

    fn connect_to(&mut self, conn_id: &str) {
        let config = match self.saved_connections.iter().find(|c| c.id == conn_id) {
            Some(c) => c.clone(),
            None => return,
        };
        let profile = self.settings.default_profile().clone();
        let result = match config.conn_type {
            #[cfg(not(target_os = "android"))]
            ConnectionType::Local => local::connect_local(&config, &profile, 24, 80),
            #[cfg(target_os = "android")]
            ConnectionType::Local => Err("Local terminal is not supported on Android".into()),
            ConnectionType::Ssh => ssh::connect_ssh(&config, &self.settings.ssh_env_vars, 24, 80),
            ConnectionType::Serial => serial::connect_serial(&config),
            ConnectionType::Ble => ble::connect_ble(&config),
        };
        match result {
            Ok(handle) => self.open_session(handle, &config, profile.scrollback_lines),
            Err(e) => self.connection_notice = Some(e),
        }
    }

    fn open_session(
        &mut self,
        handle: crate::connection::ConnectionHandle,
        config: &SavedConnection,
        scrollback_lines: usize,
    ) {
        let profile = self.settings.default_profile();
        let mut terminal = Terminal::new(DEFAULT_GRID_ROWS, DEFAULT_GRID_COLS);
        terminal.set_scrollback_limit(scrollback_lines);
        self.live_font_size = profile.font_size;
        self.virtual_keyboard = VirtualKeyboard::new(profile.keyboard_mode);

        let user_at_host = match config.conn_type {
            ConnectionType::Local => crate::platform::get().local_user_at_host(),
            ConnectionType::Ssh => {
                let user = config.ssh_user.as_deref().unwrap_or("root");
                let host = config.ssh_host.as_deref().unwrap_or("host");
                crate::platform::get().ssh_user_at_host(user, host)
            }
            _ => String::new(),
        };

        self.push_session(WorkspaceSession::Terminal(ActiveSession {
            id: uuid::Uuid::new_v4().to_string(),
            conn_type: config.conn_type.clone(),
            saved_conn_id: Some(config.id.clone()),
            name: config.name.clone(),
            user_at_host,
            handle,
            terminal,
            active_port: 0,
            ports: Vec::new(),
            inactive_port_states: Default::default(),
            port_unread: Default::default(),
            scrollback_lines,
            scroll_offset: 0,
            selection: None,
            selection_pointer: None,
            touch_state: Default::default(),
            want_terminal_focus: true,
            terminal_had_focus: false,
            row_galley_cache: Default::default(),
            layout_font_size: self.live_font_size,
            grid_rows: DEFAULT_GRID_ROWS,
            grid_cols: DEFAULT_GRID_COLS,
            last_pty_rows: DEFAULT_GRID_ROWS as u16,
            last_pty_cols: DEFAULT_GRID_COLS as u16,
            size_label_dims: (DEFAULT_GRID_COLS, DEFAULT_GRID_ROWS),
            size_label_hide_at: None,
            size_label_active: false,
            mouse_motion_last: None,
            font_generation: crate::fonts::font_generation(),
            disconnect_message: None,
        }));
    }

    fn has_open_sessions(&self) -> bool {
        !self.sessions.is_empty()
    }

    fn close_all_sessions(&mut self) {
        let ids: Vec<String> = self.sessions.iter().map(|s| s.id().to_string()).collect();
        for id in ids {
            self.close_session(&id);
        }
    }

    fn close_session(&mut self, id: &str) {
        if let Some(pos) = self.sessions.iter().position(|s| s.id() == id) {
            if let WorkspaceSession::Terminal(s) = &mut self.sessions[pos] {
                s.handle.close();
            }
            self.sessions.remove(pos);
        }
        if self.active_session_id.as_deref() == Some(id) {
            self.active_session_id = self.sessions.last().map(|s| s.id().to_string());
        }
        if self.sessions.is_empty() {
            self.active_session_id = None;
            self.save_profile_tweaks();
        }
    }

    fn save_profile_tweaks(&mut self) {
        if let Some(profile) = self
            .settings
            .profiles
            .iter_mut()
            .find(|p| p.name == self.settings.default_profile_name)
        {
            profile.font_size = self.live_font_size;
            profile.keyboard_mode = self.virtual_keyboard.mode;
            save_settings(&self.settings);
        }
    }

    fn drain_inactive_sessions(&mut self) {
        let active = self.active_session_id.as_deref();
        for session in &mut self.sessions {
            if active == Some(session.id()) {
                continue;
            }
            if let Some(term) = session.terminal_mut() {
                let mut noop = ConnectionViewAction::None;
                drain_connection(term, &mut noop);
            }
        }
    }

    fn open_new_window_for_session(&mut self, session_id: &str) {
        enum DupPlan {
            #[cfg(not(target_os = "android"))]
            TerminalLocal,
            TerminalSsh(String),
            FileSsh(String),
            FileLocal,
        }
        let plan = self.sessions.iter().find(|s| s.id() == session_id).and_then(|s| {
            match s {
                WorkspaceSession::Terminal(term) => match term.conn_type {
                    #[cfg(not(target_os = "android"))]
                    ConnectionType::Local => Some(DupPlan::TerminalLocal),
                    #[cfg(target_os = "android")]
                    ConnectionType::Local => None,
                    ConnectionType::Ssh => term
                        .saved_conn_id
                        .clone()
                        .map(DupPlan::TerminalSsh),
                    ConnectionType::Serial | ConnectionType::Ble => None,
                },
                WorkspaceSession::FileManager(fm) => match fm.mode {
                    FileManagerMode::SshSftp => fm
                        .saved_conn_id
                        .clone()
                        .map(DupPlan::FileSsh),
                    FileManagerMode::LocalDual => Some(DupPlan::FileLocal),
                },
            }
        });
        match plan {
            #[cfg(not(target_os = "android"))]
            Some(DupPlan::TerminalLocal) => self.connect_local(),
            Some(DupPlan::TerminalSsh(id)) => self.connect_to(&id),
            Some(DupPlan::FileSsh(id)) => self.open_file_manager_ssh(&id),
            Some(DupPlan::FileLocal) => self.open_file_manager_local(),
            None => {}
        }
    }

    fn apply_session_panel_action(
        &mut self,
        action: crate::ui::widget::sidebar::common::SidebarSessionAction,
        in_overlay: bool,
    ) {
        if let Some(id) = action.select_session {
            self.active_session_id = Some(id);
            self.workspace_settings = false;
            if in_overlay {
                self.sidebar.close_overlay();
            }
        }
        if let Some(id) = action.close_session {
            self.close_session(&id);
        }
        if let Some(id) = action.new_window_session {
            self.open_new_window_for_session(&id);
            if in_overlay {
                self.sidebar.close_overlay();
            }
        }
    }

    fn handle_back_navigation(&mut self, ctx: &egui::Context) -> bool {
        if self.connection_notice.take().is_some() {
            return true;
        }
        if self.show_quit_dialog {
            self.show_quit_dialog = false;
            return true;
        }
        if self.new_conn_dialog.open {
            self.new_conn_dialog = NewConnectionDialog::default();
            return true;
        }
        if self.local_term_dialog.open {
            self.local_term_dialog = LocalTerminalSettingsDialog::default();
            return true;
        }
        // In connections management mode → go back to main sidebar
        if self.sidebar_mode == SidebarMode::Connections {
            self.sidebar_mode = SidebarMode::Main;
            return true;
        }
        if self.sidebar.overlay_visible() {
            self.sidebar.close_overlay();
            return true;
        }
        if self.workspace_settings {
            self.workspace_settings = false;
            save_settings(&self.settings);
            self.live_font_size = self.settings.font_size();
            self.reload_terminal_fonts(ctx);
            return true;
        }
        if self.settings_open {
            self.settings_open = false;
            save_settings(&self.settings);
            self.live_font_size = self.settings.font_size();
            self.reload_terminal_fonts(ctx);
            return true;
        }
        if self.has_open_sessions() {
            self.show_quit_dialog = true;
            return true;
        }

        false
    }

    fn active_session_index(&self) -> Option<usize> {
        self.active_session_id
            .as_ref()
            .and_then(|id| self.sessions.iter().position(|s| s.id() == id))
    }
}

impl eframe::App for RsTerminalApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.quit_after_close {
                return;
            }
            if self.handle_back_navigation(ctx) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Android back button is handled via accesskit → close_requested()
        // in `logic()` above.  No special handling needed in `ui()`.
        // Apply UI theme on every frame (cheap — only changes if setting changed).
        self.settings.ui_theme.apply(&ctx);
        self.sidebar.sync_width(ctx.content_rect().width());

        show_connection_notice(&ctx, &mut self.connection_notice);

        // Android status‑bar inset (0 on desktop).
        let top_inset: f32 = {
            #[cfg(target_os = "android")] {
                crate::platform::get().top_inset_points(&ctx)
            }
            #[cfg(not(target_os = "android"))]
            { 0.0 }
        };

        let session_count = self.sessions.len();
        if show_quit_confirm_dialog(&ctx, &mut self.show_quit_dialog, session_count) {
            self.quit_after_close = true;
            self.close_all_sessions();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        self.drain_inactive_sessions();

        let on_settings = self.workspace_settings
            || (self.sidebar.wide && self.settings_open);

        let mut sidebar_action = MainSidebarAction {
            select_session: None,
            close_session: None,
            new_window_session: None,
            open_connection_mgmt: false,
            settings_toggled: false,
        };
        let mut conn_action = ConnectionsSidebarAction {
            go_back: false,
            new_connection: false,
            connect_connection: None,
            open_file_mgr: None,
            edit_connection: None,
            delete_connection: None,
        };

        // ── Sidebar + content layout (responsive) ────────────────────────────
        let show_sidebar = if self.sidebar.wide {
            self.sidebar.docked_visible()
        } else {
            self.sidebar.overlay_visible()
        };

        // Sidebar panel (hide when settings open on narrow)
        let is_wide = self.sidebar.wide;
        if show_sidebar && !(self.workspace_settings && !is_wide) {
            if is_wide {
                egui::Panel::left("main_sidebar")
                    .min_size(DOCK_WIDTH)
                    .max_size(280.0)
                    .resizable(true)
                    .show_inside(ui, |ui| {
                        ui.add_space(top_inset);
                        match self.sidebar_mode {
                            SidebarMode::Main => {
                                sidebar_action = main_sidebar(ui, &mut self.sidebar, &self.sessions, self.active_session_id.as_deref(), on_settings);
                            }
                            SidebarMode::Connections => {
                                conn_action = connections_sidebar(ui, &self.saved_connections);
                            }
                        }
                    });
            } else {
                egui::Panel::left("main_sidebar_narrow")
                    .min_size(ui.available_width())
                    .resizable(false)
                    .show_inside(ui, |ui| {
                        ui.add_space(top_inset);
                        match self.sidebar_mode {
                            SidebarMode::Main => {
                                sidebar_action = main_sidebar(ui, &mut self.sidebar, &self.sessions, self.active_session_id.as_deref(), on_settings);
                            }
                            SidebarMode::Connections => {
                                conn_action = connections_sidebar(ui, &self.saved_connections);
                            }
                        }
                    });
            }
        }

        // Right settings panel (wide only)
        if self.sidebar.wide && self.settings_open && !self.workspace_settings {
            let mut close_settings = false;
            egui::Panel::right("settings_panel")
                .min_size(300.0)
                .max_size(420.0)
                .resizable(true)
                .show_inside(ui, |ui| {
                    close_settings = settings_side_panel(ui, &mut self.settings);
                });
            if close_settings {
                self.settings_open = false;
                self.reload_terminal_fonts(&ctx);
            }
        }

        // Settings toggle (after sidebar rendering so the action is captured)
        if sidebar_action.settings_toggled {
            if self.sidebar.wide {
                self.settings_open = !self.settings_open;
                self.workspace_settings = false;
                self.sidebar_mode = SidebarMode::Main;
            } else {
                self.workspace_settings = true;
                self.settings_open = false;
                self.sidebar.close_overlay();
                self.sidebar_mode = SidebarMode::Main;
            }
        }

        // ── Recent-connections / empty-state actions ────────────────────────
        let mut center_connect_clicked: Option<String> = None;
        let mut center_more_clicked = false;

        // Central content (wide: always; narrow: only when sidebar hidden or settings open)
        let mut view_action = ConnectionViewAction::None;
        let mut fm_action = FileManagerAction::default();
        if self.sidebar.wide || !show_sidebar || self.workspace_settings {
            if let Some(idx) = self.active_session_index() {
                if let WorkspaceSession::Terminal(term) = &mut self.sessions[idx] {
                    term.handle.repaint.set_context(ctx.clone());
                }
            }

            egui::CentralPanel::default().show_inside(ui, |ui| {
                ui.add_space(top_inset);
                if self.workspace_settings {
                    if settings_page(ui, &mut self.settings) {
                        self.workspace_settings = false;
                        save_settings(&self.settings);
                        self.live_font_size = self.settings.font_size();
                        self.reload_terminal_fonts(ui.ctx());
                    }
                } else if let Some(idx) = self.active_session_index() {
                    match &mut self.sessions[idx] {
                        WorkspaceSession::Terminal(term) => {
                            let theme = self.settings.theme();
                            let cursor_style = self.settings.cursor_style();
                            let cell_width_scale = self.settings.default_profile().cell_width_scale;
                            view_action = connection_view(
                                ui,
                                Some(term),
                                &mut self.virtual_keyboard,
                                theme,
                                cursor_style,
                                &mut self.live_font_size,
                                cell_width_scale,
                                &mut self.sidebar,
                            );
                        }
                        WorkspaceSession::FileManager(fm) => {
                            fm_action = file_manager_view(ui, fm, &mut self.sidebar);
                        }
                    }
                } else {
                    recent_connections_view(
                        ui,
                        &mut self.sidebar,
                        &self.saved_connections,
                        &mut center_connect_clicked,
                        &mut center_more_clicked,
                    );
                }
            });
        }

        // ── Post-frame actions ────────────────────────────────────────────────
        if sidebar_action.open_connection_mgmt {
            self.sidebar_mode = SidebarMode::Connections;
        }
        if let Some(ref id) = center_connect_clicked {
            self.connect_to(id);
        }
        if center_more_clicked {
            if !self.sidebar.wide {
                self.sidebar.open_overlay();
            }
            self.sidebar_mode = SidebarMode::Connections;
        }
        if conn_action.go_back {
            self.sidebar_mode = SidebarMode::Main;
        }
        if conn_action.new_connection {
            self.new_conn_dialog.open_new();
        }
        if let Some(ref id) = conn_action.connect_connection {
            self.connect_to(id);
            self.sidebar_mode = SidebarMode::Main;
            self.sidebar.close_overlay();
        }
        if let Some(ref id) = conn_action.open_file_mgr {
            if let Some(conn) = self.saved_connections.iter().find(|c| c.id == *id) {
                match conn.conn_type {
                    ConnectionType::Local => self.open_file_manager_local(),
                    ConnectionType::Ssh => self.open_file_manager_ssh(id),
                    _ => {}
                }
            }
            self.sidebar_mode = SidebarMode::Main;
            self.sidebar.close_overlay();
        }
        if let Some(ref id) = conn_action.edit_connection {
            if let Some(conn) = self.saved_connections.iter().find(|c| c.id == *id) {
                self.new_conn_dialog.open_edit(conn);
            }
        }
        if let Some(ref id) = conn_action.delete_connection {
            self.saved_connections.retain(|c| c.id != *id);
            storage::save_connections(&self.saved_connections);
        }
        if let Some(apply) = self.local_term_dialog.show(&ctx, &self.saved_connections) {
            self.apply_local_terminal_settings(apply);
        }
        if self.workspace_settings || self.settings_open || sidebar_action.settings_toggled {
            save_settings(&self.settings);
            self.live_font_size = self.settings.font_size();
            self.reload_terminal_fonts(&ctx);
        }
        if let Some(new_conn) = self.new_conn_dialog.show(&ctx) {
            if let Some(pos) = self
                .saved_connections
                .iter()
                .position(|c| c.id == new_conn.id)
            {
                self.saved_connections[pos] = new_conn;
            } else {
                self.saved_connections.push(new_conn);
            }
            storage::save_connections(&self.saved_connections);
        }

        self.apply_session_panel_action(
            crate::ui::widget::sidebar::common::SidebarSessionAction {
                select_session: sidebar_action.select_session,
                close_session: sidebar_action.close_session,
                new_window_session: sidebar_action.new_window_session,
            },
            self.sidebar.overlay_visible(),
        );
        if matches!(view_action, ConnectionViewAction::CloseSession) || fm_action.close {
            if let Some(id) = self.active_session_id.clone() {
                self.close_session(&id);
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(400));
    }
}

// ─── Recent connections view (central panel, no active session) ──────────────

fn recent_connections_view(
    ui: &mut egui::Ui,
    sidebar: &mut Sidebar,
    connections: &[SavedConnection],
    connect_clicked: &mut Option<String>,
    more_clicked: &mut bool,
) {
    // Collect all connections, sorted by last_connected desc (recently connected first)
    let mut recent: Vec<&SavedConnection> = connections.iter().collect();
    recent.sort_by(|a, b| {
        b.last_connected
            .as_deref()
            .unwrap_or("")
            .cmp(&a.last_connected.as_deref().unwrap_or(""))
            .then_with(|| a.name.cmp(&b.name))
    });
    let show_count = recent.len().min(5);
    let recent = &recent[..show_count];

    if recent.is_empty() {
        // ── Empty state ──────────────────────────────────────────────────
        // Header bar with hamburger (matching terminal layout)
        ui.horizontal(|ui| {
            ui.style_mut().spacing.button_padding = egui::vec2(4.0, 1.0);
            ui.style_mut().spacing.item_spacing.x = 4.0;

            if sidebar.show_content_hamburger()
                && sidebar.hamburger(ui).clicked()
            {
                sidebar.hamburger_click();
            }
            ui.label(
                egui::RichText::new(rust_i18n::t!("recent_connections"))
                    .size(12.0)
                    .strong()
                    .color(ui.visuals().text_color()),
            );
        });
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("\u{1F4CB}").size(36.0));
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(rust_i18n::t!("home_no_connections"))
                    .size(15.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(rust_i18n::t!("open_terminal_hint"))
                    .size(12.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
        return;
    }

    // ── Header bar (matching terminal layout) ────────────────────────────
    ui.horizontal(|ui| {
        ui.style_mut().spacing.button_padding = egui::vec2(4.0, 1.0);
        ui.style_mut().spacing.item_spacing.x = 4.0;

        if sidebar.show_content_hamburger()
            && sidebar.hamburger(ui).clicked()
        {
            sidebar.hamburger_click();
        }
        ui.label(
            egui::RichText::new(rust_i18n::t!("recent_connections"))
                .size(12.0)
                .strong()
                .color(ui.visuals().text_color()),
        );
    });
    ui.add_space(4.0);

    // ── Connection rows ──────────────────────────────────────────────────
    let row_h = 40.0;
    let available_w = ui.available_width();

    for conn in recent {
        let row_rect = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(available_w, row_h),
        );
        let row_resp = ui.allocate_rect(row_rect, egui::Sense::click());

        if row_resp.clicked() {
            *connect_clicked = Some(conn.id.clone());
        }

        if ui.is_rect_visible(row_rect) {
            let painter = ui.painter_at(row_rect);

            let bg = if row_resp.hovered() {
                ui.visuals().widgets.hovered.bg_fill
            } else {
                ui.visuals().extreme_bg_color
            };
            painter.rect_filled(row_rect, egui::CornerRadius::same(4), bg);

            // Type icon
            let icon = conn.conn_type.icon();
            let icon_g = ui.fonts_mut(|f| {
                f.layout(
                    icon.to_string(),
                    egui::FontId::proportional(16.0),
                    ui.visuals().text_color(),
                    f32::INFINITY,
                )
            });
            painter.galley(
                egui::pos2(
                    row_rect.left() + 8.0,
                    row_rect.center().y - icon_g.size().y / 2.0,
                ),
                icon_g,
                ui.visuals().text_color(),
            );

            // Name
            let text_left = row_rect.left() + 34.0;
            let name_w = row_rect.right() - text_left - 8.0;
            let name_g = ui.fonts_mut(|f| {
                f.layout(
                    conn.name.clone(),
                    egui::FontId::proportional(13.0),
                    ui.visuals().text_color(),
                    name_w,
                )
            });
            painter.galley(
                egui::pos2(text_left, row_rect.top() + 4.0),
                name_g,
                ui.visuals().text_color(),
            );

            // Subtitle
            let det_g = ui.fonts_mut(|f| {
                f.layout(
                    crate::ui::page::home::conn_subtitle(conn),
                    egui::FontId::proportional(10.0),
                    ui.visuals().weak_text_color(),
                    name_w,
                )
            });
            painter.galley(
                egui::pos2(text_left, row_rect.top() + 22.0),
                det_g,
                ui.visuals().weak_text_color(),
            );
        }

        ui.add_space(row_h);
    }

    // ── More button ──────────────────────────────────────────────────────
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let more_label = format!("{}  →", rust_i18n::t!("view_all"));
        if ui
            .button(
                egui::RichText::new(&more_label)
                    .size(12.0)
                    .color(crate::ui::widget::style::ACCENT),
            )
            .clicked()
        {
            *more_clicked = true;
        }
    });
}
