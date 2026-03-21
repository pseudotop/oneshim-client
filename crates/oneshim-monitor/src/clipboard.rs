use oneshim_core::config::PiiFilterLevel;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

// Event types are canonical in oneshim-core; re-exported here.
pub use oneshim_core::models::event::{ClipboardContentType, ClipboardEvent};

/// Clipboard change detection monitor.
///
/// Tracks clipboard text changes by hashing content (never stores raw text
/// for privacy). The monitor is designed to be wrapped in `Arc` and polled
/// from an async loop.
///
/// # Platform support
///
/// `poll_system_clipboard()` reads the system clipboard via platform commands:
/// - **macOS**: `pbpaste`
/// - **Linux**: `xclip -selection clipboard -o`
/// - **Windows**: PowerShell `Get-Clipboard`
///
/// If the clipboard read fails (e.g., no clipboard daemon on headless Linux),
/// the poll silently returns `None`.
pub struct ClipboardMonitor {
    last_content_hash: Mutex<u64>,
    pii_filter_level: Mutex<PiiFilterLevel>,
    /// Cumulative clipboard change count since last `take_change_count()`.
    change_count: AtomicU32,
}

impl ClipboardMonitor {
    pub fn new(pii_level: PiiFilterLevel) -> Self {
        Self {
            last_content_hash: Mutex::new(0),
            pii_filter_level: Mutex::new(pii_level),
            change_count: AtomicU32::new(0),
        }
    }

    /// Check if the given text represents a new clipboard value.
    /// Returns a `ClipboardEvent` if the content changed, `None` otherwise.
    pub fn check_text_change(&self, text: &str) -> Option<ClipboardEvent> {
        let hash = hash_string(text);

        let mut last_hash = self.last_content_hash.lock().ok()?;
        if hash == *last_hash {
            return None;
        }
        *last_hash = hash;

        self.change_count.fetch_add(1, Ordering::Relaxed);

        let pii_level = self
            .pii_filter_level
            .lock()
            .map(|g| *g)
            .unwrap_or(PiiFilterLevel::Standard);

        let preview = if pii_level != PiiFilterLevel::Off {
            Some(truncate(text, 50))
        } else {
            None
        };

        Some(ClipboardEvent {
            timestamp: chrono::Utc::now(),
            content_type: ClipboardContentType::Text,
            char_count: text.len(),
            preview,
        })
    }

    /// Poll the system clipboard for changes.
    ///
    /// Reads clipboard text via platform-native commands and checks whether
    /// it differs from the previously seen content. Returns `Some(event)`
    /// on change, `None` if unchanged or if the clipboard read fails.
    pub fn poll_system_clipboard(&self) -> Option<ClipboardEvent> {
        let text = read_system_clipboard()?;
        if text.is_empty() {
            return None;
        }
        self.check_text_change(&text)
    }

    pub fn set_pii_filter_level(&self, level: PiiFilterLevel) {
        if let Ok(mut pii) = self.pii_filter_level.lock() {
            *pii = level;
        }
    }

    /// Return the number of clipboard changes since the last call and reset
    /// the counter to zero. Designed for periodic snapshot collection.
    pub fn take_change_count(&self) -> u32 {
        self.change_count.swap(0, Ordering::Relaxed)
    }
}

/// Read clipboard text from the system clipboard using platform commands.
///
/// Returns `None` if the command fails or produces no output.
fn read_system_clipboard() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("pbpaste").output().ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            None
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Try xclip first, fall back to xsel
        let output = std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output()
            .or_else(|_| {
                std::process::Command::new("xsel")
                    .args(["--clipboard", "--output"])
                    .output()
            })
            .ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            None
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "Get-Clipboard"])
            .output()
            .ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

fn hash_string(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut result: String = s.chars().take(max_len).collect();
        result.push_str("...");
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_text_change() {
        let monitor = ClipboardMonitor::new(PiiFilterLevel::Standard);
        let event = monitor.check_text_change("hello world");
        assert!(event.is_some());
        let evt = event.unwrap();
        assert_eq!(evt.content_type, ClipboardContentType::Text);
        assert_eq!(evt.char_count, 11);
    }

    #[test]
    fn no_change_on_same_text() {
        let monitor = ClipboardMonitor::new(PiiFilterLevel::Standard);
        monitor.check_text_change("hello");
        let event = monitor.check_text_change("hello");
        assert!(event.is_none());
    }

    #[test]
    fn preview_included_with_filter() {
        let monitor = ClipboardMonitor::new(PiiFilterLevel::Standard);
        let event = monitor.check_text_change("short").unwrap();
        assert!(event.preview.is_some());
    }

    #[test]
    fn no_preview_when_off() {
        let monitor = ClipboardMonitor::new(PiiFilterLevel::Off);
        let event = monitor.check_text_change("something").unwrap();
        assert!(event.preview.is_none());
    }

    #[test]
    fn change_count_increments() {
        let monitor = ClipboardMonitor::new(PiiFilterLevel::Standard);
        monitor.check_text_change("first");
        monitor.check_text_change("second");
        monitor.check_text_change("second"); // duplicate — no increment
        assert_eq!(monitor.take_change_count(), 2);
        assert_eq!(monitor.take_change_count(), 0); // reset after take
    }

    #[test]
    fn set_pii_level_changes_preview() {
        let monitor = ClipboardMonitor::new(PiiFilterLevel::Off);
        let event = monitor.check_text_change("private data").unwrap();
        assert!(event.preview.is_none());

        monitor.set_pii_filter_level(PiiFilterLevel::Standard);
        let event = monitor.check_text_change("more data").unwrap();
        assert!(event.preview.is_some());
    }
}
