use snapr_core::strings::Language;
use windows::Win32::Globalization::GetUserDefaultUILanguage;

/// Dutch when the Windows display language is Dutch (any region), English otherwise.
pub fn ui_language() -> Language {
    const LANG_DUTCH: u16 = 0x13;
    // SAFETY: no arguments, no preconditions.
    let lang_id = unsafe { GetUserDefaultUILanguage() };
    if lang_id & 0x3FF == LANG_DUTCH {
        Language::Dutch
    } else {
        Language::English
    }
}
