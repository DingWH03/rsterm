use crate::session::WorkspaceSession;
use crate::ui::sidebar::Sidebar;
use crate::ui::sidebar::SidebarPage;
use crate::ui::sidebar_common::{sidebar_brand_row, sidebar_sessions_panel, SidebarSessionAction};

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
    ui.add_space(2.0);
    ui.separator();

    let mut nav_action = HomeSidebarAction::None;
    if ui
        .selectable_label(on_home, egui::RichText::new(format!("\u{2302}  {}", rust_i18n::t!("sidebar_home"))).size(14.0))
        .clicked()
    {
        nav_action = HomeSidebarAction::Home;
    }
    if ui
        .selectable_label(
            on_settings,
            egui::RichText::new(format!("\u{2699}  {}", rust_i18n::t!("settings"))).size(14.0),
        )
        .clicked()
    {
        nav_action = HomeSidebarAction::Settings;
    }

    ui.add_space(4.0);
    ui.separator();

    let sessions_action = sidebar_sessions_panel(ui, sessions, active_session_id);

    HomeSidebarResult {
        nav: nav_action,
        sessions: sessions_action,
    }
}
