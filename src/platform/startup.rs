//! "Start with Windows" through the current user's Run key.

use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
};
use windows::core::{PCWSTR, Result, w};

use super::wide;

/// Different from the C# version ("Snapr"), so both can be switched on and off separately.
const VALUE_NAME: PCWSTR = w!("RustySnapr");

pub fn apply(enabled: bool) -> Result<()> {
    let mut key = HKEY::default();
    // SAFETY: opens (or creates) a key under HKCU and closes it again below.
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()?;
    }

    let result = if enabled {
        let exe = std::env::current_exe()
            .map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()))?;
        let value = wide(&format!("\"{}\"", exe.display()));
        // SAFETY: writes a null-terminated UTF-16 string; the byte length includes the terminator.
        let bytes =
            unsafe { std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2) };
        // SAFETY: `key` is open with KEY_SET_VALUE.
        unsafe { RegSetValueExW(key, VALUE_NAME, None, REG_SZ, Some(bytes)).ok() }
    } else {
        // SAFETY: `key` is open with KEY_SET_VALUE. A missing value is fine.
        let status = unsafe { RegDeleteValueW(key, VALUE_NAME) };
        if status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            status.ok()
        }
    };
    // SAFETY: closes the key opened above.
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}
