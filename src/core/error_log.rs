//! Appends unexpected errors to snapr.log, keeping the file small. Logging never fails loudly.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Above this size the log is moved to `snapr.log.old` and a new one is started.
pub const MAX_SIZE: u64 = 512 * 1024;

pub struct ErrorLog {
    path: PathBuf,
    gate: Mutex<()>,
}

impl ErrorLog {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            gate: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&self, source: &str, details: &str) {
        let _guard = self
            .gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Logging must never cause a second failure.
        let _ = self.try_write(source, details);
    }

    fn try_write(&self, source: &str, details: &str) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }

        // Keep one previous log around instead of growing forever.
        if fs::metadata(&self.path).is_ok_and(|m| m.len() > MAX_SIZE) {
            let mut old = self.path.clone().into_os_string();
            old.push(".old");
            fs::rename(&self.path, old)?;
        }

        let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        write!(file, "[{time}] {source}\r\n{details}\r\n\r\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_and_rotates() {
        let dir = std::env::temp_dir().join(format!("snapr-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let log = ErrorLog::new(dir.join("snapr.log"));

        log.write("UI", "first");
        log.write("Tray", "second");
        let text = fs::read_to_string(log.path()).unwrap();
        assert!(text.contains("] UI\r\nfirst"));
        assert!(text.contains("] Tray\r\nsecond"));

        fs::write(log.path(), vec![b'x'; MAX_SIZE as usize + 1]).unwrap();
        log.write("UI", "after rotation");
        assert!(
            fs::read_to_string(log.path())
                .unwrap()
                .contains("after rotation")
        );
        assert_eq!(
            fs::metadata(dir.join("snapr.log.old")).unwrap().len(),
            MAX_SIZE + 1
        );

        fs::remove_dir_all(&dir).unwrap();
    }
}
