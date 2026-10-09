use std::path::PathBuf;

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_Pictures, FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows::core::GUID;

/// %AppData% (roaming).
pub fn app_data() -> PathBuf {
    known_folder(&FOLDERID_RoamingAppData)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir)
}

/// The user's Pictures folder.
pub fn pictures() -> PathBuf {
    known_folder(&FOLDERID_Pictures)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Pictures")))
        .unwrap_or_else(std::env::temp_dir)
}

fn known_folder(id: &GUID) -> Option<PathBuf> {
    // SAFETY: on success the returned string is owned by us and freed with CoTaskMemFree.
    unsafe {
        let path = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let result = path.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(path.0 as *const _));
        result
    }
}
