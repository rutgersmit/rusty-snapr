# Snapr (Rust)

Een Rust-versie van [Snapr](https://github.com/rutgersmit/snapr), de kleine screenshot-tool voor Windows. Doel: precies dezelfde functies, maar als native exe van een paar honderd KB in plaats van een .NET-app van tientallen MB.

Het plan staat in [PLAN.md](PLAN.md).

## Status

| Fase | Inhoud | Status |
|---|---|---|
| 0 | Skelet: exe met icoon/manifest/versie, CI, tray-icoon met *Afsluiten*, single instance, foutlog | klaar |
| 1 | Kern: instellingen, sneltoetsen, teksten (EN/NL), foutlog, pixelator (gelijk aan de C#-versie) | klaar |
| 2 | Sneltoets, schermafbeelding, selectie-overlay, klembord | klaar |
| 3 | Editor: pijlen in vijf kleuren, ongedaan maken/opnieuw, kopiëren, opslaan als PNG | klaar |
| 4 | Pixeleren, nummers, selecteren en verplaatsen | gepland |
| 5–6 | Instellingenvenster, afwerking | gepland |

Je kunt nu een screenshot maken (sneltoets of klik op het tray-icoon), een gebied kiezen en er pijlen op zetten. Pixeleren en nummers staan al in de toolbar, maar zijn tot fase 4 uitgeschakeld.

## Naast de C#-versie

Deze versie kan tegelijk met de C#-Snapr draaien:

| | Snapr (C#) | Snapr (Rust) |
|---|---|---|
| Naam in tray en titels | Snapr | Snapr (Rust) |
| Instellingen en log | `%AppData%\Snapr\` | `%AppData%\RustySnapr\` |
| Starten met Windows (register) | `Snapr` | `RustySnapr` |

Het formaat van `settings.json` is identiek; je kunt het bestand van de ene naar de andere map kopiëren.

Beide gebruiken standaard `PrintScreen`. Een sneltoets kan maar door één programma tegelijk gebruikt worden, dus geef één van de twee een andere sneltoets (bijvoorbeeld `Ctrl+Shift+S`).

## Zelf bouwen

Vereist [Rust](https://rustup.rs/) (stable).

```powershell
cargo build --release      # target\release\Snapr.exe
cargo test                 # alle tests
```

De kern (`src/core`) bevat geen Windows-code en is ook op Linux/macOS te testen met `cargo test --lib`. Vanaf Linux een Windows-exe bouwen kan met mingw:

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu
```

## Testen

Zie [TESTING.md](TESTING.md): automatische tests (ook een end-to-end test onder Wine) en een checklist voor op Windows.

## Releases

Maak op GitHub een release met een tag als `0.2` of `v0.2.0`; de Release-workflow bouwt `Snapr.exe` met dat versienummer en hangt hem aan de release. Tussentijdse builds staan onder *Actions › Build › Artifacts*.
