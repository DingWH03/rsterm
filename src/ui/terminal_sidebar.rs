use crate::session::WorkspaceSession;
use crate::ui::sidebar::Sidebar;
use crate::ui::sidebar::SidebarPage;
use crate::ui::sidebar_common::{sidebar_brand_row, sidebar_sessions_panel, SidebarSessionAction};
use crate::ui::style;

pub struct TerminalSidebarAction {
    pub select_session: Option<String>,
    pub close_session: Option<String>,
    pub new_window_session: Option<String>,
    pub go_home: bool,
    /// User tapped Settings in the sidebar (app chooses full page vs side panel).
    pub settings_toggled: bool,
}

fn nav_button(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> egui::Response {
    let height = 36.0;
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let bg = if selected {
            ui.visuals().selection.bg_fill.gamma_multiply(0.35)
        } else if resp.hovered() {
            ui.visuals().widgets.hovered.bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        if bg != egui::Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, style::CORNER_RADIUS_XS, bg);
        }

        let text = format!("{icon}  {label}");
        let color = if selected {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().weak_text_color()
        };
        let galley = ui.fonts_mut(|f| {
            f.layout(text, egui::FontId::proportional(14.0), color, f32::INFINITY)
        });
        ui.painter().galley(
            egui::pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
    }

    resp
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
