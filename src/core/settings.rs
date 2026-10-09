//! settings.json, in exactly the format of the C# version (PascalCase keys).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hotkey::HotkeyGesture;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct AppSettings {
    pub hotkey: String,
    /// Empty means "not set"; see [`AppSettings::fill_defaults`].
    pub save_folder: String,
    pub copy_on_capture: bool,
    pub start_with_windows: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: HotkeyGesture::DEFAULT.to_string(),
            save_folder: String::new(),
            copy_on_capture: true,
            start_with_windows: false,
        }
    }
}

impl AppSettings {
    /// Fills in defaults that depend on the machine (the Pictures folder isn't known here).
    pub fn fill_defaults(&mut self, default_save_folder: &Path) {
        if self.save_folder.trim().is_empty() {
            self.save_folder = default_save_folder.to_string_lossy().into_owned();
        }
        if self.hotkey.trim().is_empty() {
            self.hotkey = HotkeyGesture::DEFAULT.to_string();
        }
    }

    pub fn hotkey_gesture(&self) -> HotkeyGesture {
        HotkeyGesture::parse(&self.hotkey)
    }

    pub fn save_folder(&self) -> PathBuf {
        PathBuf::from(&self.save_folder)
    }

    /// Reads the settings file. A missing or broken file never stops the app: it gives defaults.
    pub fn load(path: &Path) -> AppSettings {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(strip_bom(&text)).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(path, json)
    }
}

/// Files edited in Notepad may start with a byte-order mark, which serde_json rejects.
fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("snapr-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn reads_a_file_written_by_the_csharp_version() {
        // System.Text.Json with WriteIndented = true.
        let json = r#"{
  "Hotkey": "Ctrl\u002BShift\u002BS",
  "SaveFolder": "C:\\Users\\rutger\\Pictures\\Screenshots",
  "CopyOnCapture": false,
  "StartWithWindows": true
}"#;
        let settings: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(
            settings,
            AppSettings {
                hotkey: "Ctrl+Shift+S".into(),
                save_folder: r"C:\Users\rutger\Pictures\Screenshots".into(),
                copy_on_capture: false,
                start_with_windows: true,
            }
        );
    }

    #[test]
    fn missing_fields_get_defaults() {
        let settings: AppSettings =
            serde_json::from_str(r#"{ "StartWithWindows": true }"#).unwrap();
        assert_eq!(settings.hotkey, "PrintScreen");
        assert!(settings.copy_on_capture);
        assert!(settings.start_with_windows);
    }

    #[test]
    fn writes_pascal_case_keys() {
        let json = serde_json::to_string(&AppSettings::default()).unwrap();
        assert_eq!(
            json,
            r#"{"Hotkey":"PrintScreen","SaveFolder":"","CopyOnCapture":true,"StartWithWindows":false}"#
        );
    }

    #[test]
    fn broken_or_missing_file_gives_defaults() {
        let dir = temp_dir("broken");
        let path = dir.join("settings.json");
        assert_eq!(AppSettings::load(&path), AppSettings::default());

        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(AppSettings::load(&path), AppSettings::default());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("sub").join("settings.json");
        let settings = AppSettings {
            hotkey: "Ctrl+Alt+P".into(),
            save_folder: "/tmp/shots".into(),
            copy_on_capture: false,
            start_with_windows: true,
        };
        settings.save(&path).unwrap();
        assert_eq!(AppSettings::load(&path), settings);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn byte_order_mark_is_ignored() {
        let dir = temp_dir("bom");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(&path, "\u{feff}{\"Hotkey\":\"F9\"}").unwrap();
        assert_eq!(AppSettings::load(&path).hotkey, "F9");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fill_defaults_only_fills_blanks() {
        let mut settings = AppSettings {
            hotkey: " ".into(),
            ..AppSettings::default()
        };
        settings.fill_defaults(Path::new("/pics/Screenshots"));
        assert_eq!(settings.save_folder, "/pics/Screenshots");
        assert_eq!(settings.hotkey, "PrintScreen");

        settings.save_folder = "/elsewhere".into();
        settings.fill_defaults(Path::new("/pics/Screenshots"));
        assert_eq!(settings.save_folder, "/elsewhere");
    }
}
