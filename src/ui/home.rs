use crate::storage::types::SavedConnection;

pub fn home_screen(
    ui: &mut egui::Ui,
    connections: &[SavedConnection],
    local_clicked: &mut bool,
    fab_clicked: &mut bool,
    connect_clicked: &mut Option<String>,
    delete_clicked: &mut Option<String>,
    settings_clicked: &mut bool,
) {
    // Local Terminal — always present, distinct card
    let local_rect = render_local_terminal_card(ui);
    if local_rect.clicked() {
        *local_clicked = true;
    }
    ui.add_space(8.0);

    // Saved Connections section
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
                    let card = render_connection_card(ui, conn);

                    if card.clicked() {
                        *connect_clicked = Some(conn.id.clone());
                    }

                    card.context_menu(|ui| {
                        if ui.button("Connect").clicked() {
                            *connect_clicked = Some(conn.id.clone());
                            ui.close_menu();
                        }
                        if ui.button("Delete").clicked() {
                            to_delete = Some(i);
                            ui.close_menu();
                        }
                    });

                    ui.add_space(4.0);
                }

                if let Some(i) = to_delete {
                    *delete_clicked = Some(connections[i].id.clone());
                }
            });
    }

    // Floating action button
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
    let galley = ui.fonts(|f| {
        f.layout("+".to_string(), egui::FontId::proportional(26.0), egui::Color32::WHITE, f32::INFINITY)
    });
    painter.galley(
        egui::pos2(fab_rect.center().x - galley.rect.width() / 2.0, fab_rect.center().y - galley.rect.height() / 2.0),
        galley,
        egui::Color32::WHITE,
    );
}

fn render_local_terminal_card(ui: &mut egui::Ui) -> egui::Response {
    let desired = egui::vec2(ui.available_width(), 72.0);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let corner = egui::CornerRadius::same(8);

        painter.rect_filled(rect, corner, egui::Color32::from_rgb(20, 40, 20));
        painter.rect_stroke(rect, corner, egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 167, 69)), egui::StrokeKind::Inside);

        let icon = ui.fonts(|f| {
            f.layout("\u{1F4BB}".to_string(), egui::FontId::proportional(22.0), egui::Color32::WHITE, f32::INFINITY)
        });
        painter.galley(egui::pos2(rect.left() + 16.0, rect.center().y - icon.rect.height() / 2.0), icon, egui::Color32::WHITE);

        let name = ui.fonts(|f| {
            f.layout("Local Terminal".to_string(), egui::FontId::proportional(16.0), egui::Color32::WHITE, f32::INFINITY)
        });
        painter.galley(egui::pos2(rect.left() + 56.0, rect.top() + 14.0), name, egui::Color32::WHITE);

        let sub = ui.fonts(|f| {
            f.layout("Open a local shell session".to_string(), egui::FontId::proportional(13.0), egui::Color32::GRAY, f32::INFINITY)
        });
        painter.galley(egui::pos2(rect.left() + 56.0, rect.top() + 40.0), sub, egui::Color32::GRAY);
    }

    resp
}

fn render_connection_card(ui: &mut egui::Ui, conn: &SavedConnection) -> egui::Response {
    let desired = egui::vec2(ui.available_width(), 72.0);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let corner = egui::CornerRadius::same(8);

        painter.rect_filled(rect, corner, ui.style().visuals.extreme_bg_color);
        painter.rect_stroke(rect, corner, egui::Stroke::new(1.0, ui.style().visuals.widgets.inactive.bg_fill), egui::StrokeKind::Inside);

        let icon = ui.fonts(|f| {
            f.layout(conn.conn_type.icon().to_string(), egui::FontId::proportional(22.0), egui::Color32::WHITE, f32::INFINITY)
        });
        painter.galley(egui::pos2(rect.left() + 16.0, rect.center().y - icon.rect.height() / 2.0), icon, egui::Color32::WHITE);

        let name = ui.fonts(|f| {
            f.layout(conn.name.clone(), egui::FontId::proportional(15.0), ui.style().visuals.text_color(), f32::INFINITY)
        });
        painter.galley(egui::pos2(rect.left() + 56.0, rect.top() + 14.0), name, ui.style().visuals.text_color());

        let sub = ui.fonts(|f| {
            f.layout(format!("{}", conn.conn_type.label()), egui::FontId::proportional(13.0), egui::Color32::GRAY, f32::INFINITY)
        });
        painter.galley(egui::pos2(rect.left() + 56.0, rect.top() + 40.0), sub, egui::Color32::GRAY);
    }

    resp
}
