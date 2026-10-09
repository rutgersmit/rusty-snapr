//! Screen capture and monitor geometry, in physical pixels (the app is per-monitor DPI aware).

use snapr_core::geometry::IRect;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    BitBlt, GetDC, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    ReleaseDC, SRCCOPY,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};
use windows::core::Result;

use super::gdi::{Dib, MemDc};

/// A frozen copy of the whole virtual desktop.
pub struct ScreenShot {
    /// The pixels. Alpha is undefined, as always after a screen capture.
    pub image: Dib,
    /// Position and size of the virtual desktop in screen coordinates (may start at negative
    /// coordinates when a monitor is left of or above the primary one).
    pub bounds: IRect,
}

pub fn virtual_screen_bounds() -> IRect {
    // SAFETY: plain metric queries.
    unsafe {
        IRect::new(
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

pub fn capture_virtual_screen() -> Result<ScreenShot> {
    let bounds = virtual_screen_bounds();
    let image = Dib::new(bounds.width, bounds.height)?;
    {
        let target = MemDc::new(image.handle())?;
        // SAFETY: copies from the screen DC into our DIB; the screen DC is released afterwards.
        unsafe {
            let screen = GetDC(None);
            let result = BitBlt(
                target.hdc(),
                0,
                0,
                bounds.width,
                bounds.height,
                Some(screen),
                bounds.x,
                bounds.y,
                SRCCOPY,
            );
            ReleaseDC(None, screen);
            result?;
        }
    }
    Ok(ScreenShot { image, bounds })
}

pub fn cursor_position() -> (i32, i32) {
    let mut point = POINT::default();
    // SAFETY: writes the cursor position into `point`.
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    (point.x, point.y)
}

/// The full rectangle of the monitor nearest to a screen point.
pub fn monitor_rect_at(point: (i32, i32)) -> IRect {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: MonitorFromPoint with DEFAULTTONEAREST always returns a monitor.
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: point.0,
                y: point.1,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let _ = GetMonitorInfoW(monitor, &mut info);
    }
    let r = info.rcMonitor;
    IRect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}

/// Display scale of the monitor nearest to a screen point (1.0 = 96 DPI = 100%).
pub fn scale_at(point: (i32, i32)) -> f64 {
    let (mut x, mut y) = (96u32, 96u32);
    // SAFETY: MonitorFromPoint with DEFAULTTONEAREST always returns a monitor.
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: point.0,
                y: point.1,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y);
    }
    x.max(1) as f64 / 96.0
}
