use crate::session::WorkspaceSession;
use crate::ui::widget::sidebar::Sidebar;
use crate::ui::widget::sidebar::SidebarPage;
use crate::ui::widget::sidebar::common::{nav_button, sidebar_brand_row, sidebar_sessions_panel, SidebarSessionAction};
use crate::ui::widget::style;

pub struct HomeSidebarResult {
    pub nav: HomeSidebarAction,
    pub sessions: SidebarSessionAction,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HomeSidebarAction {
    None,
    Home,
    Settings,
}

pub fn paint_home_sidebar(
    ui: &mut egui::Ui,
    sidebar: &mut Sidebar,
    in_overlay: bool,
    on_home: bool,
    on_settings: bool,
    sessions: &[WorkspaceSession],
    active_session_id: Option<&str>,
) -> HomeSidebarResult {
    let show_ham = in_overlay && sidebar.show_panel_hamburger(SidebarPage::Home);
    sidebar_brand_row(ui, sidebar, SidebarPage::Home, show_ham);
    ui.add_space(8.0);

    let mut nav_action = HomeSidebarAction::None;

    ui.add_space(2.0);
    if nav_button(ui, "\u{2302}", &rust_i18n::t!("sidebar_home"), on_home).clicked() {
        nav_action = HomeSidebarAction::Home;
    }
    if nav_button(ui, "\u{2699}", &rust_i18n::t!("settings"), on_settings).clicked() {
        nav_action = HomeSidebarAction::Settings;
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    let sessions_action = sidebar_sessions_panel(ui, sessions, active_session_id);

    HomeSidebarResult {
        nav: nav_action,
        sessions: sessions_action,
    }
}
