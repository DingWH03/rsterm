//! Responsive sidebar: home is pinned when wide; workspace toggles when wide; overlay when narrow.

pub const WIDE_THRESHOLD: f32 = 720.0;
pub const DOCK_WIDTH: f32 = 200.0;
pub const OVERLAY_WIDTH: f32 = 260.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarPage {
    Home,
    Workspace,
}

pub struct Sidebar {
    pub wide: bool,
    /// Terminal page + wide layout: docked panel open (toggle with ☰).
    workspace_docked_open: bool,
    /// Narrow layout: slide-over panel open.
    overlay_open: bool,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            wide: false,
            workspace_docked_open: false,
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

    /// Permanent left `SidePanel` (not overlay).
    pub fn docked_visible(&self, page: SidebarPage) -> bool {
        match page {
            SidebarPage::Home => self.wide,
            SidebarPage::Workspace => self.wide && self.workspace_docked_open,
        }
    }

    pub fn overlay_visible(&self) -> bool {
        !self.wide && self.overlay_open
    }

    /// Show ☰ in the main content header (workspace: always, to toggle dock/overlay).
    pub fn show_content_hamburger(&self, page: SidebarPage) -> bool {
        match page {
            SidebarPage::Home => !self.wide,
            SidebarPage::Workspace => true,
        }
    }

    /// Show ☰ inside a docked/overlay sidebar panel (home overlay, narrow workspace).
    pub fn show_panel_hamburger(&self, page: SidebarPage) -> bool {
        match page {
            SidebarPage::Home => self.overlay_visible(),
            SidebarPage::Workspace => !self.wide,
        }
    }

    pub fn hamburger_click(&mut self, page: SidebarPage) {
        if self.wide {
            match page {
                SidebarPage::Home => {}
                SidebarPage::Workspace => {
                    self.workspace_docked_open = !self.workspace_docked_open;
                }
            }
        } else {
            self.overlay_open = !self.overlay_open;
        }
    }

    pub fn close_overlay(&mut self) {
        self.overlay_open = false;
    }

    pub fn hamburger(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.button(egui::RichText::new("\u{2630}").size(18.0))
    }

    /// Dimmed backdrop; returns `true` if the user tapped outside the panel.
    pub fn overlay_backdrop_clicked(ctx: &egui::Context, backdrop_id: egui::Id) -> bool {
        let rect = ctx.screen_rect();
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
        let rect = ctx.screen_rect();
        let w = OVERLAY_WIDTH;
        egui::Area::new(egui::Id::new(panel_id))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_top())
            .show(ctx, |ui| {
                egui::Frame::side_top_panel(ui.style()).show(ui, |ui| {
                    ui.set_min_width(w);
                    ui.set_min_height(rect.height());
                    ui.set_max_height(rect.height());
                    body(ui);
                });
            });
    }
}
