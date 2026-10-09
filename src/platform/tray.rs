//! The notification-area icon: tooltip, balloons and a context menu.

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIIF_WARNING, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos, GetSystemMetrics, HICON,
    IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, MF_SEPARATOR, MF_STRING, PostMessageW, SM_CXSMICON,
    SM_CYSMICON, SetForegroundWindow, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
    WM_NULL,
};
use windows::core::{PCWSTR, Result};

use super::{copy_wide, wide, window};

#[allow(dead_code)] // used from phase 2 on (hotkey notifications)
#[derive(Debug, Clone, Copy)]
pub enum BalloonIcon {
    Info,
    Warning,
}

pub struct TrayIcon {
    data: NOTIFYICONDATAW,
    icon: HICON,
}

impl TrayIcon {
    /// Adds the app icon (resource 1) to the notification area. Mouse events arrive at `hwnd` as
    /// `callback_message`, with the mouse message (e.g. `WM_LBUTTONUP`) in `lparam`.
    pub fn add(hwnd: HWND, callback_message: u32, tooltip: &str) -> Result<TrayIcon> {
        let icon = load_small_app_icon()?;
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: callback_message,
            hIcon: icon,
            ..Default::default()
        };
        copy_wide(&mut data.szTip, tooltip);

        let tray = TrayIcon { data, icon };
        tray.show()?;
        Ok(tray)
    }

    /// Adds the icon again, e.g. after Explorer restarted and the notification area was rebuilt.
    pub fn show(&self) -> Result<()> {
        // SAFETY: `data` is a fully initialised NOTIFYICONDATAW.
        unsafe { Shell_NotifyIconW(NIM_ADD, &self.data) }.ok()
    }

    #[allow(dead_code)] // used from phase 2 on
    pub fn show_balloon(&self, title: &str, text: &str, icon: BalloonIcon) {
        let mut data = self.data;
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = match icon {
            BalloonIcon::Info => NIIF_INFO,
            BalloonIcon::Warning => NIIF_WARNING,
        };
        copy_wide(&mut data.szInfoTitle, title);
        copy_wide(&mut data.szInfo, text);
        // SAFETY: `data` is a fully initialised NOTIFYICONDATAW. A failed balloon is harmless.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }

    pub fn remove(&self) {
        // SAFETY: removing an icon that isn't there just fails.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.data);
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        self.remove();
        // SAFETY: the icon was loaded by us and is no longer used by the shell.
        unsafe {
            let _ = DestroyIcon(self.icon);
        }
    }
}

/// An entry of the tray menu: `Some((id, text))`, or `None` for a separator.
pub type MenuItem<'a> = Option<(u32, &'a str)>;

/// Shows a context menu at the cursor and returns the chosen id, if any.
pub fn show_menu(hwnd: HWND, items: &[MenuItem<'_>]) -> Option<u32> {
    // SAFETY: the menu is created, used and destroyed within this function; the strings live
    // until the menu is shown.
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let texts: Vec<Vec<u16>> = items
            .iter()
            .map(|item| wide(item.map_or("", |(_, text)| text)))
            .collect();
        for (item, text) in items.iter().zip(&texts) {
            let _ = match item {
                Some((id, _)) => AppendMenuW(menu, MF_STRING, *id as usize, PCWSTR(text.as_ptr())),
                None => AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()),
            };
        }

        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        // Without the foreground window the menu would not close when clicking elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            cursor.x,
            cursor.y,
            None,
            hwnd,
            None,
        );
        // Lets the menu close properly the next time (documented TrackPopupMenu quirk).
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);

        (chosen.0 != 0).then_some(chosen.0 as u32)
    }
}

/// The app icon at the small-icon size, so the tray shows a crisp image at any DPI.
fn load_small_app_icon() -> Result<HICON> {
    // SAFETY: resource 1 is the icon embedded by build.rs.
    unsafe {
        let handle = LoadImageW(
            Some(window::instance()),
            PCWSTR(std::ptr::without_provenance(1)), // MAKEINTRESOURCE(1)
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_DEFAULTCOLOR,
        )?;
        Ok(HICON(handle.0))
    }
}
