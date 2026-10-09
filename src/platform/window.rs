//! Window classes, per-window state, painting and panic-safe window procedures.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use snapr_core::geometry::IRect;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CombineRgn, CreateRectRgn, DeleteObject, EndPaint, GetRegionData, GetUpdateRgn,
    HDC, HRGN, InvalidateRect, InvalidateRgn, PAINTSTRUCT, RGN_XOR, RGNDATA, RGNDATAHEADER,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA,
    GetWindowLongPtrW, HCURSOR, HWND_TOPMOST, IDC_ARROW, LoadCursorW, RegisterClassW, SW_SHOW,
    SWP_SHOWWINDOW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_NCCREATE, WNDCLASSW, WNDPROC,
};
use windows::core::{PCWSTR, Result};

use super::{errors, wide};

pub fn instance() -> HINSTANCE {
    // SAFETY: getting the handle of our own exe always succeeds.
    unsafe { GetModuleHandleW(None) }
        .map(Into::into)
        .unwrap_or_default()
}

/// Registers a window class. `cursor` is a system cursor such as `IDC_CROSS` (default: arrow).
pub fn register_class(name: PCWSTR, wndproc: WNDPROC, cursor: Option<PCWSTR>) -> Result<()> {
    // SAFETY: loads a shared system cursor.
    let cursor =
        unsafe { LoadCursorW(None, cursor.unwrap_or(IDC_ARROW)) }.unwrap_or(HCURSOR::default());
    let class = WNDCLASSW {
        lpfnWndProc: wndproc,
        hInstance: instance(),
        lpszClassName: name,
        hCursor: cursor,
        ..Default::default()
    };
    // SAFETY: `class` is fully initialised and `name` is a static string.
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(windows::core::Error::from_thread());
    }
    Ok(())
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
