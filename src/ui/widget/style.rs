//! Modern UI design system — shared colours, helpers, and constants.
//!
//! All UI chrome should reference these values for a consistent modern look.

use egui::{Color32, CornerRadius, Frame, Margin, Stroke, Vec2};

// ─── Accent palette ───────────────────────────────────────────────────────────

/// Primary accent — a vibrant, modern blue.
pub const ACCENT: Color32 = Color32::from_rgb(74, 158, 255);
/// Accent colour at a lower opacity for hover / selection backgrounds.
pub const ACCENT_BG: Color32 = Color32::from_rgba_premultiplied(74, 158, 255, 40);
/// Accent at very low opacity for subtle highlights.
pub const ACCENT_BG_SUBTLE: Color32 = Color32::from_rgba_premultiplied(74, 158, 255, 18);

/// Success / online green.
pub const GREEN: Color32 = Color32::from_rgb(61, 220, 132);
/// Green at low opacity.
pub const GREEN_BG: Color32 = Color32::from_rgba_premultiplied(61, 220, 132, 25);

/// Destructive / close red.
pub const RED: Color32 = Color32::from_rgb(255, 82, 82);
/// Red at low opacity.
pub const RED_BG: Color32 = Color32::from_rgba_premultiplied(255, 82, 82, 20);

/// Warning amber.
pub const AMBER: Color32 = Color32::from_rgb(255, 215, 64);

// ─── Surface / background hierarchy (dark theme) ────────────────────────────

/// Deepest background (window level).
pub const SURFACE_0: Color32 = Color32::from_rgb(13, 13, 15);
/// Elevated surface (panel / sidebar).
pub const SURFACE_1: Color32 = Color32::from_rgb(19, 19, 23);
/// Card / section background.
pub const SURFACE_2: Color32 = Color32::from_rgb(26, 26, 32);
/// Hovered surface.
pub const SURFACE_3: Color32 = Color32::from_rgb(32, 32, 40);
/// Active / selected surface.
pub const SURFACE_4: Color32 = Color32::from_rgb(38, 38, 48);

/// Subtle border for frames.
pub const BORDER_SUBTLE: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 12);
/// Standard border for interactive elements.
pub const BORDER: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 20);
/// Accent border for selected / focused elements.
pub const BORDER_ACCENT: Color32 = Color32::from_rgba_premultiplied(74, 158, 255, 80);

// ─── Text colours ────────────────────────────────────────────────────────────

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(232, 232, 236);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(158, 158, 166);
pub const TEXT_TERTIARY: Color32 = Color32::from_rgb(120, 120, 130);
pub const TEXT_ACCENT: Color32 = ACCENT;

// ─── Geometry ────────────────────────────────────────────────────────────────

pub const CORNER_RADIUS: CornerRadius = CornerRadius::same(10);
pub const CORNER_RADIUS_SM: CornerRadius = CornerRadius::same(6);
pub const CORNER_RADIUS_XS: CornerRadius = CornerRadius::same(4);

pub const CARD_HEIGHT: f32 = 80.0;
pub const CARD_SPACING: f32 = 8.0;

// ─── Frame helpers ───────────────────────────────────────────────────────────

/// A modern card frame with subtle border.
pub fn card_frame() -> Frame {
    Frame::new()
        .fill(SURFACE_2)
        .stroke(Stroke::new(1.0, BORDER_SUBTLE))
        .corner_radius(CORNER_RADIUS_SM)
        .inner_margin(Margin::symmetric(14, 12))
}

/// A section card for settings / grouped content.
pub fn section_frame() -> Frame {
    Frame::new()
        .fill(SURFACE_2)
        .stroke(Stroke::new(1.0, BORDER_SUBTLE))
        .corner_radius(CORNER_RADIUS_SM)
        .inner_margin(Margin::symmetric(16, 14))
}

/// A tight card for lists (profiles, sessions, etc.).
pub fn list_item_frame(fill: Color32) -> Frame {
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, BORDER_SUBTLE))
        .corner_radius(CORNER_RADIUS_XS)
        .inner_margin(Margin::symmetric(10, 8))
}

/// Themed toolbar button style (borderless, rounded).
pub fn toolbar_button() -> egui::Button<'static> {
    egui::Button::new("")
        .frame(false)
        .corner_radius(CORNER_RADIUS_XS)
}

/// Modern pill-shaped button.
pub fn pill_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(label)
        .corner_radius(CORNER_RADIUS)
        .min_size(Vec2::new(0.0, 32.0))
}

/// Modern primary (accent) button.
pub fn primary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(label)
        .fill(ACCENT)
        .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
        .corner_radius(CORNER_RADIUS_SM)
        .min_size(Vec2::new(0.0, 32.0))
}

/// Icon slot size for card toolbars.
pub const ICON_SLOT: f32 = 36.0;
/// Gap between toolbar icons.
pub const TOOLBAR_GAP: f32 = 2.0;
/// Right margin inside a card for the toolbar.
pub const TOOLBAR_MARGIN: f32 = 10.0;

/// Layout horizontal position of toolbar icons inside a card.
pub struct CardToolbar {
    pub file: Option<egui::Rect>,
    pub edit: Option<egui::Rect>,
    pub actions: Vec<egui::Rect>,
}

impl CardToolbar {
    pub fn layout(card: egui::Rect, show_file: bool, show_edit: bool) -> Self {
        let cy = card.center().y;
        let mut x = card.right() - TOOLBAR_MARGIN;
        let actions = Vec::new();

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
            let r = egui::Rect::from_center_size(
                egui::pos2(x + ICON_SLOT / 2.0, cy),
                egui::vec2(ICON_SLOT, ICON_SLOT),
            );
            Some(r)
        } else {
            None
        };

        Self { file, edit, actions }
    }

    pub fn reserved_width(show_file: bool, show_edit: bool) -> f32 {
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
