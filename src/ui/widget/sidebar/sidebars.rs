use crate::session::WorkspaceSession;
use crate::storage::types::{ConnectionType, SavedConnection};
use crate::ui::widget::sidebar::Sidebar;
use crate::ui::widget::sidebar::common::{nav_button, sidebar_brand_row};
use crate::ui::widget::sidebar::session_list::paint_session_rows;
use crate::ui::widget::style;

// ─── Main view (connection management + sessions + settings) ──────────────────

pub struct MainSidebarAction {
    pub select_session: Option<String>,
    pub close_session: Option<String>,
    pub new_window_session: Option<String>,
    pub open_connection_mgmt: bool,
    pub settings_toggled: bool,
}

/// Render the main sidebar with three sections:
///   top:    brand + [ 📋 Connection Management ]
///   middle: active sessions (scrollable, fills remaining space)
///   bottom: [ ⚙ Settings ] (pinned)
pub fn main_sidebar(
    ui: &mut egui::Ui,
    sidebar: &mut Sidebar,
    sessions: &[WorkspaceSession],
    active_id: Option<&str>,
    on_settings: bool,
) -> MainSidebarAction {
    let mut action = MainSidebarAction {
        select_session: None,
        close_session: None,
        new_window_session: None,
        open_connection_mgmt: false,
        settings_toggled: false,
    };

    // ── Top section (fixed) ──────────────────────────────────────────────────
    sidebar_brand_row(ui, sidebar, false);
    ui.add_space(1.0);

    if nav_button(ui, "", &rust_i18n::t!("connection_mgmt"), false).clicked() {
        action.open_connection_mgmt = true;
    }
    ui.add_space(1.0);
    ui.separator();
    ui.add_space(1.0);

    // ── Sessions area (fills remaining space before bottom section) ──────────
    let top_used = ui.cursor().min.y - ui.max_rect().top();
    let bottom_reserve = 52.0; // spacer + separator + button
    let scroll_h = (ui.max_rect().height() - top_used - bottom_reserve).max(32.0);

    ui.style_mut().spacing.scroll.bar_width = 6.0;
    ui.style_mut().spacing.scroll.bar_outer_margin = 0.0;

    let sess_action = egui::ScrollArea::vertical()
        .id_salt("sidebar_sessions_scroll")
        .auto_shrink([false; 2])
        .max_height(scroll_h)
        .show(ui, |ui| {
            paint_session_rows(ui, sessions, active_id)
        })
        .inner;

    action.select_session = sess_action.select_session;
    action.close_session = sess_action.close_session;
    action.new_window_session = sess_action.new_window_session;

    // ── Bottom section (fixed) ──────────────────────────────────────────────
    ui.add_space(1.0);
    ui.separator();
    ui.add_space(1.0);

    if nav_button(ui, "\u{2699}", &rust_i18n::t!("settings"), on_settings).clicked() {
        action.settings_toggled = true;
    }

    action
}

// ─── Connections list view (inside sidebar) ───────────────────────────────────

pub struct ConnectionsSidebarAction {
    pub go_back: bool,
    pub new_connection: bool,
    pub connect_connection: Option<String>,
    pub open_file_mgr: Option<String>,
    pub edit_connection: Option<String>,
    pub delete_connection: Option<String>,
}

/// Full connection list rendered inside the sidebar with a back button,
/// filter chips, and scrollable connection rows.
pub fn connections_sidebar(
    ui: &mut egui::Ui,
    connections: &[SavedConnection],
) -> ConnectionsSidebarAction {
    let mut action = ConnectionsSidebarAction {
        go_back: false,
        new_connection: false,
        connect_connection: None,
        open_file_mgr: None,
        edit_connection: None,
        delete_connection: None,
    };

    // ── Top bar: Back + New Connection ──────────────────────────────────────
    let top_h = 32.0;
    let top_w = ui.available_width();
    let top_rect = ui.allocate_exact_size(egui::vec2(top_w, top_h), egui::Sense::hover()).0;

    // Back button (left half)
    let back_rect = egui::Rect::from_min_size(top_rect.min, egui::vec2(top_w * 0.5, top_h));
    let back_resp = ui.interact(back_rect, ui.id().with("conn_back"), egui::Sense::click());
    if back_resp.clicked() {
        action.go_back = true;
    }

    // New Connection button (right half)
    let new_rect = egui::Rect::from_min_size(
        egui::pos2(top_rect.center().x, top_rect.top()),
        egui::vec2(top_w * 0.5, top_h),
    );
    let new_resp = ui.interact(new_rect, ui.id().with("conn_new"), egui::Sense::click());
    if new_resp.clicked() {
        action.new_connection = true;
    }

    if ui.is_rect_visible(top_rect) {
        let painter = ui.painter();

        // Back
        if back_resp.hovered() {
            painter.rect_filled(back_rect, style::CORNER_RADIUS_XS, ui.visuals().widgets.hovered.bg_fill);
        }
        let back_g = ui.fonts_mut(|f| {
            f.layout(
                format!("\u{2190}  {}", rust_i18n::t!("back")),
                egui::FontId::proportional(14.0),
                ui.visuals().text_color(),
                top_w * 0.4,
            )
        });
        painter.galley(
            egui::pos2(back_rect.left() + 8.0, back_rect.center().y - back_g.size().y / 2.0),
            back_g,
            ui.visuals().text_color(),
        );

        // New Connection
        if new_resp.hovered() {
            painter.rect_filled(new_rect, style::CORNER_RADIUS_XS, ui.visuals().widgets.hovered.bg_fill);
        }
        let new_g = ui.fonts_mut(|f| {
            f.layout(
                format!("+  {}", rust_i18n::t!("new_connection")),
                egui::FontId::proportional(14.0),
                style::ACCENT,
                top_w * 0.4,
            )
        });
        painter.galley(
            egui::pos2(new_rect.right() - 8.0 - new_g.size().x, new_rect.center().y - new_g.size().y / 2.0),
            new_g,
            style::ACCENT,
        );
    }
    ui.add_space(4.0);

    // ── Filter chips ────────────────────────────────────────────────────────
    let filter: Option<ConnectionType> =
        ui.data(|d| d.get_temp(egui::Id::new("sidebar_conn_filter")))
            .unwrap_or(None);

    ui.horizontal(|ui| {
        ui.style_mut().spacing.item_spacing.x = 4.0;
        if ui.selectable_label(filter.is_none(), "All").clicked() {
            ui.data_mut(|d| d.insert_temp(egui::Id::new("sidebar_conn_filter"), None::<ConnectionType>));
        }
        for ct in [
            ConnectionType::Local,
            ConnectionType::Ssh,
            ConnectionType::Serial,
            ConnectionType::Ble,
        ] {
            let sel = filter.as_ref() == Some(&ct);
            let short = match ct {
                ConnectionType::Local => "Local",
                ConnectionType::Ssh => "SSH",
                ConnectionType::Serial => "Serial",
                ConnectionType::Ble => "BLE",
            };
            if ui.selectable_label(sel, short).clicked() {
                ui.data_mut(|d| d.insert_temp(egui::Id::new("sidebar_conn_filter"), Some(ct)));
            }
        }
    });
    ui.add_space(4.0);

    // ── Connection list ─────────────────────────────────────────────────────
    let mut sorted: Vec<&SavedConnection> = match filter {
        Some(ref ft) => connections.iter().filter(|c| c.conn_type == *ft).collect(),
        None => connections.iter().collect(),
    };
    sorted.sort_by(|a, b| {
        b.favorite
            .cmp(&a.favorite)
            .then_with(|| b.last_connected.cmp(&a.last_connected))
            .then_with(|| a.name.cmp(&b.name))
    });

    ui.separator();

    if sorted.is_empty() {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(rust_i18n::t!("home_no_connections"))
                .size(13.0)
                .color(ui.visuals().weak_text_color()),
        );
    } else {
        ui.style_mut().spacing.scroll.bar_width = 6.0;
        ui.style_mut().spacing.scroll.bar_outer_margin = 0.0;
        // Persistent menu state (egui data survives frame boundaries)
        let menu_id_key = egui::Id::new("conn_menu_id");
        let menu_state: Option<String> = ui.data(|d| d.get_temp(menu_id_key)).unwrap_or(None);

        // Close menu on next click anywhere
        if menu_state.is_some() && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary)) {
            ui.data_mut(|d| d.insert_temp(menu_id_key, None::<String>));
        }

        egui::ScrollArea::vertical()
            .id_salt("sidebar_conn_list_scroll")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for conn in &sorted {
                    let row_h = 40.0;
                    let row_rect = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), row_h),
                    );
                    let row_resp = ui.allocate_rect(row_rect, egui::Sense::click());

                    // ⋮ menu button area (right side, 24px wide)
                    let dots_rect = egui::Rect::from_min_size(
                        egui::pos2(row_rect.right() - 24.0, row_rect.top()),
                        egui::vec2(24.0, row_h),
                    );
                    let dots_id = ui.id().with(("dots", &conn.id));
                    let dots_resp = ui.interact(dots_rect, dots_id, egui::Sense::click());

                    // Connect on row click (not on dots)
                    if row_resp.clicked() && !dots_resp.clicked() && !row_resp.long_touched() {
                        ui.data_mut(|d| d.insert_temp(menu_id_key, None::<String>));
                        action.connect_connection = Some(conn.id.clone());
                    }

                    // Right-click → menu
                    let show_file = matches!(conn.conn_type, ConnectionType::Local | ConnectionType::Ssh);
                    row_resp.context_menu(|ui| {
                        ui.data_mut(|d| d.insert_temp(menu_id_key, None::<String>));
                        paint_conn_menu(ui, conn, show_file, &mut action);
                    });
                    // Long-press / ⋮ click → open menu
                    if row_resp.long_touched() || dots_resp.clicked() {
                        ui.data_mut(|d| d.insert_temp(menu_id_key, Some(conn.id.clone())));
                    }

                    if ui.is_rect_visible(row_rect) {
                        let painter = ui.painter_at(row_rect);
                        if row_resp.hovered() || menu_state.as_deref() == Some(conn.id.as_str()) {
                            painter.rect_filled(
                                row_rect,
                                style::CORNER_RADIUS_XS,
                                ui.visuals().widgets.hovered.bg_fill,
                            );
                        }

                        // Name
                        let text_left = row_rect.left() + 10.0;
                        let name_w = row_rect.right() - text_left - 30.0;
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

                        // Detail line
                        let det_g = ui.fonts_mut(|f| {
                            f.layout(
                                conn_subtitle(conn),
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

                        // ⋮ character
                        let dots_g = ui.fonts_mut(|f| {
                            f.layout(
                                "\u{22EE}".to_string(),
                                egui::FontId::proportional(16.0),
                                if dots_resp.hovered() { ui.visuals().text_color() } else { ui.visuals().weak_text_color() },
                                f32::INFINITY,
                            )
                        });
                        painter.galley(
                            egui::pos2(dots_rect.center().x - dots_g.size().x / 2.0, dots_rect.center().y - dots_g.size().y / 2.0),
                            dots_g,
                            if dots_resp.hovered() { ui.visuals().text_color() } else { ui.visuals().weak_text_color() },
                        );
                    }

                    // Popup anchored to the ⋮ button (only for the active row)
                    if menu_state.as_deref() == Some(conn.id.as_str()) {
                        egui::Popup::from_response(&dots_resp)
                            .id(dots_id.with("ctx"))
                            .show(|ui| {
                                ui.set_min_width(130.0);
                                if ui.button(rust_i18n::t!("connect")).clicked() {
                                    action.connect_connection = Some(conn.id.clone());
                                    ui.data_mut(|d| d.insert_temp(menu_id_key, None::<String>));
                                }
                                if show_file {
                                    if ui.button(rust_i18n::t!("home_file_manager")).clicked() {
                                        action.open_file_mgr = Some(conn.id.clone());
                                        ui.data_mut(|d| d.insert_temp(menu_id_key, None::<String>));
                                    }
                                }
                                if ui.button(rust_i18n::t!("edit")).clicked() {
                                    action.edit_connection = Some(conn.id.clone());
                                    ui.data_mut(|d| d.insert_temp(menu_id_key, None::<String>));
                                }
                                if ui.button(rust_i18n::t!("delete")).clicked() {
                                    action.delete_connection = Some(conn.id.clone());
                                    ui.data_mut(|d| d.insert_temp(menu_id_key, None::<String>));
                                }
                            });
                    }

                    ui.add_space(2.0);
                }
            });
    }

    action
}

/// Context menu content shared by right-click.
fn paint_conn_menu(
    ui: &mut egui::Ui,
    conn: &SavedConnection,
    show_file: bool,
    action: &mut ConnectionsSidebarAction,
) {
    ui.set_min_width(130.0);
    // Clear any ⋮ menu state
    if ui.button(rust_i18n::t!("connect")).clicked() {
        action.connect_connection = Some(conn.id.clone());
        ui.close();
    }
    if show_file {
        if ui.button(rust_i18n::t!("home_file_manager")).clicked() {
            action.open_file_mgr = Some(conn.id.clone());
            ui.close();
        }
    }
    if ui.button(rust_i18n::t!("edit")).clicked() {
        action.edit_connection = Some(conn.id.clone());
        ui.close();
    }
    if ui.button(rust_i18n::t!("delete")).clicked() {
        action.delete_connection = Some(conn.id.clone());
        ui.close();
    }
}

/// Build a subtitle line combining connection type and its key details.
fn conn_subtitle(conn: &SavedConnection) -> String {
    let detail = match conn.conn_type {
        ConnectionType::Ssh => {
            let user = conn.ssh_user.as_deref().unwrap_or("root");
            let host = conn.ssh_host.as_deref().unwrap_or("?");
            let port = conn.ssh_port.unwrap_or(22);
            format!("{user}@{host}:{port}")
        }
        ConnectionType::Serial => {
            let port = conn.serial_port.as_deref().unwrap_or("?");
            if let Some(baud) = conn.serial_baud {
                format!("{port} @ {baud} baud")
            } else {
                port.to_string()
            }
        }
        ConnectionType::Ble => conn.ble_device.as_deref().unwrap_or("?").to_string(),
        ConnectionType::Local => {
            let wd = conn.working_dir.as_deref().unwrap_or("~");
            let shell = conn.shell.as_deref().unwrap_or("default");
            format!("{shell} · {wd}")
        }
    };
    format!("{detail}")
}
