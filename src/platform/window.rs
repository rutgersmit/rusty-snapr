//! Window classes, per-window state, painting and panic-safe window procedures.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use snapr_core::geometry::IRect;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CombineRgn, CreateRectRgn, DeleteObject, EndPaint, GetRegionData, GetUpdateRgn,
    HDC, HRGN, InvalidateRect, InvalidateRgn, PAINTSTRUCT, RGN_XOR, RGNDATA, RGNDATAHEADER,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect,
    GetSystemMetrics, GetWindowLongPtrW, HICON, HWND_TOPMOST, IDC_ARROW, IMAGE_ICON,
    LR_DEFAULTCOLOR, LoadCursorW, LoadImageW, RegisterClassExW, SM_CXICON, SM_CXSMICON, SM_CYICON,
    SM_CYSMICON, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SYSTEM_METRICS_INDEX,
    SetCursor, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_NCCREATE, WNDCLASSEXW, WNDPROC,
};
use windows::core::{PCWSTR, Result};

use super::{errors, wide};

pub fn instance() -> HINSTANCE {
    // SAFETY: getting the handle of our own exe always succeeds.
    unsafe { GetModuleHandleW(None) }
        .map(Into::into)
        .unwrap_or_default()
}

/// Registers a window class. `cursor` is a system cursor such as `IDC_CROSS` (default: arrow);
/// `app_icon` gives its windows the Snapr icon.
pub fn register_class(
    name: PCWSTR,
    wndproc: WNDPROC,
    cursor: Option<PCWSTR>,
    app_icon: bool,
) -> Result<()> {
    // SAFETY: loads a shared system cursor and our own icon resource.
    let (cursor, icon, small_icon) = unsafe {
        let cursor = LoadCursorW(None, cursor.unwrap_or(IDC_ARROW)).unwrap_or_default();
        let (icon, small_icon) = if app_icon {
            (
                load_app_icon(SM_CXICON, SM_CYICON),
                load_app_icon(SM_CXSMICON, SM_CYSMICON),
            )
        } else {
            (HICON::default(), HICON::default())
        };
        (cursor, icon, small_icon)
    };
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: wndproc,
        hInstance: instance(),
        lpszClassName: name,
        hCursor: cursor,
        hIcon: icon,
        hIconSm: small_icon,
        ..Default::default()
    };
    // SAFETY: `class` is fully initialised and `name` is a static string.
    if unsafe { RegisterClassExW(&class) } == 0 {
        return Err(windows::core::Error::from_thread());
    }
    Ok(())
}

/// The app icon (resource 1) at a system metric size.
///
/// # Safety
/// Plain resource loading; no preconditions beyond a valid module.
unsafe fn load_app_icon(cx: SYSTEM_METRICS_INDEX, cy: SYSTEM_METRICS_INDEX) -> HICON {
    // SAFETY: see above.
    unsafe {
        LoadImageW(
            Some(instance()),
            PCWSTR(std::ptr::without_provenance(1)), // MAKEINTRESOURCE(1)
            IMAGE_ICON,
            GetSystemMetrics(cx),
            GetSystemMetrics(cy),
            LR_DEFAULTCOLOR,
        )
        .map(|h| HICON(h.0))
        .unwrap_or_default()
    }
}

/// Creates a window that carries `state`. The window procedure gets it back with [`state`] and
/// must call [`release_state`] on `WM_NCDESTROY`.
pub fn create_window(
    class: PCWSTR,
    title: &str,
    ex_style: WINDOW_EX_STYLE,
    style: WINDOW_STYLE,
    rect: IRect,
    state: Rc<dyn Any>,
) -> Result<HWND> {
    let title = wide(title);
    // Handed over through lpCreateParams and stored in GWLP_USERDATA on WM_NCCREATE.
    let state = Box::into_raw(Box::new(state));
    // SAFETY: the class is registered by the caller; `title` outlives the call.
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class,
            PCWSTR(title.as_ptr()),
            style,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            None,
            None,
            Some(instance()),
            Some(state as *const _),
        )
    };
    // If creation fails before WM_NCCREATE the state leaks; that only happens when the class is
    // missing, which is a programming error. After WM_NCCREATE, WM_NCDESTROY releases it.
    hwnd
}

/// The state a window was created with, if it has state of type `S`.
pub fn state<S: 'static>(hwnd: HWND) -> Option<Rc<S>> {
    // SAFETY: GWLP_USERDATA is either 0 or a pointer stored by attach_state, which stays valid
    // until release_state resets it.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Rc<dyn Any>;
    if ptr.is_null() {
        return None;
    }
    // SAFETY: see above.
    let state = unsafe { &*ptr }.clone();
    state.downcast::<S>().ok()
}

/// Detaches the state on `WM_NCDESTROY`. Returns it so the caller can finish up; it is freed
/// once the last clone (possibly held by a handler further up the stack) is dropped.
pub fn release_state<S: 'static>(hwnd: HWND) -> Option<Rc<S>> {
    // SAFETY: as in `state`; the pointer is reset first so nothing can use it afterwards.
    let ptr = unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Rc<dyn Any>;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        ptr
    };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the pointer came from Box::into_raw in create_window and is freed exactly once.
    let state = unsafe { Box::from_raw(ptr) };
    state.downcast::<S>().ok()
}

fn attach_state(hwnd: HWND, lparam: LPARAM) {
    // SAFETY: on WM_NCCREATE, lparam points to the CREATESTRUCTW of this window.
    unsafe {
        let create = &*(lparam.0 as *const CREATESTRUCTW);
        let ptr = create.lpCreateParams as *mut Rc<dyn Any>;
        if !ptr.is_null() {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
        }
    }
}

/// Runs a window procedure body, catching panics.
///
/// A panic must never unwind out of an `extern "system"` function (that aborts the process), so
/// every window procedure goes through here. The panic is logged by the hook, the user is told,
/// and the message falls back to default handling, so the app keeps running. Also attaches the
/// state passed to [`create_window`] on `WM_NCCREATE`.
pub fn guarded(
    source: &'static str,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    body: impl FnOnce() -> LRESULT,
) -> LRESULT {
    if msg == WM_NCCREATE {
        attach_state(hwnd, lparam);
    }
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

/// Signed client coordinates from a mouse message's lparam.
pub fn mouse_position(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    (x, y)
}

pub fn destroy(hwnd: HWND) {
    // SAFETY: destroying our own window; WM_NCDESTROY releases its state.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

/// Shows a window on top of everything at exactly `rect` (physical pixels) and gives it focus.
pub fn show_topmost(hwnd: HWND, rect: IRect) {
    // SAFETY: plain window management calls on our own window.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            SWP_SHOWWINDOW,
        );
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
    }
}

/// Moves a window back to exactly `rect`, e.g. after Windows resized it on a DPI change.
pub fn set_topmost_bounds(hwnd: HWND, rect: IRect) {
    // SAFETY: plain window management call on our own window.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            SWP_SHOWWINDOW,
        );
    }
}

pub fn capture_mouse(hwnd: HWND) {
    // SAFETY: plain call on our own window.
    unsafe {
        SetCapture(hwnd);
    }
}

pub fn release_mouse() {
    // SAFETY: no preconditions.
    unsafe {
        let _ = ReleaseCapture();
    }
}

fn to_rect(r: IRect) -> RECT {
    RECT {
        left: r.x,
        top: r.y,
        right: r.right(),
        bottom: r.bottom(),
    }
}

/// Marks a client rectangle for repainting.
pub fn invalidate(hwnd: HWND, rect: IRect) {
    if rect.is_empty() {
        return;
    }
    let rect = to_rect(rect);
    // SAFETY: plain call on our own window.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), Some(&rect), false);
    }
}

/// Marks only the area that differs between two rectangles (their symmetric difference).
pub fn invalidate_difference(hwnd: HWND, a: IRect, b: IRect) {
    // SAFETY: the regions are created and deleted here.
    unsafe {
        let ra = CreateRectRgn(a.x, a.y, a.right(), a.bottom());
        let rb = CreateRectRgn(b.x, b.y, b.right(), b.bottom());
        CombineRgn(Some(ra), Some(ra), Some(rb), RGN_XOR);
        let _ = InvalidateRgn(hwnd, Some(ra), false);
        let _ = DeleteObject(ra.into());
        let _ = DeleteObject(rb.into());
    }
}

/// Handles `WM_PAINT`: calls `paint` for each rectangle of the update region (not just their
/// bounding box, so a thin line at each end of the screen doesn't repaint everything).
pub fn paint(hwnd: HWND, mut paint: impl FnMut(HDC, IRect)) {
    let rects = update_rects(hwnd);
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: BeginPaint/EndPaint pair on our own window.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        let rects = if rects.is_empty() {
            let r = ps.rcPaint;
            vec![IRect::new(
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
            )]
        } else {
            rects
        };
        for rect in rects.into_iter().filter(|r| !r.is_empty()) {
            paint(hdc, rect);
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

fn update_rects(hwnd: HWND) -> Vec<IRect> {
    // SAFETY: the region is created and deleted here; the buffer is sized as GetRegionData asks.
    unsafe {
        let region: HRGN = CreateRectRgn(0, 0, 0, 0);
        let mut rects = Vec::new();
        GetUpdateRgn(hwnd, region, false);
        let size = GetRegionData(region, 0, None);
        if size as usize >= size_of::<RGNDATAHEADER>() {
            // u64 for alignment of the header.
            let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
            let data = buffer.as_mut_ptr().cast::<RGNDATA>();
            if GetRegionData(region, size, Some(data)) != 0 {
                let header = &(*data).rdh;
                let first = (*data).Buffer.as_ptr().cast::<RECT>();
                for i in 0..header.nCount as usize {
                    let r = *first.add(i);
                    rects.push(IRect::new(
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                    ));
                }
            }
        }
        let _ = DeleteObject(region.into());
        rects
    }
}

/// Dark title bar, to match the dark editor (Windows 10 20H1 and later; ignored elsewhere).
pub fn use_dark_title_bar(hwnd: HWND) {
    let on: i32 = 1;
    // SAFETY: passes a 4-byte BOOL for a documented attribute.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&raw const on).cast(),
            size_of::<i32>() as u32,
        );
    }
}

/// Display scale of the monitor a window is on (1.0 = 96 DPI).
pub fn scale_of(hwnd: HWND) -> f64 {
    // SAFETY: plain query on our own window.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 }
}

/// The work area (screen minus taskbar) of the monitor a window is mostly on.
pub fn work_area_of(hwnd: HWND) -> IRect {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: MonitorFromWindow with DEFAULTTONEAREST always returns a monitor.
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let _ = GetMonitorInfoW(monitor, &mut info);
    }
    let r = info.rcWork;
    IRect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}

/// Outer window size for a client size, including the frame at the given scale.
pub fn outer_size(
    client: (i32, i32),
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    scale: f64,
) -> (i32, i32) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: client.0,
        bottom: client.1,
    };
    let dpi = (scale * 96.0).round() as u32;
    // SAFETY: computes into a local RECT.
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut rect, style, false, ex_style, dpi);
    }
    (rect.right - rect.left, rect.bottom - rect.top)
}

/// The frame size (outer minus client) at a scale.
pub fn frame_size(style: WINDOW_STYLE, ex_style: WINDOW_EX_STYLE, scale: f64) -> (i32, i32) {
    outer_size((0, 0), style, ex_style, scale)
}

pub fn client_size(hwnd: HWND) -> (i32, i32) {
    let mut rect = RECT::default();
    // SAFETY: plain query on our own window.
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }
    (rect.right - rect.left, rect.bottom - rect.top)
}

/// Moves and sizes a window without changing its z-order or activating it.
pub fn set_bounds(hwnd: HWND, rect: IRect) {
    // SAFETY: plain window management call on our own window.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Shows a normal window and brings it to the front.
pub fn show(hwnd: HWND) {
    // SAFETY: plain window management calls on our own window.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Asks for a `WM_MOUSELEAVE` when the mouse leaves the client area.
pub fn track_mouse_leave(hwnd: HWND) {
    let mut track = TRACKMOUSEEVENT {
        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    // SAFETY: `track` is fully initialised.
    unsafe {
        let _ = TrackMouseEvent(&mut track);
    }
}

/// Sets the mouse cursor to a system cursor such as `IDC_HAND`.
pub fn set_cursor(cursor: PCWSTR) {
    // SAFETY: loads and sets a shared system cursor.
    unsafe {
        if let Ok(cursor) = LoadCursorW(None, cursor) {
            SetCursor(Some(cursor));
        }
    }
}

/// Repaints the whole client area.
pub fn invalidate_all(hwnd: HWND) {
    // SAFETY: plain call on our own window.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// Whether a key (e.g. `VK_CONTROL`) is currently held down.
pub fn is_key_down(key: u16) -> bool {
    // SAFETY: plain query.
    unsafe { GetKeyState(key as i32) < 0 }
}

/// Handles `WM_PAINT` by drawing the whole client area at once (for double-buffered windows).
pub fn paint_all(hwnd: HWND, paint: impl FnOnce(HDC)) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: BeginPaint/EndPaint pair on our own window.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        paint(hdc);
        let _ = EndPaint(hwnd, &ps);
    }
}

/// The mouse position in a window's client coordinates.
pub fn cursor_in_client(hwnd: HWND) -> (i32, i32) {
    let mut point = windows::Win32::Foundation::POINT::default();
    // SAFETY: plain queries on our own window.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point);
        let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut point);
    }
    (point.x, point.y)
}

/// Asks a window to close as if the user clicked its close button.
pub fn request_close(hwnd: HWND) {
    post(hwnd, windows::Win32::UI::WindowsAndMessaging::WM_CLOSE);
}

/// Queues a message (without parameters) for one of our windows.
pub fn post(hwnd: HWND, msg: u32) {
    // SAFETY: posts a message to our own window.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(hwnd),
            msg,
            WPARAM(0),
            LPARAM(0),
        );
    }
}
