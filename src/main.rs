//! Snapr (Rust): a small screenshot tool for Windows.

// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod platform;

#[cfg(windows)]
fn main() {
    std::process::exit(app::run());
}

#[cfg(not(windows))]
fn main() {
    eprintln!(
        "Snapr only runs on Windows. The core library (`cargo test --lib`) works everywhere."
    );
    std::process::exit(1);
}
