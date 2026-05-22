//! Responsive sidebar: docked (toggleable via ☰) when wide; overlay when narrow.

pub mod common;
pub mod session_list;
pub mod sidebars;

use crate::ui::widget::style;

pub const WIDE_THRESHOLD: f32 = 720.0;
pub const DOCK_WIDTH: f32 = 200.0;
pub const OVERLAY_WIDTH: f32 = 260.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarPage {
    Workspace,
}

pub struct Sidebar {
    pub wide: bool,
    /// Wide layout: docked sidebar visible.
    docked_open: bool,
    /// Narrow layout: slide-over panel open.
    overlay_open: bool,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            wide: false,
            docked_open: true,
            overlay_open: false,
        }
    }

    pub fn sync_width(&mut self, width: f32) {
        let now_wide = width > WIDE_THRESHOLD;
        if now_wide && !self.wide {
            self.overlay_open = false;
        }
        self.wide = now_wide;
    }

    /// Docked left panel (wide layout only).
    pub fn docked_visible(&self) -> bool {
        self.wide && self.docked_open
    }

    pub fn overlay_visible(&self) -> bool {
        !self.wide && self.overlay_open
    }

    /// Show ☰ hamburger in content area.
    pub fn show_content_hamburger(&self) -> bool {
        true
    }

    /// Toggle sidebar visibility.
    pub fn hamburger_click(&mut self) {
        if self.wide {
            self.docked_open = !self.docked_open;
        } else {
            self.overlay_open = !self.overlay_open;
        }
    }

    pub fn close_overlay(&mut self) {
        self.overlay_open = false;
    }

    pub fn hamburger(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.add(
            egui::Button::new(egui::RichText::new("\u{2630}").size(18.0).color(style::TEXT_SECONDARY))
                .frame(false)
                .corner_radius(style::CORNER_RADIUS_XS),
        )
    }

    /// Dimmed backdrop; returns `true` if the user tapped outside the panel.
    pub fn overlay_backdrop_clicked(ctx: &egui::Context, backdrop_id: egui::Id) -> bool {
        let rect = ctx.content_rect();
        let mut clicked = false;
        egui::Area::new(backdrop_id)
            .order(egui::Order::Background)
            .interactable(true)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let (_, r) = ui.allocate_exact_size(rect.size(), egui::Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(120));
                clicked = r.clicked();
            });
        clicked
    }

    pub fn show_overlay<F>(ctx: &egui::Context, panel_id: &str, mut body: F)
    where
        F: FnMut(&mut egui::Ui),
    {
        let rect = ctx.content_rect();
        // Responsive overlay width: adapts to screen width on narrow devices.
        let w = OVERLAY_WIDTH.min(rect.width() * 0.82).max(180.0);
        let top_inset = {
            #[cfg(target_os = "android")]
            {
                crate::platform::get().top_inset_points(ctx)
            }
            #[cfg(not(target_os = "android"))]
            {
                0.0
            }
        };
        let panel_height = (rect.height() - top_inset).max(1.0);
        egui::Area::new(egui::Id::new(panel_id))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(rect.left(), rect.top() + top_inset))
            .show(ctx, |ui| {
                egui::Frame::side_top_panel(ui.style()).show(ui, |ui| {
                    ui.set_min_width(w);
                    ui.set_max_width(w);
                    ui.set_min_height(panel_height);
                    ui.set_max_height(panel_height);
                    body(ui);
                });
            });
    }
}
