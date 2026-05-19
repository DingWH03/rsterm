use log::warn;

/// Read plain text from the system clipboard.
pub fn read_text() -> Option<String> {
    match read_text_result() {
        Ok(Some(text)) if !text.is_empty() => Some(text),
        Ok(_) => None,
        Err(e) => {
            warn!("clipboard read failed: {e}");
            None
        }
    }
}

/// Write plain text to the system clipboard.
pub fn write_text(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    match write_text_result(text) {
        Ok(()) => true,
        Err(e) => {
            warn!("clipboard write failed: {e}");
            false
        }
    }
}

#[cfg(not(target_os = "android"))]
fn read_text_result() -> Result<Option<String>, String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    match cb.get_text() {
        Ok(text) => Ok(Some(text)),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(not(target_os = "android"))]
fn write_text_result(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_owned()).map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
fn read_text_result() -> Result<Option<String>, String> {
    match android_clipboard::get_text() {
        Ok(text) => Ok(Some(text)),
        Err(android_clipboard::Error::ContentNotAvailable) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(target_os = "android")]
fn write_text_result(text: &str) -> Result<(), String> {
    android_clipboard::set_text(text.to_owned()).map_err(|e| e.to_string())
}
