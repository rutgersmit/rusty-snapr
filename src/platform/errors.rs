//! Unexpected errors: log every one, tell the user (one dialog at a time), and keep running.
//! A tray app that dies silently also takes the hotkey with it.

use std::backtrace::Backtrace;
use std::cell::{Cell, RefCell};
use std::panic::{self, PanicHookInfo};
use std::sync::OnceLock;

use snapr_core::APP_NAME;
use snapr_core::error_log::ErrorLog;
use snapr_core::strings;
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MessageBoxW,
};
use windows::core::PCWSTR;

use super::wide;

static LOG: OnceLock<ErrorLog> = OnceLock::new();

thread_local! {
    /// What the UI thread is doing, for the log ("Tray", "UI", …).
    static SOURCE: Cell<&'static str> = const { Cell::new("Main") };
    static LAST_PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
    static SHOWING_ERROR: Cell<bool> = const { Cell::new(false) };
}

/// Sets up the log and a panic hook that writes every panic to it.
pub fn init(log: ErrorLog) {
    let _ = LOG.set(log);
    panic::set_hook(Box::new(|info| {
        let message = panic_message(info);
        let location = info
            .location()
            .map(|l| format!(" ({}:{})", l.file(), l.line()))
            .unwrap_or_default();
        let backtrace = Backtrace::force_capture();
        write_log(
            SOURCE.get(),
            &format!("panic: {message}{location}\r\n{backtrace}"),
        );
        LAST_PANIC.set(Some(message));
    }));
}

pub fn write_log(source: &str, details: &str) {
    if let Some(log) = LOG.get() {
        log.write(source, details);
    }
}

/// Runs `f` with `source` as the label for anything it logs.
pub fn with_source<T>(source: &'static str, f: impl FnOnce() -> T) -> T {
    let previous = SOURCE.replace(source);
    let result = f();
    SOURCE.set(previous);
    result
}

/// After a caught panic: tells the user, with the panic message from the hook.
pub fn report_panic() {
    let message = LAST_PANIC.take().unwrap_or_else(|| "Unknown error".into());
    show_error(&message);
}

/// Logs an error that was returned rather than panicked, and tells the user.
pub fn report_error(source: &'static str, error: &dyn std::fmt::Display) {
    write_log(source, &error.to_string());
    show_error(&error.to_string());
}

fn show_error(message: &str) {
    // One dialog at a time, even if errors repeat while it is open.
    if SHOWING_ERROR.replace(true) {
        return;
    }
    let log_path = LOG
        .get()
        .map(|l| l.path().display().to_string())
        .unwrap_or_default();
    message_box(&strings::get().unexpected_error(message, &log_path));
    SHOWING_ERROR.set(false);
}

/// A plain warning box with the app name as caption.
pub fn message_box(text: &str) {
    let text = wide(text);
    let caption = wide(APP_NAME);
    // SAFETY: both strings are null-terminated and outlive the call.
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | MB_ICONWARNING | MB_SETFOREGROUND,
        );
    }
}

fn panic_message(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".into())
}
