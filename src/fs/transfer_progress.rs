use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::session::TransferSnapshot;

/// Byte-accurate transfer progress shared between UI and worker threads.
pub struct ByteProgress {
    pub cancel: Arc<AtomicBool>,
    snapshot: Arc<Mutex<TransferSnapshot>>,
    done_bytes: AtomicU64,
    pub total_bytes: u64,
}

impl ByteProgress {
    pub fn new(
        cancel: Arc<AtomicBool>,
        snapshot: Arc<Mutex<TransferSnapshot>>,
        total_bytes: u64,
    ) -> Arc<Self> {
        Arc::new(Self {
            cancel,
            snapshot,
            done_bytes: AtomicU64::new(0),
            total_bytes: total_bytes.max(1),
        })
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn add_bytes(&self, n: u64, label: &str) {
        let done = self.done_bytes.fetch_add(n, Ordering::Relaxed) + n;
        let p = (done as f64 / self.total_bytes as f64) as f32;
        if let Ok(mut s) = self.snapshot.lock() {
            s.progress = p.clamp(0.0, 1.0);
            s.label = label.to_string();
        }
    }

    pub fn set_label(&self, label: &str) {
        if let Ok(mut s) = self.snapshot.lock() {
            s.label = label.to_string();
        }
    }
}

pub fn format_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if n >= GB {
        format!("{:.2} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.2} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}
