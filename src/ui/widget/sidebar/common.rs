use crate::session::WorkspaceSession;
use crate::ui::widget::sidebar::Sidebar;
use crate::ui::widget::sidebar::session_list::paint_session_rows;
use crate::ui::widget::style;

/// Shared sidebar navigation button (icon + label, highlighted when selected).
pub fn nav_button(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> egui::Response {
    let height = 30.0;
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
            ui.visuals().text_color()
        };
        let galley = ui.fonts_mut(|f| {
            f.layout(text, egui::FontId::proportional(13.0), color, f32::INFINITY)
        });
        ui.painter().galley(
            egui::pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
    }

    resp
}

pub struct SidebarSessionAction {
    pub select_session: Option<String>,
    pub close_session: Option<String>,
    pub new_window_session: Option<String>,
}

impl SidebarSessionAction {
    pub fn empty() -> Self {
        Self {
            select_session: None,
            close_session: None,
            new_window_session: None,
        }
    }
}

pub fn sidebar_brand_row(
    ui: &mut egui::Ui,
    sidebar: &mut Sidebar,
    show_hamburger: bool,
) {
    ui.horizontal(|ui| {
        if show_hamburger && sidebar.hamburger(ui).clicked() {
            sidebar.hamburger_click();
        }
        ui.label(
            egui::RichText::new("rsTerminal")
                .size(17.0)
                .strong()
                .color(ui.visuals().text_color()),
        );
    });
}

/// Thin wrapper (kept for backward compat with dead code; reuses `session_list`).
pub fn sidebar_sessions_panel(
    ui: &mut egui::Ui,
    sessions: &[WorkspaceSession],
    active_id: Option<&str>,
) -> SidebarSessionAction {
    let r = paint_session_rows(ui, sessions, active_id);
    SidebarSessionAction {
        select_session: r.select_session,
        close_session: r.close_session,
        new_window_session: r.new_window_session,
    }
}
