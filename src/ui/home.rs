use crate::platform;
use crate::storage::types::{ConnectionType, SavedConnection};

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
            if ui.button("Connect").clicked() {
                *local_clicked = true;
                ui.close();
            }
            if ui.button("File Manager").clicked() {
                card_menu.local_fm = true;
                ui.close();
            }
        });
        ui.add_space(8.0);
    }

    if connections.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("No saved connections yet")
                    .size(14.0)
                    .color(egui::Color32::GRAY),
            );
        });
    } else {
        ui.label(egui::RichText::new("Saved Connections").weak());
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
                        if ui.button("Connect").clicked() {
                            *connect_clicked = Some(conn.id.clone());
                            ui.close();
                        }
                        if ui.button("Edit").clicked() {
                            *edit_clicked = Some(conn.id.clone());
                            ui.close();
                        }
                        if conn.conn_type == ConnectionType::Ssh
                            && ui.button("Remote Files").clicked()
                        {
                            *sftp_clicked = Some(conn.id.clone());
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            to_delete = Some(i);
                            ui.close();
                        }
                    });

                    ui.add_space(6.0);
                }

                if let Some(i) = to_delete {
                    *delete_clicked = Some(connections[i].id.clone());
                }
            });
    }

    let fab_size = 52.0;
    let fab_pos = egui::pos2(
        ui.max_rect().right() - fab_size - 16.0,
        ui.max_rect().bottom() - fab_size - 16.0,
    );
    let fab_rect = egui::Rect::from_min_size(fab_pos, egui::vec2(fab_size, fab_size));
    let fab_resp = ui.allocate_rect(fab_rect, egui::Sense::click());
    if fab_resp.clicked() {
        *fab_clicked = true;
    }
    let painter = ui.painter_at(fab_rect);
    painter.circle_filled(fab_rect.center(), fab_size / 2.0, egui::Color32::from_rgb(33, 150, 243));
    let galley = ui.fonts_mut(|f| {
        f.layout(
            "+".to_string(),
            egui::FontId::proportional(26.0),
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

const CARD_H: f32 = 72.0;
const ICON_SLOT: f32 = 40.0;
const ICON_FONT: f32 = 22.0;
const TOOLBAR_GAP: f32 = 2.0;
const TOOLBAR_MARGIN: f32 = 12.0;

const FILE_ICON: &str = "📁";
const EDIT_ICON: &str = "✎";

struct CardToolbar {
    file: Option<egui::Rect>,
    edit: Option<egui::Rect>,
}

impl CardToolbar {
    fn layout(card: egui::Rect, show_file: bool, show_edit: bool) -> Self {
        let cy = card.center().y;
        let mut x = card.right() - TOOLBAR_MARGIN;

        let edit = if show_edit {
            x -= ICON_SLOT;
            let r = egui::Rect::from_center_size(
                egui::pos2(x + ICON_SLOT / 2.0, cy),
                egui::vec2(ICON_SLOT, ICON_SLOT),
            );
            x -= TOOLBAR_GAP;
            Some(r)
        } else {
            None
        };

        let file = if show_file {
            x -= ICON_SLOT;
            Some(egui::Rect::from_center_size(
                egui::pos2(x + ICON_SLOT / 2.0, cy),
                egui::vec2(ICON_SLOT, ICON_SLOT),
            ))
        } else {
            None
        };

        Self { file, edit }
    }

    fn reserved_width(show_file: bool, show_edit: bool) -> f32 {
        let mut w = TOOLBAR_MARGIN;
        if show_edit {
            w += ICON_SLOT;
        }
        if show_file {
            if show_edit {
                w += TOOLBAR_GAP;
            }
            w += ICON_SLOT;
        }
        w
    }
}

fn icon_color(resp: &egui::Response) -> egui::Color32 {
    if resp.hovered() {
        egui::Color32::from_rgb(33, 150, 243)
    } else {
        egui::Color32::GRAY
    }
}

fn paint_icon(ui: &egui::Ui, rect: egui::Rect, icon: &str, color: egui::Color32) {
    let galley = ui.fonts_mut(|f| {
        f.layout(
            icon.to_string(),
            egui::FontId::proportional(ICON_FONT),
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
        paint_icon(ui, rect, EDIT_ICON, icon_color(&resp));
    }
    resp
}

fn paint_file_icon(ui: &mut egui::Ui, rect: egui::Rect, id: egui::Id) -> egui::Response {
    let resp = ui.interact(rect, id, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        paint_icon(ui, rect, FILE_ICON, icon_color(&resp));
    }
    resp
}

fn paint_card_chrome(
    ui: &egui::Ui,
    rect: egui::Rect,
    fill: egui::Color32,
    stroke: egui::Stroke,
) {
    let corner = egui::CornerRadius::same(8);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, corner, fill);
    painter.rect_stroke(rect, corner, stroke, egui::StrokeKind::Inside);
}

fn render_local_terminal_card(
    ui: &mut egui::Ui,
    selected: bool,
    card_menu: &mut HomeCardMenuAction,
) -> (egui::Response, egui::Response) {
    let desired = egui::vec2(ui.available_width(), CARD_H);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());

    let mut file_resp =
        ui.interact(egui::Rect::NOTHING, ui.id().with("local_file"), egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let fill = if selected {
            egui::Color32::from_rgb(28, 58, 32)
        } else {
            egui::Color32::from_rgb(20, 40, 20)
        };
        let stroke_color = if selected {
            egui::Color32::from_rgb(72, 200, 90)
        } else {
            egui::Color32::from_rgb(40, 167, 69)
        };
        paint_card_chrome(
            ui,
            rect,
            fill,
            egui::Stroke::new(1.5, stroke_color),
        );

        let toolbar = CardToolbar::layout(rect, true, false);
        let _text_right = rect.right() - CardToolbar::reserved_width(true, false);

        let icon = ui.fonts_mut(|f| {
            f.layout(
                "\u{1F4BB}".to_string(),
                egui::FontId::proportional(22.0),
                egui::Color32::WHITE,
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(rect.left() + 16.0, rect.center().y - icon.rect.height() / 2.0),
            icon,
            egui::Color32::WHITE,
        );
        let name = ui.fonts_mut(|f| {
            f.layout(
                "Local Terminal".to_string(),
                egui::FontId::proportional(16.0),
                egui::Color32::WHITE,
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(rect.left() + 56.0, rect.top() + 14.0),
            name,
            egui::Color32::WHITE,
        );
        let sub = ui.fonts_mut(|f| {
            f.layout(
                "Open a local shell session".to_string(),
                egui::FontId::proportional(13.0),
                egui::Color32::GRAY,
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(rect.left() + 56.0, rect.top() + 40.0),
            sub,
            egui::Color32::GRAY,
        );

        if let Some(file_rect) = toolbar.file {
            file_resp = paint_file_icon(ui, file_rect, ui.id().with("local_builtin_file"));
            if file_resp.clicked() {
                card_menu.local_fm = true;
            }
        }
    }

    (resp, file_resp)
}

fn render_connection_card(
    ui: &mut egui::Ui,
    conn: &SavedConnection,
    selected: bool,
    card_menu: &mut HomeCardMenuAction,
    edit_clicked: &mut Option<String>,
) -> (egui::Response, egui::Response, egui::Response) {
    let show_file = matches!(conn.conn_type, ConnectionType::Local | ConnectionType::Ssh);
    let desired = egui::vec2(ui.available_width(), CARD_H);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());

    let noop = ui.interact(
        egui::Rect::NOTHING,
        ui.id().with(("noop", &conn.id)),
        egui::Sense::hover(),
    );
    let mut file_resp = noop.clone();
    let mut pencil_resp = noop;

    if ui.is_rect_visible(rect) {
        let fill = if selected {
            egui::Color32::from_rgb(35, 45, 58)
        } else {
            ui.style().visuals.extreme_bg_color
        };
        let stroke = if selected {
            egui::Stroke::new(1.5, egui::Color32::from_rgb(33, 150, 243))
        } else {
            egui::Stroke::new(1.0, ui.style().visuals.widgets.inactive.bg_fill)
        };
        paint_card_chrome(ui, rect, fill, stroke);

        let toolbar = CardToolbar::layout(rect, show_file, true);
        let _text_right = rect.right() - CardToolbar::reserved_width(show_file, true);

        let icon = ui.fonts_mut(|f| {
            f.layout(
                conn.conn_type.icon().to_string(),
                egui::FontId::proportional(22.0),
                egui::Color32::WHITE,
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(rect.left() + 16.0, rect.center().y - icon.rect.height() / 2.0),
            icon,
            egui::Color32::WHITE,
        );
        let name = ui.fonts_mut(|f| {
            f.layout(
                conn.name.clone(),
                egui::FontId::proportional(15.0),
                ui.style().visuals.text_color(),
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(rect.left() + 56.0, rect.top() + 14.0),
            name,
            ui.style().visuals.text_color(),
        );
        let sub = ui.fonts_mut(|f| {
            f.layout(
                conn.conn_type.label().to_string(),
                egui::FontId::proportional(13.0),
                egui::Color32::GRAY,
                f32::INFINITY,
            )
        });
        ui.painter_at(rect).galley(
            egui::pos2(rect.left() + 56.0, rect.top() + 40.0),
            sub,
            egui::Color32::GRAY,
        );

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
