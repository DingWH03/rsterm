//! Keyboard routing for the terminal surface (PTY shortcuts vs egui defaults).

use egui::{Context, Event, EventFilter, Id, Key, Modifiers, Sense, Ui, Vec2};

use crate::ui::clipboard::read_text;
use crate::ui::keyboard::ctrl_byte_for_char;

/// Stable focus id (must not depend on parent `Ui` id).
pub fn terminal_widget_id() -> Id {
    Id::new("rsterm_terminal_surface")
}

/// Inset between the panel edge and the cell grid (PTY size uses the inner area too).
pub const TERMINAL_GRID_MARGIN: f32 = 4.0;

/// Prevent arrow/tab/escape from moving egui focus away from the terminal.
pub fn terminal_event_filter() -> EventFilter {
    EventFilter {
        tab: true,
        horizontal_arrows: true,
        vertical_arrows: true,
        escape: true,
    }
}

/// Reserve the full panel, place the cell grid centered inside it, and return both rects.
///
/// `allocate_exact_size` alone uses an auto-generated id, so `set_focus_lock_filter` would
/// never match the focused widget and arrow keys steal focus after the first press.
pub fn allocate_terminal_surface(
    ui: &mut Ui,
    available: Vec2,
    grid_size: Vec2,
    sense: Sense,
) -> (egui::Rect, egui::Rect, egui::Response) {
    let id = terminal_widget_id();
    let (_, panel_rect) = ui.allocate_space(available);
    let inner = panel_rect.shrink2(egui::vec2(
        TERMINAL_GRID_MARGIN,
        TERMINAL_GRID_MARGIN,
    ));
    let grid_size = Vec2::new(
        grid_size.x.min(inner.width()),
        grid_size.y.min(inner.height()),
    );
    // Top-align like a real terminal (centering left a gap and clipped the first row in TUIs).
    let x = inner.left() + ((inner.width() - grid_size.x) * 0.5).max(0.0);
    let grid_rect = egui::Rect::from_min_size(egui::pos2(x, inner.top()), grid_size);
    let response = ui.interact(grid_rect, id, sense);
    (panel_rect, grid_rect, response)
}

/// Show/hide the Android soft keyboard (winit maps `IMEAllowed` → `show_soft_input`).
#[cfg(target_os = "android")]
pub fn sync_android_soft_input(ctx: &Context, enable: bool, ime_area: egui::Rect) {
    use egui::viewport::{IMEPurpose, ViewportCommand};
    if enable {
        ctx.send_viewport_cmd(ViewportCommand::IMERect(ime_area));
        ctx.send_viewport_cmd(ViewportCommand::IMEPurpose(IMEPurpose::Terminal));
        ctx.send_viewport_cmd(ViewportCommand::IMEAllowed(true));
    } else {
        ctx.send_viewport_cmd(ViewportCommand::IMEAllowed(false));
    }
}

#[cfg(not(target_os = "android"))]
pub fn sync_android_soft_input(_ctx: &Context, _enable: bool, _ime_area: egui::Rect) {}

/// Keep arrow/tab/escape on the terminal (egui focus navigation runs in `begin_pass`).
pub fn lock_terminal_focus(ctx: &Context) {
    ctx.memory_mut(|mem| {
        mem.set_focus_lock_filter(terminal_widget_id(), terminal_event_filter());
    });
}

/// Route keys to the PTY and remove them from egui's queue so focus/nav cannot eat repeats.
pub fn process_keyboard_input(
    ctx: &Context,
    term_focused: bool,
    has_selection: bool,
    modifiers: Modifiers,
    virtual_ctrl: bool,
    app_cursor_keys: bool,
    copy_requested: &mut bool,
    pending_input: &mut Vec<Vec<u8>>,
    paste_texts: &mut Vec<String>,
) {
    if !term_focused {
        ctx.input(|i| {
            for event in &i.events {
                if let Event::Copy = event {
                    if has_selection {
                        *copy_requested = true;
                    }
                }
            }
        });
        return;
    }

    lock_terminal_focus(ctx);

    ctx.input_mut(|i| {
        i.events.retain(|event| {
            match event {
                Event::Copy => {
                    if modifiers.shift && has_selection {
                        *copy_requested = true;
                    } else if !modifiers.shift {
                        pending_input.push(vec![0x03]);
                    }
                    false
                }
                Event::Cut => {
                    if !modifiers.shift {
                        pending_input.push(vec![0x18]);
                    }
                    false
                }
                Event::Paste(text) => {
                    if modifiers.shift {
                        paste_texts.push(text.clone());
                    } else {
                        pending_input.push(vec![0x16]);
                    }
                    false
                }
                Event::Text(text) => {
                    let ctrl = modifiers.ctrl || modifiers.command || virtual_ctrl;
                    if ctrl {
                        let mut bytes = Vec::new();
                        for c in text.chars() {
                            if (c == 'c' || c == 'C')
                                && has_selection
                                && !modifiers.shift
                            {
                                *copy_requested = true;
                                continue;
                            }
                            if let Some(b) = ctrl_byte_for_char(c) {
                                bytes.push(b);
                            }
                        }
                        if !bytes.is_empty() {
                            pending_input.push(bytes);
                        }
                    } else if !text
                        .as_bytes()
                        .iter()
                        .any(|&b| b < 0x20 || b == 0x7f)
                    {
                        pending_input.push(text.as_bytes().to_vec());
                    }
                    false
                }
                Event::Key {
                    key,
                    pressed: true,
                    modifiers: key_mods,
                    ..
                } => {
                    if *key == Key::V && key_mods.command && key_mods.shift {
                        if let Some(t) = read_text() {
                            paste_texts.push(t);
                        }
                        false
                    } else if let Some(bytes) = key_to_pty(*key, *key_mods, app_cursor_keys) {
                        pending_input.push(bytes);
                        false
                    } else {
                        true
                    }
                }
                _ => true,
            }
        });
    });
}

/// Map egui keys to bytes for the PTY.
pub fn key_to_pty(key: Key, modifiers: Modifiers, app_cursor_keys: bool) -> Option<Vec<u8>> {
    let ctrl = modifiers.ctrl || modifiers.command;
    let shift = modifiers.shift;
    let alt = modifiers.alt;
    let use_ss3 = app_cursor_keys && !ctrl && !shift && !alt;
    let result = match key {
        Key::Enter => b"\r".to_vec(),
        Key::Backspace => b"\x7f".to_vec(),
        Key::Tab => b"\t".to_vec(),
        Key::Escape => b"\x1b".to_vec(),
        Key::A if ctrl => vec![0x01],
        Key::B if ctrl => vec![0x02],
        Key::C if ctrl => vec![0x03],
        Key::D if ctrl => vec![0x04],
        Key::E if ctrl => vec![0x05],
        Key::F if ctrl => vec![0x06],
        Key::G if ctrl => vec![0x07],
        Key::H if ctrl => vec![0x08],
        Key::I if ctrl => vec![0x09],
        Key::J if ctrl => vec![0x0a],
        Key::K if ctrl => vec![0x0b],
        Key::L if ctrl => vec![0x0c],
        Key::M if ctrl => vec![0x0d],
        Key::N if ctrl => vec![0x0e],
        Key::O if ctrl => vec![0x0f],
        Key::P if ctrl => vec![0x10],
        Key::Q if ctrl => vec![0x11],
        Key::R if ctrl => vec![0x12],
        Key::S if ctrl => vec![0x13],
        Key::T if ctrl => vec![0x14],
        Key::U if ctrl => vec![0x15],
        Key::V if ctrl => vec![0x16],
        Key::W if ctrl => vec![0x17],
        Key::X if ctrl => vec![0x18],
        Key::Y if ctrl => vec![0x19],
        Key::Z if ctrl => vec![0x1a],
        Key::ArrowUp if ctrl => b"\x1b[1;5A".to_vec(),
        Key::ArrowDown if ctrl => b"\x1b[1;5B".to_vec(),
        Key::ArrowLeft if ctrl => b"\x1b[1;5D".to_vec(),
        Key::ArrowRight if ctrl => b"\x1b[1;5C".to_vec(),
        Key::ArrowUp if use_ss3 => b"\x1bOA".to_vec(),
        Key::ArrowDown if use_ss3 => b"\x1bOB".to_vec(),
        Key::ArrowRight if use_ss3 => b"\x1bOC".to_vec(),
        Key::ArrowLeft if use_ss3 => b"\x1bOD".to_vec(),
        Key::ArrowUp => b"\x1b[A".to_vec(),
        Key::ArrowDown => b"\x1b[B".to_vec(),
        Key::ArrowRight => b"\x1b[C".to_vec(),
        Key::ArrowLeft => b"\x1b[D".to_vec(),
        Key::Home if use_ss3 => b"\x1bOH".to_vec(),
        Key::End if use_ss3 => b"\x1bOF".to_vec(),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Insert => b"\x1b[2~".to_vec(),
        Key::Delete => b"\x1b[3~".to_vec(),
        Key::F1 => b"\x1bOP".to_vec(),
        Key::F2 => b"\x1bOQ".to_vec(),
        Key::F3 => b"\x1bOR".to_vec(),
        Key::F4 => b"\x1bOS".to_vec(),
        Key::F5 => b"\x1b[15~".to_vec(),
        Key::F6 => b"\x1b[17~".to_vec(),
        Key::F7 => b"\x1b[18~".to_vec(),
        Key::F8 => b"\x1b[19~".to_vec(),
        Key::F9 => b"\x1b[20~".to_vec(),
        Key::F10 => b"\x1b[21~".to_vec(),
        Key::F11 => b"\x1b[23~".to_vec(),
        Key::F12 => b"\x1b[24~".to_vec(),
        _ => return None,
    };
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Modifiers;

    #[test]
    fn app_cursor_keys_use_ss3_arrows() {
        let mods = Modifiers::default();
        assert_eq!(key_to_pty(Key::ArrowUp, mods, true), Some(b"\x1bOA".to_vec()));
        assert_eq!(key_to_pty(Key::ArrowUp, mods, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(key_to_pty(Key::Home, mods, true), Some(b"\x1bOH".to_vec()));
    }
}
