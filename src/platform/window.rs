//! Window class registration and panic-safe window procedures.

use std::panic::{AssertUnwindSafe, catch_unwind};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, RegisterClassW, WNDCLASSW, WNDPROC};
use windows::core::{PCWSTR, Result};

use super::errors;

pub fn instance() -> HINSTANCE {
    // SAFETY: getting the handle of our own exe always succeeds.
    unsafe { GetModuleHandleW(None) }
        .map(Into::into)
        .unwrap_or_default()
}

pub fn register_class(name: PCWSTR, wndproc: WNDPROC) -> Result<()> {
    let class = WNDCLASSW {
        lpfnWndProc: wndproc,
        hInstance: instance(),
        lpszClassName: name,
        ..Default::default()
    };
    // SAFETY: `class` is fully initialised and `name` is a static string.
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(windows::core::Error::from_thread());
    }
    Ok(())
}

/// Runs a window procedure body, catching panics.
///
/// A panic must never unwind out of an `extern "system"` function (that aborts the process), so
/// every window procedure goes through here. The panic is logged by the hook, the user is told,
/// and the message falls back to default handling, so the app keeps running.
pub fn guarded(
    source: &'static str,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    body: impl FnOnce() -> LRESULT,
) -> LRESULT {
    let result = errors::with_source(source, || catch_unwind(AssertUnwindSafe(body)));
    match result {
        Ok(result) => result,
        Err(_) => {
            errors::report_panic();
            default_proc(hwnd, msg, wparam, lparam)
        }
    }
}

pub fn default_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: forwards the message unchanged to Windows' default handling.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
