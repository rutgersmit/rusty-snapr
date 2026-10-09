# Plan: Snapr in Rust

Doel: een Rust-versie van [Snapr](https://github.com/rutgersmit/snapr) met **precies dezelfde functies en hetzelfde gedrag**, als drop-in vervanger: dezelfde sneltoets, hetzelfde instellingenbestand, dezelfde opstartregistratie. Gebruikers merken alleen dat de exe veel kleiner is en sneller start.

Dit plan is gebaseerd op een volledige doorlezing van de huidige C#/WPF-code (`src/Snapr`, ±2000 regels).

---

## 1. Waarom (en wat het oplevert)

| | Snapr (.NET 10 / WPF) | rusty-snapr (verwacht) |
|---|---|---|
| Exe-grootte | tientallen MB (self-contained single-file) | ~1–3 MB |
| Opstarttijd | ~0,5–1 s (runtime uitpakken + JIT) | vrijwel direct |
| Geheugen in de tray | ~50–80 MB | ~5–10 MB |
| Runtime nodig | nee (ingepakt) | nee (statisch gelinkte CRT) |

De functionaliteit blijft gelijk; de winst zit in grootte, snelheid en "voelt native".

---

## 2. Belangrijkste keuze: UI-aanpak

**Aanbeveling: rechtstreeks Win32 via de `windows`-crate, en alles zelf tekenen met `tiny-skia`.**

Redenen:

- Het lastigste deel van Snapr is niet de toolbar maar de **overlay**: een randloos, topmost venster over het hele virtuele bureaublad, op fysieke pixels, met monitoren met verschillende DPI. De C#-code moet daarvoor al naar Win32 (`SetWindowPos`, `HWND_TOPMOST`) omdat WPF het niet goed kan. GUI-frameworks (egui, iced, Slint) maken dit eerder moeilijker dan makkelijker.
- Met de manifest-instelling `PerMonitorV2` werkt Win32 gewoon in fysieke pixels. Daarmee verdwijnt de hele DIP↔pixel-omrekening uit de C#-code (`PixelsPerDip`, `ToPixels`, `MonitorRectInDips`).
- Snapr heeft weinig "gewone" UI: één toolbar met icoonknoppen, vijf kleurbolletjes, een statusbalk en een klein instellingenvenster. Alle iconen zijn al SVG-paden in `EditorWindow.xaml`; die zijn één-op-één als `tiny-skia`-paden over te nemen.
- Het ontwerpprincipe van Snapr ("dezelfde `Render` voor het scherm en voor de export", zie `IAnnotation`) past precies bij `tiny-skia`: we tekenen in een pixelbuffer op beeldresolutie, en dat buffer is zowel de preview als de PNG of het klembordplaatje.
- `tiny-skia` is pure Rust, dus de render- en editorlogica draait en test ook op Linux in CI.

Bekeken en afgevallen:
- **egui/eframe**: snel een toolbar, maar topmost multi-monitor-overlay, tray en globale sneltoets moeten toch via Win32. Dan heb je twee UI-werelden in één event loop.
- **Tauri**: WebView2 is precies het gewicht dat we kwijt willen.
- **Slint/iced**: zelfde overlay-probleem als egui, en meer framework dan we nodig hebben.

---

## 3. Crates

| Crate | Waarvoor |
|---|---|
| `windows` | Win32: vensters, GDI, tray (`Shell_NotifyIconW`), sneltoets, klembord, register, DWM, `IFileSaveDialog`/`IFileOpenDialog`, mutex |
| `tiny-skia` | Annotaties, iconen en editor-chrome tekenen (paden, quad-béziers, anti-aliasing) |
| `ab_glyph` | Cijfers in de nummermarkers rasteren (die moeten in de geëxporteerde pixels terechtkomen) |
| `png` | PNG opslaan en het "PNG"-klembordformaat |
| `serde` + `serde_json` | `settings.json` |
| `fastrand` | Seed per pixelregio (de ruis zelf is deterministisch, zie §5) |
| `chrono` | Tijdstempels in de bestandsnaam en het log |
| `embed-resource` (build) | Icoon, manifest en versie-info in de exe |

Release-profiel: `opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`, **`panic = "unwind"` laten staan** (nodig voor §6). In `.cargo/config.toml`: `target-feature=+crt-static`, zodat er geen VC++-redistributable nodig is.

---

## 4. Projectstructuur

De code is opgesplitst in een **platformonafhankelijke kern** (testbaar op Linux) en een dunne **Win32-laag**.

```
rusty-snapr/
├── Cargo.toml
├── build.rs                  # icoon + manifest + versie-resource
├── assets/                   # snapr.ico, snapr.svg, app.manifest (overgenomen), snapr.rc
├── .github/workflows/        # build.yml, release.yml
└── src/
    ├── main.rs               # entry: single instance, App aanmaken, message loop
    ├── app.rs                # App-state: settings, tray, capture-flow, foutafhandeling
    │
    ├── core/                 # geen Win32: alles hier draait onder `cargo test` op Linux
    │   ├── geometry.rs       # Point, Vec2, Rect, IRect (vervangt WPF Point/Vector/Rect/Int32Rect)
    │   ├── settings.rs       # AppSettings + laden/opslaan (JSON-compatibel)
    │   ├── hotkey.rs         # HotkeyGesture: parse/to_string, VK ↔ naam
    │   ├── strings.rs        # alle teksten, EN/NL
    │   ├── error_log.rs      # snapr.log met rotatie naar .old
    │   └── editor/
    │       ├── annotation.rs # enum Annotation { Arrow, Number, Pixelate } + id
    │       ├── arrow.rs      # geometrie, hit-test, bounds, render
    │       ├── number.rs
    │       ├── pixelate.rs
    │       ├── pixelator.rs  # bit-voor-bit port van Pixelator.cs
    │       ├── history.rs    # undo/redo
    │       ├── state.rs      # EditorState: tool, kleur, selectie, sleep-state-machine
    │       └── render.rs     # flatten(image, annotations) -> Pixmap; selectie-chrome
    │
    ├── platform/             # dunne, zo klein mogelijke unsafe-laag
    │   ├── window.rs         # venster-registratie, wndproc met catch_unwind, DPI-helpers
    │   ├── tray.rs           # NotifyIcon-equivalent incl. balloons en contextmenu
    │   ├── hotkey.rs         # RegisterHotKey op een message-only venster
    │   ├── capture.rs        # BitBlt van het virtuele scherm naar een DIB
    │   ├── clipboard.rs      # CF_DIB + "PNG", met retry
    │   ├── startup.rs        # HKCU\...\Run
    │   ├── dialogs.rs        # opslaan-dialoog, map kiezen, MessageBox
    │   └── locale.rs         # is de UI-taal Nederlands?
    │
    └── ui/
        ├── overlay.rs        # CaptureOverlayWindow
        ├── editor_window.rs  # EditorWindow: koppelt EditorState aan een venster
        ├── settings_window.rs
        ├── widgets.rs        # zelfgetekende knop, toggle, kleurbolletje, checkbox
        └── icons.rs          # de SVG-paden uit EditorWindow.xaml
```

---

## 5. Vertaling per onderdeel (pariteitslijst)

Elke regel is een functie die nu werkt en straks precies zo moet werken.

### App, tray en sneltoets (`App.xaml.cs`, `HotkeyService.cs`)

| Gedrag nu | Aanpak in Rust |
|---|---|
| Single instance via mutex `Local\Snapr.SingleInstance` | `CreateMutexW` met **dezelfde naam** + `ERROR_ALREADY_EXISTS`. Daardoor kunnen de oude en de nieuwe versie ook niet tegelijk draaien. |
| Tray-icoon; linksklik = screenshot; menu: Screenshot maken / Instellingen… / Afsluiten | `Shell_NotifyIconW` met een callback-bericht. Rechtsklik: `SetForegroundWindow` → `TrackPopupMenu` → `PostMessage(WM_NULL)` (anders sluit het menu niet). |
| Balloon "Snapr draait – druk op X" en waarschuwingen | `NIF_INFO` met `NIIF_INFO`/`NIIF_WARNING` |
| Icoon opnieuw na herstart van Explorer (WinForms deed dat automatisch) | Luisteren naar `RegisterWindowMessageW("TaskbarCreated")` en het icoon opnieuw toevoegen |
| Globale sneltoets, `MOD_NOREPEAT`, verborgen message-only venster | Zelfde: `RegisterHotKey` op een `HWND_MESSAGE`-venster, `WM_HOTKEY` |
| Melding als de sneltoets bezet is (aparte tekst voor PrintScreen) | Zelfde logica in `App::register_hotkey` |
| Sneltoets pauzeren zolang Instellingen open is | `suspend_hotkey()` / `register_hotkey()` bij openen/sluiten |
| Hooguit één overlay en één instellingenvenster tegelijk | `Option<HWND>` in de App-state |

### Schermafbeelding en overlay (`ScreenCapture.cs`, `CaptureOverlayWindow`)

| Gedrag nu | Aanpak in Rust |
|---|---|
| Hele virtuele bureaublad vastleggen in fysieke pixels | `GetSystemMetrics(SM_*VIRTUALSCREEN)` + `BitBlt(SRCCOPY \| CAPTUREBLT)` naar een 32-bit DIB-section |
| Overlay dekt alle schermen, ook bij gemengde DPI, en blijft dat na een DPI-wissel | `WS_POPUP` + `WS_EX_TOPMOST \| WS_EX_TOOLWINDOW`, `SetWindowPos` naar de virtuele schermgrenzen. Bij `WM_DPICHANGED` de voorgestelde rect **negeren** (dat corrigeert de C#-code nu achteraf). |
| Gedimd beeld met een "gat" voor de selectie, blauwe rand, kruisdraad, maatlabel `W × H`, hint bovenaan de monitor onder de muis | Tekenen met GDI op twee vooraf gemaakte memory-DC's (origineel en 50% gedimd): gedimd blitten, het origineel in het gat blitten, daarna rand, kruisdraad en tekst. Alleen de gewijzigde gebieden invalideren. Zo blijft het vloeiend, ook met meerdere 4K-schermen (`tiny-skia` op 16+ megapixel per muisbeweging is te traag). |
| Slepen = gebied; klik (< 4 px) = monitor onder de muis; Enter = alles; Esc of rechtsklik = annuleren; focus kwijt = annuleren (behalve tijdens slepen) | Zelfde state machine, `SetCapture`/`ReleaseCapture`, `WM_ACTIVATE` voor deactivatie, `MonitorFromPoint` + `GetMonitorInfoW` voor "dit scherm" |
| Bijsnijden en klemmen op de beeldranden | `IRect::clamp_to` in `core::geometry`, met unit-tests |
| Direct naar het klembord als `CopyOnCapture` aan staat | `clipboard::set_image` |

### Editor (`EditorWindow`, `Editor/*`)

| Gedrag nu | Aanpak in Rust |
|---|---|
| Beeld 1:1 in fysieke pixels, kleiner geschaald als het niet past (max. 92% van het werkgebied van de huidige monitor), min. 580×260, gecentreerd, donkere titelbalk | Venstergrootte zelf uitrekenen uit `GetMonitorInfoW(rcWork)` en `GetDpiForWindow`; opnieuw bij `WM_DPICHANGED` en `WM_MOVE`. `DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)`. |
| Toolbar: Pijl / Pixeleren / Nummers, 5 kleuren, Ongedaan maken / Opnieuw (uitgeschakeld als het niet kan), Kopiëren / Opslaan; statusbalk met afmetingen en hint/status | Zelfgetekend met `tiny-skia` (`ui/widgets.rs`), iconen uit `icons.rs`. Tooltips met de standaard Win32-tooltipcontrol (`TOOLTIPS_CLASS`). Kleuren en afmetingen uit `Theme/Styles.xaml` overnemen. |
| **Pijl**: taps, brede afgeronde kop, witte rand, zachte schaduw | Port van `ArrowAnnotation.BuildGeometry` (7 hoekpunten met elk een eigen afronding via quad-béziers) naar `tiny_skia::PathBuilder::quad_to`. Schaduw = hetzelfde pad met alfa 70, verschoven. |
| **Nummer**: afgerond vierkant, wordt breder bij 2+ cijfers, witte of donkere tekst afhankelijk van de kleur, nummer = hoogste + 1 | `ab_glyph` met **Segoe UI Bold, bij het opstarten geladen uit `C:\Windows\Fonts\segoeuib.ttf`** (Segoe mag niet mee in de exe), met als reserve een ingebouwd OFL-lettertype (bijv. Inter Bold). |
| **Pixeleren**: vast blokraster over het hele beeld, gemiddelde kleur per blok, deterministische ruis (±18) per blok, stabiel tijdens het slepen | **Bit-voor-bit port** van `Pixelator.cs`, inclusief de hashconstanten (`0x9E3779B1`, `0x85EBCA77`, …) met `wrapping_mul`. Een test vergelijkt de uitvoer met referentiewaarden uit de C#-versie. |
| Selecteren door te klikken (bovenste eerst), verslepen binnen de beeldranden, pijluiteinden verslepen, Delete/Backspace, Esc deselecteert, cursors Hand/SizeAll/Cross | `EditorState` in `core/editor/state.rs` als pure state machine: `mouse_down/move/up(p)` en `key(k)` geven terug wat er opnieuw getekend moet worden. Het venster vertaalt alleen muiscoördinaten (venster → beeldpixels) en zet de cursor. |
| Undo/redo voor toevoegen, verwijderen, verplaatsen/vervormen | `history.rs`. C# vergelijkt objectreferenties; in Rust krijgt elke annotatie een vast `id`. `Removed` bewaart index + annotatie, `Reshaped` bewaart id + handles voor en na. |
| Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z / Ctrl+C / Ctrl+S, A / P (of B) / N, 1–5, Esc sluit | `WM_KEYDOWN` + `GetKeyState(VK_CONTROL/VK_SHIFT)`. Zelfde tabel als `OnKeyDown`. |
| Kleur kiezen terwijl Pixeleren actief is → terug naar Pijl | Zit in `EditorState` |
| Opslaan: dialoog met "Snapr yyyy-MM-dd HH.mm.ss.png", start in `SaveFolder` (wordt aangemaakt), foutmelding bij mislukken | `IFileSaveDialog` met filter, standaardnaam en `SetFolder`, daarna `png`-encoder |
| Bij sluiten: is er sinds de laatste kopie iets veranderd en staat `CopyOnCapture` aan, dan het bewerkte beeld naar het klembord | `WM_CLOSE`-handler |
| Kopiëren met retry als het klembord bezet is (5× 50 ms) | `OpenClipboard` in een retry-lus. We zetten `CF_DIB` en ook het geregistreerde formaat "PNG" (zoals WPF ook meerdere formaten zet). |

**Renderpijplijn editor:** één `Pixmap` op beeldresolutie als cache voor "beeld + vastgelegde annotaties". Die wordt alleen opnieuw opgebouwd als de geschiedenis verandert. Per muisbeweging: cache kopiëren, de annotatie die nu gesleept wordt plus het selectiekader erop tekenen, en dat met `StretchDIBits` (`HALFTONE` bij verkleinen) naar het venster. `flatten()` voor export gebruikt dezelfde code zonder selectie, zoals nu.

### Instellingen (`SettingsWindow`, `SettingsService`, `StartupService`, `HotkeyGesture`)

| Gedrag nu | Aanpak in Rust |
|---|---|
| `%AppData%\Snapr\settings.json` met `Hotkey`, `SaveFolder`, `CopyOnCapture`, `StartWithWindows` | `#[serde(rename_all = "PascalCase", default)]`, dus **bestaande instellingen blijven werken**. Een kapot bestand → standaardwaarden, net als nu. |
| Sneltoetsformaat `"Ctrl+Shift+S"`, `"PrintScreen"`, cijfers als `"5"`, overige toetsen met WPF-namen (`F1`, `Insert`, `OemTilde`, `NumPad1`, …) | Tabel VK ↔ WPF-`Key`-naam in `core/hotkey.rs`, zodat oude strings goed worden ingelezen. Round-trip-tests. |
| Sneltoets opnemen: Tab/Esc/Enter zonder modifier blijven navigatie; letters/cijfers/spatie alleen met Ctrl/Alt/Win; PrintScreen alleen via key-up; Alt-combinaties komen als `WM_SYSKEYDOWN` binnen | Zelfde regels in de opnameknop |
| Map kiezen met Bladeren… | `IFileOpenDialog` met `FOS_PICKFOLDERS` |
| Starten met Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, waarde `Snapr` = `"pad\naar\exe"` | Zelfde sleutel en waardenaam. Bij een fout de bestaande "Kon niet instellen"-melding. |

Het instellingenvenster wordt ook zelfgetekend: label + sneltoetsveld (opnameknop), mappad + Bladeren…, twee checkboxes, Annuleren/Opslaan. **Eén kleine afwijking om voor te stellen:** het mappad wordt alleen-lezen en je kiest het via Bladeren…. Zo hebben we geen zelfgebouwd tekstinvoerveld nodig. Willen we vrije tekstinvoer houden, dan komt er een standaard Win32-`EDIT`-control (kost wat werk om die donker te krijgen).

### Taal, logging en fouten (`Strings.cs`, `ErrorLog.cs`, `App.HookUnhandledExceptions`)

| Gedrag nu | Aanpak in Rust |
|---|---|
| Engels, Nederlands als de Windows-weergavetaal Nederlands is | `GetUserDefaultUILanguage()`, primaire taal `LANG_DUTCH`. `strings.rs` met dezelfde `t(en, nl)`-functie en dezelfde teksten. |
| Log in `%AppData%\Snapr\snapr.log`, rotatie naar `.old` boven 512 KB, loggen faalt nooit | Één-op-één port |
| Onverwachte fout → loggen, één dialoog tegelijk, app blijft draaien | `std::panic::set_hook` logt. **Elke `wndproc` loopt binnen `catch_unwind`**: een panic die door een `extern "system"`-functie heen wil, breekt het proces af. Daarna een MessageBox met een guard tegen meerdere tegelijk. |

---

## 6. Rust-specifieke aandachtspunten

- **`unsafe` beperken tot `platform/`.** Elke Win32-aanroep zit in een kleine veilige wrapper. `ui/` en `core/` gebruiken geen `unsafe`.
- **State per venster:** een `Box<WindowState>` in `GWLP_USERDATA`, aangemaakt bij `WM_NCCREATE` en vrijgegeven bij `WM_NCDESTROY`. Herintreding (een `MessageBox` die tijdens een handler nieuwe berichten verwerkt) vangen we op met `RefCell::try_borrow_mut`, of door acties in de wachtrij te zetten en na de handler uit te voeren.
- **Panics over FFI-grenzen:** zie §5, Fouten. `panic = "unwind"` laten staan.
- **Single-threaded:** alles draait op de UI-thread, net als nu. Er is geen async nodig.
- **Annotaties als `enum` in plaats van traits:** `Arrow` en `Number` hebben handles (bewerkbaar), `Pixelate` niet. Een `match` maakt dat expliciet. De gerenderde pixelregio is een `Arc<Pixmap>`, zodat klonen voor undo goedkoop is.

---

## 7. Testen

- **`cargo test` (draait op Linux en Windows):**
  - `pixelator`: zelfde uitvoer als C# voor vaste seed en regio's, blokken aan de rand, stabiliteit tijdens slepen
  - `history`: toevoegen/verwijderen/vervormen, undo/redo, redo-stack leeg na een nieuwe actie, nummer komt terug na undo
  - `state`: klik selecteert bovenste annotatie, verslepen blijft binnen beeld, te korte pijl en lege pixelregio worden niet toegevoegd
  - `hotkey`: parsen en weer uitschrijven van alle oude formaten
  - `settings`: een JSON-bestand van de C#-versie inlezen, kapotte JSON → standaardwaarden
  - geometrie: hit-tests van pijl en nummer, bounds, klemmen
  - **golden images**: `flatten()` van een vaste scène vergelijken met een referentie-PNG (met kleine tolerantie)
- **Handmatige checklist op Windows** (in `TESTING.md`): één scherm, twee schermen met verschillende DPI (100% + 150%), scherm links van het hoofdscherm (negatieve coördinaten), PrintScreen bezet door Knipprogramma, Explorer herstarten, plakken in Paint/Word/browser/Slack, Nederlandse en Engelse Windows.

---

## 8. CI en releases

Op dezelfde manier als nu (`build.yml`, `release.yml`), maar met Rust:

- **Build** (`windows-latest`, bij elke push/PR): `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`, en `snapr.exe` uploaden als artifact.
- **Kern-tests op Linux** (`ubuntu-latest`): `cargo test --lib` voor `core/`. Snel, en het houdt de kern platformonafhankelijk.
- **Release:** bij een gepubliceerde GitHub-release met tag `0.2` / `v0.2.0` wordt de versie uit de tag gehaald en via een omgevingsvariabele door `build.rs` in de versie-resource gezet. Daarna `gh release upload <tag> target/release/Snapr.exe --clobber`. Handmatig starten met een tag blijft mogelijk.
- Lokaal ontwikkelen kan ook op Linux/macOS: `cargo test` voor de kern, en `cargo xwin build --target x86_64-pc-windows-msvc` voor een Windows-exe.

---

## 9. Fasering

Na elke fase heb je iets dat werkt en dat je kunt uitproberen.

| Fase | Inhoud | Resultaat |
|---|---|---|
| **0. Skelet** | Cargo-project, `build.rs` (icoon, manifest, versie), CI, statische CRT, message loop, single instance, tray met Afsluiten | Lege tray-app die in CI een exe oplevert |
| **1. Kern** | `geometry`, `settings`, `hotkey`, `strings`, `error_log`, `pixelator` + tests | Kern getest, compatibel met bestaande settings |
| **2. Vastleggen** | Globale sneltoets, schermafbeelding, overlay (alle interacties), klembord, balloons | **Bruikbare MVP:** sneltoets → gebied → op het klembord |
| **3. Editor-basis** | Venster en schaling, renderpijplijn, toolbar/statusbalk, pijl, undo/redo, kopiëren, opslaan, kopiëren bij sluiten | Pijlen zetten en delen |
| **4. Editor compleet** | Pixeleren, nummers, selecteren, verplaatsen, handles, Delete, cursors, alle sneltoetsen, tooltips | Functioneel gelijk aan Snapr-editor |
| **5. Instellingen** | Instellingenvenster, sneltoets opnemen, map kiezen, starten met Windows | Volledige pariteit |
| **6. Afwerking** | DPI-wisselingen, gemengde DPI, `TaskbarCreated`, foutdialoog, golden-image-tests, handmatige checklist, README (NL), release-workflow | Release 1.0 als vervanger van Snapr |

Verwachte omvang: ongeveer 3000–4000 regels Rust. Het meeste extra werk ten opzichte van C# zit in de zelfgetekende widgets en de Win32-wrappers, die WPF/WinForms nu gratis leveren.

---

## 10. Risico's

| Risico | Aanpak |
|---|---|
| Zelfgetekende UI kost meer werk dan verwacht | Beperkt houden: alleen icoonknoppen, kleurbolletjes, checkbox en opnameknop. Geen tekstinvoer (zie §5, Instellingen). |
| Pijlen en nummers zien er net anders uit dan in WPF | Geometrie letterlijk porten; vroeg schermafbeeldingen naast elkaar leggen. Golden-image-tests leggen het resultaat vast. |
| Overlay hapert op grote multi-monitor-setups | GDI-blits met vooraf gedimde buffer en alleen de gewijzigde gebieden invalideren (zie §5) |
| Fouten in `unsafe` FFI-code | Klein `platform/`-oppervlak, veilige wrappers, `clippy` met `-D warnings` |
| Segoe UI niet beschikbaar | Ingebouwd OFL-lettertype als reserve |
| SmartScreen-waarschuwing | Zelfde als nu (exe niet ondertekend); in de README laten staan |

---

## 11. Open beslissingen

1. **Drop-in vervanger?** Voorstel: ja. Zelfde mutexnaam, zelfde `%AppData%\Snapr`, zelfde Run-sleutel. Daardoor vervangt rusty-snapr Snapr zonder dat je iets opnieuw hoeft in te stellen. Het alternatief is een eigen naam en map, zodat beide naast elkaar kunnen draaien.
2. **Mapveld in Instellingen alleen-lezen** (kiezen via Bladeren…), of een echt tekstveld? Voorstel: alleen-lezen.
3. **Exe-naam:** `Snapr.exe` (vervanger) of `rusty-snapr.exe`?
