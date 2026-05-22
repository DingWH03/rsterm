#[derive(Debug, Clone)]
pub enum TermEvent {
    /// Send response string to host (e.g. answer to `CSI 18 t`).
    Response(Vec<u8>),
    /// Reserved: application `CSI 8` window resize (not used; rsTerminal owns geometry).
    #[allow(dead_code)]
    PtyResize { rows: usize, cols: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    OscString,
    OscEsc,
    DcsEntry,
    DcsParam,
    DcsIntermediate,
    DcsPassthrough,
    DcsEsc,
    DcsIgnore,
    SosPmApcString,
}

pub struct Parser {
    state: State,
    params: Vec<i64>,
    intermediates: Vec<u8>,
    current_param: i64,
    osc_buf: Vec<u8>,
    dcs_buf: Vec<u8>,
    ignore_flag: bool,
    utf8_pending: Vec<u8>,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            params: Vec::with_capacity(16),
            intermediates: Vec::new(),
            current_param: 0,
            osc_buf: Vec::new(),
            dcs_buf: Vec::new(),
            ignore_flag: false,
            utf8_pending: Vec::new(),
        }
    }

    fn flush_utf8(&mut self, handler: &mut dyn TermHandler) {
        if self.utf8_pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.utf8_pending);
        for ch in String::from_utf8_lossy(&pending).chars() {
            handler.print(ch);
        }
    }

    fn push_utf8(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        self.utf8_pending.push(byte);
        if let Ok(text) = std::str::from_utf8(&self.utf8_pending) {
            for ch in text.chars() {
                handler.print(ch);
            }
            self.utf8_pending.clear();
        } else if self.utf8_pending.len() >= 4 {
            self.flush_utf8(handler);
        }
    }

    pub fn process(&mut self, data: &[u8], handler: &mut dyn TermHandler) {
        for &byte in data {
            self.process_byte(byte, handler);
        }
    }

    fn reset_csi(&mut self) {
        self.params.clear();
        self.intermediates.clear();
        self.current_param = 0;
    }

    fn finish_csi(&mut self, handler: &mut dyn TermHandler, final_byte: u8) {
        self.params.push(self.current_param);
        handler.csi_dispatch(&self.params, &self.intermediates, final_byte);
        self.reset_csi();
        self.state = State::Ground;
    }

    fn finish_dcs(&mut self, handler: &mut dyn TermHandler) {
        let prefix: String = self
            .intermediates
            .iter()
            .map(|&b| b as char)
            .collect();
        let body = String::from_utf8_lossy(&self.dcs_buf).to_string();
        let data = format!("{prefix}{body}");
        handler.dcs_dispatch(&data);
        self.dcs_buf.clear();
        self.intermediates.clear();
        self.params.clear();
        self.current_param = 0;
        self.state = State::Ground;
    }

    /// ESC/CAN/SUB while inside CSI/DCS/OSC — abort current sequence and restart.
    fn anywhere(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x18 | 0x1A => {
                self.reset_csi();
                self.dcs_buf.clear();
                self.intermediates.clear();
                handler.execute(byte);
                self.state = State::Ground;
            }
            0x1B => {
                self.reset_csi();
                self.dcs_buf.clear();
                self.intermediates.clear();
                self.state = State::Escape;
            }
            _ => {}
        }
    }

    fn process_byte(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match self.state {
            State::Ground => self.ground(byte, handler),
            State::Escape => self.escape(byte, handler),
            State::EscapeIntermediate => self.escape_intermediate(byte, handler),
            State::CsiEntry => self.csi_entry(byte, handler),
            State::CsiParam => self.csi_param(byte, handler),
            State::CsiIntermediate => self.csi_intermediate(byte, handler),
            State::CsiIgnore => self.csi_ignore(byte, handler),
            State::OscString => self.osc_string(byte, handler),
            State::OscEsc => self.osc_esc(byte, handler),
            State::DcsEntry => self.dcs_entry(byte, handler),
            State::DcsParam => self.dcs_param(byte, handler),
            State::DcsIntermediate => self.dcs_intermediate(byte, handler),
            State::DcsPassthrough => self.dcs_passthrough(byte, handler),
            State::DcsEsc => self.dcs_esc(byte, handler),
            State::DcsIgnore => self.dcs_ignore(byte, handler),
            State::SosPmApcString => self.sos_pm_apc_string(byte, handler),
        }
    }

    fn ground(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => {
                self.flush_utf8(handler);
                handler.execute(byte);
            }
            0x18 | 0x1A => {
                self.flush_utf8(handler);
                handler.execute(byte);
            }
            0x1B => {
                self.flush_utf8(handler);
                self.state = State::Escape;
                self.intermediates.clear();
            }
            0x7F => {
                self.flush_utf8(handler);
                handler.execute(byte);
            }
            0x20..=0x7E => {
                self.flush_utf8(handler);
                handler.print(byte as char);
            }
            0x80..=0xF4 => {
                self.push_utf8(byte, handler);
            }
            _ => {}
        }
    }

    fn escape(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x20..=0x2F => {
                self.intermediates.push(byte);
                self.state = State::EscapeIntermediate;
            }
            0x30..=0x4F => {
                // SS3 introducer (ESC O): needs one more byte (e.g. arrow keys, F1–F4).
                if byte == b'O' {
                    self.intermediates.push(byte);
                    self.state = State::EscapeIntermediate;
                } else {
                    handler.esc_dispatch(&self.intermediates, byte);
                    self.state = State::Ground;
                }
            }
            0x50 => {
                // DCS
                self.state = State::DcsEntry;
                self.params.clear();
                self.current_param = 0;
            }
            0x51..=0x57 => {
                // SOS, etc.
                self.state = State::SosPmApcString;
            }
            0x58 | 0x5A => handler.esc_dispatch(&self.intermediates, byte),
            0x5B => {
                // CSI
                self.reset_csi();
                self.state = State::CsiEntry;
            }
            0x5C => handler.esc_dispatch(&self.intermediates, byte),
            0x5D => {
                // OSC
                self.state = State::OscString;
                self.osc_buf.clear();
            }
            0x5E..=0x5F => {
                self.state = State::SosPmApcString;
            }
            0x60..=0x7E => handler.esc_dispatch(&self.intermediates, byte),
            0x7F => handler.execute(byte),
            _ => {}
        }
    }

    fn escape_intermediate(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x20..=0x2F => self.intermediates.push(byte),
            0x30..=0x7E => {
                handler.esc_dispatch(&self.intermediates, byte);
                self.state = State::Ground;
            }
            0x7F => handler.execute(byte),
            _ => {}
        }
    }

    fn csi_entry(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x20..=0x2F => {
                self.intermediates.push(byte);
                self.state = State::CsiIntermediate;
            }
            0x3A => {
                self.state = State::CsiIgnore;
            }
            0x30..=0x39 => {
                self.current_param = (byte - 0x30) as i64;
                self.state = State::CsiParam;
            }
            0x3B => {
                self.params.push(self.current_param);
                self.current_param = 0;
                self.state = State::CsiParam;
            }
            0x3C..=0x3F => {
                self.intermediates.push(byte);
                self.state = State::CsiParam;
            }
            0x40..=0x7E => {
                self.finish_csi(handler, byte);
            }
            0x7F => handler.execute(byte),
            _ => self.anywhere(byte, handler),
        }
    }

    fn csi_param(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x20..=0x2F => {
                self.intermediates.push(byte);
                self.state = State::CsiIntermediate;
            }
            0x30..=0x39 => {
                self.current_param = self.current_param.saturating_mul(10).saturating_add((byte - 0x30) as i64);
            }
            0x3B => {
                self.params.push(self.current_param);
                self.current_param = 0;
            }
            0x3A => {
                // SGR subparameters (e.g. 38:2:r:g:b) — ignore until final byte.
                self.state = State::CsiIgnore;
            }
            0x3C..=0x3F => {
                self.intermediates.push(byte);
            }
            0x40..=0x7E => {
                self.finish_csi(handler, byte);
            }
            0x7F => handler.execute(byte),
            _ => self.anywhere(byte, handler),
        }
    }

    fn csi_intermediate(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x20..=0x2F => self.intermediates.push(byte),
            0x30..=0x3F => {
                self.state = State::CsiIgnore;
            }
            0x40..=0x7E => {
                self.finish_csi(handler, byte);
            }
            0x7F => handler.execute(byte),
            _ => self.anywhere(byte, handler),
        }
    }

    fn csi_ignore(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x20..=0x3F => {}
            0x40..=0x7E => {
                self.reset_csi();
                self.state = State::Ground;
            }
            0x7F => handler.execute(byte),
            _ => self.anywhere(byte, handler),
        }
    }

    fn osc_string(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            // C0 controls: execute even inside OSC string
            0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x07 => {
                // BEL terminator
                let s = String::from_utf8_lossy(&self.osc_buf).to_string();
                handler.osc_dispatch(&s);
                self.state = State::Ground;
            }
            0x1B => {
                self.state = State::OscEsc;
            }
            0x18 | 0x1A => {
                // CAN / SUB — abort OSC
                handler.execute(byte);
                self.state = State::Ground;
                self.osc_buf.clear();
            }
            0x9C => {
                // ST terminator
                let s = String::from_utf8_lossy(&self.osc_buf).to_string();
                handler.osc_dispatch(&s);
                self.state = State::Ground;
            }
            0x7F => handler.execute(byte),
            _ => {
                self.osc_buf.push(byte);
            }
        }
    }

    fn osc_esc(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        if byte == b'\\' || byte == 0x5C {
            let s = String::from_utf8_lossy(&self.osc_buf).to_string();
            handler.osc_dispatch(&s);
            self.osc_buf.clear();
            self.state = State::Ground;
        } else {
            self.osc_buf.push(0x1B);
            self.osc_buf.push(byte);
            self.state = State::OscString;
        }
    }

    fn dcs_entry(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x20..=0x2F => {
                self.intermediates.push(byte);
                self.state = State::DcsIntermediate;
            }
            0x3A => self.state = State::DcsIgnore,
            0x30..=0x39 => {
                self.current_param = (byte - 0x30) as i64;
                self.state = State::DcsParam;
            }
            0x3B => {
                self.params.push(self.current_param);
                self.current_param = 0;
                self.state = State::DcsParam;
            }
            0x3C..=0x3F => {
                self.intermediates.push(byte);
                self.state = State::DcsParam;
            }
            0x40..=0x7E => {
                self.state = State::DcsIgnore;
            }
            0x7F => {}
            _ => {}
        }
    }

    fn dcs_param(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x20..=0x2F => {
                self.intermediates.push(byte);
                self.state = State::DcsIntermediate;
            }
            0x30..=0x39 => {
                self.current_param = self.current_param.saturating_mul(10).saturating_add((byte - 0x30) as i64);
            }
            0x3B => {
                self.params.push(self.current_param);
                self.current_param = 0;
            }
            0x3A | 0x3C..=0x3F => {
                self.intermediates.push(byte);
            }
            0x40..=0x7E => {
                self.state = State::DcsIgnore;
            }
            0x7F => {}
            _ => {}
        }
    }

    fn dcs_intermediate(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x20..=0x2F => {}
            0x30..=0x3F => self.state = State::DcsIgnore,
            0x40..=0x7E => {
                self.state = State::DcsPassthrough;
                self.dcs_buf.clear();
            }
            0x7F => {}
            _ => {}
        }
    }

    fn dcs_passthrough(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x1B => self.state = State::DcsEsc,
            0x18 | 0x1A => {
                handler.execute(byte);
                self.dcs_buf.clear();
                self.intermediates.clear();
                self.state = State::Ground;
            }
            0x9C => self.finish_dcs(handler),
            0x07 => {
                self.dcs_buf.clear();
                self.intermediates.clear();
                self.state = State::Ground;
            }
            0x7F => handler.execute(byte),
            _ => self.dcs_buf.push(byte),
        }
    }

    fn dcs_esc(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        if byte == b'\\' || byte == 0x5C {
            self.finish_dcs(handler);
        } else {
            self.dcs_buf.push(0x1B);
            self.dcs_buf.push(byte);
            self.state = State::DcsPassthrough;
        }
    }

    fn dcs_ignore(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x1B => self.state = State::DcsEsc,
            0x9C => {
                self.dcs_buf.clear();
                self.intermediates.clear();
                self.state = State::Ground;
            }
            0x18 | 0x1A => {
                handler.execute(byte);
                self.dcs_buf.clear();
                self.intermediates.clear();
                self.state = State::Ground;
            }
            0x07 => {
                self.dcs_buf.clear();
                self.intermediates.clear();
                self.state = State::Ground;
            }
            0x7F => handler.execute(byte),
            _ => {}
        }
    }

    fn sos_pm_apc_string(&mut self, byte: u8, handler: &mut dyn TermHandler) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => handler.execute(byte),
            0x1B => self.state = State::Escape,
            0x9C => self.state = State::Ground,
            0x18 | 0x1A => {
                handler.execute(byte);
                self.state = State::Ground;
            }
            0x07 => self.state = State::Ground,
            0x7F => handler.execute(byte),
            _ => {}
        }
    }
}

/// Handler trait for terminal events
pub trait TermHandler {
    fn print(&mut self, c: char);
    fn execute(&mut self, byte: u8);
    fn esc_dispatch(&mut self, intermediates: &[u8], final_byte: u8);
    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], final_byte: u8);
    fn osc_dispatch(&mut self, data: &str);
    fn dcs_dispatch(&mut self, _data: &str) {}
}
