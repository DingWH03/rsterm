use crate::config::{BellStyle, CursorStyle, TerminalTheme, TerminalType};
use crate::i18n::Language;
use crate::settings::{AppSettings, Profile};
use crate::ui::keyboard::KeyboardMode;

// ─── Tab identifiers ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Profiles,
    Appearance,
    Theme,
    Behavior,
    Advanced,
}

impl SettingsTab {
    const ALL: [Self; 6] = [
        Self::General,
        Self::Profiles,
        Self::Appearance,
        Self::Theme,
        Self::Behavior,
        Self::Advanced,
    ];

    fn label(self) -> String {
        match self {
            Self::General => rust_i18n::t!("settings_tab_general").into_owned(),
            Self::Profiles => rust_i18n::t!("settings_tab_profiles").into_owned(),
            Self::Appearance => rust_i18n::t!("settings_tab_appearance").into_owned(),
            Self::Theme => rust_i18n::t!("settings_tab_theme").into_owned(),
            Self::Behavior => rust_i18n::t!("settings_tab_behavior").into_owned(),
            Self::Advanced => rust_i18n::t!("settings_tab_advanced").into_owned(),
        }
    }
}

// ─── Public entry points ──────────────────────────────────────────────────────

/// Home screen: full-page settings with Back.
pub fn settings_page(ui: &mut egui::Ui, settings: &mut AppSettings) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        if ui.button(rust_i18n::t!("back")).clicked() {
            close = true;
        }
    });
    ui.add_space(4.0);
    settings_scroll_body(ui, settings, SettingsLayout::Home);
    close
}

/// Workspace: right-hand panel; returns `true` to close.
pub fn settings_side_panel(ui: &mut egui::Ui, settings: &mut AppSettings) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(rust_i18n::t!("settings")).size(16.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new("\u{2715}").small())
                .on_hover_text(rust_i18n::t!("close"))
                .clicked()
            {
                close = true;
            }
        });
    });
    ui.label(
        egui::RichText::new(rust_i18n::t!("settings_terminal_running_hint"))
            .size(11.0)
            .color(egui::Color32::GRAY),
    );
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);
    settings_scroll_body(ui, settings, SettingsLayout::Workspace);
    close
}

// ─── Internal layout ──────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum SettingsLayout {
    Home,
    Workspace,
}

fn settings_scroll_body(ui: &mut egui::Ui, settings: &mut AppSettings, layout: SettingsLayout) {
    egui::ScrollArea::vertical()
        .id_salt(match layout {
            SettingsLayout::Home => "settings_page_scroll",
            SettingsLayout::Workspace => "settings_side_panel_scroll",
        })
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.push_id(match layout {
                SettingsLayout::Home => "home",
                SettingsLayout::Workspace => "workspace",
            }, |ui| {
                settings_page_body(ui, settings);
            });
        });
}

#[derive(Clone, Copy)]
struct TabState {
    active_tab: SettingsTab,
    selected_profile: usize,
}

impl Default for TabState {
    fn default() -> Self {
        Self { active_tab: SettingsTab::General, selected_profile: 0 }
    }
}

fn tab_bar(ui: &mut egui::Ui, state: &mut TabState) {
    ui.horizontal(|ui| {
        for tab in SettingsTab::ALL {
            let label = tab.label();
            let selected = state.active_tab == tab;
            let mut btn = egui::Button::new(egui::RichText::new(&label).size(13.0));
            if selected {
                btn = btn.fill(ui.visuals().selection.bg_fill);
            }
            if ui.add(btn).clicked() {
                state.active_tab = tab;
            }
        }
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}

pub fn settings_page_body(ui: &mut egui::Ui, settings: &mut AppSettings) {
    let mut state: TabState = ui.memory_mut(|mem|
        mem.data.get_persisted::<TabState>(ui.id().with("settings_tab_state"))
    ).unwrap_or_default();

    tab_bar(ui, &mut state);

    if state.selected_profile >= settings.profiles.len() {
        state.selected_profile = 0;
    }

    match state.active_tab {
        SettingsTab::General => general_tab(ui, settings),
        SettingsTab::Profiles => profiles_tab(ui, settings, &mut state),
        SettingsTab::Appearance => appearance_tab(ui, settings, state.selected_profile),
        SettingsTab::Theme => theme_tab(ui, settings, state.selected_profile),
        SettingsTab::Behavior => behavior_tab(ui, settings, state.selected_profile),
        SettingsTab::Advanced => advanced_tab(ui, settings, state.selected_profile),
    }

    ui.memory_mut(|mem| {
        mem.data.insert_persisted(ui.id().with("settings_tab_state"), state);
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
//  UNIFIED SETTING WIDGETS
// ═══════════════════════════════════════════════════════════════════════════════

/// Width reserved for the label column so all rows align.
const LABEL_WIDTH: f32 = 160.0;

/// A section card with title + optional subtitle.
fn section(ui: &mut egui::Ui, title: &str, subtitle: &str, add_body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(title).size(14.0).strong());
            if !subtitle.is_empty() {
                ui.label(egui::RichText::new(subtitle).size(11.0).weak());
            }
            ui.add_space(10.0);
            add_body(ui);
        });
    ui.add_space(10.0);
}

/// A single setting row with a fixed-width label on the left and the widget on the right.
fn row(ui: &mut egui::Ui, label: &str, add_widget: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_width(LABEL_WIDTH);
        ui.label(egui::RichText::new(label).size(13.0));
        ui.add(egui::Separator::default().vertical().spacing(8.0));
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(ui.available_width());
            add_widget(ui);
        });
    });
    ui.add_space(6.0);
}

/// ── Concrete setting helpers ──

/// Slider setting (e.g. font size, line spacing).
fn slider_row(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) {
    row(ui, label, |ui| {
        ui.add(egui::Slider::new(value, range).show_value(true));
    });
}

/// Integer slider (logarithmic).
fn slider_row_usize(ui: &mut egui::Ui, label: &str, value: &mut usize, range: std::ops::RangeInclusive<usize>, logarithmic: bool) {
    row(ui, label, |ui| {
        let mut s = egui::Slider::new(value, range).show_value(true);
        if logarithmic {
            s = s.logarithmic(true);
        }
        ui.add(s);
    });
}

/// Checkbox toggle.
fn toggle_row(ui: &mut egui::Ui, label: &str, value: &mut bool) {
    row(ui, label, |ui| {
        ui.checkbox(value, "");
    });
}

/// Combo-box (dropdown) selector.
fn combo_row<T: PartialEq + Copy + 'static>(
    ui: &mut egui::Ui,
    label: &str,
    current: &mut T,
    options: &[T],
    display: fn(T) -> String,
    id_salt: &str,
) {
    row(ui, label, |ui| {
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(display(*current))
            .show_ui(ui, |ui| {
                for &opt in options {
                    let d = display(opt);
                    if ui.selectable_label(*current == opt, d).clicked() {
                        *current = opt;
                    }
                }
            });
    });
}

/// Radio-button group (horizontal).
fn radio_group<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    label: &str,
    current: &mut T,
    options: &[T],
    display: fn(T) -> String,
) {
    row(ui, label, |ui| {
        ui.horizontal(|ui| {
            for &opt in options {
                let d = display(opt);
                if ui.selectable_label(*current == opt, d).clicked() {
                    *current = opt;
                }
            }
        });
    });
}

/// Single-line text input.
fn text_row(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str, width: f32) {
    row(ui, label, |ui| {
        ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(width));
    });
}

/// Color edit button + hex label.
fn color_row(ui: &mut egui::Ui, label: &str, color: &mut egui::Color32) {
    row(ui, label, |ui| {
        let mut rgb = [
            color.r() as f32 / 255.0,
            color.g() as f32 / 255.0,
            color.b() as f32 / 255.0,
        ];
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            *color = egui::Color32::from_rgb(
                (rgb[0] * 255.0) as u8,
                (rgb[1] * 255.0) as u8,
                (rgb[2] * 255.0) as u8,
            );
        }
        ui.monospace(format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b()));
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
//  TAB CONTENT
// ═══════════════════════════════════════════════════════════════════════════════

// ─── General Tab ──────────────────────────────────────────────────────────────

fn general_tab(ui: &mut egui::Ui, settings: &mut AppSettings) {
    section(ui, &rust_i18n::t!("language"), "", |ui| {
        language_selector(ui, settings);
    });

    section(ui, &rust_i18n::t!("ui_theme"), "", |ui| {
        ui_theme_selector(ui, settings);
    });

    section(
        ui,
        &rust_i18n::t!("settings_section_ssh_env"),
        &rust_i18n::t!("settings_section_ssh_env_desc"),
        |ui| ssh_env_editor(ui, settings),
    );
}

// ─── Profiles Tab ─────────────────────────────────────────────────────────────

fn profiles_tab(ui: &mut egui::Ui, settings: &mut AppSettings, state: &mut TabState) {
    section(ui, &rust_i18n::t!("settings_section_profiles"), &rust_i18n::t!("settings_section_profiles_desc"), |ui| {
        profile_selector(ui, settings, state);
    });

    if settings.profiles.is_empty() {
        return;
    }

    let profile_idx = state.selected_profile;
    let is_default = settings.default_profile_name == settings.profiles[profile_idx].name;

    if let Some(profile) = settings.profiles.get_mut(profile_idx) {
        section(
            ui,
            &format!("{}: {}", rust_i18n::t!("settings_profile_detail"), profile.name),
            "",
            |ui| {
                profile_detail_editor(ui, profile, profile_idx, is_default);
            },
        );
    }
}

fn profile_detail_editor(
    ui: &mut egui::Ui,
    profile: &mut Profile,
    _profile_idx: usize,
    is_default: bool,
) {
    text_row(ui, &rust_i18n::t!("settings_profile_name"), &mut profile.name, "", 200.0);
    text_row(ui, &rust_i18n::t!("settings_profile_description"), &mut profile.description, "", 200.0);

    if is_default {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(rust_i18n::t!("settings_current_default")).color(ui.visuals().weak_text_color()));
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button(rust_i18n::t!("settings_duplicate_profile")).clicked() {
            let new_name = format!("{} (Copy)", profile.name);
            let _copy = profile.duplicate(&new_name);
        }
        if ui.button(rust_i18n::t!("settings_export_profile")).clicked() {
            if let Ok(json) = profile.export_json() {
                ui.ctx().copy_text(json);
            }
        }
    });

    ui.add_space(4.0);
    row(ui, &rust_i18n::t!("settings_theme_preset"), |ui| {
        let presets = TerminalTheme::presets();
        egui::ComboBox::from_id_salt("theme_preset_combo")
            .selected_text("—")
            .show_ui(ui, |ui| {
                for (name, preset_fn) in &presets {
                    if ui.selectable_label(false, *name).clicked() {
                        profile.theme = preset_fn();
                    }
                }
            });
    });
}

fn profile_selector(ui: &mut egui::Ui, settings: &mut AppSettings, state: &mut TabState) {
    if settings.profiles.is_empty() {
        ui.label(egui::RichText::new(rust_i18n::t!("settings_no_profiles")).color(egui::Color32::GRAY));
        if ui.button(rust_i18n::t!("settings_create_profile")).clicked() {
            let mut p = Profile::default();
            p.name = format!("Profile {}", settings.profiles.len() + 1);
            settings.profiles.push(p);
            state.selected_profile = settings.profiles.len() - 1;
        }
        return;
    }

    let mut to_delete: Option<usize> = None;
    let names: Vec<String> = settings.profiles.iter().map(|p| p.name.clone()).collect();

    for (i, name) in names.iter().enumerate() {
        let is_default = *name == settings.default_profile_name;
        let is_selected = i == state.selected_profile;
        let bg = if is_selected {
            ui.visuals().selection.bg_fill.gamma_multiply(0.3)
        } else {
            egui::Color32::TRANSPARENT
        };

        egui::Frame::new()
            .fill(bg)
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let label = if is_default {
                        format!("● {name}")
                    } else {
                        name.clone()
                    };
                    if ui.selectable_label(is_selected, egui::RichText::new(&label).size(13.0)).clicked() {
                        state.selected_profile = i;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !is_default && settings.profiles.len() > 1 {
                            if ui.small_button("\u{2715}").on_hover_text(rust_i18n::t!("delete")).clicked() {
                                to_delete = Some(i);
                            }
                        }
                        if !is_default && ui.small_button("★").on_hover_text(rust_i18n::t!("settings_set_default")).clicked() {
                            settings.default_profile_name = name.clone();
                        }
                    });
                });
            });
        ui.add_space(2.0);
    }

    if let Some(idx) = to_delete {
        settings.profiles.remove(idx);
        if state.selected_profile >= settings.profiles.len() {
            state.selected_profile = settings.profiles.len().saturating_sub(1);
        }
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let mut new_name = String::new();
        ui.add(
            egui::TextEdit::singleline(&mut new_name)
                .hint_text(rust_i18n::t!("settings_new_profile_hint"))
                .desired_width(140.0),
        );
        if ui.button(rust_i18n::t!("settings_create_profile")).clicked() && !new_name.is_empty() {
            let mut p = Profile::default();
            p.name = new_name;
            settings.profiles.push(p);
            state.selected_profile = settings.profiles.len() - 1;
        }
    });
}

// ─── Appearance Tab ───────────────────────────────────────────────────────────

fn appearance_tab(ui: &mut egui::Ui, settings: &mut AppSettings, profile_idx: usize) {
    let Some(profile) = settings.profiles.get_mut(profile_idx) else {
        ui.colored_label(egui::Color32::YELLOW, rust_i18n::t!("settings_profile_not_found"));
        return;
    };
    section(ui, &rust_i18n::t!("settings_section_appearance"), &rust_i18n::t!("settings_section_appearance_desc"), |ui| {
        slider_row(ui, &rust_i18n::t!("settings_font_size"), &mut profile.font_size, 8.0..=32.0);
        slider_row(ui, &rust_i18n::t!("settings_line_spacing"), &mut profile.line_spacing, 0.5..=2.0);
        slider_row(ui, &rust_i18n::t!("settings_cell_width"), &mut profile.cell_width_scale, 0.5..=1.5);

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        radio_group(ui, &rust_i18n::t!("settings_cursor_style"), &mut profile.cursor_style, &CursorStyle::ALL, |cs| cs.label());

        toggle_row(ui, &rust_i18n::t!("settings_bold_is_bright"), &mut profile.bold_is_bright);

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        slider_row_usize(ui, &rust_i18n::t!("settings_scrollback_lines"), &mut profile.scrollback_lines, 100..=100_000, true);

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        radio_group(ui, &rust_i18n::t!("settings_default_keyboard"), &mut profile.keyboard_mode, &[KeyboardMode::Full, KeyboardMode::Special], |km| {
            match km {
                KeyboardMode::Full => rust_i18n::t!("settings_keyboard_full").into_owned(),
                KeyboardMode::Special => rust_i18n::t!("settings_keyboard_special").into_owned(),
            }
        });
    });
}

// ─── Theme Tab ────────────────────────────────────────────────────────────────

fn theme_tab(ui: &mut egui::Ui, settings: &mut AppSettings, profile_idx: usize) {
    let Some(profile) = settings.profiles.get_mut(profile_idx) else {
        ui.colored_label(egui::Color32::YELLOW, rust_i18n::t!("settings_profile_not_found"));
        return;
    };
    section(ui, &rust_i18n::t!("settings_theme_preset"), &rust_i18n::t!("settings_theme_preset_desc"), |ui| {
        ui.horizontal(|ui| {
            let presets = TerminalTheme::presets();
            for (name, preset_fn) in &presets {
                if ui.button(*name).clicked() {
                    profile.theme = preset_fn();
                }
            }
        });
    });
    section(ui, &rust_i18n::t!("settings_theme_colors"), "", |ui| {
        theme_colors_editor(ui, profile);
    });
}

fn theme_colors_editor(ui: &mut egui::Ui, profile: &mut Profile) {
    ui.label(egui::RichText::new(rust_i18n::t!("theme_basic")).size(12.0).weak());
    color_row(ui, &rust_i18n::t!("theme_bg"), &mut profile.theme.bg);
    color_row(ui, &rust_i18n::t!("theme_fg"), &mut profile.theme.fg);
    color_row(ui, &rust_i18n::t!("theme_cursor"), &mut profile.theme.cursor);
    color_row(ui, &rust_i18n::t!("theme_selection"), &mut profile.theme.selection);
    ui.add_space(6.0);
    ui.label(egui::RichText::new(rust_i18n::t!("theme_standard")).size(12.0).weak());
    color_row(ui, &rust_i18n::t!("theme_black"), &mut profile.theme.black);
    color_row(ui, &rust_i18n::t!("theme_red"), &mut profile.theme.red);
    color_row(ui, &rust_i18n::t!("theme_green"), &mut profile.theme.green);
    color_row(ui, &rust_i18n::t!("theme_yellow"), &mut profile.theme.yellow);
    color_row(ui, &rust_i18n::t!("theme_blue"), &mut profile.theme.blue);
    color_row(ui, &rust_i18n::t!("theme_magenta"), &mut profile.theme.magenta);
    color_row(ui, &rust_i18n::t!("theme_cyan"), &mut profile.theme.cyan);
    color_row(ui, &rust_i18n::t!("theme_white"), &mut profile.theme.white);
    ui.add_space(6.0);
    ui.label(egui::RichText::new(rust_i18n::t!("theme_bright")).size(12.0).weak());
    color_row(ui, &rust_i18n::t!("theme_bright_black"), &mut profile.theme.bright_black);
    color_row(ui, &rust_i18n::t!("theme_bright_red"), &mut profile.theme.bright_red);
    color_row(ui, &rust_i18n::t!("theme_bright_green"), &mut profile.theme.bright_green);
    color_row(ui, &rust_i18n::t!("theme_bright_yellow"), &mut profile.theme.bright_yellow);
    color_row(ui, &rust_i18n::t!("theme_bright_blue"), &mut profile.theme.bright_blue);
    color_row(ui, &rust_i18n::t!("theme_bright_magenta"), &mut profile.theme.bright_magenta);
    color_row(ui, &rust_i18n::t!("theme_bright_cyan"), &mut profile.theme.bright_cyan);
    color_row(ui, &rust_i18n::t!("theme_bright_white"), &mut profile.theme.bright_white);
}

// ─── Behavior Tab ─────────────────────────────────────────────────────────────

fn behavior_tab(ui: &mut egui::Ui, settings: &mut AppSettings, profile_idx: usize) {
    let Some(profile) = settings.profiles.get_mut(profile_idx) else {
        ui.colored_label(egui::Color32::YELLOW, rust_i18n::t!("settings_profile_not_found"));
        return;
    };
    section(ui, &rust_i18n::t!("settings_terminal_behavior"), &rust_i18n::t!("settings_terminal_behavior_desc"), |ui| {
        combo_row(ui, &rust_i18n::t!("settings_terminal_type"), &mut profile.terminal_type, &TerminalType::ALL, |tt| tt.label().to_string(), "terminal_type_combo");

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        radio_group(ui, &rust_i18n::t!("settings_bell"), &mut profile.bell, &BellStyle::ALL, |bs| bs.label().to_string());

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        toggle_row(ui, &rust_i18n::t!("settings_bracketed_paste"), &mut profile.enable_bracketed_paste);
        toggle_row(ui, &rust_i18n::t!("settings_sgr_mouse"), &mut profile.enable_sgr_mouse);
        toggle_row(ui, &rust_i18n::t!("settings_auto_wrap"), &mut profile.auto_wrap);

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        text_row(ui, &rust_i18n::t!("settings_word_separators"), &mut profile.word_separators, "", 200.0);
    });
}

// ─── Advanced Tab ─────────────────────────────────────────────────────────────

fn advanced_tab(ui: &mut egui::Ui, settings: &mut AppSettings, profile_idx: usize) {
    let Some(profile) = settings.profiles.get_mut(profile_idx) else {
        ui.colored_label(egui::Color32::YELLOW, rust_i18n::t!("settings_profile_not_found"));
        return;
    };
    section(ui, &rust_i18n::t!("settings_env_vars"), &rust_i18n::t!("settings_env_vars_desc"), |ui| {
        env_var_editor(ui, &mut profile.env_vars);
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
//  SHARED EDITORS
// ═══════════════════════════════════════════════════════════════════════════════

// ─── SSH env vars editor (global) ─────────────────────────────────────────────

fn ssh_env_editor(ui: &mut egui::Ui, settings: &mut AppSettings) {
    env_var_editor(ui, &mut settings.ssh_env_vars);
}

// ─── Generic env var editor ───────────────────────────────────────────────────

fn env_var_editor(ui: &mut egui::Ui, vars: &mut std::collections::HashMap<String, String>) {
    let mut to_remove: Option<String> = None;
    let mut new_key = String::new();
    let mut new_val = String::new();
    let mut keys: Vec<String> = vars.keys().cloned().collect();
    keys.sort();

    if keys.is_empty() {
        ui.label(egui::RichText::new(rust_i18n::t!("settings_no_variables")).size(12.0).color(egui::Color32::GRAY));
    }
    for key in &keys {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(key).monospace());
            ui.label("=");
            if let Some(val) = vars.get(key) {
                let mut v = val.clone();
                ui.add(egui::TextEdit::singleline(&mut v).desired_width(120.0));
                vars.insert(key.clone(), v);
            }
            if ui.small_button("\u{2715}").clicked() { to_remove = Some(key.clone()); }
        });
    }
    if let Some(key) = to_remove { vars.remove(&key); }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut new_key).hint_text("KEY").desired_width(80.0));
        ui.label("=");
        ui.add(egui::TextEdit::singleline(&mut new_val).hint_text("value").desired_width(100.0));
        if ui.button(rust_i18n::t!("add")).clicked() && !new_key.is_empty() {
            vars.insert(new_key.clone(), new_val.clone());
        }
    });
}

// ─── Language selector ────────────────────────────────────────────────────────

fn language_selector(ui: &mut egui::Ui, settings: &mut AppSettings) {
    row(ui, &rust_i18n::t!("language"), |ui| {
        egui::ComboBox::from_id_salt("language_selector")
            .selected_text(settings.language.label())
            .show_ui(ui, |ui| {
                for lang in Language::ALL {
                    let label = lang.label();
                    if ui.selectable_label(settings.language == lang, label).clicked() {
                        settings.language = lang;
                        lang.apply();
                    }
                }
            });
    });
}

// ─── UI theme selector ────────────────────────────────────────────────────────

fn ui_theme_selector(ui: &mut egui::Ui, settings: &mut AppSettings) {
    use crate::i18n::UiTheme;
    row(ui, &rust_i18n::t!("ui_theme"), |ui| {
        egui::ComboBox::from_id_salt("ui_theme_selector")
            .selected_text(settings.ui_theme.label())
            .show_ui(ui, |ui| {
                for theme in UiTheme::ALL {
                    let label = theme.label();
                    if ui.selectable_label(settings.ui_theme == theme, label).clicked() {
                        settings.ui_theme = theme;
                    }
                }
            });
    });
}
