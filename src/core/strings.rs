//! All user-facing text. English by default; Dutch when the Windows display language is Dutch.
//! Same texts as `Localization/Strings.cs` in the C# version.

use std::sync::OnceLock;

use crate::APP_NAME;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Dutch,
}

static LANGUAGE: OnceLock<Language> = OnceLock::new();

/// Sets the language once at startup. Later calls are ignored.
pub fn init(language: Language) {
    let _ = LANGUAGE.set(language);
}

/// The texts in the language chosen at startup (English if none was set).
pub fn get() -> Strings {
    Strings::new(*LANGUAGE.get().unwrap_or(&Language::English))
}

#[derive(Debug, Clone, Copy)]
pub struct Strings {
    dutch: bool,
}

impl Strings {
    pub const fn new(language: Language) -> Self {
        Self {
            dutch: matches!(language, Language::Dutch),
        }
    }

    const fn t(&self, en: &'static str, nl: &'static str) -> &'static str {
        if self.dutch { nl } else { en }
    }

    // ---- Tray & notifications ------------------------------------------------------------

    pub fn tray_capture(&self) -> &'static str {
        self.t("Take screenshot", "Screenshot maken")
    }

    pub fn tray_settings(&self) -> &'static str {
        self.t("Settings…", "Instellingen…")
    }

    pub fn tray_exit(&self) -> &'static str {
        self.t("Exit", "Afsluiten")
    }

    pub fn running_title(&self) -> String {
        if self.dutch {
            format!("{APP_NAME} draait")
        } else {
            format!("{APP_NAME} is running")
        }
    }

    pub fn running_text(&self, hotkey: &str) -> String {
        if self.dutch {
            format!("Druk op {hotkey} om een screenshot te maken.")
        } else {
            format!("Press {hotkey} to take a screenshot.")
        }
    }

    pub fn hotkey_unavailable_title(&self) -> &'static str {
        self.t("Shortcut not available", "Sneltoets niet beschikbaar")
    }

    pub fn print_screen_in_use(&self) -> &'static str {
        self.t(
            "PrintScreen is already in use. Turn off \"Use the Print screen key to open Snipping Tool\" in \
             Windows Settings > Accessibility > Keyboard, or choose a different shortcut.",
            "PrintScreen is al in gebruik. Zet in Windows-instellingen > Toegankelijkheid > Toetsenbord \
             \"Knipprogramma openen met PrtSc\" uit, of kies een andere sneltoets.",
        )
    }

    pub fn hotkey_in_use(&self, hotkey: &str) -> String {
        if self.dutch {
            format!(
                "{hotkey} is al in gebruik door een ander programma. Kies een andere sneltoets."
            )
        } else {
            format!("{hotkey} is already in use by another program. Choose a different shortcut.")
        }
    }

    pub fn unexpected_error(&self, message: &str, log_path: &str) -> String {
        if self.dutch {
            format!(
                "Er ging iets mis, maar {APP_NAME} blijft draaien.\n\n{message}\n\nDetails staan in:\n{log_path}"
            )
        } else {
            format!(
                "Something went wrong, but {APP_NAME} keeps running.\n\n{message}\n\nDetails were written to:\n{log_path}"
            )
        }
    }

    pub fn startup_failed(&self) -> &'static str {
        self.t(
            "Could not set 'Start with Windows'.",
            "Kon 'start met Windows' niet instellen.",
        )
    }

    // ---- Capture overlay -----------------------------------------------------------------

    pub fn capture_hint(&self) -> &'static str {
        self.t(
            "Drag to select an area  ·  Click for this screen  ·  Enter for all screens  ·  Esc to cancel",
            "Sleep om een gebied te kiezen  ·  Klik voor dit scherm  ·  Enter voor alle schermen  ·  Esc annuleert",
        )
    }

    // ---- Editor --------------------------------------------------------------------------

    pub fn tool_arrow(&self) -> &'static str {
        self.t("Arrow (A)", "Pijl (A)")
    }

    pub fn tool_pixelate(&self) -> &'static str {
        self.t("Pixelate (P)", "Pixeleren (P)")
    }

    pub fn tool_number(&self) -> &'static str {
        self.t("Numbers (N)", "Nummers (N)")
    }

    /// Tooltips of the five colour swatches, in toolbar order.
    pub fn colors(&self) -> [&'static str; 5] {
        [
            self.t("Red (1)", "Rood (1)"),
            self.t("Orange (2)", "Oranje (2)"),
            self.t("Yellow (3)", "Geel (3)"),
            self.t("Green (4)", "Groen (4)"),
            self.t("Blue (5)", "Blauw (5)"),
        ]
    }

    pub fn undo(&self) -> &'static str {
        self.t("Undo (Ctrl+Z)", "Ongedaan maken (Ctrl+Z)")
    }

    pub fn redo(&self) -> &'static str {
        self.t("Redo (Ctrl+Y)", "Opnieuw (Ctrl+Y)")
    }

    pub fn copy(&self) -> &'static str {
        self.t("Copy (Ctrl+C)", "Kopiëren (Ctrl+C)")
    }

    pub fn save(&self) -> &'static str {
        self.t("Save (Ctrl+S)", "Opslaan (Ctrl+S)")
    }

    pub fn editor_hint(&self) -> &'static str {
        self.t(
            "A arrow  ·  P pixelate  ·  N numbers  ·  Del delete  ·  Ctrl+C copy  ·  Ctrl+S save  ·  Esc close",
            "A pijl  ·  P pixeleren  ·  N nummers  ·  Del verwijderen  ·  Ctrl+C kopiëren  ·  Ctrl+S opslaan  ·  Esc sluiten",
        )
    }

    pub fn copied(&self) -> &'static str {
        self.t("Copied to clipboard", "Gekopieerd naar klembord")
    }

    pub fn clipboard_busy(&self) -> &'static str {
        self.t(
            "Clipboard is busy, please try again",
            "Klembord is bezet, probeer opnieuw",
        )
    }

    /// Name of the PNG file type in the save dialog.
    pub fn png_file_type(&self) -> &'static str {
        self.t("PNG image", "PNG-afbeelding")
    }

    pub fn saved(&self, file_name: &str) -> String {
        if self.dutch {
            format!("Opgeslagen: {file_name}")
        } else {
            format!("Saved: {file_name}")
        }
    }

    pub fn save_failed(&self, error: &str) -> String {
        if self.dutch {
            format!("Opslaan mislukt:\n{error}")
        } else {
            format!("Saving failed:\n{error}")
        }
    }

    // ---- Settings ------------------------------------------------------------------------

    pub fn settings_title(&self) -> String {
        if self.dutch {
            format!("{APP_NAME} – Instellingen")
        } else {
            format!("{APP_NAME} – Settings")
        }
    }

    pub fn shortcut(&self) -> &'static str {
        self.t("Shortcut", "Sneltoets")
    }

    pub fn shortcut_help(&self) -> &'static str {
        self.t(
            "Click the field and press the key combination you want. If PrintScreen doesn't work, turn off \
             \"Use the Print screen key to open Snipping Tool\" under Accessibility › Keyboard in Windows.",
            "Klik in het veld en druk de gewenste toetscombinatie. Werkt PrintScreen niet, zet dan in Windows bij \
             Toegankelijkheid › Toetsenbord \"Knipprogramma openen met PrtSc\" uit.",
        )
    }

    pub fn press_shortcut(&self) -> &'static str {
        self.t("Press a key combination…", "Druk op een toetscombinatie…")
    }

    pub fn save_folder(&self) -> &'static str {
        self.t("Save folder", "Opslagmap")
    }

    pub fn browse(&self) -> &'static str {
        self.t("Browse…", "Bladeren…")
    }

    pub fn choose_save_folder(&self) -> &'static str {
        self.t("Choose save folder", "Kies opslagmap")
    }

    pub fn copy_on_capture(&self) -> &'static str {
        self.t(
            "Copy screenshot to the clipboard right away",
            "Screenshot direct naar het klembord kopiëren",
        )
    }

    pub fn start_with_windows(&self) -> &'static str {
        self.t("Start with Windows", "Starten met Windows")
    }

    pub fn cancel(&self) -> &'static str {
        self.t("Cancel", "Annuleren")
    }

    pub fn save_button(&self) -> &'static str {
        self.t("Save", "Opslaan")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_language() {
        let en = Strings::new(Language::English);
        let nl = Strings::new(Language::Dutch);
        assert_eq!(en.tray_exit(), "Exit");
        assert_eq!(nl.tray_exit(), "Afsluiten");
        assert_eq!(
            nl.running_text("PrintScreen"),
            "Druk op PrintScreen om een screenshot te maken."
        );
        assert_eq!(en.running_title(), "Snapr (Rust) is running");
    }
}
