use crate::settings::{AppSettings, Profile};
use crate::ui::keyboard::KeyboardMode;

/// Home screen: full-page settings with Back.
pub fn settings_page(ui: &mut egui::Ui, settings: &mut AppSettings) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        if ui.button("← Back").clicked() {
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
        ui.label(egui::RichText::new("Settings").size(16.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new("\u{2715}").small())
                .on_hover_text("Close")
                .clicked()
            {
                close = true;
            }
        });
    });
    ui.label(
        egui::RichText::new("终端在后台继续运行")
            .size(11.0)
            .color(egui::Color32::GRAY),
    );
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);
    settings_scroll_body(ui, settings, SettingsLayout::Workspace);
    close
}

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

pub fn settings_page_body(ui: &mut egui::Ui, settings: &mut AppSettings) {
    settings_section(
        ui,
        "配置文件",
        "管理终端主题与键盘等个性化方案（后续支持导入/导出）",
        |ui| {
            ui.horizontal(|ui| {
                ui.label("当前默认：");
                ui.label(
                    egui::RichText::new(&settings.default_profile_name)
                        .strong()
                        .size(14.0),
                );
            });
            ui.add_space(6.0);
            profile_list_editor(ui, settings);
        },
    );

    settings_section(ui, "外观", "字体、滚动缓冲与默认键盘", |ui| {
        let profile = settings
            .profiles
            .iter_mut()
            .find(|p| p.name == settings.default_profile_name);

        if let Some(profile) = profile {
            labeled_row(ui, "字体大小", |ui| {
                ui.add(egui::Slider::new(&mut profile.font_size, 8.0..=32.0).show_value(true));
            });
            labeled_row(ui, "回滚行数", |ui| {
                ui.add(
                    egui::Slider::new(&mut profile.scrollback_lines, 100..=100_000)
                        .logarithmic(true)
                        .show_value(true),
                );
            });
            ui.add_space(4.0);
            ui.label(egui::RichText::new("默认键盘").size(12.0).weak());
            ui.horizontal(|ui| {
                ui.radio_value(&mut profile.keyboard_mode, KeyboardMode::Full, "全键盘");
                ui.radio_value(
                    &mut profile.keyboard_mode,
                    KeyboardMode::Special,
                    "功能键",
                );
            });

            ui.add_space(8.0);
            egui::CollapsingHeader::new("主题颜色")
                .id_salt("theme_colors")
                .default_open(false)
                .show(ui, |ui| {
                    theme_colors_editor(ui, profile);
                });
        } else {
            ui.colored_label(egui::Color32::YELLOW, "未找到默认配置文件");
        }
    });

    settings_section(ui, "SSH 环境变量", "新建 SSH 连接时注入的默认环境", |ui| {
        ssh_env_editor(ui, settings);
    });
}

fn settings_section(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    add_body: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(title).size(14.0).strong());
            ui.label(egui::RichText::new(subtitle).size(11.0).weak());
            ui.add_space(10.0);
            add_body(ui);
        });
    ui.add_space(10.0);
}

fn labeled_row(ui: &mut egui::Ui, label: &str, add_widget: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(13.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add_widget);
    });
    ui.add_space(6.0);
}

fn profile_list_editor(ui: &mut egui::Ui, settings: &mut AppSettings) {
    let names: Vec<String> = settings.profiles.iter().map(|p| p.name.clone()).collect();
    for name in &names {
        let is_default = *name == settings.default_profile_name;
        ui.horizontal(|ui| {
            if is_default {
                ui.label(egui::RichText::new(format!("● {name}")).strong());
            } else {
                ui.label(name.as_str());
                if ui.small_button("设为默认").clicked() {
                    settings.default_profile_name = name.clone();
                }
                if ui.small_button("删除").clicked() {
                    settings.profiles.retain(|p| p.name != *name);
                }
            }
        });
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let mut new_name = String::new();
        ui.add(
            egui::TextEdit::singleline(&mut new_name)
                .hint_text("新配置名称…")
                .desired_width(140.0),
        );
        if ui.button("新建配置").clicked() && !new_name.is_empty() {
            let mut p = Profile::default();
            p.name = new_name;
            settings.profiles.push(p);
        }
    });
}

fn theme_colors_editor(ui: &mut egui::Ui, profile: &mut crate::settings::Profile) {
    ui.label(egui::RichText::new("基础").size(12.0).weak());
    color_edit(ui, "背景", &mut profile.theme.bg);
    color_edit(ui, "前景", &mut profile.theme.fg);
    color_edit(ui, "光标", &mut profile.theme.cursor);
    color_edit(ui, "选中", &mut profile.theme.selection);
    ui.add_space(6.0);
    ui.label(egui::RichText::new("标准色").size(12.0).weak());
    color_edit(ui, "黑", &mut profile.theme.black);
    color_edit(ui, "红", &mut profile.theme.red);
    color_edit(ui, "绿", &mut profile.theme.green);
    color_edit(ui, "黄", &mut profile.theme.yellow);
    color_edit(ui, "蓝", &mut profile.theme.blue);
    color_edit(ui, "洋红", &mut profile.theme.magenta);
    color_edit(ui, "青", &mut profile.theme.cyan);
    color_edit(ui, "白", &mut profile.theme.white);
    ui.add_space(6.0);
    ui.label(egui::RichText::new("高亮").size(12.0).weak());
    color_edit(ui, "亮黑", &mut profile.theme.bright_black);
    color_edit(ui, "亮红", &mut profile.theme.bright_red);
    color_edit(ui, "亮绿", &mut profile.theme.bright_green);
    color_edit(ui, "亮黄", &mut profile.theme.bright_yellow);
    color_edit(ui, "亮蓝", &mut profile.theme.bright_blue);
    color_edit(ui, "亮洋红", &mut profile.theme.bright_magenta);
    color_edit(ui, "亮青", &mut profile.theme.bright_cyan);
    color_edit(ui, "亮白", &mut profile.theme.bright_white);
}

fn ssh_env_editor(ui: &mut egui::Ui, settings: &mut AppSettings) {
    let mut to_remove: Option<String> = None;
    let mut new_key = String::new();
    let mut new_val = String::new();

    let mut keys: Vec<String> = settings.ssh_env_vars.keys().cloned().collect();
    keys.sort();

    if keys.is_empty() {
        ui.label(
            egui::RichText::new("暂无变量")
                .size(12.0)
                .color(egui::Color32::GRAY),
        );
    }

    for key in &keys {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(key).monospace());
            ui.label("=");
            if let Some(val) = settings.ssh_env_vars.get(key) {
                let mut v = val.clone();
                ui.add(egui::TextEdit::singleline(&mut v).desired_width(120.0));
                settings.ssh_env_vars.insert(key.clone(), v);
            }
            if ui.small_button("\u{2715}").clicked() {
                to_remove = Some(key.clone());
            }
        });
    }

    if let Some(key) = to_remove {
        settings.ssh_env_vars.remove(&key);
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut new_key)
                .hint_text("KEY")
                .desired_width(80.0),
        );
        ui.label("=");
        ui.add(
            egui::TextEdit::singleline(&mut new_val)
                .hint_text("value")
                .desired_width(100.0),
        );
        if ui.button("添加").clicked() && !new_key.is_empty() {
            settings.ssh_env_vars.insert(new_key.clone(), new_val.clone());
        }
    });
}

fn color_edit(ui: &mut egui::Ui, label: &str, color: &mut egui::Color32) {
    ui.horizontal(|ui| {
        ui.label(label);
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
