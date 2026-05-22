pub mod transfer;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Key, Modifiers};

use crate::fs::entry_info;
use crate::fs::local;
use crate::fs::sftp::{join_remote, SftpClient};
use crate::fs::FileEntry;
use crate::session::{
    FileActivePane, FileClipboard, FileClipboardMode, FileManagerMode, FileManagerSession,
    InfoDialog, PaneState, RemotePane, RenameDialog,
};
use crate::ui::page::file_manager::transfer::{apply_transfer_done, PasteTarget};
use crate::ui::widget::sidebar::{Sidebar, SidebarPage};
use crate::ui::widget::style;

#[derive(Default)]
pub struct FileManagerAction {
    pub close: bool,
}

#[derive(Default)]
struct PaneOps {
    go_up: bool,
    open_index: Option<usize>,
    paste: bool,
    /// Leave multi-select mode and clear row highlights (bottom bar / Cancel).
    dismiss_multiselect: bool,
    /// Copy / cut / delete targets (bottom bar, keyboard, or context menu).
    bulk_copy: Option<Vec<usize>>,
    bulk_cut: Option<Vec<usize>>,
    bulk_delete: Option<Vec<usize>>,
    rename_index: Option<usize>,
    info_index: Option<usize>,
}

const BOTTOM_BAR_H: f32 = 40.0;
const CONTEXT_MENU_MIN_WIDTH: f32 = 140.0;
/// Right-click and touch long-press both open the same context menu.
fn install_context_menu(
    ui: &egui::Ui,
    resp: &egui::Response,
    mut build: impl FnMut(&mut egui::Ui),
) {
    let menu_id = resp.id.with("ctx_popup");
    resp.context_menu(|ui| build(ui));
    if resp.long_touched() {
        resp.ctx.memory_mut(|m| m.open_popup(menu_id));
    }
    let long_touch_open = resp
        .long_touched()
        .then_some(egui::SetOpenCommand::Bool(true));
    egui::Popup::from_response(resp)
        .id(menu_id)
        .open_memory(long_touch_open)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(CONTEXT_MENU_MIN_WIDTH);
            build(ui);
        });
}

pub fn file_manager_view(
    ui: &mut egui::Ui,
    session: &mut FileManagerSession,
    sidebar: &mut Sidebar,
) -> FileManagerAction {
    refresh_if_needed(session);
    if let Some(done) = session.transfer.poll(ui.ctx()) {
        apply_transfer_done(session, done);
    }
    // Keep sidebar path labels animating (marquee) while a file manager session is open.
    ui.ctx().request_repaint();

    let mut action = FileManagerAction::default();
    let has_clipboard = session.clipboard.is_some();
    let transfer_ui = session.transfer.read_ui();

    ui.horizontal(|ui| {
        if sidebar.show_content_hamburger(SidebarPage::Workspace)
            && sidebar.hamburger(ui).clicked()
        {
            sidebar.hamburger_click(SidebarPage::Workspace);
        }
        ui.label(
            egui::RichText::new(&session.title)
                .size(14.0)
                .strong()
                .color(ui.visuals().text_color()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if toolbar_button(ui, egui::RichText::new("✕").size(14.0).color(style::RED))
                .clicked()
            {
                action.close = true;
            }
            if !transfer_ui.active {
                if let Some(msg) = &session.status {
                    ui.label(egui::RichText::new(msg).small().weak());
                }
            }
            if transfer_ui.active {
                if ui.button("Stop").clicked() {
                    session.transfer.request_cancel();
                }
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_width(200.0);
                    ui.set_max_width(320.0);
                    ui.label(
                        egui::RichText::new(&transfer_ui.label)
                            .small()
                            .color(ui.visuals().text_color()),
                    );
                    ui.add(
                        egui::ProgressBar::new(transfer_ui.progress.clamp(0.0, 1.0))
                            .show_percentage()
                            .desired_width(ui.available_width()),
                    );
                });
            }
        });
    });
    ui.separator();

    let block_pane_keyboard = session.rename_dialog.open || session.info_dialog.open;

    if !block_pane_keyboard
        && !session.transfer.is_active()
        && ui.input(|i| i.key_pressed(Key::F5))
    {
        transfer_to_opposite_pane(session);
    }

    let available = ui.available_size();
    let pane_w = (available.x - 8.0) / 2.0;
    let pane_h = available.y;

    let pane_size = egui::vec2(pane_w, pane_h);
    ui.horizontal(|ui| {
        ui.set_min_height(pane_h);
        paint_pane_column(ui, pane_size, |ui| {
            match session.mode {
                FileManagerMode::SshSftp => {
                    if let Some(remote) = session.remote.as_mut() {
                        let (clicked, ops) = paint_remote_pane(
                            ui,
                            remote,
                            &mut session.remote_anchor,
                            &mut session.clipboard,
                            &mut session.status,
                            &mut session.rename_dialog,
                            &mut session.info_dialog,
                            "fm_scroll_remote",
                            has_clipboard,
                            block_pane_keyboard,
                            session.active_pane == FileActivePane::Remote,
                        );
                        if clicked {
                            session.active_pane = FileActivePane::Remote;
                        }
                        if ops.paste {
                            paste_into_pane(session, FileActivePane::Remote);
                        }
                    }
                }
                FileManagerMode::LocalDual => {
                    if let Some(left) = session.left_local.as_mut() {
                        let (clicked, ops) = paint_local_pane(
                            ui,
                            left,
                            FileActivePane::LeftLocal,
                            &mut session.local_anchor,
                            &mut session.clipboard,
                            &mut session.status,
                            &mut session.rename_dialog,
                            &mut session.info_dialog,
                            None,
                            "fm_scroll_left",
                            has_clipboard,
                            block_pane_keyboard,
                            session.active_pane == FileActivePane::LeftLocal,
                        );
                        if clicked {
                            session.active_pane = FileActivePane::LeftLocal;
                        }
                        if ops.paste {
                            paste_into_pane(session, FileActivePane::LeftLocal);
                        }
                    }
                }
            }
        });

        ui.add_space(8.0);

        paint_pane_column(ui, pane_size, |ui| {
            let remote_client = session.remote.as_ref().map(|r| &r.client);
            let (clicked, ops) = paint_local_pane(
                ui,
                &mut session.right,
                FileActivePane::Right,
                &mut session.right_anchor,
                &mut session.clipboard,
                &mut session.status,
                &mut session.rename_dialog,
                &mut session.info_dialog,
                remote_client,
                "fm_scroll_right",
                has_clipboard,
                block_pane_keyboard,
                session.active_pane == FileActivePane::Right,
            );
            if clicked {
                session.active_pane = FileActivePane::Right;
            }
            if ops.paste {
                paste_into_pane(session, FileActivePane::Right);
            }
        });
    });

    show_rename_dialog(ui.ctx(), session);
    show_info_dialog(ui.ctx(), session);

    action
}

/// Fixed-size column so the left pane cannot overlap the right pane and steal clicks.
fn paint_pane_column<R>(ui: &mut egui::Ui, size: egui::Vec2, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let rect = egui::Rect::from_min_size(ui.cursor().min, size);
    let _ = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), body)
        .inner
}

fn toolbar_button(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(
        egui::Button::new(label)
            .frame(false)
            .corner_radius(style::CORNER_RADIUS_XS),
    )
}

fn opposite_pane(active: FileActivePane, mode: FileManagerMode) -> FileActivePane {
    match mode {
        FileManagerMode::SshSftp => match active {
            FileActivePane::Remote => FileActivePane::Right,
            _ => FileActivePane::Remote,
        },
        FileManagerMode::LocalDual => match active {
            FileActivePane::LeftLocal => FileActivePane::Right,
            _ => FileActivePane::LeftLocal,
        },
    }
}

fn transfer_to_opposite_pane(session: &mut FileManagerSession) {
    let active = session.active_pane;
    copy_from_pane(session, active);
    let dest = opposite_pane(active, session.mode);
    paste_into_pane(session, dest);
    session.status = Some("Transferred to opposite pane".into());
}

fn copy_from_pane(session: &mut FileManagerSession, pane: FileActivePane) {
    let clip = match pane {
        FileActivePane::Remote => session.remote.as_ref().map(|remote| {
            let paths = selected_remote_paths(remote);
            (paths, true)
        }),
        FileActivePane::LeftLocal => session.left_local.as_ref().map(|left| {
            let paths: Vec<String> = selected_local_paths(left)
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            (paths, false)
        }),
        FileActivePane::Right => {
            let paths: Vec<String> = selected_local_paths(&session.right)
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            Some((paths, false))
        }
    };
    if let Some((paths, from_remote)) = clip.filter(|(p, _)| !p.is_empty()) {
        session.clipboard = Some(FileClipboard {
            mode: FileClipboardMode::Copy,
            from_remote,
            paths,
        });
    }
}

fn paste_into_pane(session: &mut FileManagerSession, pane: FileActivePane) {
    let Some(clip) = session.clipboard.clone() else {
        session.status = Some("Clipboard is empty".into());
        return;
    };
    if session.transfer.is_active() {
        session.status = Some("Transfer already in progress".into());
        return;
    }
    let remote_client = session.remote.as_ref().map(|r| Arc::clone(&r.client));
    match pane {
        FileActivePane::Remote => {
            let Some(remote) = session.remote.as_ref() else {
                return;
            };
            session.transfer.start_paste(
                PasteTarget::Remote,
                clip,
                None,
                Some(remote.cwd.clone()),
                remote_client,
            );
        }
        FileActivePane::LeftLocal => {
            let Some(left) = session.left_local.as_ref() else {
                return;
            };
            session.transfer.start_paste(
                PasteTarget::LocalLeft,
                clip,
                Some(left.cwd.clone()),
                None,
                remote_client,
            );
        }
        FileActivePane::Right => {
            session.transfer.start_paste(
                PasteTarget::LocalRight,
                clip,
                Some(session.right.cwd.clone()),
                None,
                remote_client,
            );
        }
    }
}

fn refresh_if_needed(session: &mut FileManagerSession) {
    if let Some(remote) = session.remote.as_mut() {
        if remote.loading {
            remote.loading = false;
            match remote.client.list_dir(&remote.cwd) {
                Ok(entries) => {
                    remote.entries = entries;
                    remote.error = None;
                }
                Err(e) => remote.error = Some(e),
            }
        }
    }
    if let Some(left) = session.left_local.as_mut() {
        refresh_local_pane(left);
    }
    refresh_local_pane(&mut session.right);
}

fn refresh_local_pane(pane: &mut PaneState) {
    if !pane.loading {
        return;
    }
    pane.loading = false;
    match local::list_dir(&pane.cwd) {
        Ok(entries) => {
            pane.entries = entries;
            pane.error = None;
        }
        Err(e) => pane.error = Some(e),
    }
}

fn paint_remote_pane(
    ui: &mut egui::Ui,
    remote: &mut RemotePane,
    anchor: &mut Option<usize>,
    clipboard: &mut Option<FileClipboard>,
    status: &mut Option<String>,
    rename_dialog: &mut RenameDialog,
    info_dialog: &mut InfoDialog,
    scroll_id: &str,
    has_clipboard: bool,
    block_keyboard: bool,
    is_active: bool,
) -> (bool, PaneOps) {
    let mut ops = PaneOps::default();
    let pane_focus_id = ui.id().with((scroll_id, "focus"));
    let mut list_clicked = false;

    ui.vertical(|ui| {
        if paint_pane_toolbar(ui, &remote.cwd, &mut remote.select_mode, &mut remote.selected) {
            ops.go_up = true;
        }
        if let Some(err) = &remote.error {
            ui.colored_label(egui::Color32::LIGHT_RED, err);
        }
        let show_bottom = remote.select_mode || has_clipboard;
        let bottom_h = if show_bottom { BOTTOM_BAR_H } else { 0.0 };
        let list_h = (ui.available_height() - bottom_h).max(32.0);
        list_clicked = paint_file_list_area(
            ui,
            pane_focus_id,
            scroll_id,
            list_h,
            remote.select_mode,
            has_clipboard,
            &mut ops,
            |ui, ops| {
                paint_remote_list(
                    ui,
                    pane_focus_id,
                    scroll_id,
                    remote,
                    anchor,
                    clipboard,
                    status,
                    block_keyboard,
                    is_active,
                    list_h,
                    ops,
                )
            },
        );
        paint_bottom_action_bar(
            ui,
            remote.select_mode,
            !remote.selected.is_empty(),
            has_clipboard,
            &mut remote.selected,
            clipboard,
            &mut ops,
        );
        run_remote_ops(
            ui,
            remote,
            clipboard,
            status,
            rename_dialog,
            info_dialog,
            &mut ops,
        );
    });

    (list_clicked, ops)
}

fn paint_local_pane(
    ui: &mut egui::Ui,
    pane: &mut PaneState,
    pane_side: FileActivePane,
    anchor: &mut Option<usize>,
    clipboard: &mut Option<FileClipboard>,
    status: &mut Option<String>,
    rename_dialog: &mut RenameDialog,
    info_dialog: &mut InfoDialog,
    remote_client: Option<&Arc<SftpClient>>,
    scroll_id: &str,
    has_clipboard: bool,
    block_keyboard: bool,
    is_active: bool,
) -> (bool, PaneOps) {
    let mut ops = PaneOps::default();
    let pane_focus_id = ui.id().with((scroll_id, "focus"));
    let cwd = pane.cwd.display().to_string();
    let mut list_clicked = false;

    ui.vertical(|ui| {
        if paint_pane_toolbar(ui, &cwd, &mut pane.select_mode, &mut pane.selected) {
            ops.go_up = true;
        }
        if let Some(err) = &pane.error {
            ui.colored_label(egui::Color32::LIGHT_RED, err);
        }
        let show_bottom = pane.select_mode || has_clipboard;
        let bottom_h = if show_bottom { BOTTOM_BAR_H } else { 0.0 };
        let list_h = (ui.available_height() - bottom_h).max(32.0);
        list_clicked = paint_file_list_area(
            ui,
            pane_focus_id,
            scroll_id,
            list_h,
            pane.select_mode,
            has_clipboard,
            &mut ops,
            |ui, ops| {
                paint_local_list(
                    ui,
                    pane_focus_id,
                    scroll_id,
                    pane,
                    anchor,
                    clipboard,
                    status,
                    block_keyboard,
                    is_active,
                    list_h,
                    ops,
                )
            },
        );
        paint_bottom_action_bar(
            ui,
            pane.select_mode,
            !pane.selected.is_empty(),
            has_clipboard,
            &mut pane.selected,
            clipboard,
            &mut ops,
        );
        run_local_ops(
            ui,
            pane,
            pane_side,
            clipboard,
            status,
            rename_dialog,
            info_dialog,
            &mut ops,
        );
    });

    (list_clicked, ops)
}

/// Normal mode: right-click empty list area → horizontal Paste menu only.
fn paint_blank_context_menu(ui: &mut egui::Ui, has_clipboard: bool, ops: &mut PaneOps) {
    paint_horizontal_context_menu(ui, |ui| {
        if has_clipboard {
            if ui.button("Paste").clicked() {
                ops.paste = true;
                ui.close();
            }
        } else {
            ui.label(egui::RichText::new("Clipboard empty").weak());
        }
    });
}

fn paint_horizontal_context_menu(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    ui.set_min_width(CONTEXT_MENU_MIN_WIDTH);
    ui.horizontal(|ui| {
        ui.style_mut().spacing.item_spacing.x = 8.0;
        content(ui);
    });
}

/// Multi-select on: Copy / Cut / Delete / Cancel — any click ends multi-select.
/// After clipboard filled: Paste / Cancel (Cancel clears clipboard).
fn paint_bottom_action_bar(
    ui: &mut egui::Ui,
    select_mode: bool,
    has_selection: bool,
    has_clipboard: bool,
    selected: &mut HashSet<usize>,
    clipboard: &mut Option<FileClipboard>,
    ops: &mut PaneOps,
) {
    if !select_mode && !has_clipboard {
        return;
    }
    ui.separator();
    ui.horizontal(|ui| {
        if select_mode {
            ui.add_enabled_ui(has_selection, |ui| {
                if ui.button("Copy").clicked() {
                    ops.bulk_copy = Some(selected.iter().copied().collect());
                    ops.dismiss_multiselect = true;
                }
                if ui.button("Cut").clicked() {
                    ops.bulk_cut = Some(selected.iter().copied().collect());
                    ops.dismiss_multiselect = true;
                }
                if ui.button(rust_i18n::t!("delete")).clicked() {
                    ops.bulk_delete = Some(selected.iter().copied().collect());
                    ops.dismiss_multiselect = true;
                }
            });
            if ui.button(rust_i18n::t!("cancel")).clicked() {
                ops.dismiss_multiselect = true;
            }
        } else if has_clipboard {
            if ui.button("Paste").clicked() {
                ops.paste = true;
            }
            if ui.button(rust_i18n::t!("cancel")).clicked() {
                *clipboard = None;
            }
        }
    });
}

/// List viewport: rows inside; blank right-click only in normal mode.
fn paint_file_list_area(
    ui: &mut egui::Ui,
    pane_focus_id: egui::Id,
    scroll_id: &str,
    list_h: f32,
    select_mode: bool,
    has_clipboard: bool,
    ops: &mut PaneOps,
    paint_list: impl FnOnce(&mut egui::Ui, &mut PaneOps) -> bool,
) -> bool {
    let list_size = egui::vec2(ui.available_width(), list_h);
    let (list_rect, list_bg) = ui.allocate_exact_size(list_size, egui::Sense::click());
    if !select_mode {
        install_context_menu(ui, &list_bg, |ui| {
            paint_blank_context_menu(ui, has_clipboard, ops);
        });
    }

    let mut interacted = ui
        .allocate_new_ui(egui::UiBuilder::new().max_rect(list_rect), |ui| {
            paint_list(ui, ops)
        })
        .inner;

    if list_bg.clicked_by(egui::PointerButton::Primary) {
        interacted = true;
        ui.memory_mut(|m| m.request_focus(pane_focus_id));
    }

    interacted
}

fn paint_remote_list(
    ui: &mut egui::Ui,
    pane_focus_id: egui::Id,
    scroll_id: &str,
    remote: &mut RemotePane,
    anchor: &mut Option<usize>,
    clipboard: &mut Option<FileClipboard>,
    status: &mut Option<String>,
    block_keyboard: bool,
    is_active: bool,
    list_max_height: f32,
    ops: &mut PaneOps,
) -> bool {
    let entries = remote.entries.clone();
    let mut interacted = false;

    let _scroll = egui::ScrollArea::vertical()
        .id_salt(scroll_id)
        .max_height(list_max_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if remote.loading {
                ui.label(egui::RichText::new("Loading…").weak());
                return;
            }
            if entries.is_empty() {
                ui.label(egui::RichText::new("(empty folder)").weak());
                return;
            }
            for (i, ent) in entries.iter().enumerate() {
                let focused = ui.memory(|m| m.has_focus(pane_focus_id)) || is_active;
                let is_sel = remote.selected.contains(&i);
                let is_focus = remote.focus_index == Some(i);
                let label = entry_label(ent, is_focus && focused);
                let resp = ui.selectable_label(is_sel, label);
                install_context_menu(ui, &resp, |ui| {
                    row_context_menu_remote(ui, remote, i, ent, ops);
                });
                if resp.double_clicked() && ent.is_dir {
                    ops.open_index = Some(i);
                    continue;
                }
                if resp.clicked_by(egui::PointerButton::Primary) {
                    interacted = true;
                    ui.memory_mut(|m| m.request_focus(pane_focus_id));
                    let mods = resp.ctx.input(|inp| inp.modifiers);
                    apply_selection_click(
                        &mut remote.selected,
                        &mut remote.focus_index,
                        anchor,
                        remote.select_mode,
                        i,
                        mods,
                    );
                }
            }
        });

    let focused = ui.memory(|m| m.has_focus(pane_focus_id)) || is_active;
    if focused && !block_keyboard {
        handle_list_keyboard(
            ui,
            &remote.entries,
            &mut remote.selected,
            &mut remote.focus_index,
            remote.select_mode,
            anchor,
            ops,
        );
    }

    interacted
}

fn paint_local_list(
    ui: &mut egui::Ui,
    pane_focus_id: egui::Id,
    scroll_id: &str,
    pane: &mut PaneState,
    anchor: &mut Option<usize>,
    clipboard: &mut Option<FileClipboard>,
    status: &mut Option<String>,
    block_keyboard: bool,
    is_active: bool,
    list_max_height: f32,
    ops: &mut PaneOps,
) -> bool {
    let entries = pane.entries.clone();
    let mut interacted = false;

    let _scroll = egui::ScrollArea::vertical()
        .id_salt(scroll_id)
        .max_height(list_max_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if pane.loading {
                ui.label(egui::RichText::new("Loading…").weak());
                return;
            }
            if entries.is_empty() {
                ui.label(egui::RichText::new("(empty folder)").weak());
                return;
            }
            for (i, ent) in entries.iter().enumerate() {
                let focused = ui.memory(|m| m.has_focus(pane_focus_id)) || is_active;
                let is_sel = pane.selected.contains(&i);
                let is_focus = pane.focus_index == Some(i);
                let label = entry_label(ent, is_focus && focused);
                let resp = ui.selectable_label(is_sel, label);
                install_context_menu(ui, &resp, |ui| {
                    row_context_menu_local(ui, pane, i, ent, ops);
                });
                if resp.double_clicked() && ent.is_dir {
                    ops.open_index = Some(i);
                    continue;
                }
                if resp.clicked_by(egui::PointerButton::Primary) {
                    interacted = true;
                    ui.memory_mut(|m| m.request_focus(pane_focus_id));
                    let mods = resp.ctx.input(|inp| inp.modifiers);
                    apply_selection_click(
                        &mut pane.selected,
                        &mut pane.focus_index,
                        anchor,
                        pane.select_mode,
                        i,
                        mods,
                    );
                }
            }
        });

    let focused = ui.memory(|m| m.has_focus(pane_focus_id)) || is_active;
    if focused && !block_keyboard {
        handle_list_keyboard(
            ui,
            &pane.entries,
            &mut pane.selected,
            &mut pane.focus_index,
            pane.select_mode,
            anchor,
            ops,
        );
    }

    interacted
}

fn handle_list_keyboard(
    ui: &egui::Ui,
    entries: &[FileEntry],
    selected: &mut HashSet<usize>,
    focus_index: &mut Option<usize>,
    select_mode: bool,
    anchor: &mut Option<usize>,
    ops: &mut PaneOps,
) {
    if ui.ctx().wants_keyboard_input() {
        return;
    }

    let len = entries.len();
    if len == 0 {
        return;
    }

    let input = ui.input(|inp| inp.clone());

    if input.key_pressed(Key::A) && input.modifiers.ctrl {
        selected.clear();
        for i in 0..len {
            selected.insert(i);
        }
        *focus_index = Some(0);
        *anchor = Some(0);
        return;
    }

    if input.key_pressed(Key::C) && input.modifiers.ctrl {
        let indices: Vec<usize> = selected.iter().copied().collect();
        if !indices.is_empty() {
            ops.bulk_copy = Some(indices);
            if select_mode {
                ops.dismiss_multiselect = true;
            }
        }
        return;
    }
    if input.key_pressed(Key::X) && input.modifiers.ctrl {
        let indices: Vec<usize> = selected.iter().copied().collect();
        if !indices.is_empty() {
            ops.bulk_cut = Some(indices);
            if select_mode {
                ops.dismiss_multiselect = true;
            }
        }
        return;
    }
    if input.key_pressed(Key::V) && input.modifiers.ctrl {
        ops.paste = true;
        return;
    }
    if input.key_pressed(Key::Delete) {
        let indices: Vec<usize> = selected.iter().copied().collect();
        if !indices.is_empty() {
            ops.bulk_delete = Some(indices);
            if select_mode {
                ops.dismiss_multiselect = true;
            }
        }
        return;
    }

    if input.key_pressed(Key::Backspace) || input.key_pressed(Key::ArrowLeft) {
        ops.go_up = true;
        return;
    }

    if input.key_pressed(Key::Space) && select_mode {
        if let Some(idx) = *focus_index {
            toggle_index(selected, idx);
            *anchor = Some(idx);
        }
        return;
    }

    if input.key_pressed(Key::ArrowRight) || input.key_pressed(Key::Enter) {
        if let Some(idx) = *focus_index {
            if entries.get(idx).is_some_and(|e| e.is_dir) {
                ops.open_index = Some(idx);
            }
        }
        return;
    }

    let delta = if input.key_pressed(Key::ArrowDown) {
        1
    } else if input.key_pressed(Key::ArrowUp) {
        -1
    } else {
        return;
    };

    let next = match *focus_index {
        Some(i) => (i as i32 + delta).clamp(0, len as i32 - 1) as usize,
        None => if delta > 0 { 0 } else { len - 1 },
    };

    if input.modifiers.shift {
        let a = anchor.unwrap_or(next);
        selected.clear();
        let lo = a.min(next);
        let hi = a.max(next);
        for i in lo..=hi {
            selected.insert(i);
        }
    } else if !select_mode {
        selected.clear();
        selected.insert(next);
        *anchor = Some(next);
    } else {
        *anchor = Some(next);
    }
    *focus_index = Some(next);
}

/// Left-click only; right-click is handled solely by context menus.
fn apply_selection_click(
    selected: &mut HashSet<usize>,
    focus_index: &mut Option<usize>,
    anchor: &mut Option<usize>,
    select_mode: bool,
    idx: usize,
    mods: Modifiers,
) {
    *focus_index = Some(idx);

    if mods.shift {
        if let Some(a) = *anchor {
            selected.clear();
            let lo = a.min(idx);
            let hi = a.max(idx);
            for i in lo..=hi {
                selected.insert(i);
            }
        } else {
            selected.clear();
            selected.insert(idx);
            *anchor = Some(idx);
        }
        return;
    }

    if mods.ctrl {
        toggle_index(selected, idx);
        *anchor = Some(idx);
        return;
    }

    if select_mode {
        toggle_index(selected, idx);
        *anchor = Some(idx);
        return;
    }

    selected.clear();
    selected.insert(idx);
    *anchor = Some(idx);
}

fn toggle_index(selected: &mut HashSet<usize>, idx: usize) {
    if selected.contains(&idx) {
        selected.remove(&idx);
    } else {
        selected.insert(idx);
    }
}

fn dismiss_multiselect_local(pane: &mut PaneState) {
    pane.select_mode = false;
    pane.selected.clear();
}

fn dismiss_multiselect_remote(remote: &mut RemotePane) {
    remote.select_mode = false;
    remote.selected.clear();
}

fn run_local_ops(
    _ui: &mut egui::Ui,
    pane: &mut PaneState,
    pane_side: FileActivePane,
    clipboard: &mut Option<FileClipboard>,
    status: &mut Option<String>,
    rename_dialog: &mut RenameDialog,
    info_dialog: &mut InfoDialog,
    ops: &mut PaneOps,
) {
    if ops.go_up {
        parent_local(pane);
        pane.loading = true;
    }
    if let Some(i) = ops.open_index.take() {
        open_local_entry(pane, i);
    }

    if let Some(indices) = ops.bulk_copy.take() {
        copy_local_indices(pane, &indices, clipboard, status);
    }
    if let Some(indices) = ops.bulk_cut.take() {
        cut_local_indices(pane, &indices, clipboard, status);
    }
    if let Some(indices) = ops.bulk_delete.take() {
        delete_local_indices(pane, &indices, status);
    }

    if let Some(idx) = ops.rename_index.take() {
        if let Some(ent) = pane.entries.get(idx) {
            rename_dialog.open_for(pane_side, &ent.name);
        }
    }

    if let Some(idx) = ops.info_index.take() {
        if let Some(ent) = pane.entries.get(idx) {
            let path = local::join_path(&pane.cwd, &ent.name);
            match entry_info::local_entry_info(&path) {
                Ok(info) => info_dialog.show(info),
                Err(e) => *status = Some(e),
            }
        }
    }

    if ops.dismiss_multiselect {
        dismiss_multiselect_local(pane);
    }
}

fn run_remote_ops(
    _ui: &mut egui::Ui,
    remote: &mut RemotePane,
    clipboard: &mut Option<FileClipboard>,
    status: &mut Option<String>,
    rename_dialog: &mut RenameDialog,
    info_dialog: &mut InfoDialog,
    ops: &mut PaneOps,
) {
    if ops.go_up {
        parent_remote(remote);
        remote.loading = true;
    }
    if let Some(i) = ops.open_index.take() {
        open_remote_entry(remote, i);
    }

    if let Some(indices) = ops.bulk_copy.take() {
        copy_remote_indices(remote, &indices, clipboard, status);
    }
    if let Some(indices) = ops.bulk_cut.take() {
        cut_remote_indices(remote, &indices, clipboard, status);
    }
    if let Some(indices) = ops.bulk_delete.take() {
        delete_remote_indices(remote, &indices, status);
    }

    if let Some(idx) = ops.rename_index.take() {
        if let Some(ent) = remote.entries.get(idx) {
            rename_dialog.open_for(FileActivePane::Remote, &ent.name);
        }
    }

    if let Some(idx) = ops.info_index.take() {
        if let Some(ent) = remote.entries.get(idx) {
            let path = join_remote(&remote.cwd, &ent.name);
            match remote.client.entry_info(&path) {
                Ok(info) => info_dialog.show(info),
                Err(e) => *status = Some(e),
            }
        }
    }

    if ops.dismiss_multiselect {
        dismiss_multiselect_remote(remote);
    }
}

fn show_info_dialog(ctx: &egui::Context, session: &mut FileManagerSession) {
    if !session.info_dialog.open {
        return;
    }

    let mut close = false;
    egui::Window::new("Info")
        .collapsible(false)
        .resizable(true)
        .default_width(420.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::Grid::new("file_info_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    for crate::session::InfoLine(key, value) in &session.info_dialog.lines {
                        ui.label(egui::RichText::new(key).strong());
                        ui.label(value);
                        ui.end_row();
                    }
                });
            ui.add_space(12.0);
            if ui.button(rust_i18n::t!("close")).clicked() {
                close = true;
            }
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                close = true;
            }
        });

    if close {
        session.info_dialog.open = false;
    }
}

fn show_rename_dialog(ctx: &egui::Context, session: &mut FileManagerSession) {
    if !session.rename_dialog.open {
        return;
    }

    let mut close = false;
    let mut confirm = false;

    egui::Window::new("Rename")
        .collapsible(false)
        .resizable(false)
        .default_width(360.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!("Original: {}", session.rename_dialog.old_name()));
            ui.add_space(6.0);
            ui.label("New name:");
            let name_edit = ui.text_edit_singleline(&mut session.rename_dialog.new_name);
            name_edit.request_focus();
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(rust_i18n::t!("cancel")).clicked() {
                    close = true;
                }
                if ui.button("Confirm").clicked() {
                    confirm = true;
                }
            });
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                close = true;
            }
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                confirm = true;
            }
        });

    if close {
        session.rename_dialog.open = false;
        return;
    }

    if confirm {
        let pane = session.rename_dialog.pane;
        let old_name = session.rename_dialog.old_name().to_string();
        let new_name = session.rename_dialog.new_name.trim().to_string();
        match apply_rename(session, pane, &old_name, &new_name) {
            Ok(()) => {
                session.status = Some(format!("Renamed \"{old_name}\" → \"{new_name}\""));
                session.rename_dialog.open = false;
            }
            Err(e) => session.status = Some(e),
        }
    }
}

fn apply_rename(
    session: &mut FileManagerSession,
    pane: FileActivePane,
    old_name: &str,
    new_name: &str,
) -> Result<(), String> {
    match pane {
        FileActivePane::Remote => {
            let remote = session.remote.as_mut().ok_or("No remote pane")?;
            let from = join_remote(&remote.cwd, old_name);
            let to = join_remote(&remote.cwd, new_name);
            remote.client.rename(&from, &to)?;
            remote.loading = true;
        }
        FileActivePane::LeftLocal => {
            let pane = session.left_local.as_mut().ok_or("No left pane")?;
            local::rename_entry(&pane.cwd, old_name, new_name)?;
            pane.loading = true;
        }
        FileActivePane::Right => {
            local::rename_entry(&session.right.cwd, old_name, new_name)?;
            session.right.loading = true;
        }
    }
    Ok(())
}

fn open_local_entry(pane: &mut PaneState, idx: usize) {
    let Some(ent) = pane.entries.get(idx) else {
        return;
    };
    if ent.is_dir {
        pane.cwd = local::join_path(&pane.cwd, &ent.name);
        pane.loading = true;
        pane.selected.clear();
        pane.focus_index = None;
    }
}

fn open_remote_entry(remote: &mut RemotePane, idx: usize) {
    let Some(ent) = remote.entries.get(idx) else {
        return;
    };
    if ent.is_dir {
        remote.cwd = join_remote(&remote.cwd, &ent.name);
        remote.loading = true;
        remote.selected.clear();
        remote.focus_index = None;
    }
}

fn paint_pane_toolbar(
    ui: &mut egui::Ui,
    cwd: &str,
    select_mode: &mut bool,
    selected: &mut HashSet<usize>,
) -> bool {
    let mut go_up = false;
    ui.horizontal(|ui| {
        if ui.small_button("↑").on_hover_text("Parent folder").clicked() {
            go_up = true;
        }
        ui.label(egui::RichText::new(cwd).small().weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.checkbox(select_mode, "Multi-select").changed() && !*select_mode {
                selected.clear();
            }
        });
    });
    go_up
}

/// Context menu targets: all selected rows when any, else the right-clicked row only.
fn indices_for_context_action(selected: &HashSet<usize>, right_clicked: usize) -> Vec<usize> {
    if selected.is_empty() {
        vec![right_clicked]
    } else {
        selected.iter().copied().collect()
    }
}

fn entry_label(ent: &FileEntry, focused: bool) -> String {
    let icon = if ent.is_dir { "📁" } else { "📄" };
    let name = if focused {
        format!("▸ {icon} {}", ent.name)
    } else {
        format!("{icon} {}", ent.name)
    };
    name
}

fn row_context_menu_local(
    ui: &mut egui::Ui,
    pane: &PaneState,
    idx: usize,
    ent: &FileEntry,
    ops: &mut PaneOps,
) {
    let in_multiselect = pane.select_mode;
    paint_horizontal_context_menu(ui, |ui| {
        if ent.is_dir && ui.button("Open").clicked() {
            ops.open_index = Some(idx);
            if in_multiselect {
                ops.dismiss_multiselect = true;
            }
            ui.close();
            return;
        }
        if ui.button("Copy").clicked() {
            ops.bulk_copy = Some(indices_for_context_action(&pane.selected, idx));
            if in_multiselect {
                ops.dismiss_multiselect = true;
            }
            ui.close();
        }
        if ui.button("Cut").clicked() {
            ops.bulk_cut = Some(indices_for_context_action(&pane.selected, idx));
            if in_multiselect {
                ops.dismiss_multiselect = true;
            }
            ui.close();
        }
        if ui.button(rust_i18n::t!("delete")).clicked() {
            ops.bulk_delete = Some(indices_for_context_action(&pane.selected, idx));
            if in_multiselect {
                ops.dismiss_multiselect = true;
            }
            ui.close();
        }
        if ui.button("Rename").clicked() {
            ops.rename_index = Some(idx);
            ui.close();
        }
        if ui.button("Info").clicked() {
            ops.info_index = Some(idx);
            ui.close();
        }
    });
}

fn row_context_menu_remote(
    ui: &mut egui::Ui,
    remote: &RemotePane,
    idx: usize,
    ent: &FileEntry,
    ops: &mut PaneOps,
) {
    let in_multiselect = remote.select_mode;
    paint_horizontal_context_menu(ui, |ui| {
        if ent.is_dir && ui.button("Open").clicked() {
            ops.open_index = Some(idx);
            if in_multiselect {
                ops.dismiss_multiselect = true;
            }
            ui.close();
            return;
        }
        if ui.button("Copy").clicked() {
            ops.bulk_copy = Some(indices_for_context_action(&remote.selected, idx));
            if in_multiselect {
                ops.dismiss_multiselect = true;
            }
            ui.close();
        }
        if ui.button("Cut").clicked() {
            ops.bulk_cut = Some(indices_for_context_action(&remote.selected, idx));
            if in_multiselect {
                ops.dismiss_multiselect = true;
            }
            ui.close();
        }
        if ui.button(rust_i18n::t!("delete")).clicked() {
            ops.bulk_delete = Some(indices_for_context_action(&remote.selected, idx));
            if in_multiselect {
                ops.dismiss_multiselect = true;
            }
            ui.close();
        }
        if ui.button("Rename").clicked() {
            ops.rename_index = Some(idx);
            ui.close();
        }
        if ui.button("Info").clicked() {
            ops.info_index = Some(idx);
            ui.close();
        }
    });
}

fn parent_local(pane: &mut PaneState) {
    if let Some(parent) = pane.cwd.parent() {
        pane.cwd = parent.to_path_buf();
        pane.selected.clear();
        pane.focus_index = None;
    }
}

fn parent_remote(remote: &mut RemotePane) {
    let p = Path::new(&remote.cwd);
    if let Some(parent) = p.parent() {
        remote.cwd = if parent.as_os_str().is_empty() {
            "/".to_string()
        } else {
            parent.to_string_lossy().into_owned()
        };
        remote.selected.clear();
        remote.focus_index = None;
    }
}

fn selected_local_paths(pane: &PaneState) -> Vec<PathBuf> {
    pane.selected
        .iter()
        .filter_map(|&i| pane.entries.get(i))
        .map(|e| local::join_path(&pane.cwd, &e.name))
        .collect()
}

fn selected_remote_paths(remote: &RemotePane) -> Vec<String> {
    remote
        .selected
        .iter()
        .filter_map(|&i| remote.entries.get(i))
        .map(|e| join_remote(&remote.cwd, &e.name))
        .collect()
}

fn local_paths_for_indices(pane: &PaneState, indices: &[usize]) -> Vec<String> {
    indices
        .iter()
        .filter_map(|&i| pane.entries.get(i))
        .map(|e| local::join_path(&pane.cwd, &e.name).to_string_lossy().into_owned())
        .collect()
}

fn remote_paths_for_indices(remote: &RemotePane, indices: &[usize]) -> Vec<String> {
    indices
        .iter()
        .filter_map(|&i| remote.entries.get(i))
        .map(|e| join_remote(&remote.cwd, &e.name))
        .collect()
}

fn cut_local_indices(
    pane: &PaneState,
    indices: &[usize],
    clipboard: &mut Option<FileClipboard>,
    status: &mut Option<String>,
) {
    let paths = local_paths_for_indices(pane, indices);
    if paths.is_empty() {
        *status = Some("No items".into());
        return;
    }
    let n = paths.len();
    *clipboard = Some(FileClipboard {
        mode: FileClipboardMode::Cut,
        from_remote: false,
        paths,
    });
    *status = Some(format!("Cut {n} item(s)"));
}

fn copy_local_indices(
    pane: &PaneState,
    indices: &[usize],
    clipboard: &mut Option<FileClipboard>,
    status: &mut Option<String>,
) {
    let paths = local_paths_for_indices(pane, indices);
    if paths.is_empty() {
        *status = Some("No items".into());
        return;
    }
    let n = paths.len();
    *clipboard = Some(FileClipboard {
        mode: FileClipboardMode::Copy,
        from_remote: false,
        paths,
    });
    *status = Some(format!("Copied {n} item(s)"));
}

fn cut_remote_indices(
    remote: &RemotePane,
    indices: &[usize],
    clipboard: &mut Option<FileClipboard>,
    status: &mut Option<String>,
) {
    let paths = remote_paths_for_indices(remote, indices);
    if paths.is_empty() {
        *status = Some("No items".into());
        return;
    }
    let n = paths.len();
    *clipboard = Some(FileClipboard {
        mode: FileClipboardMode::Cut,
        from_remote: true,
        paths,
    });
    *status = Some(format!("Cut {n} item(s)"));
}

fn copy_remote_indices(
    remote: &RemotePane,
    indices: &[usize],
    clipboard: &mut Option<FileClipboard>,
    status: &mut Option<String>,
) {
    let paths = remote_paths_for_indices(remote, indices);
    if paths.is_empty() {
        *status = Some("No items".into());
        return;
    }
    let n = paths.len();
    *clipboard = Some(FileClipboard {
        mode: FileClipboardMode::Copy,
        from_remote: true,
        paths,
    });
    *status = Some(format!("Copied {n} item(s)"));
}

fn delete_local_indices(pane: &mut PaneState, indices: &[usize], status: &mut Option<String>) {
    let paths: Vec<PathBuf> = indices
        .iter()
        .filter_map(|&i| pane.entries.get(i))
        .map(|e| local::join_path(&pane.cwd, &e.name))
        .collect();
    if paths.is_empty() {
        *status = Some("No items".into());
        return;
    }
    let mut errors = Vec::new();
    for p in &paths {
        if let Err(e) = local::remove_path(p) {
            errors.push(e);
        }
    }
    if errors.is_empty() {
        *status = Some(format!("Deleted {} item(s)", paths.len()));
        pane.loading = true;
    } else {
        *status = Some(errors.join("; "));
    }
}

fn delete_remote_indices(remote: &mut RemotePane, indices: &[usize], status: &mut Option<String>) {
    let paths = remote_paths_for_indices(remote, indices);
    if paths.is_empty() {
        *status = Some("No items".into());
        return;
    }
    let mut errors = Vec::new();
    for path in &paths {
        let is_dir = remote
            .entries
            .iter()
            .filter(|e| e.is_dir)
            .any(|e| join_remote(&remote.cwd, &e.name) == *path);
        let err = if is_dir {
            remote.client.remove(path, true)
        } else {
            remote.client.remove(path, false)
        };
        if let Err(e) = err {
            errors.push(e);
        }
    }
    if errors.is_empty() {
        *status = Some(format!("Deleted {} item(s)", paths.len()));
        remote.loading = true;
    } else {
        *status = Some(errors.join("; "));
    }
}

