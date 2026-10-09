//! Tooltips for areas of a window (the editor's toolbar buttons), using the common control.

use snapr_core::geometry::IRect;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::Controls::{
    ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, TOOLTIPS_CLASSW, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_NEWTOOLRECTW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, SendMessageW, WINDOW_STYLE, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PWSTR, Result};

use super::window;

/// Registers the common controls (tooltips). Call once at startup.
pub fn init_common_controls() {
    let init = INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_WIN95_CLASSES,
    };
    // SAFETY: plain initialisation with a fully filled struct.
    unsafe {
        let _ = InitCommonControlsEx(&init);
    }
}

pub struct Tooltips {
    tooltip: HWND,
    owner: HWND,
}

impl Tooltips {
    /// A tooltip control for `owner`; destroyed together with it.
    pub fn new(owner: HWND) -> Result<Tooltips> {
        // SAFETY: creates a standard tooltip window owned by `owner`.
        let tooltip = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST,
                TOOLTIPS_CLASSW,
                None,
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                Some(owner),
                None,
                Some(window::instance()),
                None,
            )?
        };
        Ok(Tooltips { tooltip, owner })
    }

    /// Adds a tooltip with text for an area of the owner window.
    pub fn add(&self, id: usize, rect: IRect, text: &str) {
        let mut text = super::wide(text);
        let mut info = self.info(id, rect);
        info.lpszText = PWSTR(text.as_mut_ptr()); // the control copies the text
        self.send(TTM_ADDTOOLW, &info);
    }

    /// Moves a tooltip's area, e.g. after the window was resized.
    pub fn move_to(&self, id: usize, rect: IRect) {
        let info = self.info(id, rect);
        self.send(TTM_NEWTOOLRECTW, &info);
    }

    fn info(&self, id: usize, rect: IRect) -> TTTOOLINFOW {
        TTTOOLINFOW {
            cbSize: size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_SUBCLASS,
            hwnd: self.owner,
            uId: id,
            rect: RECT {
                left: rect.x,
                top: rect.y,
                right: rect.right(),
                bottom: rect.bottom(),
            },
            ..Default::default()
        }
    }

    fn send(&self, msg: u32, info: &TTTOOLINFOW) {
        // SAFETY: `info` is a fully initialised TTTOOLINFOW that outlives the synchronous call.
        unsafe {
            SendMessageW(
                self.tooltip,
                msg,
                Some(WPARAM(0)),
                Some(LPARAM(info as *const _ as isize)),
            );
        }
    }
}
