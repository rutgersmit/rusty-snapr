//! Full-desktop overlay showing the frozen screenshot (port of `CaptureOverlayWindow`). The user
//! drags a region, clicks for the monitor under the cursor, or presses Enter for everything.
//!
//! Drawing: the screenshot is kept twice, as is and darkened. A repaint copies the darkened
//! version, the original inside the selection, and draws the chrome (crosshair, border, labels)
//! on top. Only the areas that change are repainted, so it stays smooth on large desktops.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use snapr_core::capture::{self, Gesture};
use snapr_core::draw;
use snapr_core::editor::Bitmap;
use snapr_core::geometry::IRect;
use snapr_core::strings;
use tiny_skia::{FillRule, Paint, PixmapMut, Rect, Transform};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::{
    IDC_CROSS, WA_INACTIVE, WM_ACTIVATE, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT, WM_RBUTTONUP, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

use crate::platform::gdi::{self, Dib, Font, MemDc};
use crate::platform::screen::{self, ScreenShot};
use crate::platform::window;

const CLASS: PCWSTR = w!("RustySnaprOverlay");

/// Label and hint background (#E62C2C2E).
const LABEL_BACKGROUND: (u8, u8, u8, u8) = (0x2C, 0x2C, 0x2E, 0xE6);
/// Crosshair (#B0FFFFFF), dashed 4 on, 3 off.
const CROSSHAIR_ALPHA: u8 = 0xB0;
const DASH_ON: f64 = 4.0;
const DASH_OFF: f64 = 3.0;

pub type OnClose = Box<dyn FnOnce(Option<Bitmap>)>;

pub fn register_class() -> Result<()> {
    window::register_class(CLASS, Some(wndproc), Some(IDC_CROSS))
}

/// Captures the screen and shows the overlay. `on_close` gets the selected image, or `None` when
/// the user cancelled.
pub fn show(on_close: OnClose) -> Result<()> {
    let shot = screen::capture_virtual_screen()?;
    let mut dim = Dib::new(shot.bounds.width, shot.bounds.height)?;
    capture::dim_pixels(shot.image.pixels(), dim.pixels_mut());

    let cursor = screen::cursor_position();
    let scale = screen::scale_at(cursor);
    let bright_dc = MemDc::new(shot.image.handle())?;
    let dim_dc = MemDc::new(dim.handle())?;
    let label_font = Font::segoe_ui(dip(12.0, scale));
    let hint_font = Font::segoe_ui(dip(13.0, scale));

    // The hint goes near the top of the monitor the cursor is on.
    let hint_text = strings::get().capture_hint();
    let (text_width, text_height) = hint_font.measure(bright_dc.hdc(), hint_text);
    let monitor = screen::monitor_rect_at(cursor).offset(-shot.bounds.x, -shot.bounds.y);
    let hint = capture::hint_rect(
        monitor,
        text_width + 2 * dip(12.0, scale),
        text_height + 2 * dip(7.0, scale),
        scale,
    );

    let bounds = shot.bounds;
    let overlay = Rc::new(Overlay {
        hwnd: Cell::new(HWND::default()),
        bright_dc,
        dim_dc,
        shot,
        dim,
        scale,
        label_font,
        hint_font,
        hint_text,
        state: RefCell::new(State {
            hint: Some(hint),
            ..State::default()
        }),
        on_close: Cell::new(Some(on_close)),
    });
    let hwnd = window::create_window(
        CLASS,
        "Snapr capture",
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        WS_POPUP,
        bounds,
        overlay.clone(),
    )?;
    overlay.hwnd.set(hwnd);
    window::show_topmost(hwnd, bounds);
    Ok(())
}

/// DIPs to physical pixels.
fn dip(value: f64, scale: f64) -> i32 {
    (value * scale).round() as i32
}

struct Overlay {
    hwnd: Cell<HWND>,
    // The DCs hold the bitmaps below selected, so they are declared (and dropped) first.
    bright_dc: MemDc,
    dim_dc: MemDc,
    shot: ScreenShot,
    #[allow(dead_code)] // kept alive for dim_dc
    dim: Dib,
    scale: f64,
    label_font: Font,
    hint_font: Font,
    hint_text: &'static str,
    state: RefCell<State>,
    on_close: Cell<Option<OnClose>>,
}

#[derive(Default)]
struct State {
    /// Last mouse position (client = virtual-screen pixels); `None` until the mouse moves.
    cursor: Option<(i32, i32)>,
    drag_start: Option<(i32, i32)>,
    selection: Option<IRect>,
    /// The hint box, until the user starts dragging.
    hint: Option<IRect>,
    result: Option<Bitmap>,
    closing: bool,
}

/// Everything drawn on top of the screenshot, to find out what needs repainting.
#[derive(Clone, Copy, PartialEq)]
struct Chrome {
    cursor: Option<(i32, i32)>,
    selection: Option<IRect>,
    label: Option<IRect>,
    hint: Option<IRect>,
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    window::guarded("Capture", hwnd, msg, wparam, lparam, || {
        if msg == WM_NCDESTROY {
            if let Some(overlay) = window::release_state::<Overlay>(hwnd) {
                overlay.closed();
            }
            return window::default_proc(hwnd, msg, wparam, lparam);
        }
        match window::state::<Overlay>(hwnd) {
            Some(overlay) => overlay
                .handle(msg, wparam, lparam)
                .unwrap_or_else(|| window::default_proc(hwnd, msg, wparam, lparam)),
            None => window::default_proc(hwnd, msg, wparam, lparam),
        }
    })
}

impl Overlay {
    fn hwnd(&self) -> HWND {
        self.hwnd.get()
    }

    fn size(&self) -> (i32, i32) {
        (self.shot.bounds.width, self.shot.bounds.height)
    }

    // Note: no `state` borrow may be held while calling into Windows (capture, destroy), because
    // those calls send messages back to this window synchronously.
    fn handle(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_ERASEBKGND => return Some(LRESULT(1)),
            WM_PAINT => window::paint(self.hwnd(), |hdc, rect| self.paint(hdc, rect)),
            WM_MOUSEMOVE => self.mouse_move(window::mouse_position(lparam)),
            WM_LBUTTONDOWN => self.mouse_down(window::mouse_position(lparam)),
            WM_LBUTTONUP => self.mouse_up(window::mouse_position(lparam)),
            WM_RBUTTONUP => self.close(None),
            WM_KEYDOWN => match wparam.0 as u16 {
                k if k == VK_ESCAPE.0 => self.close(None),
                k if k == VK_RETURN.0 => {
                    let (width, height) = self.size();
                    self.finish(IRect::new(0, 0, width, height));
                }
                _ => return None,
            },
            WM_ACTIVATE if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE => {
                // Another window took over (e.g. Alt+Tab): cancel, unless mid-drag.
                let dragging = self.state.borrow().drag_start.is_some();
                if !dragging {
                    self.close(None);
                }
                return None;
            }
            // Windows resizes the window to a "suggested" rect on a DPI change; keep covering
            // the whole desktop instead.
            WM_DPICHANGED => window::set_topmost_bounds(self.hwnd(), self.shot.bounds),
            _ => return None,
        }
        Some(LRESULT(0))
    }

    fn mouse_down(&self, p: (i32, i32)) {
        self.update(|state| {
            state.drag_start = Some(p);
            state.selection = None;
            state.hint = None;
        });
        window::capture_mouse(self.hwnd());
    }

    fn mouse_move(&self, p: (i32, i32)) {
        self.update(|state| {
            state.cursor = Some(p);
            if let Some(start) = state.drag_start {
                state.selection = Some(capture::drag_rect(start, p));
            }
        });
    }

    fn mouse_up(&self, p: (i32, i32)) {
        let Some(start) = self.state.borrow_mut().drag_start.take() else {
            return;
        };
        window::release_mouse();

        match capture::finish_drag(start, p, self.scale) {
            // A plain click captures the monitor under the cursor.
            Gesture::Click => {
                let monitor = screen::monitor_rect_at(screen::cursor_position());
                self.finish(monitor.offset(-self.shot.bounds.x, -self.shot.bounds.y));
            }
            Gesture::Region(rect) => self.finish(rect),
        }
    }

    /// Applies a state change and repaints what it affected.
    fn update(&self, change: impl FnOnce(&mut State)) {
        let mut state = self.state.borrow_mut();
        let before = self.chrome(&state);
        change(&mut state);
        let after = self.chrome(&state);
        drop(state);
        if before != after {
            self.invalidate_changes(before, after);
        }
    }

    fn chrome(&self, state: &State) -> Chrome {
        Chrome {
            cursor: state.cursor,
            selection: state.selection,
            label: state.selection.map(|s| self.label_rect(s)),
            hint: state.hint,
        }
    }

    fn label_rect(&self, selection: IRect) -> IRect {
        let (width, height) = self
            .label_font
            .measure(self.bright_dc.hdc(), &capture::size_text(selection));
        let (x, y) = capture::size_label_origin(selection, self.scale);
        IRect::new(
            x,
            y,
            width + 2 * dip(7.0, self.scale),
            height + 2 * dip(3.0, self.scale),
        )
    }

    fn line_width(&self) -> i32 {
        dip(1.0, self.scale).max(1)
    }

    fn invalidate_changes(&self, before: Chrome, after: Chrome) {
        let hwnd = self.hwnd();
        let (width, height) = self.size();
        let line = self.line_width();

        if before.cursor != after.cursor {
            for (x, y) in [before.cursor, after.cursor].into_iter().flatten() {
                window::invalidate(hwnd, IRect::new(0, y - 1, width, line + 2));
                window::invalidate(hwnd, IRect::new(x - 1, 0, line + 2, height));
            }
        }
        if before.selection != after.selection {
            // The hole: only where old and new differ. The borders: both outlines.
            let empty = IRect::default();
            window::invalidate_difference(
                hwnd,
                before.selection.unwrap_or(empty),
                after.selection.unwrap_or(empty),
            );
            for selection in [before.selection, after.selection].into_iter().flatten() {
                for strip in frame(selection, line + 1) {
                    window::invalidate(hwnd, strip);
                }
            }
        }
        // The label's text changes with the selection even when its box stays the same size.
        let label_changed = before.selection != after.selection;
        let hint_changed = before.hint != after.hint;
        for (old, new, changed) in [
            (before.label, after.label, label_changed),
            (before.hint, after.hint, hint_changed),
        ] {
            if changed {
                for rect in [old, new].into_iter().flatten() {
                    window::invalidate(hwnd, rect.inflate(1));
                }
            }
        }
    }

    fn paint(&self, hdc: HDC, area: IRect) {
        let Ok(mut back) = Dib::new(area.width, area.height) else {
            return;
        };
        let Ok(back_dc) = MemDc::new(back.handle()) else {
            return;
        };
        let state = self.state.borrow();
        let chrome = self.chrome(&state);
        drop(state);

        // Darkened screenshot, with the original inside the selection.
        gdi::blit(
            back_dc.hdc(),
            0,
            0,
            area.width,
            area.height,
            self.dim_dc.hdc(),
            area.x,
            area.y,
        );
        if let Some(hole) = chrome.selection.and_then(|s| s.intersect(area)) {
            gdi::blit(
                back_dc.hdc(),
                hole.x - area.x,
                hole.y - area.y,
                hole.width,
                hole.height,
                self.bright_dc.hdc(),
                hole.x,
                hole.y,
            );
        }

        self.paint_shapes(&mut back, area, &chrome);

        // Text last, with GDI (ClearType, Segoe UI like the rest of Windows).
        let to_local = |r: IRect| r.offset(-area.x, -area.y);
        if let (Some(selection), Some(label)) = (chrome.selection, chrome.label) {
            let text = capture::size_text(selection);
            gdi::draw_text_centered(
                back_dc.hdc(),
                &self.label_font,
                &text,
                to_local(label),
                (255, 255, 255),
            );
        }
        if let Some(hint) = chrome.hint {
            gdi::draw_text_centered(
                back_dc.hdc(),
                &self.hint_font,
                self.hint_text,
                to_local(hint),
                (0xF2, 0xF2, 0xF7),
            );
        }

        gdi::blit(
            hdc,
            area.x,
            area.y,
            area.width,
            area.height,
            back_dc.hdc(),
            0,
            0,
        );
        drop(back_dc);
    }

    /// Crosshair, selection border and label backgrounds, in the same order as the C# version.
    fn paint_shapes(&self, back: &mut Dib, area: IRect, chrome: &Chrome) {
        let (width, height) = (back.width() as u32, back.height() as u32);
        let pixels = back.pixels_mut();
        // tiny-skia blends as if the pixels were opaque; a GDI blit leaves alpha undefined.
        pixels.chunks_exact_mut(4).for_each(|p| p[3] = 255);
        let Some(mut pixmap) = PixmapMut::from_bytes(pixels, width, height) else {
            return;
        };
        let transform = Transform::from_translate(-area.x as f32, -area.y as f32);
        let (full_width, full_height) = self.size();
        let line = self.line_width() as f32;
        let mut paint = Paint::default();

        if let Some((x, y)) = chrome.cursor {
            paint.set_color(draw::color(255, 255, 255, CROSSHAIR_ALPHA));
            let on = (DASH_ON * self.scale) as f32;
            let period = ((DASH_ON + DASH_OFF) * self.scale) as f32;
            // Only the dashes that fall inside the repainted area.
            let dashes = |from: i32, to: i32| {
                let first = (from as f32 / period).floor() as i32;
                let last = (to as f32 / period).ceil() as i32;
                (first..=last).map(move |k| k as f32 * period)
            };
            if area
                .intersect(IRect::new(0, y, full_width, line as i32))
                .is_some()
            {
                for start in dashes(area.x, area.right()) {
                    fill(&mut pixmap, &paint, transform, start, y as f32, on, line);
                }
            }
            if area
                .intersect(IRect::new(x, 0, line as i32, full_height))
                .is_some()
            {
                for start in dashes(area.y, area.bottom()) {
                    fill(&mut pixmap, &paint, transform, x as f32, start, line, on);
                }
            }
        }

        if let Some(selection) = chrome.selection {
            paint.set_color(draw::color(255, 255, 255, 255));
            // Inside the selection, like a WPF Rectangle's stroke.
            for strip in frame(selection, line as i32) {
                let (x, y, w, h) = (
                    strip.x as f32,
                    strip.y as f32,
                    strip.width as f32,
                    strip.height as f32,
                );
                fill(&mut pixmap, &paint, transform, x, y, w, h);
            }
        }

        let (r, g, b, a) = LABEL_BACKGROUND;
        paint.set_color(draw::color(r, g, b, a));
        for (rect, radius) in [(chrome.label, 5.0), (chrome.hint, 8.0)] {
            let Some(rect) = rect else { continue };
            let radius = (radius * self.scale) as f32;
            let (x, y, w, h) = (
                rect.x as f32,
                rect.y as f32,
                rect.width as f32,
                rect.height as f32,
            );
            if let Some(path) = draw::rounded_rect(x, y, w, h, radius) {
                pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
            }
        }
    }

    fn finish(&self, rect: IRect) {
        let (width, height) = self.size();
        let rect = rect.clamp_within(width, height);
        let image =
            Bitmap::from_region(self.shot.image.pixels(), width as u32, height as u32, rect);
        self.close(Some(image));
    }

    fn close(&self, result: Option<Bitmap>) {
        {
            let mut state = self.state.borrow_mut();
            if state.closing {
                return;
            }
            state.closing = true;
            state.result = result;
        }
        window::destroy(self.hwnd());
    }

    /// Called once the window is gone.
    fn closed(&self) {
        let result = self.state.borrow_mut().result.take();
        if let Some(on_close) = self.on_close.take() {
            on_close(result);
        }
    }
}

/// The four edge strips of `width` pixels just inside `rect`.
fn frame(rect: IRect, width: i32) -> [IRect; 4] {
    let w = width.min(rect.width).max(0);
    let h = width.min(rect.height).max(0);
    [
        IRect::new(rect.x, rect.y, rect.width, h),
        IRect::new(rect.x, rect.bottom() - h, rect.width, h),
        IRect::new(rect.x, rect.y, w, rect.height),
        IRect::new(rect.right() - w, rect.y, w, rect.height),
    ]
}

fn fill(
    pixmap: &mut PixmapMut<'_>,
    paint: &Paint<'_>,
    transform: Transform,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    if let Some(rect) = Rect::from_xywh(x, y, w, h) {
        pixmap.fill_rect(rect, paint, transform, None);
    }
}
