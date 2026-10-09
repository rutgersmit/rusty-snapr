//! The platform-independent core of Snapr: settings, hotkeys, texts, error log and the editor
//! logic. Nothing in here talks to Windows, so all of it runs under `cargo test` on any OS.

pub mod editor;
pub mod error_log;
pub mod geometry;
pub mod hotkey;
pub mod settings;
pub mod strings;

/// Shown in the tray, notifications and window titles, so it can't be confused with the C# Snapr
/// while both run side by side.
pub const APP_NAME: &str = "Snapr (Rust)";

/// Folder under %AppData% for settings and the log. Differs from the C# version ("Snapr").
pub const DATA_FOLDER_NAME: &str = "RustySnapr";
