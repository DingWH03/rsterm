use crate::session::WorkspaceSession;
use crate::ui::sidebar::Sidebar;
use crate::ui::sidebar::SidebarPage;
use crate::ui::style;

/// Reserved width for action buttons on the right of each row.
const SESSION_ACTIONS_WIDTH: f32 = 52.0;
const SESSION_ROW_H: f32 = 28.0;
/// New window (duplicate session).
const ICON_NEW_WINDOW: &str = "\u{29C9}";

pub struct SidebarSessionAction {
    pub select_session: Option<String>,
    pub close_session: Option<String>,
    pub new_window_session: Option<String>,
}

pub fn sidebar_brand_row(
    ui: &mut egui::Ui,
    sidebar: &mut Sidebar,
    page: SidebarPage,
    show_hamburger: bool,
) {
    ui.horizontal(|ui| {
        if show_hamburger && sidebar.hamburger(ui).clicked() {
            sidebar.hamburger_click(page);
        }
        ui.label(
            egui::RichText::new("rsTerminal")
                .size(17.0)
                .strong()
                .color(ui.visuals().text_color()),
        );
    });
}

pub fn sidebar_sessions_panel(
    ui: &mut egui::Ui,
    sessions: &[WorkspaceSession],
    active_id: Option<&str>,
) -> SidebarSessionAction {
    let mut action = SidebarSessionAction {
        select_session: None,
        close_session: None,
        new_window_session: None,
    };

    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(rust_i18n::t!("sidebar_sessions"))
            .size(11.0)
            .color(ui.visuals().weak_text_color())
            .strong(),
    );
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .id_salt("sidebar_sessions_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            if sessions.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(rust_i18n::t!("sidebar_no_sessions"))
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                );
                return;
            }

            for session in sessions {
                paint_session_row(ui, session, active_id, &mut action);
            }
            // Marquee titles need continuous repaints.
            if sessions.iter().any(|s| {
                let t = format!("{} {}", s.icon(), s.tab_label());
                t.chars().count() > 18
            }) {
                ui.ctx().request_repaint();
            }
        });

    action
}

fn paint_session_row(
    ui: &mut egui::Ui,
    session: &WorkspaceSession,
    active_id: Option<&str>,
    action: &mut SidebarSessionAction,
) {
    let active = active_id == Some(session.id());
    let full_text = format!("{} {}", session.icon(), session.tab_label());
    let show_new = session.sidebar_has_new_window();
    let actions_w = if show_new {
        SESSION_ACTIONS_WIDTH
    } else {
        SESSION_ACTIONS_WIDTH * 0.5
    };

    ui.horizontal(|ui| {
        let row_w = ui.available_width();
        let label_w = (row_w - actions_w).max(48.0);

        let label_resp = paint_session_label(ui, &full_text, active, label_w);

        if label_resp.clicked() {
            action.select_session = Some(session.id().to_string());
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.set_width(actions_w);
            let weak_color = ui.visuals().weak_text_color();
            let close_btn = egui::Button::new(
                egui::RichText::new("\u{2715}").size(11.0).color(weak_color),
            )
            .frame(false)
            .corner_radius(style::CORNER_RADIUS_XS);
            if ui.add(close_btn)
                .on_hover_text(rust_i18n::t!("close"))
                .clicked()
            {
                action.close_session = Some(session.id().to_string());
            }
            if show_new {
                let new_btn = egui::Button::new(
                    egui::RichText::new(ICON_NEW_WINDOW).size(13.0).color(weak_color),
                )
                .frame(false)
                .corner_radius(style::CORNER_RADIUS_XS);
                if ui.add(new_btn)
                    .on_hover_text(rust_i18n::t!("new_window"))
                    .clicked()
                {
                    action.new_window_session = Some(session.id().to_string());
                }
            }
        });
    });
    ui.add_space(2.0);
}

/// Fixed-width label: ellipsis if slightly long; marquee if much longer than the slot.
fn paint_session_label(
    ui: &mut egui::Ui,
    text: &str,
    active: bool,
    width: f32,
) -> egui::Response {
    let font_id = egui::FontId::proportional(13.0);
    let text_color = if active {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().text_color()
    };
    let sel_fill = ui.visuals().selection.bg_fill.gamma_multiply(0.4);
    let hover_fill = ui.visuals().widgets.hovered.bg_fill;
    let corner = style::CORNER_RADIUS_XS;

    let galley = ui.fonts_mut(|f| {
        f.layout(
            text.to_owned(),
            font_id.clone(),
            text_color,
            f32::INFINITY,
        )
    });
    let text_w = galley.size().x;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, SESSION_ROW_H), egui::Sense::click());

    if active {
        ui.painter().rect_filled(rect, corner, sel_fill);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, corner, hover_fill);
    }

    let clip = rect.shrink2(egui::vec2(4.0, 0.0));
    let painter = ui.painter().with_clip_rect(clip);

    if text_w <= clip.width() {
        let pos = egui::pos2(
            clip.left(),
            clip.center().y - galley.size().y * 0.5,
        );
        painter.galley(pos, galley, text_color);
        return response;
    }

    // Slightly over budget: static ellipsis (no animation cost).
    let ellipsis_w = ui
        .fonts_mut(|f| {
            f.layout(
                "…".to_owned(),
                font_id.clone(),
                text_color,
                f32::INFINITY,
            )
            .size()
            .x
        });
    let budget = (clip.width() - ellipsis_w).max(8.0);
    if text_w <= clip.width() * 1.15 {
        let fitted = truncate_galley_to_width(ui, text, &font_id, text_color, budget);
        let pos = egui::pos2(
            clip.left(),
            clip.center().y - fitted.size().y * 0.5,
        );
        painter.galley(pos, fitted, text_color);
        let dots = egui::pos2(
            clip.left() + budget,
            clip.center().y - galley.size().y * 0.5,
        );
        painter.galley(
            dots,
            ui.fonts_mut(|f| {
                f.layout("…".into(), font_id, text_color, f32::INFINITY)
            }),
            text_color,
        );
        return response;
    }

    // Long labels: loop scroll (unique id per session for stable state).
    let gap = 28.0;
    let cycle = text_w + gap;
    let t = ui.input(|i| i.time) as f32;
    let offset = (t * 36.0) % cycle;
    ui.ctx().request_repaint();

    let y = clip.center().y - galley.size().y * 0.5;
    painter.galley(egui::pos2(clip.left() - offset, y), galley.clone(), text_color);
    painter.galley(
        egui::pos2(clip.left() - offset + cycle, y),
        galley,
        text_color,
    );

    response
}

fn truncate_galley_to_width(
    ui: &egui::Ui,
    text: &str,
    font_id: &egui::FontId,
    color: egui::Color32,
    max_w: f32,
) -> std::sync::Arc<egui::Galley> {
    let chars: Vec<char> = text.chars().collect();
    let mut end = chars.len();
    while end > 0 {
        let s: String = chars[..end].iter().collect();
        let g = ui.fonts_mut(|f| f.layout(s, font_id.clone(), color, f32::INFINITY));
        if g.size().x <= max_w {
            return g;
        }
        end -= 1;
    }
    ui.fonts_mut(|f| f.layout(String::new(), font_id.clone(), color, f32::INFINITY))
}
