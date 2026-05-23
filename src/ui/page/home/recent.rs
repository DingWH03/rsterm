use crate::storage::types::SavedConnection;
use crate::ui::widget::sidebar::Sidebar;

pub fn recent_connections_view(
    ui: &mut egui::Ui,
    sidebar: &mut Sidebar,
    connections: &[SavedConnection],
    connect_clicked: &mut Option<String>,
    more_clicked: &mut bool,
) {
    // Collect all connections, sorted by last_connected desc (recently connected first)
    let mut recent: Vec<&SavedConnection> = connections.iter().collect();
    recent.sort_by(|a, b| {
        b.last_connected
            .as_deref()
            .unwrap_or("")
            .cmp(&a.last_connected.as_deref().unwrap_or(""))
            .then_with(|| a.name.cmp(&b.name))
    });
    let show_count = recent.len().min(5);
    let recent = &recent[..show_count];

    if recent.is_empty() {
        // ── Empty state ──────────────────────────────────────────────────
        // Header bar with hamburger (matching terminal layout)
        ui.horizontal(|ui| {
            ui.style_mut().spacing.button_padding = egui::vec2(4.0, 1.0);
            ui.style_mut().spacing.item_spacing.x = 4.0;

            if sidebar.show_content_hamburger()
                && sidebar.hamburger(ui).clicked()
            {
                sidebar.hamburger_click();
            }
            ui.label(
                egui::RichText::new(rust_i18n::t!("recent_connections"))
                    .size(12.0)
                    .strong()
                    .color(ui.visuals().text_color()),
            );
        });
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("\u{1F4CB}").size(36.0));
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(rust_i18n::t!("home_no_connections"))
                    .size(15.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(rust_i18n::t!("open_terminal_hint"))
                    .size(12.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
        return;
    }

    // ── Header bar (matching terminal layout) ────────────────────────────
    ui.horizontal(|ui| {
        ui.style_mut().spacing.button_padding = egui::vec2(4.0, 1.0);
        ui.style_mut().spacing.item_spacing.x = 4.0;

        if sidebar.show_content_hamburger()
            && sidebar.hamburger(ui).clicked()
        {
            sidebar.hamburger_click();
        }
        ui.label(
            egui::RichText::new(rust_i18n::t!("recent_connections"))
                .size(12.0)
                .strong()
                .color(ui.visuals().text_color()),
        );
    });
    ui.add_space(4.0);

    // ── Connection rows ──────────────────────────────────────────────────
    let row_h = 40.0;
    let available_w = ui.available_width();

    for conn in recent {
        let row_rect = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(available_w, row_h),
        );
        let row_resp = ui.allocate_rect(row_rect, egui::Sense::click());

        if row_resp.clicked() {
            *connect_clicked = Some(conn.id.clone());
        }

        if ui.is_rect_visible(row_rect) {
            let painter = ui.painter_at(row_rect);

            let bg = if row_resp.hovered() {
                ui.visuals().widgets.hovered.bg_fill
            } else {
                ui.visuals().extreme_bg_color
            };
            painter.rect_filled(row_rect, egui::CornerRadius::same(4), bg);

            // Type icon
            let icon = conn.conn_type.icon();
            let icon_g = ui.fonts_mut(|f| {
                f.layout(
                    icon.to_string(),
                    egui::FontId::proportional(16.0),
                    ui.visuals().text_color(),
                    f32::INFINITY,
                )
            });
            painter.galley(
                egui::pos2(
                    row_rect.left() + 8.0,
                    row_rect.center().y - icon_g.size().y / 2.0,
                ),
                icon_g,
                ui.visuals().text_color(),
            );

            // Name
            let text_left = row_rect.left() + 34.0;
            let name_w = row_rect.right() - text_left - 8.0;
            let name_g = ui.fonts_mut(|f| {
                f.layout(
                    conn.name.clone(),
                    egui::FontId::proportional(13.0),
                    ui.visuals().text_color(),
                    name_w,
                )
            });
            painter.galley(
                egui::pos2(text_left, row_rect.top() + 4.0),
                name_g,
                ui.visuals().text_color(),
            );

            // Subtitle
            let det_g = ui.fonts_mut(|f| {
                f.layout(
                    crate::ui::page::home::conn_subtitle(conn),
                    egui::FontId::proportional(10.0),
                    ui.visuals().weak_text_color(),
                    name_w,
                )
            });
            painter.galley(
                egui::pos2(text_left, row_rect.top() + 22.0),
                det_g,
                ui.visuals().weak_text_color(),
            );
        }

        ui.add_space(row_h);
    }

    // ── More button ──────────────────────────────────────────────────────
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let more_label = format!("{}  →", rust_i18n::t!("view_all"));
        if ui
            .button(
                egui::RichText::new(&more_label)
                    .size(12.0)
                    .color(crate::ui::widget::style::ACCENT),
            )
            .clicked()
        {
            *more_clicked = true;
        }
    });
}
