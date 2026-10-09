//! The editor window (port of `EditorWindow`): the screenshot at 1:1 physical pixels (smaller
//! if it doesn't fit), a toolbar and a status bar. The editing logic lives in
//! `snapr_core::editor::state`; this module turns window messages into calls on it and paints.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use snapr_core::editor::chrome::{self, Button, Layout, ToolbarState};
use snapr_core::editor::state::{Changes, Command, EditorState, command_for_key};
use snapr_core::editor::{Bitmap, Rgb, Tool, png, render};
use snapr_core::geometry::IRect;
use snapr_core::{APP_NAME, strings};
use tiny_skia::PixmapMut;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    HTCLIENT, IDC_ARROW, IDC_CROSS, IDC_HAND, MINMAXINFO, WINDOW_EX_STYLE, WM_CAPTURECHANGED,
    WM_CLOSE, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT, WM_SETCURSOR, WM_SIZE, WS_OVERLAPPEDWINDOW,
};
use windows::core::{PCWSTR, Result, w};

use crate::app;
use crate::platform::gdi::{self, Align, Dib, Font, MemDc};
use crate::platform::{clipboard, dialogs, screen, window};

const CLASS: PCWSTR = w!("RustySnaprEditor");
const EX_STYLE: WINDOW_EX_STYLE = WINDOW_EX_STYLE(0);

pub fn register_class() -> Result<()> {
    window::register_class(CLASS, Some(wndproc), None, true)
}

/// Opens an editor for a captured image, centred on the monitor under the mouse.
pub fn open(image: Bitmap) -> Result<()> {
    let cursor = screen::cursor_position();
    let scale = screen::scale_at(cursor);
    let work = screen::work_area_at(cursor);
    let frame = window::frame_size(WS_OVERLAPPEDWINDOW, EX_STYLE, scale);
    let (min_outer, max_outer) = size_limits(work, scale);
    let client = chrome::preferred_client_size(
        (image.width(), image.height()),
        scale,
        (min_outer.0 - frame.0, min_outer.1 - frame.1),
        (max_outer.0 - frame.0, max_outer.1 - frame.1),
    );
    let outer = (client.0 + frame.0, client.1 + frame.1);
    let bounds = IRect::new(
        work.x + (work.width - outer.0) / 2,
        work.y + (work.height - outer.1) / 2,
        outer.0,
        outer.1,
    );

    let size = (image.width(), image.height());
    let editor = Rc::new(Editor {
        hwnd: Cell::new(HWND::default()),
        state: RefCell::new(EditorState::new(image, scale)),
        view: RefCell::new(View {
            layout: chrome::layout(client, size, scale),
            hover: None,
            pressed: None,
            tracking_leave: false,
            status: strings::get().editor_hint().to_string(),
            composite: None,
            fonts: Fonts::new(scale),
        }),
    });
    let hwnd = window::create_window(
        CLASS,
        APP_NAME,
        EX_STYLE,
        WS_OVERLAPPEDWINDOW,
        bounds,
        editor.clone(),
    )?;
    editor.hwnd.set(hwnd);
    window::use_dark_title_bar(hwnd);
    editor.relayout();
    window::show(hwnd);
    Ok(())
}

/// Minimum and maximum outer window size: 580×260 DIPs, and 92% of the work area.
fn size_limits(work: IRect, scale: f64) -> ((i32, i32), (i32, i32)) {
    let (min_w, min_h) = chrome::MIN_WINDOW_SIZE;
    let min = (
        (min_w * scale).round() as i32,
        (min_h * scale).round() as i32,
    );
    let max = (
        ((work.width as f64 * chrome::MAX_WORK_AREA_SHARE) as i32).max(min.0),
        ((work.height as f64 * chrome::MAX_WORK_AREA_SHARE) as i32).max(min.1),
    );
    (min, max)
}

struct Editor {
    hwnd: Cell<HWND>,
    state: RefCell<EditorState>,
    view: RefCell<View>,
}

struct View {
    layout: Layout,
    hover: Option<Button>,
    /// Toolbar button under a mouse press, clicked when released over it.
    pressed: Option<Button>,
    tracking_leave: bool,
    status: String,
    /// The image with committed annotations, and the state revision it belongs to.
    composite: Option<(u64, Bitmap)>,
    fonts: Fonts,
}

struct Fonts {
    text: Font,
    number_icon: Font,
}

impl Fonts {
    fn new(scale: f64) -> Fonts {
        let size = (12.0 * scale).round() as i32;
        Fonts {
            text: Font::segoe_ui(size),
            number_icon: Font::segoe_ui_bold(size),
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    window::guarded("Editor", hwnd, msg, wparam, lparam, || {
        if msg == WM_NCDESTROY {
            window::release_state::<Editor>(hwnd);
            return window::default_proc(hwnd, msg, wparam, lparam);
        }
        match window::state::<Editor>(hwnd) {
            Some(editor) => editor
                .handle(msg, wparam, lparam)
                .unwrap_or_else(|| window::default_proc(hwnd, msg, wparam, lparam)),
            None => window::default_proc(hwnd, msg, wparam, lparam),
        }
    })
}

// Note: no `state`/`view` borrow may be held while calling into Windows in ways that send
// messages back (capture, dialogs, destroy), since this window's procedure would run again.
impl Editor {
    fn hwnd(&self) -> HWND {
        self.hwnd.get()
    }

    fn handle(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_ERASEBKGND => return Some(LRESULT(1)),
            WM_PAINT => window::paint_all(self.hwnd(), |hdc| self.paint(hdc)),
            WM_SIZE => self.relayout(),
            WM_DPICHANGED => {
                // SAFETY-free: lparam points to the suggested window rect (documented).
                let suggested = unsafe { *(lparam.0 as *const RECT) };
                let scale = (wparam.0 & 0xFFFF) as f64 / 96.0;
                self.state.borrow_mut().set_scale(scale);
                self.view.borrow_mut().fonts = Fonts::new(scale);
                window::set_bounds(
                    self.hwnd(),
                    IRect::new(
                        suggested.left,
                        suggested.top,
                        suggested.right - suggested.left,
                        suggested.bottom - suggested.top,
                    ),
                );
                self.relayout();
            }
            WM_GETMINMAXINFO => {
                let scale = window::scale_of(self.hwnd());
                let (min, max) = size_limits(window::work_area_of(self.hwnd()), scale);
                // SAFETY: lparam points to the MINMAXINFO to fill in (documented).
                let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
                info.ptMinTrackSize.x = min.0;
                info.ptMinTrackSize.y = min.1;
                info.ptMaxTrackSize.x = max.0;
                info.ptMaxTrackSize.y = max.1;
            }
            WM_SETCURSOR if (lparam.0 & 0xFFFF) as u32 == HTCLIENT => {
                self.update_cursor();
                return Some(LRESULT(1));
            }
            WM_MOUSEMOVE => self.mouse_move(window::mouse_position(lparam)),
            WM_MOUSELEAVE => {
                let mut view = self.view.borrow_mut();
                view.tracking_leave = false;
                if view.hover.take().is_some() {
                    drop(view);
                    self.invalidate_toolbar();
                }
            }
            WM_LBUTTONDOWN => self.mouse_down(window::mouse_position(lparam)),
            WM_LBUTTONUP => self.mouse_up(window::mouse_position(lparam)),
            // E.g. Alt+Tab mid-drag: keep what was dragged so far instead of getting stuck.
            WM_CAPTURECHANGED => {
                self.view.borrow_mut().pressed = None;
                let changes = self.state.borrow_mut().finish_drag();
                self.apply(changes);
            }
            WM_KEYDOWN => {
                let ctrl = window::is_key_down(VK_CONTROL.0);
                let shift = window::is_key_down(VK_SHIFT.0);
                let command = command_for_key(wparam.0 as u16, ctrl, shift)?;
                self.run(command);
            }
            WM_CLOSE => {
                self.closing();
                window::destroy(self.hwnd());
            }
            _ => return None,
        }
        Some(LRESULT(0))
    }

    // ---- Layout & painting ---------------------------------------------------------------

    fn relayout(&self) {
        let client = window::client_size(self.hwnd());
        if client.0 <= 0 || client.1 <= 0 {
            return; // minimised
        }
        let scale = window::scale_of(self.hwnd());
        let size = {
            let state = self.state.borrow();
            (state.image().width(), state.image().height())
        };
        self.view.borrow_mut().layout = chrome::layout(client, size, scale);
        window::invalidate_all(self.hwnd());
    }

    fn invalidate_toolbar(&self) {
        let toolbar = self.view.borrow().layout.toolbar;
        window::invalidate(self.hwnd(), toolbar);
    }

    fn invalidate_image(&self) {
        let image = self.view.borrow().layout.image_pixels();
        window::invalidate(self.hwnd(), image);
    }

    fn apply(&self, changes: Changes) {
        if changes.image {
            self.invalidate_image();
        }
        if changes.toolbar {
            self.invalidate_toolbar();
        }
    }

    fn paint(&self, hdc: HDC) {
        let (width, height) = window::client_size(self.hwnd());
        if width <= 0 || height <= 0 {
            return;
        }
        let Ok(mut back) = Dib::new(width, height) else {
            return;
        };
        let Ok(back_dc) = MemDc::new(back.handle()) else {
            return;
        };

        self.update_composite();
        let state = self.state.borrow();
        let view = self.view.borrow();
        let layout = &view.layout;
        let toolbar = ToolbarState {
            tool: state.tool(),
            color: state.color_index(),
            can_undo: state.can_undo(),
            can_redo: state.can_redo(),
            hover: view.hover,
        };

        // Background, toolbar and frame.
        if let Some(mut pixmap) =
            PixmapMut::from_bytes(back.pixels_mut(), width as u32, height as u32)
        {
            chrome::draw(&mut pixmap, layout, &toolbar);
        }

        // The image, with the annotation being drawn on top.
        if let Some((_, composite)) = &view.composite {
            let target = layout.image_pixels();
            match state.pending() {
                Some(pending) => {
                    let mut frame = composite.clone();
                    render::draw_annotations(&mut frame, [pending]);
                    gdi::draw_bitmap(back_dc.hdc(), target, &frame);
                }
                None => gdi::draw_bitmap(back_dc.hdc(), target, composite),
            }
        }

        // Texts.
        let subtle = rgb(chrome::SUBTLE_FOREGROUND);
        let size_text = format!("{} × {}", state.image().width(), state.image().height());
        let text_font = &view.fonts.text;
        let left = layout.status_left();
        gdi::draw_text(
            back_dc.hdc(),
            text_font,
            &size_text,
            left,
            subtle,
            Align::Left,
        );
        // The status gets the space right of the size, cut off with "…" if it doesn't fit.
        let (size_width, _) = text_font.measure(back_dc.hdc(), &size_text);
        let right = layout.status_right();
        let start = left.x + size_width + left.x;
        let status_rect = IRect::new(start, right.y, right.right() - start, right.height);
        gdi::draw_text(
            back_dc.hdc(),
            text_font,
            &view.status,
            status_rect,
            subtle,
            Align::Right,
        );
        if let Some(rect) = layout.button_rect(Button::Tool(Tool::Number)) {
            let on_accent = toolbar.tool == Tool::Number;
            let color = if toolbar.is_enabled(Button::Tool(Tool::Number)) || on_accent {
                rgb(chrome::FOREGROUND)
            } else {
                dimmed(chrome::FOREGROUND, chrome::TOOLBAR_BACKGROUND)
            };
            gdi::draw_text(
                back_dc.hdc(),
                &view.fonts.number_icon,
                "1",
                rect,
                color,
                Align::Center,
            );
        }

        gdi::blit(hdc, 0, 0, width, height, back_dc.hdc(), 0, 0);
    }

    /// Rebuilds the cached image-with-annotations when the annotations changed.
    fn update_composite(&self) {
        let state = self.state.borrow();
        let mut view = self.view.borrow_mut();
        let current = view.composite.as_ref().map(|(revision, _)| *revision);
        if current != Some(state.revision()) {
            view.composite = Some((state.revision(), state.flatten()));
        }
    }

    // ---- Mouse ---------------------------------------------------------------------------

    fn mouse_move(&self, p: (i32, i32)) {
        if self.state.borrow().is_dragging() {
            let point = self.view.borrow().layout.to_image(p);
            let changes = self.state.borrow_mut().mouse_move(point);
            self.apply(changes);
            return;
        }

        let mut view = self.view.borrow_mut();
        if !view.tracking_leave {
            view.tracking_leave = true;
            window::track_mouse_leave(self.hwnd());
        }
        let hover = view.layout.button_at(p);
        if hover != view.hover {
            view.hover = hover;
            drop(view);
            self.invalidate_toolbar();
        }
    }

    fn mouse_down(&self, p: (i32, i32)) {
        let (button, on_image, point) = {
            let view = self.view.borrow();
            (
                view.layout.button_at(p),
                view.layout.image_contains(p),
                view.layout.to_image(p),
            )
        };
        if let Some(button) = button {
            if self.toolbar_state().is_enabled(button) {
                self.view.borrow_mut().pressed = Some(button);
                window::capture_mouse(self.hwnd());
            }
        } else if on_image {
            let changes = self.state.borrow_mut().mouse_down(point);
            if changes != Changes::NONE {
                window::capture_mouse(self.hwnd());
            }
            self.apply(changes);
        }
    }

    fn mouse_up(&self, p: (i32, i32)) {
        let pressed = self.view.borrow_mut().pressed.take();
        let released_on = self.view.borrow().layout.button_at(p);
        let changes = self.state.borrow_mut().finish_drag();
        window::release_mouse();
        self.apply(changes);

        if let Some(button) = pressed
            && released_on == Some(button)
        {
            self.click(button);
        }
    }

    fn update_cursor(&self) {
        let p = window::cursor_in_client(self.hwnd());
        let view = self.view.borrow();
        let cursor = match view.layout.button_at(p) {
            Some(button) if self.toolbar_state().is_enabled(button) => IDC_HAND,
            Some(_) => IDC_ARROW,
            None if view.layout.image_contains(p) || self.state.borrow().is_dragging() => IDC_CROSS,
            None => IDC_ARROW,
        };
        window::set_cursor(cursor);
    }

    fn toolbar_state(&self) -> ToolbarState {
        let state = self.state.borrow();
        ToolbarState {
            tool: state.tool(),
            color: state.color_index(),
            can_undo: state.can_undo(),
            can_redo: state.can_redo(),
            hover: self.view.borrow().hover,
        }
    }

    // ---- Commands ------------------------------------------------------------------------

    fn click(&self, button: Button) {
        let command = match button {
            Button::Tool(tool) => Command::Tool(tool),
            Button::Color(index) => Command::Color(index),
            Button::Undo => Command::Undo,
            Button::Redo => Command::Redo,
            Button::Copy => Command::Copy,
            Button::Save => Command::Save,
        };
        self.run(command);
    }

    fn run(&self, command: Command) {
        match command {
            Command::Close => window::request_close(self.hwnd()),
            Command::Copy => self.copy(),
            Command::Save => self.save(),
            other => {
                let changes = {
                    let mut state = self.state.borrow_mut();
                    match other {
                        Command::Undo => state.undo(),
                        Command::Redo => state.redo(),
                        Command::Tool(tool) => state.select_tool(tool),
                        Command::Color(index) => state.select_color(index),
                        _ => Changes::NONE,
                    }
                };
                self.apply(changes);
            }
        }
    }

    fn set_status(&self, text: String) {
        let status = {
            let mut view = self.view.borrow_mut();
            view.status = text;
            view.layout.status
        };
        window::invalidate(self.hwnd(), status);
    }

    fn copy(&self) {
        let image = self.state.borrow().flatten();
        let s = strings::get();
        if clipboard::set_image(self.hwnd(), &image) {
            self.state.borrow_mut().mark_copied();
            self.set_status(s.copied().to_string());
        } else {
            self.set_status(s.clipboard_busy().to_string());
        }
    }

    fn save(&self) {
        let s = strings::get();
        let folder = app::settings().save_folder();
        // Fall back to the dialog's own default folder if this one can't be created.
        let _ = std::fs::create_dir_all(&folder);
        let name = format!(
            "Snapr {}.png",
            chrono::Local::now().format("%Y-%m-%d %H.%M.%S")
        );
        let Some(path) = dialogs::save_png(self.hwnd(), &folder, &name, s.png_file_type()) else {
            return;
        };

        let image = self.state.borrow().flatten();
        match std::fs::write(&path, png::encode(&image)) {
            Ok(()) => {
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.set_status(s.saved(&file_name));
            }
            Err(error) => dialogs::error_box(self.hwnd(), &s.save_failed(&error.to_string())),
        }
    }

    /// Leave the edited version on the clipboard, like Shottr does.
    fn closing(&self) {
        let changed = self.state.borrow().changed_since_copy();
        if changed && app::settings().copy_on_capture {
            let image = self.state.borrow().flatten();
            clipboard::set_image(self.hwnd(), &image);
        }
    }
}

fn rgb(color: Rgb) -> (u8, u8, u8) {
    (color.0, color.1, color.2)
}

/// A colour at the disabled opacity over a background.
fn dimmed(color: Rgb, background: Rgb) -> (u8, u8, u8) {
    let mix = |c: u8, b: u8| {
        (c as f32 * chrome::DISABLED_OPACITY + b as f32 * (1.0 - chrome::DISABLED_OPACITY)) as u8
    };
    (
        mix(color.0, background.0),
        mix(color.1, background.1),
        mix(color.2, background.2),
    )
}
