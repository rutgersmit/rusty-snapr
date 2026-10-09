//! A global keyboard shortcut such as "PrintScreen" or "Ctrl+Shift+S".
//!
//! The text format is the one the C# Snapr stores in settings.json: modifiers in the order
//! Ctrl, Shift, Alt, Win, then the key by its WPF `Key` name, except that digits are written as
//! "0".."9" and PrintScreen as "PrintScreen". Every WPF alias is accepted when parsing (e.g.
//! "Return" and "Enter"), so a C# settings file reads the same here and vice versa.

use std::fmt;

/// Modifier flags. The bit values are the Win32 `MOD_*` constants, so they can be passed to
/// `RegisterHotKey` directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Modifiers = Modifiers(0);
    pub const ALT: Modifiers = Modifiers(0x1);
    pub const CONTROL: Modifiers = Modifiers(0x2);
    pub const SHIFT: Modifiers = Modifiers(0x4);
    pub const WIN: Modifiers = Modifiers(0x8);

    pub const fn contains(self, other: Modifiers) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Modifiers) -> Modifiers {
        Modifiers(self.0 | other.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The `MOD_*` flags for `RegisterHotKey`.
    pub const fn native(self) -> u32 {
        self.0 as u32
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Modifiers;
    fn bitor(self, other: Modifiers) -> Modifiers {
        self.union(other)
    }
}

impl std::ops::BitOrAssign for Modifiers {
    fn bitor_assign(&mut self, other: Modifiers) {
        *self = self.union(other);
    }
}

/// Win32 virtual-key codes used by the logic below.
pub mod vk {
    pub const TAB: u16 = 0x09;
    pub const RETURN: u16 = 0x0D;
    pub const SHIFT: u16 = 0x10;
    pub const CONTROL: u16 = 0x11;
    pub const MENU: u16 = 0x12;
    pub const ESCAPE: u16 = 0x1B;
    pub const SPACE: u16 = 0x20;
    pub const SNAPSHOT: u16 = 0x2C;
    pub const LWIN: u16 = 0x5B;
    pub const RWIN: u16 = 0x5C;
    pub const LSHIFT: u16 = 0xA0;
    pub const RMENU: u16 = 0xA5;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HotkeyGesture {
    pub modifiers: Modifiers,
    /// Win32 virtual-key code.
    pub key: u16,
}

impl Default for HotkeyGesture {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl HotkeyGesture {
    pub const DEFAULT: HotkeyGesture = HotkeyGesture {
        modifiers: Modifiers::NONE,
        key: vk::SNAPSHOT,
    };

    pub const fn new(modifiers: Modifiers, key: u16) -> Self {
        Self { modifiers, key }
    }

    pub fn is_modifier_key(key: u16) -> bool {
        matches!(
            key,
            vk::SHIFT | vk::CONTROL | vk::MENU | vk::LWIN | vk::RWIN | vk::LSHIFT..=vk::RMENU
        )
    }

    /// Parses the settings text. Anything unusable (empty, unknown key, only modifiers) gives the
    /// default, like the C# version.
    pub fn parse(text: &str) -> HotkeyGesture {
        let mut modifiers = Modifiers::NONE;
        let mut key = None;
        for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
                "shift" => modifiers |= Modifiers::SHIFT,
                "alt" => modifiers |= Modifiers::ALT,
                "win" | "windows" => modifiers |= Modifiers::WIN,
                "printscreen" | "prtsc" => key = Some(vk::SNAPSHOT),
                lower => {
                    if let Some(code) = key_from_name(lower) {
                        key = Some(code);
                    }
                }
            }
        }
        match key {
            Some(key) if !Self::is_modifier_key(key) => HotkeyGesture::new(modifiers, key),
            _ => HotkeyGesture::DEFAULT,
        }
    }

    /// The gesture for a key pressed in the settings recorder, or `None` if it should be ignored:
    /// a lone modifier, a key without a name, or a plain letter/digit/space that would fire while
    /// typing (those need Ctrl, Alt or Win).
    pub fn from_recorded(modifiers: Modifiers, key: u16) -> Option<HotkeyGesture> {
        if Self::is_modifier_key(key) || key_name(key).is_none() {
            return None;
        }
        let typing_key = matches!(key, 0x30..=0x39 | 0x41..=0x5A | vk::SPACE);
        let only_shift = modifiers == Modifiers::NONE || modifiers == Modifiers::SHIFT;
        if typing_key && only_shift {
            return None;
        }
        Some(HotkeyGesture::new(modifiers, key))
    }

    /// Tab, Esc and Enter without modifiers keep their normal dialog meaning in the recorder.
    pub fn is_dialog_navigation(modifiers: Modifiers, key: u16) -> bool {
        modifiers.is_empty() && matches!(key, vk::TAB | vk::ESCAPE | vk::RETURN)
    }
}

impl fmt::Display for HotkeyGesture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (flag, name) in [
            (Modifiers::CONTROL, "Ctrl"),
            (Modifiers::SHIFT, "Shift"),
            (Modifiers::ALT, "Alt"),
            (Modifiers::WIN, "Win"),
        ] {
            if self.modifiers.contains(flag) {
                write!(f, "{name}+")?;
            }
        }
        match key_name(self.key) {
            Some(name) => f.write_str(&name),
            None => write!(f, "VK{}", self.key),
        }
    }
}

/// Display name for a virtual key, as WPF names it (digits as "0".."9").
pub fn key_name(key: u16) -> Option<String> {
    match key {
        vk::SNAPSHOT => Some("PrintScreen".into()),
        0x30..=0x39 | 0x41..=0x5A => Some(char::from(key as u8).to_string()),
        0x60..=0x69 => Some(format!("NumPad{}", key - 0x60)),
        0x70..=0x87 => Some(format!("F{}", key - 0x70 + 1)),
        _ => NAMED_KEYS
            .iter()
            .find(|(code, _)| *code == key)
            .map(|(_, names)| names[0].to_string()),
    }
}

/// Virtual key for a (lower-case) WPF key name or alias.
fn key_from_name(lower: &str) -> Option<u16> {
    let bytes = lower.as_bytes();
    match bytes {
        [d @ b'0'..=b'9'] => return Some(*d as u16),
        [b'd', d @ b'0'..=b'9'] => return Some(*d as u16),
        [c @ b'a'..=b'z'] => return Some(c.to_ascii_uppercase() as u16),
        _ => {}
    }
    if let Some(n) = lower
        .strip_prefix("numpad")
        .and_then(|n| n.parse::<u16>().ok())
    {
        return (n <= 9).then_some(0x60 + n);
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
        return (1..=24).contains(&n).then_some(0x70 + n - 1);
    }
    if let Some(n) = lower.strip_prefix("vk").and_then(|n| n.parse::<u16>().ok()) {
        return Some(n);
    }
    NAMED_KEYS
        .iter()
        .find(|(_, names)| names.iter().any(|name| name.eq_ignore_ascii_case(lower)))
        .map(|(code, _)| *code)
}

/// Virtual key → WPF names. The first name is used for display; all are accepted when parsing.
const NAMED_KEYS: &[(u16, &[&str])] = &[
    (0x08, &["Back"]),
    (0x09, &["Tab"]),
    (0x0C, &["Clear"]),
    (0x0D, &["Enter", "Return"]),
    (0x10, &["Shift"]),
    (0x11, &["Ctrl"]),
    (0x12, &["Alt", "System"]),
    (0x13, &["Pause"]),
    (0x14, &["CapsLock", "Capital"]),
    (0x1B, &["Escape"]),
    (0x20, &["Space"]),
    (0x21, &["PageUp", "Prior"]),
    (0x22, &["PageDown", "Next"]),
    (0x23, &["End"]),
    (0x24, &["Home"]),
    (0x25, &["Left"]),
    (0x26, &["Up"]),
    (0x27, &["Right"]),
    (0x28, &["Down"]),
    (0x29, &["Select"]),
    (0x2A, &["Print"]),
    (0x2B, &["Execute"]),
    (0x2C, &["PrintScreen", "Snapshot"]),
    (0x2D, &["Insert"]),
    (0x2E, &["Delete"]),
    (0x2F, &["Help"]),
    (0x5B, &["LWin"]),
    (0x5C, &["RWin"]),
    (0x5D, &["Apps"]),
    (0x5F, &["Sleep"]),
    (0x6A, &["Multiply"]),
    (0x6B, &["Add"]),
    (0x6C, &["Separator"]),
    (0x6D, &["Subtract"]),
    (0x6E, &["Decimal"]),
    (0x6F, &["Divide"]),
    (0x90, &["NumLock"]),
    (0x91, &["Scroll"]),
    (0xA0, &["LeftShift"]),
    (0xA1, &["RightShift"]),
    (0xA2, &["LeftCtrl"]),
    (0xA3, &["RightCtrl"]),
    (0xA4, &["LeftAlt"]),
    (0xA5, &["RightAlt"]),
    (0xA6, &["BrowserBack"]),
    (0xA7, &["BrowserForward"]),
    (0xA8, &["BrowserRefresh"]),
    (0xA9, &["BrowserStop"]),
    (0xAA, &["BrowserSearch"]),
    (0xAB, &["BrowserFavorites"]),
    (0xAC, &["BrowserHome"]),
    (0xAD, &["VolumeMute"]),
    (0xAE, &["VolumeDown"]),
    (0xAF, &["VolumeUp"]),
    (0xB0, &["MediaNextTrack"]),
    (0xB1, &["MediaPreviousTrack"]),
    (0xB2, &["MediaStop"]),
    (0xB3, &["MediaPlayPause"]),
    (0xB4, &["LaunchMail"]),
    (0xB5, &["SelectMedia"]),
    (0xB6, &["LaunchApplication1"]),
    (0xB7, &["LaunchApplication2"]),
    (0xBA, &["OemSemicolon", "Oem1"]),
    (0xBB, &["OemPlus"]),
    (0xBC, &["OemComma"]),
    (0xBD, &["OemMinus"]),
    (0xBE, &["OemPeriod"]),
    (0xBF, &["OemQuestion", "Oem2"]),
    (0xC0, &["OemTilde", "Oem3"]),
    (0xDB, &["OemOpenBrackets", "Oem4"]),
    (0xDC, &["OemPipe", "Oem5"]),
    (0xDD, &["OemCloseBrackets", "Oem6"]),
    (0xDE, &["OemQuotes", "Oem7"]),
    (0xDF, &["Oem8"]),
    (0xE2, &["OemBackslash", "Oem102"]),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn gesture(modifiers: Modifiers, key: u16) -> HotkeyGesture {
        HotkeyGesture::new(modifiers, key)
    }

    #[test]
    fn default_is_printscreen() {
        assert_eq!(HotkeyGesture::DEFAULT.to_string(), "PrintScreen");
        assert_eq!(HotkeyGesture::parse(""), HotkeyGesture::DEFAULT);
        assert_eq!(HotkeyGesture::parse("   "), HotkeyGesture::DEFAULT);
    }

    #[test]
    fn parses_strings_written_by_the_csharp_version() {
        let ctrl_shift = Modifiers::CONTROL | Modifiers::SHIFT;
        let cases = [
            ("PrintScreen", gesture(Modifiers::NONE, vk::SNAPSHOT)),
            ("Ctrl+Shift+S", gesture(ctrl_shift, 0x53)),
            (
                "Ctrl+Alt+5",
                gesture(Modifiers::CONTROL | Modifiers::ALT, 0x35),
            ),
            (
                "Win+Shift+F12",
                gesture(Modifiers::WIN | Modifiers::SHIFT, 0x7B),
            ),
            ("Alt+Snapshot", gesture(Modifiers::ALT, vk::SNAPSHOT)),
            ("Ctrl+Return", gesture(Modifiers::CONTROL, vk::RETURN)),
            ("Ctrl+Enter", gesture(Modifiers::CONTROL, vk::RETURN)),
            ("Ctrl+OemTilde", gesture(Modifiers::CONTROL, 0xC0)),
            ("Ctrl+Oem3", gesture(Modifiers::CONTROL, 0xC0)),
            ("Ctrl+NumPad7", gesture(Modifiers::CONTROL, 0x67)),
            ("Ctrl+Next", gesture(Modifiers::CONTROL, 0x22)),
            ("Pause", gesture(Modifiers::NONE, 0x13)),
            ("Scroll", gesture(Modifiers::NONE, 0x91)),
            ("Ctrl+D7", gesture(Modifiers::CONTROL, 0x37)),
            (" ctrl + shift + s ", gesture(ctrl_shift, 0x53)),
            ("control+prtsc", gesture(Modifiers::CONTROL, vk::SNAPSHOT)),
        ];
        for (text, expected) in cases {
            assert_eq!(HotkeyGesture::parse(text), expected, "{text}");
        }
    }

    #[test]
    fn unusable_text_gives_the_default() {
        for text in [
            "Ctrl+Shift",
            "Ctrl+LeftCtrl",
            "Banana",
            "Ctrl+F25",
            "Ctrl+NumPad12",
            "+",
        ] {
            assert_eq!(HotkeyGesture::parse(text), HotkeyGesture::DEFAULT, "{text}");
        }
    }

    #[test]
    fn formats_like_the_csharp_version() {
        let all = Modifiers::CONTROL | Modifiers::SHIFT | Modifiers::ALT | Modifiers::WIN;
        assert_eq!(gesture(all, 0x53).to_string(), "Ctrl+Shift+Alt+Win+S");
        assert_eq!(gesture(Modifiers::CONTROL, 0x35).to_string(), "Ctrl+5");
        assert_eq!(gesture(Modifiers::NONE, 0x70).to_string(), "F1");
        assert_eq!(
            gesture(Modifiers::SHIFT, vk::SNAPSHOT).to_string(),
            "Shift+PrintScreen"
        );
    }

    #[test]
    fn every_named_key_round_trips() {
        let keys =
            (0u16..=0xFF).filter(|&k| key_name(k).is_some() && !HotkeyGesture::is_modifier_key(k));
        for key in keys {
            let g = gesture(Modifiers::CONTROL | Modifiers::ALT, key);
            assert_eq!(HotkeyGesture::parse(&g.to_string()), g, "{g}");
        }
    }

    #[test]
    fn recorder_rules() {
        // Plain letters, digits and space would fire while typing.
        assert_eq!(HotkeyGesture::from_recorded(Modifiers::NONE, 0x41), None);
        assert_eq!(HotkeyGesture::from_recorded(Modifiers::SHIFT, 0x31), None);
        assert_eq!(
            HotkeyGesture::from_recorded(Modifiers::NONE, vk::SPACE),
            None
        );
        assert_eq!(
            HotkeyGesture::from_recorded(Modifiers::CONTROL, 0x41),
            Some(gesture(Modifiers::CONTROL, 0x41))
        );
        // Function keys and PrintScreen work on their own.
        assert!(HotkeyGesture::from_recorded(Modifiers::NONE, 0x7B).is_some());
        assert!(HotkeyGesture::from_recorded(Modifiers::NONE, vk::SNAPSHOT).is_some());
        // Lone modifiers are ignored.
        assert_eq!(
            HotkeyGesture::from_recorded(Modifiers::CONTROL, vk::CONTROL),
            None
        );
        assert_eq!(HotkeyGesture::from_recorded(Modifiers::NONE, 0xA3), None);

        assert!(HotkeyGesture::is_dialog_navigation(
            Modifiers::NONE,
            vk::TAB
        ));
        assert!(!HotkeyGesture::is_dialog_navigation(
            Modifiers::CONTROL,
            vk::TAB
        ));
    }

    #[test]
    fn native_modifier_bits() {
        assert_eq!(
            (Modifiers::ALT | Modifiers::CONTROL | Modifiers::SHIFT | Modifiers::WIN).native(),
            0xF
        );
    }
}
