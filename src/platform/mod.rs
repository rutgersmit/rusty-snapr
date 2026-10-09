//! Thin, safe wrappers around the Win32 calls Snapr needs. All `unsafe` code lives here.

pub mod clipboard;
pub mod errors;
pub mod gdi;
pub mod instance;
pub mod locale;
pub mod paths;
pub mod screen;
pub mod tray;
pub mod window;

/// A null-terminated UTF-16 copy of `s`, for Win32 string parameters.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copies `s` into a fixed-size UTF-16 buffer, truncating if needed and always null-terminating.
pub fn copy_wide(buffer: &mut [u16], s: &str) {
    let max = buffer.len().saturating_sub(1);
    let mut len = 0;
    for (slot, unit) in buffer.iter_mut().zip(s.encode_utf16().take(max)) {
        *slot = unit;
        len += 1;
    }
    if let Some(end) = buffer.get_mut(len) {
        *end = 0;
    }
}
