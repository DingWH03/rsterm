use crate::connection::{ble, local, serial, ssh};
use crate::settings::{AppSettings, save_settings};
use crate::storage;
use crate::storage::types::{ConnectionType, SavedConnection};
use crate::terminal::renderer::TerminalRenderer;
use crate::terminal::Terminal;
use crate::ui::connection_view::{
    drain_connection, ActiveSession, ConnectionViewAction, connection_view,
};
use crate::ui::dialogs::NewConnectionDialog;
use crate::ui::home::home_screen;
use crate::ui::home_sidebar::{paint_home_sidebar, HomeSidebarAction};
use crate::ui::keyboard::VirtualKeyboard;
use crate::ui::settings_page::{settings_page, settings_side_panel};
use crate::ui::sidebar::{Sidebar, SidebarPage, DOCK_WIDTH};
use crate::ui::terminal_sidebar::{terminal_sidebar, TerminalSidebarAction};
use log::info;

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Workspace,
}

pub struct RstermApp {
    settings: AppSettings,
    saved_connections: Vec<SavedConnection>,
    sessions: Vec<ActiveSession>,
    active_session_id: Option<String>,
    terminal_renderer: TerminalRenderer,
    virtual_keyboard: VirtualKeyboard,
    new_conn_dialog: NewConnectionDialog,
    page: Page,
    live_font_size: f32,
    sidebar: Sidebar,
    /// Home central panel: settings instead of connection list.
    home_settings: bool,
    settings_open: bool,
}

impl Default for RstermApp {
    fn default() -> Self {
        let settings = crate::settings::load_settings();
        let live_font_size = settings.font_size();
        let kbd_mode = settings.default_profile().keyboard_mode;
        let saved = storage::load_connections();
        Self {
            settings,
            saved_connections: saved,
            sessions: Vec::new(),
            active_session_id: None,
            terminal_renderer: TerminalRenderer::new(24, 80),
            virtual_keyboard: VirtualKeyboard::new(kbd_mode),
            new_conn_dialog: NewConnectionDialog::default(),
            page: Page::Home,
            live_font_size,
            sidebar: Sidebar::new(),
            home_settings: false,
            settings_open: false,
        }
    }
}

impl RstermApp {
    fn push_session(&mut self, session: ActiveSession) {
        let id = session.id.clone();
        self.active_session_id = Some(id);
        self.sessions.push(session);
        self.page = Page::Workspace;
    }

    fn connect_local(&mut self) {
        let profile = self.settings.default_profile().clone();
        let config = SavedConnection::new_local("Local Terminal", None);
        match local::connect_local(&config, &profile, 24, 80) {
            Ok(handle) => self.open_session(handle, &config, profile.scrollback_lines),
            Err(e) => info!("Local connection failed: {e}"),
        }
    }

    fn connect_to(&mut self, conn_id: &str) {
        let config = match self.saved_connections.iter().find(|c| c.id == conn_id) {
            Some(c) => c.clone(),
            None => return,
        };
        let profile = self.settings.default_profile().clone();
        let result = match config.conn_type {
            ConnectionType::Local => local::connect_local(&config, &profile, 24, 80),
            ConnectionType::Ssh => ssh::connect_ssh(&config, &self.settings.ssh_env_vars, 24, 80),
            ConnectionType::Serial => serial::connect_serial(&config),
            ConnectionType::Ble => ble::connect_ble(&config),
        };
        match result {
            Ok(handle) => self.open_session(handle, &config, profile.scrollback_lines),
            Err(e) => info!("Connection failed: {e}"),
        }
    }

    fn open_session(
        &mut self,
        handle: crate::connection::ConnectionHandle,
        config: &SavedConnection,
        scrollback_lines: usize,
    ) {
        let profile = self.settings.default_profile();
        let mut terminal = Terminal::new(self.terminal_renderer.rows, self.terminal_renderer.cols);
        terminal.set_scrollback_limit(scrollback_lines);
        self.live_font_size = profile.font_size;
        self.virtual_keyboard = VirtualKeyboard::new(profile.keyboard_mode);

        let user_at_host = match config.conn_type {
            ConnectionType::Local => crate::platform::local_user_at_host(),
            ConnectionType::Ssh => {
                let user = config.ssh_user.as_deref().unwrap_or("root");
                let host = config.ssh_host.as_deref().unwrap_or("host");
                crate::platform::ssh_user_at_host(user, host)
            }
            _ => String::new(),
        };

        self.push_session(ActiveSession {
            id: uuid::Uuid::new_v4().to_string(),
            conn_type: config.conn_type.clone(),
            saved_conn_id: Some(config.id.clone()),
            name: config.name.clone(),
            user_at_host,
            handle,
            terminal,
            scroll_offset: 0,
            selection: None,
            selection_pointer: None,
            want_terminal_focus: true,
            terminal_had_focus: false,
            row_galley_cache: Default::default(),
            layout_font_size: self.live_font_size,
            last_pty_rows: self.terminal_renderer.rows as u16,
            last_pty_cols: self.terminal_renderer.cols as u16,
            size_label_dims: (
                self.terminal_renderer.cols,
                self.terminal_renderer.rows,
            ),
            size_label_hide_at: None,
            size_label_active: false,
        });
    }

    fn close_session(&mut self, id: &str) {
        if let Some(pos) = self.sessions.iter().position(|s| s.id == id) {
            self.sessions[pos].handle.close();
            self.sessions.remove(pos);
        }
        if self.active_session_id.as_deref() == Some(id) {
            self.active_session_id = self.sessions.last().map(|s| s.id.clone());
        }
        if self.sessions.is_empty() {
            self.active_session_id = None;
            self.page = Page::Home;
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
            if active == Some(session.id.as_str()) {
                continue;
            }
            let mut noop = ConnectionViewAction::None;
            drain_connection(session, &mut noop);
        }
    }

    fn open_new_window_for_session(&mut self, session_id: &str) {
        let plan = self.sessions.iter().find(|s| s.id == session_id).map(|s| {
            (
                s.conn_type.clone(),
                s.saved_conn_id.clone(),
            )
        });
        let Some((conn_type, saved_conn_id)) = plan else {
            return;
        };
        match conn_type {
            ConnectionType::Local => self.connect_local(),
            ConnectionType::Ssh => {
                if let Some(ref id) = saved_conn_id {
                    self.connect_to(id);
                }
            }
            ConnectionType::Serial | ConnectionType::Ble => {}
        }
    }

    fn apply_session_panel_action(
        &mut self,
        action: crate::ui::sidebar_common::SidebarSessionAction,
        in_overlay: bool,
    ) {
        if let Some(id) = action.select_session {
            self.active_session_id = Some(id);
            self.page = Page::Workspace;
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

    fn handle_home_sidebar_result(
        &mut self,
        result: crate::ui::home_sidebar::HomeSidebarResult,
        in_overlay: bool,
    ) {
        match result.nav {
            HomeSidebarAction::Home => {
                self.home_settings = false;
                if in_overlay {
                    self.sidebar.close_overlay();
                }
            }
            HomeSidebarAction::Settings => {
                self.home_settings = true;
                if in_overlay {
                    self.sidebar.close_overlay();
                }
            }
            HomeSidebarAction::None => {}
        }
        self.apply_session_panel_action(result.sessions, in_overlay);
    }
}

impl eframe::App for RstermApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sidebar.sync_width(ctx.screen_rect().width());

        match self.page {
            Page::Home => {
                let mut local_clicked = false;
                let mut fab_clicked = false;
                let mut connect_clicked = None;
                let mut delete_clicked = None;
                let mut _settings_clicked = false;

                if self.sidebar.docked_visible(SidebarPage::Home) {
                    egui::SidePanel::left("home_sidebar")
                        .min_width(DOCK_WIDTH)
                        .max_width(280.0)
                        .resizable(false)
                        .show(ctx, |ui| {
                            let r = paint_home_sidebar(
                                ui,
                                &mut self.sidebar,
                                false,
                                !self.home_settings,
                                self.home_settings,
                                &self.sessions,
                                self.active_session_id.as_deref(),
                            );
                            self.handle_home_sidebar_result(r, false);
                        });
                }

                if self.sidebar.overlay_visible() {
                    if Sidebar::overlay_backdrop_clicked(ctx, egui::Id::new("home_overlay_backdrop")) {
                        self.sidebar.close_overlay();
                    }
                    Sidebar::show_overlay(ctx, "home_sidebar_overlay", |ui| {
                        let r = paint_home_sidebar(
                            ui,
                            &mut self.sidebar,
                            true,
                            !self.home_settings,
                            self.home_settings,
                            &self.sessions,
                            self.active_session_id.as_deref(),
                        );
                        self.handle_home_sidebar_result(r, true);
                    });
                }

                egui::CentralPanel::default().show(ctx, |ui| {
                    if self.sidebar.show_content_hamburger(SidebarPage::Home) {
                        ui.horizontal(|ui| {
                            if self.sidebar.hamburger(ui).clicked() {
                                self.sidebar.hamburger_click(SidebarPage::Home);
                            }
                            ui.label(egui::RichText::new("rsTerm").weak().size(13.0));
                        });
                        ui.separator();
                    }

                    if self.home_settings {
                        if settings_page(ui, &mut self.settings) {
                            self.home_settings = false;
                            save_settings(&self.settings);
                            self.live_font_size = self.settings.font_size();
                        }
                    } else {
                        home_screen(
                            ui,
                            &self.saved_connections,
                            &mut local_clicked,
                            &mut fab_clicked,
                            &mut connect_clicked,
                            &mut delete_clicked,
                            &mut _settings_clicked,
                        );
                    }
                });

                if local_clicked {
                    self.connect_local();
                }
                if fab_clicked {
                    self.new_conn_dialog.open = true;
                }
                if let Some(ref id) = connect_clicked {
                    self.connect_to(id);
                }
                if let Some(ref id) = delete_clicked {
                    self.saved_connections.retain(|c| c.id != *id);
                    storage::save_connections(&self.saved_connections);
                }
                if let Some(new_conn) = self.new_conn_dialog.show(ctx) {
                    self.saved_connections.push(new_conn);
                    storage::save_connections(&self.saved_connections);
                }
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }

            Page::Workspace => {
                self.drain_inactive_sessions();

                let mut sidebar_action = TerminalSidebarAction {
                    select_session: None,
                    close_session: None,
                    new_window_session: None,
                    go_home: false,
                    settings_open: self.settings_open,
                };

                if self.sidebar.docked_visible(SidebarPage::Workspace) {
                    egui::SidePanel::left("workspace_sidebar")
                        .min_width(DOCK_WIDTH)
                        .max_width(300.0)
                        .resizable(true)
                        .show(ctx, |ui| {
                            sidebar_action = terminal_sidebar(
                                ui,
                                &mut self.sidebar,
                                &mut self.settings_open,
                                &self.sessions,
                                self.active_session_id.as_deref(),
                            );
                        });
                }

                if self.sidebar.overlay_visible() {
                    if Sidebar::overlay_backdrop_clicked(ctx, egui::Id::new("workspace_overlay_backdrop"))
                    {
                        self.sidebar.close_overlay();
                    }
                    Sidebar::show_overlay(ctx, "workspace_sidebar_overlay", |ui| {
                        sidebar_action = terminal_sidebar(
                            ui,
                            &mut self.sidebar,
                            &mut self.settings_open,
                            &self.sessions,
                            self.active_session_id.as_deref(),
                        );
                    });
                }

                if self.settings_open {
                    let mut close_settings = false;
                    egui::SidePanel::right("workspace_settings_panel")
                        .min_width(300.0)
                        .max_width(420.0)
                        .resizable(true)
                        .show(ctx, |ui| {
                            close_settings = settings_side_panel(ui, &mut self.settings);
                        });
                    if close_settings {
                        self.settings_open = false;
                    }
                }

                let mut view_action = ConnectionViewAction::None;
                if let Some(idx) = self
                    .active_session_id
                    .as_ref()
                    .and_then(|id| self.sessions.iter().position(|s| &s.id == id))
                {
                    self.sessions[idx].handle.repaint.set_context(ctx.clone());
                }

                egui::CentralPanel::default().show(ctx, |ui| {
                    let active_idx = self
                        .active_session_id
                        .as_ref()
                        .and_then(|id| self.sessions.iter().position(|s| &s.id == id));
                    if let Some(idx) = active_idx {
                        let theme = self.settings.theme();
                        view_action = connection_view(
                            ui,
                            Some(&mut self.sessions[idx]),
                            &mut self.terminal_renderer,
                            &mut self.virtual_keyboard,
                            theme,
                            &mut self.live_font_size,
                            &mut self.sidebar,
                            &mut self.settings_open,
                        );
                    } else {
                        ui.vertical_centered(|ui| {
                            ui.add_space(40.0);
                            ui.label("No active terminal");
                            ui.add_space(8.0);
                            if ui.button("Open Local Terminal").clicked() {
                                self.connect_local();
                            }
                        });
                    }
                });

                if sidebar_action.go_home {
                    self.save_profile_tweaks();
                    self.page = Page::Home;
                    self.sidebar.close_overlay();
                }
                if sidebar_action.settings_open || self.settings_open {
                    save_settings(&self.settings);
                    self.live_font_size = self.settings.font_size();
                }

                self.apply_session_panel_action(
                    crate::ui::sidebar_common::SidebarSessionAction {
                        select_session: sidebar_action.select_session,
                        close_session: sidebar_action.close_session,
                        new_window_session: sidebar_action.new_window_session,
                    },
                    self.sidebar.overlay_visible(),
                );
                if matches!(view_action, ConnectionViewAction::CloseSession) {
                    if let Some(id) = self.active_session_id.clone() {
                        self.close_session(&id);
                    }
                }

                ctx.request_repaint_after(std::time::Duration::from_millis(400));
            }
        }
    }
}
