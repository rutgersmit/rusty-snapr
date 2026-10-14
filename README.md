# Snapr (Rust)

Een Rust-versie van [Snapr](https://github.com/rutgersmit/snapr), de kleine screenshot-tool voor Windows. Doel: precies dezelfde functies, maar als native exe van een paar honderd KB in plaats van een .NET-app van tientallen MB.

Het plan staat in [PLAN.md](PLAN.md); de verschillen met de C#-versie in [VERSCHILLEN.md](VERSCHILLEN.md).

## Functies

Hetzelfde als de C#-versie:

- **Sneltoets**: standaard `PrintScreen`, instelbaar via het tray-icoon › *Instellingen…*
- **Gebied kiezen**: sleep een rechthoek, klik voor het hele scherm onder de muis, `Enter` voor alle schermen, `Esc` om te annuleren.
- **Pijlen** (`A`) in vijf kleuren (`1`–`5`).
- **Markeren** (`H`): een markeerstift die over tekst kleurt (tekst blijft leesbaar). Een bibberige, ongeveer rechte sleepbeweging wordt één strakke streep, die recht trekt naar horizontaal (of verticaal) als hij daar dichtbij ligt; alleen een duidelijke bocht wordt gevolgd, en dan glad gestreken. De markeerstift onthoudt zijn eigen kleur (eerst geel).
- **Pixeleren** (`P`): sleep over tekst om die onleesbaar te maken, met een beetje willekeurige ruis per blok.
- **Nummers** (`N`): elke klik plaatst het volgende nummer; slepen verfijnt de plek.
- **Verplaatsen**: klik op een pijl, markering of nummer om hem te selecteren en te verslepen; sleep een uiteinde van een pijl of rechte markering om hem te vervormen. `Delete` verwijdert, `Esc` deselecteert.
- Ongedaan maken / opnieuw (`Ctrl+Z` / `Ctrl+Y`), kopiëren (`Ctrl+C`), opslaan als PNG (`Ctrl+S`), sluiten (`Esc`).
- De screenshot staat direct op het klembord; sluit je de editor na bewerken, dan staat de bewerkte versie op het klembord.
- Optioneel automatisch starten met Windows.
- **Taal**: Engels, of Nederlands als de weergavetaal van Windows Nederlands is.

## Status

| Fase | Inhoud | Status |
|---|---|---|
| 0 | Skelet: exe met icoon/manifest/versie, CI, tray-icoon met *Afsluiten*, single instance, foutlog | klaar |
| 1 | Kern: instellingen, sneltoetsen, teksten (EN/NL), foutlog, pixelator (gelijk aan de C#-versie) | klaar |
| 2 | Sneltoets, schermafbeelding, selectie-overlay, klembord | klaar |
| 3 | Editor: pijlen in vijf kleuren, ongedaan maken/opnieuw, kopiëren, opslaan als PNG | klaar |
| 4 | Pixeleren, nummers, selecteren en verplaatsen, tooltips | klaar |
| 5 | Instellingenvenster: sneltoets opnemen, opslagmap, direct kopiëren, starten met Windows | klaar |
| 6 | Afwerking: snelle weergave van grote screenshots, afsluiten met open editors, golden-image test, documentatie | klaar |

De editor is functioneel gelijk aan die van de C#-versie: pijlen, pixeleren en nummers, selecteren, verplaatsen, uiteinden verslepen, verwijderen, ongedaan maken, kopiëren en opslaan.

## Naast de C#-versie

Deze versie kan tegelijk met de C#-Snapr draaien:

| | Snapr (C#) | Snapr (Rust) |
|---|---|---|
| Naam in tray en titels | Snapr | Snapr (Rust) |
| Instellingen en log | `%AppData%\Snapr\` | `%AppData%\RustySnapr\` |
| Starten met Windows (register) | `Snapr` | `RustySnapr` |

Het formaat van `settings.json` is identiek; je kunt het bestand van de ene naar de andere map kopiëren.

Eén bewuste afwijking: in het instellingenvenster kies je de opslagmap met *Bladeren…*; het pad zelf is niet als tekst te bewerken.

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

## Lettertype

De nummers gebruiken Segoe UI Bold, net als de C#-versie. Is dat lettertype er niet, dan valt Snapr terug op de cijfers van Carlito Bold (SIL Open Font License, zie `assets/fonts/`).

## Testen

Zie [TESTING.md](TESTING.md): automatische tests (ook een end-to-end test onder Wine) en een checklist voor op Windows.

## Releases

Maak op GitHub een release met een tag als `0.2` of `v0.2.0`; de Release-workflow bouwt `Snapr.exe` met dat versienummer en hangt hem aan de release. Tussentijdse builds staan onder *Actions › Build › Artifacts*.
