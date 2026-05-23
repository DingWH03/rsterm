use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum KeyboardMode {
    Special,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Alpha,
    Symbols,
}

#[derive(Clone, Copy)]
enum KeyAction {
    /// Send fixed escape sequence (Tab, Esc, arrows, …).
    Bytes(&'static [u8]),
    /// Send `label` as UTF-8 (letters, digits, punctuation).
    Text,
    Shift,
    Symbols,
    Fn,
    Ctrl,
    Space,
    Enter,
    FKey(u8),
}

struct KeyDef {
    label: &'static str,
    action: KeyAction,
    width: f32,
}

struct KeyMetrics {
    key_w: f32,
    key_h: f32,
    gap: f32,
    row_gap: f32,
    font_size: f32,
    frame_pad: f32,
}

pub struct VirtualKeyboard {
    pub visible: bool,
    pub mode: KeyboardMode,
    shift: bool,
    ctrl: bool,
    show_fn: bool,
    layer: Layer,
    /// Terminal currently owns Android's system IME. This is a logical state,
    /// not a visibility flag: Android Back may hide the keyboard while terminal
    /// focus remains, and the next terminal tap should reopen it.
    #[cfg(target_os = "android")]
    pub terminal_ime_enabled: bool,
}

impl VirtualKeyboard {
    pub fn new(mode: KeyboardMode) -> Self {
        Self {
            visible: false,
            mode,
            shift: false,
            ctrl: false,
            show_fn: false,
            layer: Layer::Alpha,
            #[cfg(target_os = "android")]
            terminal_ime_enabled: false,
        }
    }

    /// Layout mode used for sizing and painting.
    pub fn effective_mode(&self) -> KeyboardMode {
        self.mode
    }

    /// Sticky Ctrl from the on-screen keyboard (used with system IME on Android).
    pub fn ctrl_active(&self) -> bool {
        self.ctrl
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            KeyboardMode::Special => KeyboardMode::Full,
            KeyboardMode::Full => KeyboardMode::Special,
        };
    }

    /// Total vertical space for keyboard + separator above it (matches layout in `connection_view`).
    pub fn reserved_height(&self, avail_w: f32) -> f32 {
        if !self.visible {
            return 0.0;
        }
        self.content_height(avail_w) + Self::SEPARATOR_ABOVE
    }

    /// Keyboard body height only (must match rows drawn in `show_*`).
    pub fn content_height(&self, avail_w: f32) -> f32 {
        let compact = self.effective_mode() == KeyboardMode::Full;
        let m = KeyMetrics::for_width(avail_w, self.layout_row_units(), compact);
        let row = m.row_pixel_height();
        let n = self.row_count() as f32;
        n * row + (n - 1.0).max(0.0) * m.row_gap
    }

    fn row_count(&self) -> u32 {
        match self.effective_mode() {
            KeyboardMode::Special => {
                2
            }
            KeyboardMode::Full => {
                let main_rows = match self.layer {
                    Layer::Alpha => 3,
                    Layer::Symbols => 2,
                };
                2 + main_rows + u32::from(self.show_fn) // number row + bottom row
            }
        }
    }

    const SEPARATOR_ABOVE: f32 = 6.0;

    fn layout_row_units(&self) -> f32 {
        match self.effective_mode() {
            KeyboardMode::Special => 14.0,
            KeyboardMode::Full => {
                if self.layer == Layer::Symbols {
                    12.0
                } else {
                    15.0
                }
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Vec<Vec<u8>> {
        if !self.visible {
            return Vec::new();
        }
        match self.effective_mode() {
            KeyboardMode::Special => self.show_special(ui),
            KeyboardMode::Full => self.show_full(ui),
        }
    }
}

impl KeyMetrics {
    fn for_width(avail_w: f32, row_units: f32, compact: bool) -> Self {
        let avail_w = avail_w.max(280.0);
        let (min_w, max_w, min_h, max_h, h_ratio, frame_pad, row_gap_mul) = if compact {
            (20.0, 38.0, 24.0, 32.0, 0.72, 2.0, 0.55)
        } else {
            (22.0, 44.0, 28.0, 36.0, 0.82, 4.0, 1.0)
        };
        let gap = (avail_w * 0.006).clamp(2.0, 3.0);
        let unit_count = row_units.max(1.0);
        let gap_total = gap * (unit_count - 1.0).max(0.0);
        let key_w = ((avail_w - gap_total) / unit_count).clamp(min_w, max_w);
        let key_h = (key_w * h_ratio).clamp(min_h, max_h);
        let font_size = (key_w * 0.40).clamp(10.0, 14.0);
        let row_gap = (gap * row_gap_mul).max(1.0);
        Self {
            key_w,
            key_h,
            gap,
            row_gap,
            font_size,
            frame_pad,
        }
    }

    fn row_pixel_height(&self) -> f32 {
        self.key_h + self.frame_pad
    }
}

impl VirtualKeyboard {
    fn show_special(&mut self, ui: &mut egui::Ui) -> Vec<Vec<u8>> {
        let mut output = Vec::new();
        let avail = ui.available_width();
        let m = KeyMetrics::for_width(avail, 14.0, false);
        ui.spacing_mut().item_spacing = egui::vec2(m.gap, m.row_gap);

        let row1 = [
            key("Esc", KeyAction::Bytes(b"\x1b"), 1.0),
            key("F1", KeyAction::FKey(1), 1.0),
            key("F2", KeyAction::FKey(2), 1.0),
            key("F3", KeyAction::FKey(3), 1.0),
            key("F4", KeyAction::FKey(4), 1.0),
            key("F5", KeyAction::FKey(5), 1.0),
            key("F6", KeyAction::FKey(6), 1.0),
            key("F7", KeyAction::FKey(7), 1.0),
            key("F8", KeyAction::FKey(8), 1.0),
            key("F9", KeyAction::FKey(9), 1.0),
            key("F10", KeyAction::FKey(10), 1.0),
            key("F11", KeyAction::FKey(11), 1.0),
            key("F12", KeyAction::FKey(12), 1.0),
        ];
        self.paint_row(ui, &m, &row1, &mut output);

        let row2 = [
            key("Tab", KeyAction::Bytes(b"\t"), 1.1),
            key("Ctrl", KeyAction::Ctrl, 1.1),
            key("⌫", KeyAction::Bytes(b"\x7f"), 1.1),
            key("Del", KeyAction::Bytes(b"\x1b[3~"), 1.1),
            key("Ins", KeyAction::Bytes(b"\x1b[2~"), 1.1),
            key("↑", KeyAction::Bytes(b"\x1b[A"), 1.0),
            key("↓", KeyAction::Bytes(b"\x1b[B"), 1.0),
            key("←", KeyAction::Bytes(b"\x1b[D"), 1.0),
            key("→", KeyAction::Bytes(b"\x1b[C"), 1.0),
            key("Hom", KeyAction::Bytes(b"\x1b[H"), 1.05),
            key("End", KeyAction::Bytes(b"\x1b[F"), 1.05),
            key("PU", KeyAction::Bytes(b"\x1b[5~"), 1.05),
            key("PD", KeyAction::Bytes(b"\x1b[6~"), 1.05),
            key("↵", KeyAction::Bytes(b"\r"), 1.15),
        ];
        self.paint_row(ui, &m, &row2, &mut output);

        output
    }

    fn show_full(&mut self, ui: &mut egui::Ui) -> Vec<Vec<u8>> {
        let mut output = Vec::new();
        let avail = ui.available_width();
        let row_units = if self.layer == Layer::Symbols {
            12.0
        } else {
            13.5
        };
        let m = KeyMetrics::for_width(avail, row_units, true);
        ui.spacing_mut().item_spacing = egui::vec2(m.gap, m.row_gap);

        // Number row
        let mut row1 = vec![
            key(
                if self.show_fn { "Fn✓" } else { "Fn" },
                KeyAction::Fn,
                1.15,
            ),
            key("Esc", KeyAction::Bytes(b"\x1b"), 1.0),
        ];
        for n in 1..=10u8 {
            let label = digit_label(n, self.layer, self.shift);
            row1.push(key(label, KeyAction::Text, 1.0));
        }
        row1.push(key("⌫", KeyAction::Bytes(b"\x7f"), 1.2));
        self.paint_row(ui, &m, &row1, &mut output);

        if self.show_fn {
            self.paint_fn_row(ui, &m, &mut output);
        }

        match self.layer {
            Layer::Alpha => self.paint_alpha(ui, &m, &mut output),
            Layer::Symbols => self.paint_symbols(ui, &m, &mut output),
        }
        output
    }

    fn paint_alpha(&mut self, ui: &mut egui::Ui, m: &KeyMetrics, output: &mut Vec<Vec<u8>>) {
        let q_row = ["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"];
        self.paint_letter_row(ui, m, 0.5, &q_row, output);

        let a_row = ["a", "s", "d", "f", "g", "h", "j", "k", "l"];
        self.paint_letter_row(ui, m, 1.0, &a_row, output);

        let mut z_row = vec![
            key("Ctrl", KeyAction::Ctrl, 1.2),
            key(if self.shift { "⇧✓" } else { "⇧" }, KeyAction::Shift, 1.2),
        ];
        for c in ["z", "x", "c", "v", "b", "n", "m"] {
            let label = letter_label(c, self.shift);
            z_row.push(key(label, KeyAction::Text, 1.0));
        }
        z_row.push(key("⌫", KeyAction::Bytes(b"\x7f"), 1.35));
        self.paint_row(ui, m, &z_row, output);

        let bottom = [
            key("Tab", KeyAction::Bytes(b"\t"), 1.0),
            key("#+=", KeyAction::Symbols, 1.1),
            key(",", KeyAction::Text, 1.0),
            key(".", KeyAction::Text, 1.0),
            key("/", KeyAction::Text, 1.0),
            key("-", KeyAction::Text, 1.0),
            key("_", KeyAction::Text, 1.0),
            key("=", KeyAction::Text, 1.0),
            key("Space", KeyAction::Space, 3.2),
            key("↵", KeyAction::Enter, 1.4),
        ];
        self.paint_row(ui, m, &bottom, output);
    }

    fn paint_symbols(&mut self, ui: &mut egui::Ui, m: &KeyMetrics, output: &mut Vec<Vec<u8>>) {
        let row1 = [
            key("!", KeyAction::Text, 1.0),
            key("@", KeyAction::Text, 1.0),
            key("#", KeyAction::Text, 1.0),
            key("$", KeyAction::Text, 1.0),
            key("%", KeyAction::Text, 1.0),
            key("^", KeyAction::Text, 1.0),
            key("&", KeyAction::Text, 1.0),
            key("*", KeyAction::Text, 1.0),
            key("(", KeyAction::Text, 1.0),
            key(")", KeyAction::Text, 1.0),
        ];
        self.paint_row(ui, m, &row1, output);

        let mut row2 = vec![key(
            if self.shift { "⇧✓" } else { "⇧" },
            KeyAction::Shift,
            1.2,
        )];
        for ch in ["~", "|", "{", "}", "<", ">", "?", "\"", "+"] {
            row2.push(key(ch, KeyAction::Text, 1.0));
        }
        row2.push(key("⌫", KeyAction::Bytes(b"\x7f"), 1.2));
        self.paint_row(ui, m, &row2, output);

        let bottom = [
            key("Tab", KeyAction::Bytes(b"\t"), 1.0),
            key("Ctrl", KeyAction::Ctrl, 1.1),
            key("ABC", KeyAction::Symbols, 1.1),
            key("[", KeyAction::Text, 1.0),
            key("]", KeyAction::Text, 1.0),
            key(":", KeyAction::Text, 1.0),
            key(";", KeyAction::Text, 1.0),
            key("'", KeyAction::Text, 1.0),
            key("`", KeyAction::Text, 1.0),
            key("\\", KeyAction::Text, 1.0),
            key("Space", KeyAction::Space, 2.4),
            key("↵", KeyAction::Enter, 1.3),
        ];
        self.paint_row(ui, m, &bottom, output);
    }

    fn paint_letter_row(
        &mut self,
        ui: &mut egui::Ui,
        m: &KeyMetrics,
        indent_units: f32,
        letters: &[&str],
        output: &mut Vec<Vec<u8>>,
    ) {
        let indent = indent_units * (m.key_w + m.gap);
        let widths: Vec<f32> = vec![1.0; letters.len()];
        let row_w = self.row_width_units(m, &widths) + indent;
        let pad = row_padding(ui, row_w);
        let shift = self.shift;
        ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.add_space(indent);
            for c in letters {
                let label = letter_label(c, shift);
                if paint_button(ui, label, m.key_w, m.key_h, m.font_size, false) {
                    self.emit_text_label(label, output);
                }
            }
        });
    }

    fn paint_fn_row(&self, ui: &mut egui::Ui, m: &KeyMetrics, output: &mut Vec<Vec<u8>>) {
        let widths: Vec<f32> = (0..12).map(|_| 1.0).collect();
        let row_w = self.row_width_units(m, &widths);
        let pad = row_padding(ui, row_w);
        ui.horizontal(|ui| {
            ui.add_space(pad);
            for i in 1..=12u8 {
                let label = format!("F{i}");
                if paint_button(ui, &label, m.key_w, m.key_h, m.font_size, false) {
                    output.push(fkey_seq(i));
                }
            }
        });
    }

    fn paint_row(&mut self, ui: &mut egui::Ui, m: &KeyMetrics, keys: &[KeyDef], output: &mut Vec<Vec<u8>>) {
        let row_w = self.row_width(m, keys);
        let pad = row_padding(ui, row_w);
        ui.horizontal(|ui| {
            ui.add_space(pad);
            for kd in keys {
                if self.paint_key(ui, m, kd) {
                    self.emit_key(kd, output);
                }
            }
        });
    }

    fn paint_key(&self, ui: &mut egui::Ui, m: &KeyMetrics, kd: &KeyDef) -> bool {
        let w = m.key_w * kd.width;
        let active = matches!(kd.action, KeyAction::Shift if self.shift)
            || matches!(kd.action, KeyAction::Ctrl if self.ctrl)
            || matches!(kd.action, KeyAction::Fn if self.show_fn)
            || (matches!(kd.action, KeyAction::Symbols) && self.layer == Layer::Symbols);
        paint_button(ui, kd.label, w, m.key_h, m.font_size, active)
    }

    /// Send one text key label (letters, punctuation) honoring sticky Ctrl.
    fn emit_text_label(&mut self, label: &str, output: &mut Vec<Vec<u8>>) {
        if self.ctrl {
            if let Some(ch) = label.chars().next()
                && label.chars().nth(1).is_none()
                && let Some(byte) = ctrl_byte_for_char(ch)
            {
                output.push(vec![byte]);
                return;
            }
        }
        output.push(label.as_bytes().to_vec());
    }

    fn emit_key(&mut self, kd: &KeyDef, output: &mut Vec<Vec<u8>>) {
        match kd.action {
            KeyAction::Bytes(bytes) => output.push(bytes.to_vec()),
            KeyAction::Text => self.emit_text_label(kd.label, output),
            KeyAction::FKey(n) => output.push(fkey_seq(n)),
            KeyAction::Shift => self.shift = !self.shift,
            KeyAction::Ctrl => self.ctrl = !self.ctrl,
            KeyAction::Symbols => {
                self.layer = match self.layer {
                    Layer::Alpha => Layer::Symbols,
                    Layer::Symbols => Layer::Alpha,
                };
                self.shift = false;
            }
            KeyAction::Fn => self.show_fn = !self.show_fn,
            KeyAction::Space => output.push(b" ".to_vec()),
            KeyAction::Enter => output.push(b"\r".to_vec()),
        }
    }

    fn row_width(&self, m: &KeyMetrics, keys: &[KeyDef]) -> f32 {
        let widths: Vec<f32> = keys.iter().map(|k| k.width).collect();
        self.row_width_units(m, &widths)
    }

    fn row_width_units(&self, m: &KeyMetrics, widths: &[f32]) -> f32 {
        if widths.is_empty() {
            return 0.0;
        }
        let keys_w: f32 = widths.iter().map(|w| m.key_w * w).sum();
        let gaps = m.gap * (widths.len().saturating_sub(1) as f32);
        keys_w + gaps
    }

}

fn row_padding(ui: &egui::Ui, row_w: f32) -> f32 {
    ((ui.available_width() - row_w) * 0.5).max(0.0)
}

fn key(label: &'static str, action: KeyAction, width: f32) -> KeyDef {
    KeyDef {
        label,
        action,
        width,
    }
}

fn paint_button(
    ui: &mut egui::Ui,
    label: &str,
    w: f32,
    h: f32,
    font_size: f32,
    active: bool,
) -> bool {
    let visuals = ui.visuals();
    let fill = if active {
        visuals.widgets.active.bg_fill
    } else {
        visuals.widgets.inactive.bg_fill
    };
    let stroke = if active {
        visuals.widgets.active.bg_stroke
    } else {
        visuals.widgets.inactive.bg_stroke
    };
    let btn = egui::Button::new(egui::RichText::new(label).size(font_size).monospace())
        .fill(fill)
        .stroke(stroke);
    ui.add_sized(egui::vec2(w, h), btn).clicked()
}

fn letter_label(c: &str, shift: bool) -> &'static str {
    if shift {
        match c {
            "a" => "A",
            "b" => "B",
            "c" => "C",
            "d" => "D",
            "e" => "E",
            "f" => "F",
            "g" => "G",
            "h" => "H",
            "i" => "I",
            "j" => "J",
            "k" => "K",
            "l" => "L",
            "m" => "M",
            "n" => "N",
            "o" => "O",
            "p" => "P",
            "q" => "Q",
            "r" => "R",
            "s" => "S",
            "t" => "T",
            "u" => "U",
            "v" => "V",
            "w" => "W",
            "x" => "X",
            "y" => "Y",
            "z" => "Z",
            _ => "?",
        }
    } else {
        match c {
            "a" => "a",
            "b" => "b",
            "c" => "c",
            "d" => "d",
            "e" => "e",
            "f" => "f",
            "g" => "g",
            "h" => "h",
            "i" => "i",
            "j" => "j",
            "k" => "k",
            "l" => "l",
            "m" => "m",
            "n" => "n",
            "o" => "o",
            "p" => "p",
            "q" => "q",
            "r" => "r",
            "s" => "s",
            "t" => "t",
            "u" => "u",
            "v" => "v",
            "w" => "w",
            "x" => "x",
            "y" => "y",
            "z" => "z",
            _ => "?",
        }
    }
}

fn digit_label(n: u8, layer: Layer, shift: bool) -> &'static str {
    const DIGITS: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
    const SHIFTED: [&str; 10] = ["!", "@", "#", "$", "%", "^", "&", "*", "(", ")"];
    let i = (n as usize).saturating_sub(1).min(9);
    if layer == Layer::Symbols && shift {
        SHIFTED[i]
    } else {
        DIGITS[i]
    }
}

/// Map one character to a control byte when Ctrl is held (e.g. `c` → ETX).
pub fn ctrl_byte_for_char(c: char) -> Option<u8> {
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        'A'..='Z' => Some(c as u8 - b'A' + 1),
        '@' => Some(0),
        '[' => Some(27),
        '\\' => Some(28),
        ']' => Some(29),
        '^' => Some(30),
        '_' => Some(31),
        ' ' => Some(0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_mapping() {
        assert_eq!(ctrl_byte_for_char('c'), Some(0x03));
        assert_eq!(ctrl_byte_for_char('D'), Some(0x04));
    }
}

fn fkey_seq(n: u8) -> Vec<u8> {
    if n <= 4 {
        format!("\x1bO{}", b'P' + n - 1).into_bytes()
    } else if n <= 10 {
        format!("\x1b[{}~", n + 10).into_bytes()
    } else if n <= 12 {
        format!("\x1b[{}~", n + 12).into_bytes()
    } else {
        Vec::new()
    }
}
