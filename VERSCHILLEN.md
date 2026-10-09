# Verschillen tussen Snapr (C#) en Snapr (Rust)

Doel van de Rust-versie is hetzelfde gedrag. Hieronder alles wat (bewust of onvermijdelijk) anders is, zodat je gericht kunt vergelijken. De checklist om beide naast elkaar te testen staat in [TESTING.md](TESTING.md).

## Om naast elkaar te kunnen draaien

| | C# | Rust |
|---|---|---|
| Naam in tray, meldingen, titels | Snapr | Snapr (Rust) |
| Instellingen en log | `%AppData%\Snapr\` | `%AppData%\RustySnapr\` |
| Starten met Windows (`HKCU\…\Run`) | waarde `Snapr` | waarde `RustySnapr` |
| Single-instance-mutex | `Local\Snapr.SingleInstance` | `Local\RustySnapr.SingleInstance` |

Het formaat van `settings.json` is identiek. Beide gebruiken standaard `PrintScreen`; geef één van de twee een andere sneltoets, anders meldt de tweede dat de sneltoets bezet is.

## Bewuste functionele verschillen

- **Opslagmap**: alleen te kiezen met *Bladeren…* (of door op het pad te klikken); het pad is niet als tekst te typen. Lange paden worden in het midden ingekort.
- **Toetsenbord in Instellingen**: `Esc` annuleert, `Enter` slaat op, `Tab` stopt het opnemen van de sneltoets. Er is geen Tab-navigatie langs alle knoppen en geen spatie om een vinkje te zetten.
- **Klembord**: naast de gewone bitmap (DIB) zet de Rust-versie ook een **PNG** op het klembord. Browsers en chat-apps (Gmail, Slack, Teams) pakken die bij voorkeur; kleuren en scherpte zijn dan exact.
- **Editor naar een scherm met andere schaal slepen**: het venster neemt de grootte die Windows voorstelt; de C#-versie past de grootte daarna opnieuw aan de inhoud aan.
- **Tijdens het verplaatsen** van een pijl of nummer ligt die tijdelijk bovenop andere annotaties; na het loslaten staat hij weer op zijn eigen plek in de stapel. (Zo blijft verslepen vloeiend, ook op enorme screenshots.)
- **Overlay op schermen met verschillende schaal**: hint en maatlabel volgen de schaal van het scherm waar de muis bij het starten staat (C#: de schaal van het overlay-venster).
- **Lettertype van de nummers**: Segoe UI Bold, net als C#. Ontbreekt dat, dan gebruikt Rust de cijfers van Carlito Bold (meegeleverd), C# het reservelettertype van WPF.

## Onvermijdelijke (cosmetische) verschillen

- **Systeemstijl in plaats van WPF/WinForms-stijl** voor het tray-menu, de tooltips, de ballonmeldingen en de dialogen voor opslaan en map kiezen. Inhoud en teksten zijn gelijk.
- **Tekst** in de vensters wordt met de tekstweergave van Windows (GDI, ClearType) getekend in plaats van door WPF; letterbreedtes en anti-aliasing kunnen een fractie verschillen.
- **Anti-aliasing van pijlen en nummers**: dezelfde geometrie (letterlijk overgenomen), maar een andere renderer (tiny-skia i.p.v. WPF). Randen kunnen per pixel net iets anders vallen.
- **Verkleinde weergave** van grote screenshots in de editor: de screenshot wordt met GDI's HALFTONE-filter verkleind, WPF gebruikt zijn eigen filter. Kopiëren en opslaan gebeuren altijd op volle grootte en zijn niet beïnvloed.

## Niet-functioneel

| | C# | Rust |
|---|---|---|
| Download (exe) | ± 70 MB (self-contained .NET 10, gecomprimeerd) | ± 0,6 MB |
| Runtime nodig | nee (ingepakt in de exe) | nee (statisch gelinkt) |
| Opstarten | runtime uitpakken + JIT | direct |

Ruwe tijden van de zware stappen in de Rust-versie (Xeon 2,1 GHz, `cargo run --release --example timings`):

| Stap | 4K-scherm | twee 4K-schermen |
|---|---|---|
| Overlay: bureaublad verduisteren (eenmalig bij openen) | 20 ms | 38 ms |
| Editor: één frame tijdens tekenen/slepen | < 1 ms | < 1 ms |
| Kopiëren/opslaan: alles samenvoegen op volle grootte | 10 ms | 46 ms |
| Kopiëren/opslaan: PNG maken | 71 ms | 164 ms |

Geheugen en opstarttijd zijn het best te vergelijken met Taakbeheer en een stopwatch op dezelfde pc; die cijfers heb ik niet zelf kunnen meten (er is geen Windows-pc in de bouwomgeving).
