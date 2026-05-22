pub mod sidebar;

use crate::platform;
use crate::storage::types::{ConnectionType, SavedConnection};
use crate::ui::widget::style;

/// Direct actions from per-card toolbar icons (not context menus).
#[derive(Default)]
pub struct HomeCardMenuAction {
    pub local_fm: bool,
    pub sftp_id: Option<String>,
}

pub fn home_screen(
    ui: &mut egui::Ui,
    connections: &[SavedConnection],
    selected_conn_id: &mut Option<String>,
    card_menu: &mut HomeCardMenuAction,
    local_clicked: &mut bool,
    _local_fm_clicked: &mut bool,
    fab_clicked: &mut bool,
    connect_clicked: &mut Option<String>,
    edit_clicked: &mut Option<String>,
    sftp_clicked: &mut Option<String>,
    delete_clicked: &mut Option<String>,
    settings_clicked: &mut bool,
) {
    let _ = settings_clicked;


    if platform::capabilities().local_terminal {
        let (local_body, local_file) =
            render_local_terminal_card(ui, selected_conn_id.is_none(), card_menu);
        if local_body.clicked() && !local_file.clicked() {
            *selected_conn_id = None;
            *local_clicked = true;
        }
        local_body.context_menu(|ui| {
            if ui.button(rust_i18n::t!("connect")).clicked() {
                *local_clicked = true;
                ui.close();
            }
            if ui.button(rust_i18n::t!("home_file_manager")).clicked() {
                card_menu.local_fm = true;
                ui.close();
            }
        });
        ui.add_space(style::CARD_SPACING);
    }

    // ── Saved connections section ───────────────────────────────────────────
    if connections.is_empty() {
        ui.add_space(32.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("\u{1F4CB}")
                    .size(36.0),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(rust_i18n::t!("home_no_connections"))
                    .size(15.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Tap + to add your first connection")
                    .size(12.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
        ui.add_space(8.0);
    } else {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(rust_i18n::t!("home_saved_connections"))
                    .size(13.0)
                    .color(ui.visuals().weak_text_color())
                    .strong(),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("{}", connections.len()))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
        ui.add_space(6.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut to_delete: Option<usize> = None;

                for (i, conn) in connections.iter().enumerate() {
                    let selected = selected_conn_id.as_deref() == Some(conn.id.as_str());
                    let (card, file_btn, pencil) = render_connection_card(
                        ui,
                        conn,
                        selected,
                        card_menu,
                        edit_clicked,
                    );

                    if card.clicked() && !file_btn.clicked() && !pencil.clicked() {
                        *selected_conn_id = Some(conn.id.clone());
                        *connect_clicked = Some(conn.id.clone());
                    }

                    card.context_menu(|ui| {
                        if ui.button(rust_i18n::t!("connect")).clicked() {
                            *connect_clicked = Some(conn.id.clone());
                            ui.close();
                        }
                        if ui.button(rust_i18n::t!("edit")).clicked() {
                            *edit_clicked = Some(conn.id.clone());
                            ui.close();
                        }
                        if conn.conn_type == ConnectionType::Ssh
                            && ui.button(rust_i18n::t!("home_remote_files")).clicked()
                        {
                            *sftp_clicked = Some(conn.id.clone());
                            ui.close();
                        }
                        if ui.button(rust_i18n::t!("delete")).clicked() {
                            to_delete = Some(i);
                            ui.close();
                        }
                    });

                    ui.add_space(style::CARD_SPACING);
                }

                if let Some(i) = to_delete {
                    *delete_clicked = Some(connections[i].id.clone());
                }
            });
    }

    // ── Floating Action Button ──────────────────────────────────────────────
    paint_fab(ui, fab_clicked);
}

// ─── FAB ──────────────────────────────────────────────────────────────────────

fn paint_fab(ui: &mut egui::Ui, fab_clicked: &mut bool) {
    let fab_size = 56.0;
    let shadow_offset = 2.0;
    let fab_pos = egui::pos2(
        ui.max_rect().right() - fab_size - 20.0,
        ui.max_rect().bottom() - fab_size - 20.0 - shadow_offset,
    );
    let fab_rect = egui::Rect::from_min_size(fab_pos, egui::vec2(fab_size, fab_size));
    let fab_resp = ui.allocate_rect(fab_rect, egui::Sense::click());
    if fab_resp.clicked() {
        *fab_clicked = true;
    }

    if ui.is_rect_visible(fab_rect) {
        let painter = ui.painter_at(fab_rect);

        // Shadow
        let shadow_rect = fab_rect.translate(egui::vec2(0.0, shadow_offset));
        painter.circle_filled(shadow_rect.center(), fab_size / 2.0, egui::Color32::from_black_alpha(60));

        // Main circle
        let bg = if fab_resp.hovered() {
            style::ACCENT.gamma_multiply(1.15)
        } else {
            style::ACCENT
        };
        painter.circle_filled(fab_rect.center(), fab_size / 2.0, bg);

        // Plus icon
        let galley = ui.fonts_mut(|f| {
            f.layout(
                "+".to_string(),
                egui::FontId::proportional(28.0),
                egui::Color32::WHITE,
                f32::INFINITY,
            )
        });
        painter.galley(
            egui::pos2(
                fab_rect.center().x - galley.rect.width() / 2.0,
                fab_rect.center().y - galley.rect.height() / 2.0,
            ),
            galley,
            egui::Color32::WHITE,
        );
    }
}

// ─── Card constants ───────────────────────────────────────────────────────────

const CARD_ICON_FONT: f32 = 22.0;

const FILE_ICON: &str = "\u{1F4C1}";
const EDIT_ICON: &str = "\u{270E}";

// ─── Icon helpers ─────────────────────────────────────────────────────────────

fn icon_color(ui: &egui::Ui, resp: &egui::Response) -> egui::Color32 {
    if resp.hovered() {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().weak_text_color()
    }
}

fn paint_icon(ui: &egui::Ui, rect: egui::Rect, icon: &str, color: egui::Color32) {
    let galley = ui.fonts_mut(|f| {
        f.layout(
            icon.to_string(),
            egui::FontId::proportional(CARD_ICON_FONT),
            color,
            f32::INFINITY,
        )
    });
    ui.painter_at(rect).galley(
        egui::pos2(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        color,
    );
}

fn paint_edit_icon(ui: &mut egui::Ui, rect: egui::Rect, id: egui::Id) -> egui::Response {
    let resp = ui.interact(rect, id, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        paint_icon(ui, rect, EDIT_ICON, icon_color(ui, &resp));
    }
    resp
}

fn paint_file_icon(ui: &mut egui::Ui, rect: egui::Rect, id: egui::Id) -> egui::Response {
    let resp = ui.interact(rect, id, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        paint_icon(ui, rect, FILE_ICON, icon_color(ui, &resp));
    }
    resp
}

// ─── Card chrome ──────────────────────────────────────────────────────────────

fn paint_card_chrome(
    ui: &egui::Ui,
    rect: egui::Rect,
    fill: egui::Color32,
    stroke: egui::Stroke,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, style::CORNER_RADIUS_SM, fill);
    painter.rect_stroke(rect, style::CORNER_RADIUS_SM, stroke, egui::StrokeKind::Inside);
}

// ─── Local terminal card ──────────────────────────────────────────────────────

fn render_local_terminal_card(
    ui: &mut egui::Ui,
    selected: bool,
    card_menu: &mut HomeCardMenuAction,
) -> (egui::Response, egui::Response) {
    // Same style as connection cards — consistency across all cards
    let desired = egui::vec2(ui.available_width(), style::CARD_HEIGHT);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());

    let mut file_resp =
        ui.interact(egui::Rect::NOTHING, ui.id().with("local_file"), egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        paint_card_chrome(
            ui,
            rect,
            card_fill(ui, selected, resp.hovered()),
            card_stroke(ui, selected, resp.hovered()),
        );

        let icon_x = rect.left() + 16.0;
        let icon_y = rect.center().y;

        // Icon — use accent color like other cards
        let icon = ui.fonts_mut(|f| {
            f.layout(
                "\u{1F4BB}".to_string(),
                egui::FontId::proportional(CARD_ICON_FONT),
                style::ACCENT,
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(icon_x, icon_y - icon.rect.height() / 2.0),
            icon,
            style::ACCENT,
        );

        let text_left = rect.left() + 56.0;
        let name_top = rect.top() + 14.0;
        let sub_top = rect.top() + 42.0;

        // Title
        let name_g = ui.fonts_mut(|f| {
            f.layout(
                rust_i18n::t!("home_local_terminal").to_string(),
                egui::FontId::proportional(16.0),
                ui.visuals().text_color(),
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(text_left, name_top),
            name_g,
            ui.visuals().text_color(),
        );

        // Subtitle
        let sub_g = ui.fonts_mut(|f| {
            f.layout(
                "Open a local shell session".to_string(),
                egui::FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(text_left, sub_top),
            sub_g,
            ui.visuals().weak_text_color(),
        );

        // File icon in toolbar area
        let toolbar = style::CardToolbar::layout(rect, true, false);
        if let Some(file_rect) = toolbar.file {
            file_resp = paint_file_icon(ui, file_rect, ui.id().with("local_builtin_file"));
            if file_resp.clicked() {
                card_menu.local_fm = true;
            }
        }
    }

    (resp, file_resp)
}

/// Dynamic card background — works in both light and dark themes.
fn card_fill(ui: &egui::Ui, selected: bool, hovered: bool) -> egui::Color32 {
    if selected {
        ui.visuals().selection.bg_fill.gamma_multiply(0.35)
    } else if hovered {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        ui.visuals().extreme_bg_color
    }
}

/// Dynamic card stroke — works in both light and dark themes.
fn card_stroke(ui: &egui::Ui, selected: bool, hovered: bool) -> egui::Stroke {
    if selected {
        egui::Stroke::new(1.5, ui.visuals().selection.stroke.color)
    } else if hovered {
        egui::Stroke::new(1.0, ui.visuals().widgets.hovered.bg_stroke.color)
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke
    }
}

// ─── Connection card ──────────────────────────────────────────────────────────

fn render_connection_card(
    ui: &mut egui::Ui,
    conn: &SavedConnection,
    selected: bool,
    card_menu: &mut HomeCardMenuAction,
    edit_clicked: &mut Option<String>,
) -> (egui::Response, egui::Response, egui::Response) {
    let show_file = matches!(conn.conn_type, ConnectionType::Local | ConnectionType::Ssh);
    let desired = egui::vec2(ui.available_width(), style::CARD_HEIGHT);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());

    let noop = ui.interact(
        egui::Rect::NOTHING,
        ui.id().with(("noop", &conn.id)),
        egui::Sense::hover(),
    );
    let mut file_resp = noop.clone();
    let mut pencil_resp = noop;

    if ui.is_rect_visible(rect) {
        paint_card_chrome(
            ui,
            rect,
            card_fill(ui, selected, resp.hovered()),
            card_stroke(ui, selected, resp.hovered()),
        );

        let icon_x = rect.left() + 16.0;
        let icon_y = rect.center().y;

        // Connection type icon
        let icon = ui.fonts_mut(|f| {
            f.layout(
                conn.conn_type.icon().to_string(),
                egui::FontId::proportional(CARD_ICON_FONT),
                style::ACCENT,
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(icon_x, icon_y - icon.rect.height() / 2.0),
            icon,
            style::ACCENT,
        );

        let text_left = rect.left() + 56.0;
        let name_top = rect.top() + 14.0;
        let sub_top = rect.top() + 42.0;

        // Name
        let name_g = ui.fonts_mut(|f| {
            f.layout(
                conn.name.clone(),
                egui::FontId::proportional(15.0),
                ui.visuals().text_color(),
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(text_left, name_top),
            name_g,
            ui.visuals().text_color(),
        );

        // Type label
        let sub_g = ui.fonts_mut(|f| {
            f.layout(
                conn.conn_type.label().to_string(),
                egui::FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(text_left, sub_top),
            sub_g,
            ui.visuals().weak_text_color(),
        );

        // Toolbar icons
        let toolbar = style::CardToolbar::layout(rect, show_file, true);

        if let Some(edit_rect) = toolbar.edit {
            pencil_resp = paint_edit_icon(ui, edit_rect, ui.id().with(("edit", &conn.id)));
            if pencil_resp.clicked() {
                *edit_clicked = Some(conn.id.clone());
            }
        }

        if let Some(file_rect) = toolbar.file {
            file_resp = paint_file_icon(ui, file_rect, ui.id().with(("file", &conn.id)));
            if file_resp.clicked() {
                match conn.conn_type {
                    ConnectionType::Local => card_menu.local_fm = true,
                    ConnectionType::Ssh => card_menu.sftp_id = Some(conn.id.clone()),
                    ConnectionType::Serial | ConnectionType::Ble => {}
                }
            }
        }
    }

    (resp, file_resp, pencil_resp)
}
