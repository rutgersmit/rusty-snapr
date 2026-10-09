//! The tray app: startup, the hidden main window and the tray icon.

use std::cell::{Cell, OnceCell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use snapr_core::editor::Bitmap;
use snapr_core::error_log::ErrorLog;
use snapr_core::hotkey::vk;
use snapr_core::settings::AppSettings;
use snapr_core::{APP_NAME, DATA_FOLDER_NAME, strings};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, MSG, PostQuitMessage,
    RegisterWindowMessageW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP,
    WM_CONTEXTMENU, WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP,
};
use windows::core::{PCWSTR, Result, w};

use crate::platform::errors;
use crate::platform::instance::SingleInstance;
use crate::platform::tray::{self, BalloonIcon, TrayIcon};
use crate::platform::{clipboard, locale, paths, tooltip, wide, window};
use crate::ui::{editor, overlay};

/// Different from the C# version's mutex, so both can run side by side.
const SINGLE_INSTANCE_MUTEX: &str = r"Local\RustySnapr.SingleInstance";
const MAIN_WINDOW_CLASS: PCWSTR = w!("RustySnaprMain");

/// Tray icon mouse events.
const WM_TRAY: u32 = WM_APP + 1;
/// Opens the editor for the last capture, once the overlay window is completely gone.
const WM_OPEN_EDITOR: u32 = WM_APP + 2;

const HOTKEY_ID: i32 = 0x5A1;

const MENU_CAPTURE: u32 = 1;
const MENU_EXIT: u32 = 3;

thread_local! {
    static APP: OnceCell<Rc<App>> = const { OnceCell::new() };
}

/// The current settings (defaults if the app isn't running, e.g. during shutdown).
pub fn settings() -> AppSettings {
    App::current()
        .map(|app| app.settings.clone())
        .unwrap_or_default()
}

pub fn run() -> i32 {
    // Apartment-threaded COM for the shell's file dialogs.
    // SAFETY: called once on the UI thread before any window exists.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    }
    let data_dir = paths::app_data().join(DATA_FOLDER_NAME);
    errors::init(ErrorLog::new(data_dir.join("snapr.log")));
    strings::init(locale::ui_language());
    tooltip::init_common_controls();
    // The numbered markers use Segoe UI Bold like the C# version; a built-in font otherwise.
    if let Ok(font) = std::fs::read(paths::fonts().join("segoeuib.ttf")) {
        snapr_core::editor::number::set_font(font);
    }

    let Some(_instance) = SingleInstance::acquire(SINGLE_INSTANCE_MUTEX) else {
        return 0;
    };

    let app = match App::start(data_dir) {
        Ok(app) => app,
        Err(error) => {
            errors::report_error("Startup", &error);
            return 1;
        }
    };
    APP.with(|cell| {
        let _ = cell.set(app.clone());
    });
    app.register_hotkey(true);

    message_loop()
}

fn message_loop() -> i32 {
    let mut msg = MSG::default();
    // SAFETY: standard Win32 message loop on the thread that owns all our windows.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    msg.wParam.0 as i32
}

pub struct App {
    /// Hidden top-level window that receives tray and hotkey events. Not a message-only window,
    /// because those don't get the "TaskbarCreated" broadcast.
    hwnd: HWND,
    tray: TrayIcon,
    /// Sent by Explorer when it (re)creates the taskbar; the tray icon must then be re-added.
    taskbar_created: u32,
    settings: AppSettings,
    #[allow(dead_code)] // saved from the settings window (phase 5)
    settings_path: PathBuf,
    hotkey_registered: Cell<bool>,
    overlay_open: Cell<bool>,
    /// A finished capture waiting for WM_OPEN_EDITOR.
    pending_capture: RefCell<Option<Bitmap>>,
}

impl App {
    fn start(data_dir: PathBuf) -> Result<Rc<App>> {
        let settings_path = data_dir.join("settings.json");
        let mut settings = AppSettings::load(&settings_path);
        settings.fill_defaults(&paths::pictures().join("Screenshots"));

        window::register_class(MAIN_WINDOW_CLASS, Some(main_wndproc), None, false)?;
        overlay::register_class()?;
        editor::register_class()?;
        let title = wide(APP_NAME);
        // SAFETY: the class was registered above; the title outlives the call.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                MAIN_WINDOW_CLASS,
                PCWSTR(title.as_ptr()),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                None,
                None,
                Some(window::instance()),
                None,
            )?
        };

        // SAFETY: registering a well-known message name has no preconditions.
        let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        let tray = TrayIcon::add(hwnd, WM_TRAY, APP_NAME)?;

        Ok(Rc::new(App {
            hwnd,
            tray,
            taskbar_created,
            settings,
            settings_path,
            hotkey_registered: Cell::new(false),
            overlay_open: Cell::new(false),
            pending_capture: RefCell::new(None),
        }))
    }

    pub fn current() -> Option<Rc<App>> {
        APP.with(|cell| cell.get().cloned())
    }

    /// (Re)registers the configured hotkey and tells the user when Windows refuses it.
    fn register_hotkey(&self, show_success: bool) {
        self.unregister_hotkey();
        let s = strings::get();
        let gesture = self.settings.hotkey_gesture();
        let modifiers = HOT_KEY_MODIFIERS(gesture.modifiers.native()) | MOD_NOREPEAT;
        // SAFETY: registers a hotkey for our own window.
        let registered =
            unsafe { RegisterHotKey(Some(self.hwnd), HOTKEY_ID, modifiers, gesture.key as u32) }
                .is_ok();
        self.hotkey_registered.set(registered);

        let name = gesture.to_string();
        if registered {
            if show_success {
                self.tray.show_balloon(
                    &s.running_title(),
                    &s.running_text(&name),
                    BalloonIcon::Info,
                );
            }
        } else {
            // Windows 11 binds PrintScreen to the Snipping Tool by default; explain how to free it.
            let message = if gesture.key == vk::SNAPSHOT {
                s.print_screen_in_use().to_string()
            } else {
                s.hotkey_in_use(&name)
            };
            self.tray
                .show_balloon(s.hotkey_unavailable_title(), &message, BalloonIcon::Warning);
        }
    }

    fn unregister_hotkey(&self) {
        if self.hotkey_registered.replace(false) {
            // SAFETY: unregisters the hotkey registered above.
            unsafe {
                let _ = UnregisterHotKey(Some(self.hwnd), HOTKEY_ID);
            }
        }
    }

    fn start_capture(&self) {
        if self.overlay_open.get() {
            return;
        }
        self.overlay_open.set(true);
        let shown = overlay::show(Box::new(|result| {
            if let Some(app) = App::current() {
                app.capture_finished(result);
            }
        }));
        if let Err(error) = shown {
            // Otherwise every later capture would be ignored.
            self.overlay_open.set(false);
            errors::report_error("Capture", &error);
        }
    }

    /// Called while the overlay window is being destroyed. The editor opens a moment later, from
    /// the message queue, so it isn't created (and activated) in the middle of that.
    fn capture_finished(&self, result: Option<Bitmap>) {
        self.overlay_open.set(false);
        let Some(image) = result else { return };
        if self.settings.copy_on_capture {
            clipboard::set_image(self.hwnd, &image);
        }
        *self.pending_capture.borrow_mut() = Some(image);
        window::post(self.hwnd, WM_OPEN_EDITOR);
    }

    fn open_editor(&self) {
        let Some(image) = self.pending_capture.borrow_mut().take() else {
            return;
        };
        if let Err(error) = editor::open(image) {
            errors::report_error("Editor", &error);
        }
    }

    fn show_tray_menu(&self) {
        let s = strings::get();
        let chosen = tray::show_menu(
            self.hwnd,
            &[
                Some((MENU_CAPTURE, s.tray_capture())),
                None,
                Some((MENU_EXIT, s.tray_exit())),
            ],
        );
        match chosen {
            Some(MENU_CAPTURE) => self.start_capture(),
            Some(MENU_EXIT) => self.exit(),
            _ => {}
        }
    }

    fn exit(&self) {
        self.unregister_hotkey();
        self.tray.remove();
        // SAFETY: our own window; WM_DESTROY then ends the message loop.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

extern "system" fn main_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    window::guarded("Tray", hwnd, msg, wparam, lparam, || {
        // Messages during CreateWindowExW arrive before the app exists.
        let Some(app) = App::current() else {
            return window::default_proc(hwnd, msg, wparam, lparam);
        };
        match msg {
            WM_TRAY => {
                match lparam.0 as u32 {
                    WM_LBUTTONUP => app.start_capture(),
                    WM_RBUTTONUP | WM_CONTEXTMENU => app.show_tray_menu(),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_OPEN_EDITOR => {
                app.open_editor();
                LRESULT(0)
            }
            WM_HOTKEY if wparam.0 == HOTKEY_ID as usize => {
                app.start_capture();
                LRESULT(0)
            }
            WM_DESTROY => {
                // SAFETY: ends the message loop on this thread.
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ if msg == app.taskbar_created && msg != 0 => {
                if let Err(error) = app.tray.show() {
                    errors::write_log("Tray", &format!("re-adding tray icon failed: {error}"));
                }
                LRESULT(0)
            }
            _ => window::default_proc(hwnd, msg, wparam, lparam),
        }
    })
}
