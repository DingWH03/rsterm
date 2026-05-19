use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct EntryInfo {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub permissions: String,
    pub modified: String,
}

pub fn local_entry_info(path: &Path) -> Result<EntryInfo, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let is_dir = meta.is_dir();
    let size = if is_dir {
        dir_size(path)
    } else {
        meta.len()
    };
    Ok(EntryInfo {
        path: path.display().to_string(),
        name,
        kind: if is_dir { "Folder".into() } else { "File".into() },
        size,
        permissions: format_mode(&meta),
        modified: format_time(meta.modified().ok()),
    })
}

pub fn dir_size(path: &Path) -> u64 {
    let Ok(meta) = fs::metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    if !meta.is_dir() {
        return 0;
    }
    let mut total = 0u64;
    if let Ok(read) = fs::read_dir(path) {
        for item in read.flatten() {
            let p = item.path();
            total += dir_size(&p);
        }
    }
    total
}

pub fn local_paths_total_bytes(paths: &[String]) -> u64 {
    paths.iter().map(|p| dir_size(Path::new(p))).sum()
}

#[cfg(unix)]
fn format_mode(meta: &fs::Metadata) -> String {
    use std::os::unix::fs::PermissionsExt;
    let mode = meta.permissions().mode();
    format_unix_mode(mode)
}

#[cfg(not(unix))]
fn format_mode(_meta: &fs::Metadata) -> String {
    "—".into()
}

pub fn format_unix_mode(mode: u32) -> String {
    const BITS: [(u32, char); 9] = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    let mut s = String::with_capacity(9);
    for (bit, ch) in BITS {
        s.push(if mode & bit != 0 { ch } else { '-' });
    }
    format!("{s} ({mode:o})")
}

pub fn format_time(t: Option<SystemTime>) -> String {
    let Some(ts) = t else {
        return "—".into();
    };
    let Ok(dur) = ts.duration_since(SystemTime::UNIX_EPOCH) else {
        return "—".into();
    };
    let secs = dur.as_secs();
    // UTC wall time without extra deps
    let days = secs / 86400;
    let time = secs % 86400;
    let h = time / 3600;
    let m = (time % 3600) / 60;
    let s = time % 60;
    let (y, mo, d) = epoch_days_to_ymd(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02} UTC")
}

fn epoch_days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Algorithm from civil date conversion (simplified Gregorian)
    days += 719468;
    let era = days / 146097;
    let doe = days % 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mp < 10 { y } else { y + 1 };
    (y, mo, d)
}
