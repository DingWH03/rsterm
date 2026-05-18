pub mod paint;
pub mod parser;
pub mod renderer;
pub mod screen;

use parser::{Parser, TermEvent};
use screen::Screen;

pub struct Terminal {
    pub screen: Screen,
    pub title: String,
    parser: Parser,
    raw_mode: bool,
}

impl Terminal {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            screen: Screen::new(rows, cols),
            title: String::new(),
            parser: Parser::new(),
            raw_mode: false,
        }
    }

    pub fn set_raw_mode(&mut self, raw: bool) {
        self.raw_mode = raw;
    }

    pub fn write(&mut self, data: &[u8]) {
        if self.raw_mode {
            for &byte in data {
                if byte == b'\n' || byte == b'\r' {
                    self.screen.newline();
                } else if byte == 0x08 {
                    self.screen.backspace();
                } else if byte == 0x09 {
                    self.screen.advance_tabs();
                } else if byte >= 0x20 && byte <= 0x7e {
                    self.screen.put_char(byte as char);
                }
            }
        } else {
            self.parser.process(data, &mut self.screen);
        }
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.screen.resize(rows, cols);
    }

    pub fn set_scrollback_limit(&mut self, limit: usize) {
        self.screen.set_scrollback_limit(limit);
    }

    pub fn drain_pending(&mut self) -> Vec<TermEvent> {
        self.screen.drain_outgoing()
    }
}

#[cfg(test)]
mod tests {
    use super::{TermEvent, Terminal};

    fn row_plaintext(term: &Terminal, row: usize) -> String {
        term.screen.cells[row]
            .iter()
            .map(|c| c.ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn utf8_chinese_filename_is_preserved() {
        let mut term = Terminal::new(1, 20);
        term.write("文件.txt".as_bytes());
        let row: String = term.screen.cells[0].iter().map(|c| c.ch).collect();
        assert!(row.contains('文'), "expected Chinese chars in buffer, got {row:?}");
        assert!(row.contains('件'));
    }

    #[test]
    fn wide_char_uses_two_columns() {
        let mut term = Terminal::new(1, 12);
        term.write("文件".as_bytes());
        assert_eq!(term.screen.cells[0][0].ch, '文');
        assert!(term.screen.cells[0][1].wide_continuation);
        assert_eq!(term.screen.cells[0][2].ch, '件');
        assert!(term.screen.cells[0][3].wide_continuation);
        assert_eq!(term.screen.cursor_x, 4);
    }

    #[test]
    fn utf8_prompt_symbol_is_preserved() {
        let mut term = Terminal::new(1, 40);
        term.write(b"hi \xc2\xbb ");
        let chars: String = term.screen.cells[0].iter().map(|c| c.ch).collect();
        assert!(chars.contains('\u{bb}'));
    }

    #[test]
    fn zsh_prompt_and_typed_char_visible() {
        let prompt = b"%                                                                                                                      \r \r\r\x1b[01;32mdwh@dwh-82sk\x1b[00m \x1b[01;34mrust/rsterm\x1b[00m \x1b[33m(master) \x1b[00m\x1b[00m\xc2\xbb \x1b[?2004h";
        let typed = b"a\x08\x08\x1b[31ma\x1b[39m";
        let mut term = Terminal::new(24, 120);
        term.write(prompt);
        term.write(typed);
        let cy = term.screen.cursor_y;
        let line = row_plaintext(&term, cy);
        assert!(line.contains("dwh"), "prompt text missing on cursor row: {line:?}");
        assert!(line.contains('a'), "typed char missing on cursor row: {line:?}");
    }

    #[test]
    fn prompt_visible_after_full_screen_ls_output() {
        let prompt = b"\r\n%                                                                                                                      \r \r\r\x1b[01;32mdwh@dwh-82sk\x1b[00m \x1b[01;34mrust/rsterm\x1b[00m \x1b[33m(master) \x1b[00m\x1b[00m\xc2\xbb \x1b[?2004h";
        let mut body = String::new();
        for i in 0..34 {
            body.push_str(&format!("line{i:03} file.txt\n"));
        }
        let mut term = Terminal::new(35, 100);
        term.write(body.as_bytes());
        term.write(prompt);
        let cy = term.screen.cursor_y;
        let line = row_plaintext(&term, cy);
        assert!(
            line.contains("dwh"),
            "cursor row {cy} should have prompt, got {line:?}"
        );
        // Also check any row has prompt
        let any: bool = term
            .screen
            .cells
            .iter()
            .any(|row| row.iter().any(|c| c.ch == 'd' || c.ch == '@'));
        assert!(any, "no prompt chars anywhere on screen");
    }

    #[test]
    fn backspace_moves_cursor_without_erasing() {
        let mut term = Terminal::new(1, 20);
        term.write(b"abc");
        term.write(&[0x08]);
        assert_eq!(term.screen.cursor_x, 2);
        let row: String = term.screen.cells[0].iter().map(|c| c.ch).collect();
        assert_eq!(row.chars().filter(|c| *c != ' ').collect::<String>(), "abc");
    }

    #[test]
    fn del_erases_cell_before_cursor() {
        let mut term = Terminal::new(1, 20);
        term.write(b"abc");
        term.write(&[0x7f]);
        let row: String = term.screen.cells[0].iter().map(|c| c.ch).collect();
        assert_eq!(row.chars().filter(|c| *c != ' ').collect::<String>(), "ab");
    }

    #[test]
    fn alternate_screen_1049_save_restore() {
        let mut term = Terminal::new(5, 40);
        term.write(b"saved");
        term.write(b"\x1b[?1049h");
        assert!(term.screen.in_alternate_screen());
        assert_eq!(term.screen.cells[0][0].ch, ' ');
        term.write(b"vim\x1b[?1049l");
        assert!(!term.screen.in_alternate_screen());
        let row: String = term.screen.cells[0].iter().map(|c| c.ch).collect();
        assert!(row.contains('s'), "main screen should be restored: {row:?}");
    }

    #[test]
    fn vim_smcup_sequence_paints_on_alternate() {
        let mut term = Terminal::new(24, 80);
        term.write(b"prompt> ");
        // xterm smcup + stack save + clear + home (typical vim/less entry)
        term.write(b"\x1b[?1049h\x1b[22;0;0t\x1b[2J\x1b[H");
        assert!(term.screen.in_alternate_screen());
        term.write(b"~");
        assert_eq!(term.screen.cells[0][0].ch, '~');
        term.write(b"\x1b[?1049l");
        assert!(!term.screen.in_alternate_screen());
        let row: String = term.screen.cells[0].iter().map(|c| c.ch).collect();
        assert!(row.contains('p'), "main prompt should return after vim: {row:?}");
    }

    #[test]
    fn csi_esc_aborts_incomplete_sequence_before_1049h() {
        let mut term = Terminal::new(24, 80);
        term.write(b"\x1b[3;1\x1b[?1049h\x1b[2J\x1b[HOK");
        assert!(
            term.screen.in_alternate_screen(),
            "1049h must work after aborted CSI"
        );
        assert_eq!(term.screen.cells[0][0].ch, 'O');
    }

    #[test]
    fn dcs_st_terminator_unblocks_alternate_screen() {
        let mut term = Terminal::new(24, 80);
        term.write(b"\x1bP+q436f\x1b\\\x1b[?1049h\x1b[2J\x1b[Hvim");
        assert!(term.screen.in_alternate_screen());
        assert_eq!(term.screen.cells[0][0].ch, 'v');
    }

    #[test]
    fn xtgettcap_co_reply_then_smcup() {
        let mut term = Terminal::new(24, 80);
        term.write(b"\x1bP+q436f\x1b\\");
        let pending = term.drain_pending();
        assert_eq!(pending.len(), 1);
        let TermEvent::Response(bytes) = pending[0].clone() else {
            panic!("expected xtgettcap response");
        };
        assert!(bytes.starts_with(b"\x1bP1+r436f=323536"));
        assert!(bytes.ends_with(b"\x1b\\"));
        term.write(b"\x1b[?1049h\x1b[2J\x1b[H~");
        assert!(term.screen.in_alternate_screen());
        assert_eq!(term.screen.cells[0][0].ch, '~');
    }

    #[test]
    fn csi_8_resize_emits_pty_resize_event() {
        let mut term = Terminal::new(24, 80);
        term.write(b"\x1b[8;30;100t");
        let pending = term.drain_pending();
        assert!(
            pending.iter().any(|e| matches!(e, TermEvent::PtyResize { rows: 30, cols: 100 })),
            "CSI 8 must request matching PTY size for ncurses apps like htop, got {pending:?}"
        );
        assert_eq!(term.screen.rows, 30);
        assert_eq!(term.screen.cols, 100);
    }

    #[test]
    fn window_size_report_18t() {
        let mut term = Terminal::new(24, 80);
        term.write(b"\x1b[18t");
        let pending = term.drain_pending();
        assert_eq!(pending.len(), 1);
        let TermEvent::Response(bytes) = pending[0].clone() else {
            panic!("expected window size report");
        };
        assert_eq!(bytes, b"\x1b[8;24;80t");
    }

    #[test]
    fn bare_lf_then_crlf_on_blank_line_is_skipped() {
        let mut term = Terminal::new(4, 40);
        term.write(b"line1\n");
        assert_eq!(term.screen.cursor_y, 1);
        term.write(b"\r\n");
        assert_eq!(
            term.screen.cursor_y, 1,
            "zsh-style \\r\\n on an already blank line must not add another row"
        );
    }

    #[test]
    fn zsh_pre_prompt_sequence_does_not_leave_percent_only_row() {
        let prompt = b"\r\n%                                                                                                                      \r \r\r\x1b[01;32mdwh@dwh-82sk\x1b[00m \x1b[01;34m~\x1b[00m \xc2\xbb ";
        let mut term = Terminal::new(6, 80);
        term.write(b"last-file-line\n");
        term.write(prompt);
        for (y, _) in term.screen.cells.iter().enumerate() {
            let line = row_plaintext(&term, y);
            if line == "%" {
                panic!("row {y} is lone %% after zsh pre-prompt sequence");
            }
        }
        let cy = term.screen.cursor_y;
        let cursor_line = row_plaintext(&term, cy);
        assert!(
            cursor_line.contains("dwh"),
            "prompt should be on cursor row {cy}, got {cursor_line:?}"
        );
    }

    #[test]
    fn zsh_prompt_sp_not_shown_when_output_ends_with_newline() {
        let mut term = Terminal::new(4, 40);
        // ls-style: line of output ending with LF, then zsh prompt (no PROMPT_SP %).
        term.write(b"file.txt\n");
        term.write(b"\rprompt> ");
        let row0: String = term.screen.cells[0].iter().map(|c| c.ch).collect();
        assert!(
            !row0.contains('%'),
            "row0 should be output, not zsh %% marker: {row0:?}"
        );
        let row1: String = term.screen.cells[1].iter().map(|c| c.ch).collect();
        assert!(
            row1.contains("prompt"),
            "prompt should follow on the next line: {row1:?}"
        );
    }

    #[test]
    fn consecutive_newlines_after_output_are_collapsed() {
        let mut term = Terminal::new(3, 80);
        term.write(b"line1");
        term.write(b"\r\n");
        assert_eq!(term.screen.cursor_y, 1);
        term.write(b"\r\n");
        assert_eq!(term.screen.cursor_y, 1, "second LF right after first should be skipped");
        term.write(b"next");
        assert_eq!(term.screen.cursor_y, 1);
        term.write(b"\r\n");
        assert_eq!(term.screen.cursor_y, 2, "LF after printed text should apply");
    }

    #[test]
    fn crlf_is_single_newline() {
        let mut term = Terminal::new(3, 10);
        term.write(b"line1\r\nline2");
        assert_eq!(term.screen.cursor_y, 1, "LF after CR should advance one row");
        assert_eq!(term.screen.cells[0][0].ch, 'l');
        assert_eq!(term.screen.cells[0][4].ch, '1');
        assert_eq!(term.screen.cells[1][0].ch, 'l');
        assert_eq!(term.screen.cells[1][4].ch, '2');
    }

    #[test]
    fn deferred_cr_applies_before_next_char() {
        let mut term = Terminal::new(1, 10);
        term.write(b"abcde\rxy");
        assert_eq!(term.screen.cells[0][0].ch, 'x');
        assert_eq!(term.screen.cells[0][1].ch, 'y');
        assert_eq!(row_plaintext(&term, 0), "xy");
    }

    #[test]
    fn el_to_eol_fills_with_current_background() {
        let mut term = Terminal::new(1, 16);
        term.write(b"\x1b[44mHEADER\x1b[0K");
        for i in 6..12 {
            assert_eq!(
                term.screen.cells[0][i].bg,
                crate::terminal::screen::Color::Indexed(4),
                "column {i} should keep blue background after EL"
            );
        }
    }

    #[test]
    fn cr_on_alternate_screen_does_not_clear_row_for_partial_redraw() {
        let mut term = Terminal::new(1, 20);
        term.write(b"\x1b[?1049h");
        term.write(b"PID 1234  %CPU 15.0\r");
        term.write(b"PID ");
        let row: String = term.screen.cells[0].iter().map(|c| c.ch).collect();
        assert!(
            row.contains('1') && row.contains('%'),
            "partial redraw after CR must keep untouched columns, got {row:?}"
        );
    }

    #[test]
    fn cr_overwrite_clears_trailing_for_progress_bar() {
        let mut term = Terminal::new(1, 80);
        term.write(
            b"Get:48 http://mirrors.example.com/debian bookworm/main amd64 linux-image-6.1.0-48-amd64 amd64 6.1.172-1 [70.2 MB]",
        );
        term.write(b"\rProgress: [ 99%] [#####################################.]");
        let row = row_plaintext(&term, 0);
        assert!(
            row.starts_with("Progress:"),
            "progress should start the line, got {row:?}"
        );
        assert!(
            !row.contains("Get:48") && !row.contains("linux-image"),
            "trailing download text must be cleared, got {row:?}"
        );
    }

    #[test]
    fn zsh_backspace_redraw_preserves_char_under_cursor() {
        let mut term = Terminal::new(1, 40);
        term.write(b"hello");
        assert_eq!(term.screen.cursor_x, 5);
        // zsh moves left with BS and re-highlights characters
        term.write(&[0x08, 0x08]);
        assert_eq!(term.screen.cursor_x, 3);
        assert_eq!(term.screen.cells[0][3].ch, 'l');
    }

    #[test]
    fn alternate_screen_uses_full_height_after_decsctbm() {
        let mut term = Terminal::new(5, 10);
        term.write(b"\x1b[2;4r");
        term.write(b"\x1b[?1049h\x1b[5;1HZ");
        assert_eq!(
            term.screen.cells[4][0].ch, 'Z',
            "CUP to last row must work on a full-height alternate buffer"
        );
    }

    #[test]
    fn ss3_cursor_left_and_right() {
        let mut term = Terminal::new(1, 10);
        term.write(b"abcde");
        assert_eq!(term.screen.cursor_x, 5);
        term.write(b"\x1bOD");
        assert_eq!(term.screen.cursor_x, 4);
        term.write(b"\x1bOC");
        assert_eq!(term.screen.cursor_x, 5);
    }

    #[test]
    fn scrollback_virtual_start_maps_history_to_viewport() {
        let mut term = Terminal::new(3, 8);
        term.write(b"AAA\nBBB\nCCC\nDDD\n");
        let sb = term.screen.scrollback_lines();
        assert!(sb >= 1, "expected scrollback after overflow");

        // offset=1: top viewport row is the newest scrollback line
        assert_eq!(
            term.screen.scrollback_row(sb.saturating_sub(1)).unwrap()[0].ch,
            term.screen.scrollback_row(sb - 1).unwrap()[0].ch
        );

        // offset=sb: top viewport row is the oldest scrollback line
        assert_eq!(term.screen.scrollback_row(0).unwrap()[0].ch, 'A');
    }

    #[test]
    fn xterm256_gray_is_not_default_fg() {
        let theme = crate::config::TerminalTheme::default();
        let gray = theme.indexed_color(244);
        assert_ne!(gray, theme.fg);
    }

    #[test]
    #[test]
    fn gray_suggest_chinese_does_not_shift_left() {
        let mut term = Terminal::new(1, 60);
        term.write(b"\x1b[01;32mdwh@dwh-82sk\x1b[00m \x1b[01;34m~\x1b[00m \xc2\xbb ");
        term.write(b"vim ");
        let bb_col = term
            .screen
            .cells[0]
            .iter()
            .position(|c| c.ch == '\u{bb}')
            .expect("»");
        term.write(b"\x1b[38;5;244m \xe8\x87\xaa\xe5\x8a\xa8\xe4\xbf\x9d\xe5\xad\x98\\ .xmi\x1b[39m");
        assert_eq!(
            term.screen.cells[0][bb_col].ch,
            '\u{bb}',
            "row: {:?}",
            term.screen.cells[0]
                .iter()
                .take(40)
                .map(|c| c.ch)
                .collect::<String>()
        );
    }

    #[test]
    fn gray_suggest_one_space_does_not_shift_left() {
        let mut term = Terminal::new(1, 40);
        term.write(b"\x1b[01;34m~\x1b[00m \xc2\xbb ");
        term.write(b"vim ");
        let bb_col = term
            .screen
            .cells[0]
            .iter()
            .position(|c| c.ch == '\u{bb}')
            .expect("» on line");
        let v_col = term
            .screen
            .cells[0]
            .iter()
            .position(|c| c.ch == 'v')
            .expect("v on line");
        let cursor_before = term.screen.cursor_x;
        term.write(b"\x1b[38;5;244m");
        assert_eq!(term.screen.cursor_x, cursor_before, "SGR must not move cursor");
        term.write(b" ");
        assert_eq!(
            term.screen.cells[0][bb_col].ch,
            '\u{bb}',
            "» must remain after gray space"
        );
        assert_eq!(term.screen.cells[0][v_col].ch, 'v');
        assert_eq!(
            term.screen.cursor_x, cursor_before,
            "POSTDISPLAY leading space does not advance cursor"
        );
    }

    #[test]
    fn zsh_autosuggest_redraw_does_not_corrupt_prompt() {
        let prompt = b"\x1b[01;32mdwh@dwh-82sk\x1b[00m \x1b[01;34m~\x1b[00m \xc2\xbb ";
        let suggest_text =
            b"\x1b[38;5;244m \xe8\x87\xaa\xe5\x8a\xa8\xe4\xbf\x9d\xe5\xad\x98\\ .xmi\x1b[39m";
        let redraw = b"\x1b[14D\x08\x08\x08\x08\x1b[32mv\x1b[32mi\x1b[32mm\x1b[39m\x1b[1C";
        let mut term = Terminal::new(1, 100);
        term.write(prompt);
        term.write(b"vim ");
        let bb_col = term
            .screen
            .cells[0]
            .iter()
            .position(|c| c.ch == '\u{bb}')
            .expect("»");
        let v_col = term
            .screen
            .cells[0]
            .iter()
            .position(|c| c.ch == 'v')
            .expect("v");
        term.write(suggest_text);
        assert_eq!(term.screen.cells[0][bb_col].ch, '\u{bb}');
        let after_suggest = term.screen.cursor_x;
        assert_eq!(after_suggest, v_col + 4 + 14, "cursor after POSTDISPLAY");
        term.write(redraw);
        let row: String = term.screen.cells[0].iter().map(|c| c.ch).collect();
        assert!(
            term.screen.cells[0][bb_col].ch == '\u{bb}',
            "» corrupted after redraw: {row:?}"
        );
        assert!(
            !row.contains("~ m"),
            "stray m in prompt region: {row:?}"
        );
        assert_eq!(term.screen.cells[0][v_col].ch, 'v');
        assert_eq!(term.screen.cursor_x, v_col + 4, "cursor after 'vim '");
    }

    #[test]
    fn postdisplay_leading_space_cursor_matches_zsh_cub() {
        let mut term = Terminal::new(1, 100);
        term.write(b"\x1b[01;32mdwh@dwh-82sk\x1b[00m \x1b[01;34m~\x1b[00m \xc2\xbb ");
        term.write(b"vim ");
        let start = term.screen.cursor_x;
        term.write(b"\x1b[38;5;244m \xe8\x87\xaa\xe5\x8a\xa8\xe4\xbf\x9d\xe5\xad\x98\\ .xmi\x1b[39m");
        assert_eq!(
            term.screen.cursor_x,
            start + 14,
            "cursor must advance 14 cols (zsh CUB count), not 15"
        );
    }

    #[test]
    fn vim_space_then_gray_autosuggest() {
        use crate::terminal::screen::Color;

        let prompt = b"\x1b[01;32mprompt\x1b[00m \xc2\xbb ";
        let mut term = Terminal::new(1, 60);
        term.write(prompt);
        let start = term.screen.cursor_x;
        term.write(b"vim ");
        assert_eq!(term.screen.cursor_x, start + 4);
        // zsh: clear suffix, print gray suggestion, restore cursor
        term.write(b"\x1b[K\x1b[s\x1b[38;5;244m run\x1b[0m\x1b[u");
        assert_eq!(term.screen.cursor_x, start + 4);
        let row = &term.screen.cells[0];
        assert_eq!(row[start].ch, 'v');
        assert_eq!(row[start + 1].ch, 'i');
        assert_eq!(row[start + 2].ch, 'm');
        assert_eq!(row[start + 3].ch, ' ');
        assert_eq!(row[start + 4].ch, 'r');
        assert_eq!(row[start + 4].fg, Color::Indexed(244));
        assert_eq!(row[start + 5].ch, 'u');
        assert_eq!(row[start + 5].fg, Color::Indexed(244));
    }

    #[test]
    fn scosc_scorc_for_inline_suggestion() {
        use crate::terminal::screen::Color;

        let mut term = Terminal::new(1, 30);
        term.write(b"ab");
        assert_eq!(term.screen.cursor_x, 2);
        term.write(b"\x1b[s\x1b[38;5;244mzzz\x1b[0m\x1b[u");
        assert_eq!(term.screen.cursor_x, 2);
        let row = &term.screen.cells[0];
        assert_eq!(row[2].ch, 'z');
        assert_eq!(row[2].fg, Color::Indexed(244));
    }
}
