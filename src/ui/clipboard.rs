use log::warn;

/// Read system clipboard text (X11 / Wayland / etc.).
pub fn read_text() -> Option<String> {
    match arboard::Clipboard::new() {
        Ok(mut cb) => match cb.get_text() {
            Ok(text) if !text.is_empty() => Some(text),
            Ok(_) => None,
            Err(e) => {
                warn!("clipboard read failed: {e}");
                None
            }
        },
        Err(e) => {
            warn!("clipboard init failed: {e}");
            None
        }
    }
}
