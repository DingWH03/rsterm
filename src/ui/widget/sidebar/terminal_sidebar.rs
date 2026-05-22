use crate::session::WorkspaceSession;
use crate::ui::widget::sidebar::Sidebar;
use crate::ui::widget::sidebar::SidebarPage;
use crate::ui::widget::sidebar::common::{nav_button, sidebar_brand_row, sidebar_sessions_panel};
use crate::ui::widget::style;

pub struct TerminalSidebarAction {
    pub select_session: Option<String>,
    pub close_session: Option<String>,
    pub new_window_session: Option<String>,
    pub go_home: bool,
    /// User tapped Settings in the sidebar (app chooses full page vs side panel).
    pub settings_toggled: bool,
}

pub fn terminal_sidebar(
    ui: &mut egui::Ui,
    sidebar: &mut Sidebar,
    on_settings: bool,
    sessions: &[WorkspaceSession],
    active_id: Option<&str>,
) -> TerminalSidebarAction {
    let mut action = TerminalSidebarAction {
        select_session: None,
        close_session: None,
        new_window_session: None,
        go_home: false,
        settings_toggled: false,
    };

    let show_ham = sidebar.show_panel_hamburger(SidebarPage::Workspace);
    sidebar_brand_row(ui, sidebar, SidebarPage::Workspace, show_ham);
    ui.add_space(8.0);

    if nav_button(ui, "\u{2302}", &rust_i18n::t!("sidebar_home"), false).clicked() {
        action.go_home = true;
    }
    if nav_button(ui, "\u{2699}", &rust_i18n::t!("settings"), on_settings).clicked() {
        action.settings_toggled = true;
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    let sess = sidebar_sessions_panel(ui, sessions, active_id);
    action.select_session = sess.select_session;
    action.close_session = sess.close_session;
    action.new_window_session = sess.new_window_session;

    action
}
