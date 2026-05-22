use crate::session::WorkspaceSession;
use crate::ui::widget::style;

pub struct SessionRowAction {
    pub select_session: Option<String>,
    pub close_session: Option<String>,
    pub new_window_session: Option<String>,
}

/// Paint all session rows inside a ScrollArea caller (no nested scroll).
pub fn paint_session_rows(
    ui: &mut egui::Ui,
    sessions: &[WorkspaceSession],
    active_id: Option<&str>,
) -> SessionRowAction {
    let mut action = SessionRowAction {
        select_session: None,
        close_session: None,
        new_window_session: None,
    };

    if sessions.is_empty() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(rust_i18n::t!("sidebar_no_sessions"))
                .size(12.0)
                .color(ui.visuals().weak_text_color()),
        );
        return action;
    }

    for session in sessions {
        paint_session_row(ui, session, active_id, &mut action);
    }

    // Request repaint for marquee animation of long labels
    if sessions.iter().any(|s| {
        let t = format!("{} {}", s.icon(), s.tab_label());
        t.chars().count() > 28
    }) {
        ui.ctx().request_repaint();
    }

    action
}

const SESSION_ACTIONS_WIDTH: f32 = 52.0;
const SESSION_ROW_H: f32 = 28.0;
const ICON_NEW_WINDOW: &str = "\u{29C9}";

fn paint_session_row(
    ui: &mut egui::Ui,
    session: &WorkspaceSession,
    active_id: Option<&str>,
    action: &mut SessionRowAction,
) {
    let active = active_id == Some(session.id());
    let full_text = format!("{} {}", session.icon(), session.tab_label());
    let display_text: String = full_text.chars().take(28).collect();
    let display_text = if full_text.chars().count() > 28 {
        format!("{}…", display_text)
    } else {
        display_text
    };
    let show_new = session.sidebar_has_new_window();

    let row_w = ui.available_width();
    let actions_w = if show_new {
        SESSION_ACTIONS_WIDTH
    } else {
        SESSION_ACTIONS_WIDTH * 0.5
    };
    let label_w = (row_w - actions_w - 4.0).max(48.0);
    let row_h = SESSION_ROW_H;

    let (rect, resp) = ui.allocate_exact_size(egui::vec2(row_w, row_h), egui::Sense::click());

    if resp.clicked() {
        action.select_session = Some(session.id().to_string());
    }

    if ui.is_rect_visible(rect) {
        // Label
        let label_rect = egui::Rect::from_min_size(rect.min, egui::vec2(label_w, row_h));
        let label_resp = ui.interact(label_rect, ui.id().with(("sess_label", session.id())), egui::Sense::click());
        if label_resp.clicked() {
            action.select_session = Some(session.id().to_string());
        }
        paint_label_in_rect(ui, label_rect, &display_text, active);

        // Action buttons on the right
        let weak_color = ui.visuals().weak_text_color();
        let painter = ui.painter();

        // Close button
        let close_rect = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 14.0, rect.center().y),
            egui::vec2(22.0, 22.0),
        );
        let close_id = ui.id().with(("sess_close", session.id()));
        let close_resp = ui.interact(close_rect, close_id, egui::Sense::click());
        if close_resp.clicked() {
            action.close_session = Some(session.id().to_string());
        }
        let close_g = ui.fonts_mut(|f| {
            f.layout("\u{2715}".into(), egui::FontId::proportional(11.0), weak_color, f32::INFINITY)
        });
        painter.galley(
            egui::pos2(close_rect.center().x - close_g.size().x / 2.0, close_rect.center().y - close_g.size().y / 2.0),
            close_g,
            weak_color,
        );

        // New-window button
        if show_new {
            let new_rect = egui::Rect::from_center_size(
                egui::pos2(close_rect.left() - 14.0, rect.center().y),
                egui::vec2(22.0, 22.0),
            );
            let new_id = ui.id().with(("sess_new", session.id()));
            let new_resp = ui.interact(new_rect, new_id, egui::Sense::click());
            if new_resp.clicked() {
                action.new_window_session = Some(session.id().to_string());
            }
            let new_g = ui.fonts_mut(|f| {
                f.layout(ICON_NEW_WINDOW.into(), egui::FontId::proportional(13.0), weak_color, f32::INFINITY)
            });
            painter.galley(
                egui::pos2(new_rect.center().x - new_g.size().x / 2.0, new_rect.center().y - new_g.size().y / 2.0),
                new_g,
                weak_color,
            );
        }
    }

    ui.add_space(2.0);
}

fn paint_label_in_rect(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    text: &str,
    active: bool,
) {
    let font_id = egui::FontId::proportional(13.0);
    let text_color = if active {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().text_color()
    };
    let sel_fill = ui.visuals().selection.bg_fill.gamma_multiply(0.4);
    let hover_fill = ui.visuals().widgets.hovered.bg_fill;
    let corner = style::CORNER_RADIUS_XS;

    let painter = ui.painter_at(rect);

    let bg = if active {
        sel_fill
    } else {
        // simple hover check
        let hovered = rect.contains(ui.input(|i| i.pointer.interact_pos().unwrap_or(egui::Pos2::ZERO)));
        if hovered { hover_fill } else { egui::Color32::TRANSPARENT }
    };
    if bg != egui::Color32::TRANSPARENT {
        painter.rect_filled(rect, corner, bg);
    }

    let clip = rect.shrink2(egui::vec2(4.0, 0.0));
    let full = ui.fonts_mut(|f| {
        f.layout(text.to_owned(), font_id.clone(), text_color, f32::INFINITY)
    });

    if full.size().x <= clip.width() {
        painter.galley(
            egui::pos2(clip.left(), clip.center().y - full.size().y * 0.5),
            full,
            text_color,
        );
    } else {
        // Truncate with ellipsis
        let ellipsis = "…";
        let ellipsis_w = ui.fonts_mut(|f| {
            f.layout(ellipsis.to_owned(), font_id.clone(), text_color, f32::INFINITY).size().x
        });
        let budget = (clip.width() - ellipsis_w).max(8.0);
        let chars: Vec<char> = text.chars().collect();
        let mut end = chars.len();
        while end > 0 {
            let s: String = chars[..end].iter().collect();
            let g = ui.fonts_mut(|f| f.layout(s, font_id.clone(), text_color, f32::INFINITY));
            if g.size().x <= budget {
                painter.galley(
                    egui::pos2(clip.left(), clip.center().y - g.size().y * 0.5),
                    g,
                    text_color,
                );
                painter.galley(
                    egui::pos2(clip.left() + budget, clip.center().y - full.size().y * 0.5),
                    ui.fonts_mut(|f| f.layout(ellipsis.into(), font_id, text_color, f32::INFINITY)),
                    text_color,
                );
                break;
            }
            end -= 1;
        }
    }
}
