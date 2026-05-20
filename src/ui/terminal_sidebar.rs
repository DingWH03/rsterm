use crate::session::WorkspaceSession;
use crate::ui::sidebar::Sidebar;
use crate::ui::sidebar::SidebarPage;
use crate::ui::sidebar_common::{sidebar_brand_row, sidebar_sessions_panel, SidebarSessionAction};

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
    ui.add_space(2.0);
    ui.separator();

    if ui
        .selectable_label(false, egui::RichText::new(format!("\u{2302}  {}", rust_i18n::t!("sidebar_home"))).size(14.0))
        .clicked()
    {
        action.go_home = true;
    }
    if ui
        .selectable_label(
            on_settings,
            egui::RichText::new(format!("\u{2699}  {}", rust_i18n::t!("settings"))).size(14.0),
        )
        .clicked()
    {
        action.settings_toggled = true;
    }

    ui.add_space(4.0);
    ui.separator();

    let sess = sidebar_sessions_panel(ui, sessions, active_id);
    action.select_session = sess.select_session;
    action.close_session = sess.close_session;
    action.new_window_session = sess.new_window_session;

    action
}
