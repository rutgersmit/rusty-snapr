//! Standard dialogs: save file and message boxes with an owner window.

use std::path::{Path, PathBuf};

use snapr_core::APP_NAME;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog,
    FileSaveDialog, IFileOpenDialog, IFileSaveDialog, IShellItem, SHCreateItemFromParsingName,
    SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::core::PCWSTR;

use super::wide;

/// Asks where to save a PNG. Starts in `folder` with `file_name` filled in; `None` if cancelled.
pub fn save_png(owner: HWND, folder: &Path, file_name: &str, type_name: &str) -> Option<PathBuf> {
    let type_name = wide(type_name);
    let spec = wide("*.png");
    let file_name = wide(file_name);
    let extension = wide("png");
    let folder = wide(&folder.to_string_lossy());

    // SAFETY: COM is initialised on this thread at startup; all strings outlive the dialog.
    unsafe {
        let dialog: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let filters = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(type_name.as_ptr()),
            pszSpec: PCWSTR(spec.as_ptr()),
        }];
        dialog.SetFileTypes(&filters).ok()?;
        dialog.SetFileName(PCWSTR(file_name.as_ptr())).ok()?;
        dialog
            .SetDefaultExtension(PCWSTR(extension.as_ptr()))
            .ok()?;
        let options = dialog.GetOptions().unwrap_or_default();
        let _ = dialog.SetOptions(options | FOS_OVERWRITEPROMPT | FOS_PATHMUSTEXIST);
        // An unusable folder just leaves the dialog in its own default folder.
        if let Ok(item) =
            SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(folder.as_ptr()), None)
        {
            let _ = dialog.SetFolder(&item);
        }

        dialog.Show(Some(owner)).ok()?; // an error here means "cancelled"
        let result = dialog.GetResult().ok()?;
        let path = result.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        text.map(PathBuf::from)
    }
}

/// Asks for a folder, starting in `initial` if it exists. `None` if cancelled.
pub fn pick_folder(owner: HWND, title: &str, initial: &Path) -> Option<PathBuf> {
    let title = wide(title);
    let initial = wide(&initial.to_string_lossy());
    // SAFETY: COM is initialised on this thread at startup; all strings outlive the dialog.
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().unwrap_or_default();
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
            .ok()?;
        let _ = dialog.SetTitle(PCWSTR(title.as_ptr()));
        if let Ok(item) =
            SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(initial.as_ptr()), None)
        {
            let _ = dialog.SetFolder(&item);
        }
        dialog.Show(Some(owner)).ok()?;
        let result = dialog.GetResult().ok()?;
        let path = result.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        text.map(PathBuf::from)
    }
}

pub fn error_box(owner: HWND, text: &str) {
    let text = wide(text);
    let caption = wide(APP_NAME);
    // SAFETY: both strings are null-terminated and outlive the call.
    unsafe {
        MessageBoxW(
            Some(owner),
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}
