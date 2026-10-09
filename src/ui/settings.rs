//! The settings window (port of `SettingsWindow`): shortcut recorder, save folder, two options.
//!
//! Differences from the C# version: the folder is chosen with Browse… only (no free text), and
//! keyboard navigation is limited to Esc (cancel), Enter (save) and Tab (stop recording).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use snapr_core::draw;
use snapr_core::editor::Rgb;
use snapr_core::editor::chrome::{
    ACCENT, CONTROL_BACKGROUND, FOREGROUND, HOVER_BACKGROUND, SUBTLE_FOREGROUND, WINDOW_BACKGROUND,
};
use snapr_core::geometry::IRect;
use snapr_core::hotkey::{HotkeyGesture, Modifiers, vk};
use snapr_core::settings::AppSettings;
use snapr_core::strings;
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{FW_NORMAL, FW_SEMIBOLD, HDC};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SetFocus, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HTCLIENT, IDC_ARROW, IDC_HAND, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT,
    WM_SETCURSOR, WM_SYSKEYDOWN, WM_SYSKEYUP, WS_CAPTION, WS_OVERLAPPED, WS_SYSMENU,
};
use windows::core::{PCWSTR, Result, w};

use crate::app;
use crate::platform::gdi::{self, Align, Dib, Font, MemDc};
use crate::platform::{dialogs, screen, window};

const CLASS: PCWSTR = w!("RustySnaprSettings");
const EX_STYLE: WINDOW_EX_STYLE = WINDOW_EX_STYLE(0);

pub fn register_class() -> Result<()> {
    window::register_class(CLASS, Some(wndproc), None, true)
}

/// Opens the settings window (centred on the monitor with the mouse) and returns its handle.
pub fn open(settings: &AppSettings) -> Result<HWND> {
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
    let cursor = screen::cursor_position();
    let scale = screen::scale_at(cursor);
    let work = screen::work_area_at(cursor);
    let frame = window::frame_size(style, EX_STYLE, scale);

    let fonts = Fonts::new(scale);
    let client_width = (460.0 * scale).round() as i32 - frame.0;
    let layout = Layout::new(client_width, scale, &fonts)?;
    let outer = (client_width + frame.0, layout.height + frame.1);
    let bounds = IRect::new(
        work.x + (work.width - outer.0) / 2,
        work.y + (work.height - outer.1) / 2,
        outer.0,
        outer.1,
    );

    let form = Form {
        hotkey: settings.hotkey_gesture(),
        folder: settings.save_folder.clone(),
        copy_on_capture: settings.copy_on_capture,
        start_with_windows: settings.start_with_windows,
        recording: false,
        prompt: false,
    };
    let window = Rc::new(Settings {
        hwnd: Cell::new(HWND::default()),
        form: RefCell::new(form),
        view: RefCell::new(View {
            layout,
            fonts,
            hover: None,
            pressed: None,
            tracking_leave: false,
        }),
    });
    let title = strings::get().settings_title();
    let hwnd = window::create_window(CLASS, &title, EX_STYLE, style, bounds, window.clone())?;
    window.hwnd.set(hwnd);
    window::use_dark_title_bar(hwnd);
    window::show(hwnd);
    Ok(hwnd)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    HotkeyField,
    ResetHotkey,
    FolderField,
    Browse,
    CopyOnCapture,
    StartWithWindows,
    Cancel,
    Save,
}

struct Form {
    hotkey: HotkeyGesture,
    folder: String,
    copy_on_capture: bool,
    start_with_windows: bool,
    /// The shortcut field has focus and records the next key combination.
    recording: bool,
    /// Show "Press a key combination…" until something is recorded (like the C# version).
    prompt: bool,
}

struct Fonts {
    label: Font,
    text: Font,
    help: Font,
}

impl Fonts {
    fn new(scale: f64) -> Fonts {
        let px = |size: f64| (size * scale).round() as i32;
        Fonts {
            label: Font::segoe_ui_weight(px(13.0), FW_SEMIBOLD.0 as i32),
            text: Font::segoe_ui_weight(px(13.0), FW_NORMAL.0 as i32),
            help: Font::segoe_ui(px(12.0)),
        }
    }
}

/// Positions in client pixels, following the XAML: 20 margin, 30 high rows, 6/18/8/22 gaps.
struct Layout {
    scale: f64,
    height: i32,
    shortcut_label: IRect,
    help: IRect,
    folder_label: IRect,
    parts: Vec<(Part, IRect)>,
}

impl Layout {
    fn new(width: i32, scale: f64, fonts: &Fonts) -> Result<Layout> {
        // A scratch DC to measure text.
        let scratch = Dib::new(1, 1)?;
        let dc = MemDc::new(scratch.handle())?;
        let s = strings::get();
        let px = |dip: f64| (dip * scale).round() as i32;
        let margin = px(20.0);
        let content = width - 2 * margin;
        let row = px(30.0);
        let line = fonts.label.measure(dc.hdc(), "Ag").1;
        let button_width = |text: &str| {
            fonts
                .text
                .measure(dc.hdc(), text)
                .0
                .saturating_add(px(28.0))
                .max(px(88.0))
        };
        let mut parts = Vec::new();

        let mut y = margin;
        let shortcut_label = IRect::new(margin, y, content, line);
        y += line + px(6.0);
        let reset = button_width("PrintScreen");
        parts.push((
            Part::ResetHotkey,
            IRect::new(margin + content - reset, y, reset, row),
        ));
        parts.push((
            Part::HotkeyField,
            IRect::new(margin, y, content - reset - px(8.0), row),
        ));
        y += row + px(6.0);

        let help_height = gdi::measure_wrapped(dc.hdc(), &fonts.help, s.shortcut_help(), content);
        let help = IRect::new(margin, y, content, help_height);
        y += help_height + px(18.0);

        let folder_label = IRect::new(margin, y, content, line);
        y += line + px(6.0);
        let browse = button_width(s.browse());
        parts.push((
            Part::Browse,
            IRect::new(margin + content - browse, y, browse, row),
        ));
        parts.push((
            Part::FolderField,
            IRect::new(margin, y, content - browse - px(8.0), row),
        ));
        y += row + px(18.0);

        let check_height = line.max(px(18.0));
        let check_width = |text: &str| px(13.0 + 6.0) + fonts.text.measure(dc.hdc(), text).0;
        parts.push((
            Part::CopyOnCapture,
            IRect::new(margin, y, check_width(s.copy_on_capture()), check_height),
        ));
        y += check_height + px(8.0);
        parts.push((
            Part::StartWithWindows,
            IRect::new(margin, y, check_width(s.start_with_windows()), check_height),
        ));
        y += check_height + px(22.0);

        let save = button_width(s.save_button());
        let cancel = button_width(s.cancel());
        let right = margin + content;
        parts.push((Part::Save, IRect::new(right - save, y, save, row)));
        parts.push((
            Part::Cancel,
            IRect::new(right - save - px(8.0) - cancel, y, cancel, row),
        ));
        y += row + margin;

        Ok(Layout {
            scale,
            height: y,
            shortcut_label,
            help,
            folder_label,
            parts,
        })
    }

    fn part_at(&self, p: (i32, i32)) -> Option<Part> {
        self.parts
            .iter()
            .find(|(_, r)| p.0 >= r.x && p.0 < r.right() && p.1 >= r.y && p.1 < r.bottom())
            .map(|(part, _)| *part)
    }

    fn rect(&self, part: Part) -> IRect {
        self.parts
            .iter()
            .find(|(p, _)| *p == part)
            .map(|(_, r)| *r)
            .unwrap_or_default()
    }
}

struct View {
    layout: Layout,
    fonts: Fonts,
    hover: Option<Part>,
    pressed: Option<Part>,
    tracking_leave: bool,
}

struct Settings {
    hwnd: Cell<HWND>,
    form: RefCell<Form>,
    view: RefCell<View>,
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    window::guarded("Settings", hwnd, msg, wparam, lparam, || {
        if msg == WM_NCDESTROY {
            window::release_state::<Settings>(hwnd);
            return window::default_proc(hwnd, msg, wparam, lparam);
        }
        match window::state::<Settings>(hwnd) {
            Some(settings) => settings
                .handle(msg, wparam, lparam)
                .unwrap_or_else(|| window::default_proc(hwnd, msg, wparam, lparam)),
            None => window::default_proc(hwnd, msg, wparam, lparam),
        }
    })
}

// As in the editor: no RefCell borrow is held while calling into Windows in ways that send
// messages back (dialogs, capture, destroy).
impl Settings {
    fn hwnd(&self) -> HWND {
        self.hwnd.get()
    }

    fn handle(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_ERASEBKGND => return Some(LRESULT(1)),
            WM_PAINT => window::paint_all(self.hwnd(), |hdc| self.paint(hdc)),
            WM_SETCURSOR if (lparam.0 & 0xFFFF) as u32 == HTCLIENT => {
                let p = window::cursor_in_client(self.hwnd());
                let part = self.view.borrow().layout.part_at(p);
                let clickable =
                    matches!(part, Some(p) if p != Part::HotkeyField && p != Part::FolderField);
                window::set_cursor(if clickable { IDC_HAND } else { IDC_ARROW });
                return Some(LRESULT(1));
            }
            WM_MOUSEMOVE => self.mouse_move(window::mouse_position(lparam)),
            WM_MOUSELEAVE => {
                let mut view = self.view.borrow_mut();
                view.tracking_leave = false;
                view.hover = None;
                drop(view);
                window::invalidate_all(self.hwnd());
            }
            WM_LBUTTONDOWN => self.mouse_down(window::mouse_position(lparam)),
            WM_LBUTTONUP => self.mouse_up(window::mouse_position(lparam)),
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let key = wparam.0 as u16;
                if !self.key_down(key) && msg == WM_SYSKEYDOWN {
                    return None; // Alt+F4 and friends when not recording
                }
            }
            // Windows only delivers a key-up for PrintScreen.
            WM_KEYUP | WM_SYSKEYUP => {
                if wparam.0 as u16 == vk::SNAPSHOT && self.form.borrow().recording {
                    self.record(vk::SNAPSHOT);
                } else if msg == WM_SYSKEYUP {
                    return None;
                }
            }
            WM_CLOSE => window::destroy(self.hwnd()),
            // The global hotkey was paused while this window was open.
            WM_DESTROY => app::settings_closed(),
            _ => return None,
        }
        Some(LRESULT(0))
    }

    // ---- Keyboard ------------------------------------------------------------------------

    /// Returns whether the key was handled.
    fn key_down(&self, key: u16) -> bool {
        let modifiers = current_modifiers();
        let recording = self.form.borrow().recording;
        if !recording || HotkeyGesture::is_dialog_navigation(modifiers, key) {
            match key {
                vk::ESCAPE => window::request_close(self.hwnd()),
                vk::RETURN => self.save(),
                vk::TAB => self.set_recording(false),
                _ => return false,
            }
            return true;
        }
        self.record(key);
        true
    }

    fn record(&self, key: u16) {
        let Some(gesture) = HotkeyGesture::from_recorded(current_modifiers(), key) else {
            return;
        };
        let mut form = self.form.borrow_mut();
        form.hotkey = gesture;
        form.prompt = false;
        drop(form);
        self.invalidate(Part::HotkeyField);
    }

    fn set_recording(&self, recording: bool) {
        let changed = {
            let mut form = self.form.borrow_mut();
            form.prompt = recording;
            std::mem::replace(&mut form.recording, recording) != recording
        };
        if changed {
            self.invalidate(Part::HotkeyField);
        }
    }

    // ---- Mouse ---------------------------------------------------------------------------

    fn mouse_move(&self, p: (i32, i32)) {
        let mut view = self.view.borrow_mut();
        if !view.tracking_leave {
            view.tracking_leave = true;
            window::track_mouse_leave(self.hwnd());
        }
        let hover = view.layout.part_at(p);
        if hover != view.hover {
            view.hover = hover;
            drop(view);
            window::invalidate_all(self.hwnd());
        }
    }

    fn mouse_down(&self, p: (i32, i32)) {
        // Keys go to this window (the recorder lives in it).
        // SAFETY: plain focus call on our own window.
        unsafe {
            let _ = SetFocus(Some(self.hwnd()));
        }
        let part = self.view.borrow().layout.part_at(p);
        self.set_recording(part == Some(Part::HotkeyField));
        if part.is_some_and(|p| p != Part::HotkeyField) {
            self.view.borrow_mut().pressed = part;
            window::capture_mouse(self.hwnd());
        }
    }

    fn mouse_up(&self, p: (i32, i32)) {
        let pressed = self.view.borrow_mut().pressed.take();
        window::release_mouse();
        let released_on = self.view.borrow().layout.part_at(p);
        if let Some(part) = pressed
            && released_on == Some(part)
        {
            self.click(part);
        }
    }

    fn click(&self, part: Part) {
        match part {
            Part::ResetHotkey => {
                self.form.borrow_mut().hotkey = HotkeyGesture::DEFAULT;
                self.invalidate(Part::HotkeyField);
            }
            Part::Browse | Part::FolderField => self.browse(),
            Part::CopyOnCapture => {
                let mut form = self.form.borrow_mut();
                form.copy_on_capture = !form.copy_on_capture;
                drop(form);
                self.invalidate(part);
            }
            Part::StartWithWindows => {
                let mut form = self.form.borrow_mut();
                form.start_with_windows = !form.start_with_windows;
                drop(form);
                self.invalidate(part);
            }
            Part::Cancel => window::request_close(self.hwnd()),
            Part::Save => self.save(),
            Part::HotkeyField => {}
        }
    }

    fn browse(&self) {
        let current = PathBuf::from(&self.form.borrow().folder);
        let title = strings::get().choose_save_folder();
        if let Some(folder) = dialogs::pick_folder(self.hwnd(), title, &current) {
            self.form.borrow_mut().folder = folder.to_string_lossy().into_owned();
            self.invalidate(Part::FolderField);
        }
    }

    fn save(&self) {
        let settings = {
            let form = self.form.borrow();
            AppSettings {
                hotkey: form.hotkey.to_string(),
                save_folder: form.folder.trim().to_string(),
                copy_on_capture: form.copy_on_capture,
                start_with_windows: form.start_with_windows,
            }
        };
        app::apply_settings(settings);
        window::request_close(self.hwnd()); // closing re-registers the (new) hotkey
    }

    fn invalidate(&self, part: Part) {
        let rect = self.view.borrow().layout.rect(part);
        window::invalidate(self.hwnd(), rect.inflate(2));
    }

    // ---- Painting ------------------------------------------------------------------------

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
        let view = self.view.borrow();
        let form = self.form.borrow();
        let layout = &view.layout;
        let s = strings::get();
        let scale = layout.scale;

        if let Some(mut pixmap) =
            PixmapMut::from_bytes(back.pixels_mut(), width as u32, height as u32)
        {
            pixmap.fill(color(WINDOW_BACKGROUND, 1.0));
            for &(part, rect) in &layout.parts {
                let hover = view.hover == Some(part);
                match part {
                    Part::HotkeyField | Part::FolderField => {
                        let focused = part == Part::HotkeyField && form.recording;
                        draw_field(&mut pixmap, rect, focused, scale);
                    }
                    Part::CopyOnCapture | Part::StartWithWindows => {
                        let checked = if part == Part::CopyOnCapture {
                            form.copy_on_capture
                        } else {
                            form.start_with_windows
                        };
                        draw_checkbox(&mut pixmap, rect, checked, hover, scale);
                    }
                    _ => {
                        let background = if part == Part::Save {
                            ACCENT
                        } else {
                            CONTROL_BACKGROUND
                        };
                        let opacity = if hover { 0.85 } else { 1.0 };
                        fill_rounded(
                            &mut pixmap,
                            rect,
                            (6.0 * scale) as f32,
                            color(background, opacity),
                        );
                    }
                }
            }
        }

        let dc = back_dc.hdc();
        let fg = rgb(FOREGROUND);
        gdi::draw_text(
            dc,
            &view.fonts.label,
            s.shortcut(),
            layout.shortcut_label,
            fg,
            Align::Left,
        );
        gdi::draw_text(
            dc,
            &view.fonts.label,
            s.save_folder(),
            layout.folder_label,
            fg,
            Align::Left,
        );
        gdi::draw_wrapped(
            dc,
            &view.fonts.help,
            s.shortcut_help(),
            layout.help,
            rgb(SUBTLE_FOREGROUND),
        );

        let inner = |r: IRect| {
            let pad = (7.0 * scale).round() as i32;
            IRect::new(r.x + pad, r.y, r.width - 2 * pad, r.height)
        };
        let hotkey_text = if form.prompt {
            s.press_shortcut().to_string()
        } else {
            form.hotkey.to_string()
        };
        gdi::draw_text(
            dc,
            &view.fonts.text,
            &hotkey_text,
            inner(layout.rect(Part::HotkeyField)),
            fg,
            Align::Left,
        );
        gdi::draw_path(
            dc,
            &view.fonts.text,
            &form.folder,
            inner(layout.rect(Part::FolderField)),
            fg,
        );

        for (part, text) in [
            (Part::ResetHotkey, "PrintScreen"),
            (Part::Browse, s.browse()),
            (Part::Cancel, s.cancel()),
            (Part::Save, s.save_button()),
        ] {
            gdi::draw_text(
                dc,
                &view.fonts.text,
                text,
                layout.rect(part),
                fg,
                Align::Center,
            );
        }
        for (part, text) in [
            (Part::CopyOnCapture, s.copy_on_capture()),
            (Part::StartWithWindows, s.start_with_windows()),
        ] {
            let r = layout.rect(part);
            let offset = (19.0 * scale).round() as i32;
            gdi::draw_text(
                dc,
                &view.fonts.text,
                text,
                IRect::new(r.x + offset, r.y, r.width - offset, r.height),
                fg,
                Align::Left,
            );
        }

        gdi::blit(hdc, 0, 0, width, height, dc, 0, 0);
    }
}

/// Ctrl/Shift/Alt/Win as currently held down.
fn current_modifiers() -> Modifiers {
    let mut modifiers = Modifiers::NONE;
    if window::is_key_down(VK_CONTROL.0) {
        modifiers |= Modifiers::CONTROL;
    }
    if window::is_key_down(VK_SHIFT.0) {
        modifiers |= Modifiers::SHIFT;
    }
    if window::is_key_down(VK_MENU.0) {
        modifiers |= Modifiers::ALT;
    }
    if window::is_key_down(VK_LWIN.0) || window::is_key_down(VK_RWIN.0) {
        modifiers |= Modifiers::WIN;
    }
    modifiers
}

/// A dark text box: #3A3A3C with a 1 px #48484A border (accent when focused).
fn draw_field(pixmap: &mut PixmapMut<'_>, r: IRect, focused: bool, scale: f64) {
    let border = (scale.round() as i32).max(1);
    fill_rect(
        pixmap,
        r,
        color(if focused { ACCENT } else { HOVER_BACKGROUND }, 1.0),
    );
    fill_rect(
        pixmap,
        IRect::new(
            r.x + border,
            r.y + border,
            r.width - 2 * border,
            r.height - 2 * border,
        ),
        color(CONTROL_BACKGROUND, 1.0),
    );
}

/// The standard (light) WPF check box: white box with a grey border and a dark check mark.
fn draw_checkbox(pixmap: &mut PixmapMut<'_>, row: IRect, checked: bool, hover: bool, scale: f64) {
    let size = (13.0 * scale).round() as i32;
    let r = IRect::new(row.x, row.y + (row.height - size) / 2, size, size);
    let border = (scale.round() as i32).max(1);
    let (edge, fill) = if hover {
        (Rgb(0x55, 0x93, 0xFF), Rgb(0xF3, 0xF9, 0xFF))
    } else {
        (Rgb(0x70, 0x70, 0x70), Rgb(0xFF, 0xFF, 0xFF))
    };
    fill_rect(pixmap, r, color(edge, 1.0));
    fill_rect(
        pixmap,
        IRect::new(
            r.x + border,
            r.y + border,
            r.width - 2 * border,
            r.height - 2 * border,
        ),
        color(fill, 1.0),
    );
    if !checked {
        return;
    }
    let (x, y, s) = (r.x as f32, r.y as f32, size as f32);
    let mut pb = PathBuilder::new();
    pb.move_to(x + s * 0.22, y + s * 0.52);
    pb.line_to(x + s * 0.42, y + s * 0.72);
    pb.line_to(x + s * 0.8, y + s * 0.28);
    if let Some(path) = pb.finish() {
        let stroke = Stroke {
            width: (1.8 * scale) as f32,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        let mut paint = Paint::default();
        paint.set_color(color(Rgb(0x21, 0x21, 0x21), 1.0));
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

fn color(rgb: Rgb, opacity: f32) -> tiny_skia::Color {
    draw::color(rgb.0, rgb.1, rgb.2, (opacity * 255.0).round() as u8)
}

fn rgb(c: Rgb) -> (u8, u8, u8) {
    (c.0, c.1, c.2)
}

fn fill_rect(pixmap: &mut PixmapMut<'_>, r: IRect, c: tiny_skia::Color) {
    if let Some(rect) =
        tiny_skia::Rect::from_xywh(r.x as f32, r.y as f32, r.width as f32, r.height as f32)
    {
        let mut paint = Paint::default();
        paint.set_color(c);
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

fn fill_rounded(pixmap: &mut PixmapMut<'_>, r: IRect, radius: f32, c: tiny_skia::Color) {
    if let Some(path) = draw::rounded_rect(
        r.x as f32,
        r.y as f32,
        r.width as f32,
        r.height as f32,
        radius,
    ) {
        let mut paint = Paint::default();
        paint.set_color(c);
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}
