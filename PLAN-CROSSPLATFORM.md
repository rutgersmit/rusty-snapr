# Plan: Snapr cross-platform (Windows, macOS, Ubuntu/Debian)

## Context

`rusty-snapr` is nu een Windows-only tray-app (Rust, ±9100 regels). De helft daarvan (`src/core`, ±5000 regels: instellingen, sneltoetsen, teksten EN/NL, foutlog, editorlogica, pixelator, pijlen/markeerstift/nummers, undo/redo, toolbar-layout en -tekenen met tiny-skia, golden-image test) bevat al **geen** Windows-code. De andere helft (`src/platform`, `src/ui`, `src/app.rs`) is pure Win32 (`windows`-crate, GDI, wndprocs, Win32-controls in het instellingenvenster).

Doel: dezelfde app opnieuw opbouwen in een **nieuwe repo**, met **één gedeelde stack** voor Windows, macOS en Linux (X11 **en** Wayland), met hetzelfde gedrag en hetzelfde `settings.json`-formaat. De platform-specifieke code moet zo klein mogelijk blijven; alles wat getekend wordt, tekenen we zelf met tiny-skia (dat principe werkt nu al voor de editor).

Gekozen (met gebruiker afgestemd): gedeelde stack i.p.v. Win32 + losse backends · X11 + Wayland · nieuwe repo.

---

## 1. Architectuur

```
snapr/                       (nieuwe repo, cargo workspace)
├─ crates/core       ← overgenomen uit rusty-snapr/src/core (platform-neutraal, headless testbaar)
├─ crates/ui         ← al het tekenwerk: overlay, editor-chrome, instellingen, tooltips, tekst
├─ crates/platform   ← dunne laag: capture, klembord, tray, sneltoets, autostart, paden, IPC
└─ app/              ← bin: winit-eventloop, vensterbeheer, koppelt alles aan elkaar
```

Elk venster = `winit::Window` + `softbuffer::Surface`; de inhoud is een tiny-skia `Pixmap` die naar softbuffer wordt gekopieerd. Dezelfde render-functies leveren de preview én de export (zoals nu).

### Crates

| Taak | Crate | Opmerking |
|---|---|---|
| Vensters, input, DPI, cursors | `winit` 0.30 | ook `with_activation_policy(Accessory)` op macOS (geen Dock-icoon) |
| Pixels naar scherm | `softbuffer` | CPU-blit, geen GPU nodig |
| Tekenen | `tiny-skia`, `ab_glyph` | al in gebruik |
| Tray-icoon + menu | `tray-icon` + `muda` | Linux: AppIndicator via GTK-thread |
| Globale sneltoets | `global-hotkey` (Win/macOS/X11) + `ashpd` GlobalShortcuts-portal (Wayland) | |
| Screenshot | `xcap` (Win/macOS/X11) + `ashpd` Screenshot-portal (Wayland) | |
| Klembord | `arboard` (feature `wayland-data-control`) | + PNG-formaat waar mogelijk |
| Dialogen (opslaan, map kiezen) | `rfd` (Linux: feature `xdg-portal`) | |
| Meldingen (nu: ballonnen) | `notify-rust` | |
| Starten bij inloggen | `auto-launch` | Run-key / LaunchAgent / `~/.config/autostart/*.desktop` |
| Single instance + IPC | `interprocess` (local socket) | tweede start stuurt `capture`/`settings` door |
| Paden, taal | `directories`, `sys-locale` | |
| Rest | `png`, `serde`, `serde_json`, `chrono`, `fastrand` | ongewijzigd |

---

## 2. Wat er uit rusty-snapr hergebruikt wordt

Vrijwel ongewijzigd overnemen naar `crates/core`:
- `src/core/settings.rs`, `strings.rs`, `error_log.rs`, `geometry.rs`, `capture.rs`, `draw.rs`
- `src/core/editor/*` (state, history, arrow, highlight, number, pixelate/pixelator, bitmap, png, render, chrome, annotation)
- `tests/golden.rs` + `tests/golden/scene.png`, `examples/timings.rs`
- `assets/` (iconen, `snapr-digits-bold.ttf` + OFL)

Aanpassingen in core:
1. **`hotkey.rs`**: nu gebaseerd op Win32 VK-codes en `MOD_*`-bits. Vervangen door een eigen `Key`-enum + `Modifiers {ctrl, shift, alt, super}`. Het **tekstformaat blijft identiek** ("Ctrl+Shift+S", "PrintScreen", WPF-namen + aliassen), zodat bestaande `settings.json` blijft werken. Conversie naar `global_hotkey::HotKey` en vanuit `winit::keyboard::KeyCode` komt in `platform`/`app`. Bestaande hotkey-tests blijven gelden.
2. **`editor/state.rs::command_for_key(key: u16, ctrl, shift)`**: krijgt de nieuwe `Key` en een "primaire modifier" (Ctrl op Win/Linux, Cmd op macOS) → `Cmd+Z`, `Cmd+Shift+Z`/`Cmd+Y`, `Cmd+C`, `Cmd+S` op Mac.
3. **Lettertype nummers**: Segoe UI is er alleen op Windows. Overal de meegeleverde Carlito-cijfers gebruiken (identieke export op alle OS'en; golden test hoeft dan niet per OS). Optioneel Segoe op Windows behouden — aanbeveling: niet, eenvoudiger en consistent.
4. **Constantes** `APP_NAME`/`DATA_FOLDER_NAME`: naam "Snapr", map per OS via `directories` (`%AppData%\Snapr`, `~/Library/Application Support/Snapr`, `~/.config/snapr`).

Opnieuw te schrijven (Win32 → gedeelde stack), met de huidige bestanden als functionele referentie:
- `src/app.rs` → `app/src/main.rs` (eventloop, tray-menu, sneltoets, open-editor-flow, afsluiten met open editors)
- `src/ui/overlay.rs` → overlay (selectie, klik = scherm, Enter = alle schermen, Esc, maatlabel, hint)
- `src/ui/editor.rs` → editorvenster (inputmapping naar `EditorState`, cursors, tooltips, statusbalk)
- `src/ui/settings.rs` (Win32-controls) → **zelf getekend** instellingenvenster: sneltoets opnemen, opslagmap + *Bladeren…*, "direct kopiëren", "starten bij inloggen", OK/Annuleren
- `src/platform/gdi.rs` (GDI-tekst) → tekstrendering met `ab_glyph` + meegeleverd UI-lettertype (bv. Inter of Noto Sans, OFL) in `crates/ui`
- `src/platform/tooltip.rs` → zelf getekende tooltips

---

## 3. Platformlaag (`crates/platform`)

Eén module per functie, met `cfg` alleen waar de crates verschillen:

```rust
pub trait Capturer { fn monitors(&self) -> Vec<MonitorInfo>; fn capture_all(&self) -> Result<Vec<(MonitorInfo, Bitmap)>>; }
pub trait HotkeyService { fn register(&mut self, hk: &Hotkey) -> Result<(), HotkeyError>; /* events via EventLoopProxy */ }
```

- **Capture**: `xcap` per monitor op fysieke pixels; op Wayland één aanroep naar de Screenshot-portal (niet-interactief), daarna opknippen naar monitoren aan de hand van winit's monitorposities/schaal.
- **Sneltoets**: `global-hotkey` waar het kan; op Wayland de GlobalShortcuts-portal (GNOME ≥ 48, KDE). **Fallback overal**: `snapr --capture` (via IPC naar de draaiende instantie), zodat je op oudere GNOME (Ubuntu 24.04) een systeemsneltoets kunt instellen. Instellingenvenster toont dan uitleg i.p.v. het opnameveld.
- **Klembord**: `arboard` bitmap; PNG-formaat extra waar arboard/OS het toelaat.
- **Tray**: `tray-icon`; op Linux draait GTK op een eigen thread. Geen tray beschikbaar (GNOME zonder AppIndicator-extensie, Debian standaard) → app blijft werken, instellingen via `snapr --settings` / `.desktop`-actie.
- **Autostart, paden, taal, single instance**: zie crates hierboven.

---

## 4. Overlay-ontwerp (het lastigste stuk)

Huidig: één venster over het hele virtuele bureaublad. Dat kan niet op macOS en Wayland. Nieuw:
- **Eerst capture, dan per monitor één randloos fullscreen venster** (`Fullscreen::Borderless(Some(monitor))`) dat zijn deel van de bevroren screenshot toont.
- Selectiestaat leeft centraal in **globale fysieke coördinaten** (`core::capture` + `geometry`); elk venster tekent alleen zijn uitsnede. Slepen over monitoren heen: het venster met de muisgrab blijft events krijgen; posities worden naar globaal omgerekend. Op Wayland (geen globale posities) is selectie beperkt tot één monitor; Enter = alle schermen blijft werken.
- macOS: vensterniveau boven menubalk/Dock zetten (via `objc2` op `NSWindow.level`, enige plek met wat ObjC-code); Retina = schaal 2, editor toont 1:1 fysieke pixels zoals nu.

---

## 5. Fases

| Fase | Inhoud | Klaar als |
|---|---|---|
| 0 | Nieuwe repo, workspace, CI-matrix (windows-latest, macos-latest, ubuntu-latest), fmt/clippy/test | lege app start op alle drie |
| 1 | Core overnemen + hotkey-abstractie + `command_for_key` + Carlito overal | alle bestaande unit- en golden-tests groen op 3 OS'en |
| 2 | `crates/ui`: tekst (ab_glyph), widgets, tooltips; headless golden-tests voor overlay/editor/instellingen | renders identiek op 3 OS'en |
| 3 | App-skelet: winit-loop, tray (Vastleggen / Instellingen / Afsluiten), single instance + `--capture`/`--settings`, foutlog, sneltoets (Win/macOS/X11) | sneltoets en tray werken |
| 4 | Capture + overlay + klembord (Win/macOS/X11) | selectie op klembord |
| 5 | Editor-venster: alle tools, selecteren/verslepen, undo/redo, kopiëren, opslaan (rfd), sluiten = kopiëren | functioneel gelijk aan nu |
| 6 | Instellingenvenster + autostart + meldingen + taal EN/NL | gelijk aan nu |
| 7 | Wayland: Screenshot- en GlobalShortcuts-portal, fallbacks, tray-fallback | werkt op Ubuntu 24.04 + 25.04/26.04 (GNOME) en KDE |
| 8 | macOS-afwerking: Screen Recording-toestemming detecteren en uitleggen (`CGPreflightScreenCaptureAccess`), `LSUIElement`, standaard sneltoets | werkt na verse installatie |
| 9 | Packaging + release-workflow (zie §6), README/TESTING/VERSCHILLEN bijwerken | release met 3 downloads |

Standaard sneltoets: `PrintScreen` op Windows/Linux; op macOS (geen PrintScreen-toets) `Shift+Win+2` (= ⇧⌘2; ⇧⌘3/4/5 zijn van het systeem).

---

## 6. Packaging & releases

- **Windows**: `Snapr.exe` (icoon/manifest/versie via `embed-resource`, PerMonitorV2, `crt-static`) — zoals nu; optioneel later MSI.
- **macOS**: `Snapr.app` (universal: `lipo` van x86_64 + aarch64) in een `.dmg`. Codesign + notarisatie vereist een Apple Developer-account (€99/jr); zonder dat werkt het, maar moet de gebruiker de app via *Rechtsklik › Open* toestaan. Toestemming voor schermopname is altijd nodig.
- **Linux**: `.deb` via `cargo-deb` (deps: `libgtk-3-0`, `libayatana-appindicator3-1`, `libxdo3`), plus `.desktop`-bestand met acties *Vastleggen*/*Instellingen*. Optioneel AppImage.
- `release.yml` bouwt alle drie op tag (`0.2`/`v0.2.0`) en hangt ze aan de release.

---

## 7. Risico's

- **Wayland/GNOME**: GlobalShortcuts-portal pas vanaf GNOME 48; screenshot-portal kan de eerste keer een toestemmingsdialoog tonen; fractionele schaal bij opknippen van de portal-screenshot. → `--capture`-fallback en per-monitor-overlay.
- **Tray op GNOME** zonder AppIndicator-extensie: onzichtbaar. → app werkt ook zonder tray.
- **macOS**: overlay boven menubalk vereist een stukje `objc2`; TCC-toestemming verloopt na her-signeren.
- **Grote screenshots** (bijv. 3× 4K): softbuffer-blit + verkleinde weergave moeten snel genoeg zijn; `examples/timings.rs` meenemen en per OS meten (vervanging voor GDI HALFTONE: eigen box-filter in core).

---

## 8. Verificatie

- `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test --workspace` in de CI-matrix op Windows, macOS en Ubuntu.
- Golden-image tests (bestaande `tests/golden.rs` + nieuwe voor overlay/editor-chrome/instellingen) moeten op alle drie identiek zijn.
- Compatibiliteitstest: bestaande `settings.json` van de C#- en Rust-versie inlezen en weer wegschrijven = byte-gelijk.
- Handmatige checklist (uitbreiding van huidige `TESTING.md`) per platform: Windows 11 (multi-monitor, verschillende DPI), macOS (Retina + extern scherm, eerste-start-toestemming), Ubuntu 24.04 GNOME Wayland, Ubuntu Xorg-sessie, Debian 12/13 GNOME en KDE: sneltoets, overlay (sleep/klik/Enter/Esc), alle editortools, kopiëren/plakken in browser en chat-app, opslaan, instellingen, autostart na herstart.
- Optioneel: end-to-end onder Xvfb in CI (`--capture` via IPC, klembord uitlezen), naar het voorbeeld van `tools/wine-e2e.sh`.
