# Testen

## Automatisch

- `cargo test --lib`: de kern (instellingen, sneltoetsen, pixelator, overlay-logica, PNG). Draait op elk OS.
- `tools/wine-e2e.sh`: start `Snapr.exe` onder Wine op een virtueel X-scherm en test met `xdotool` het selecteren en het klembord. Instructies staan bovenin het script.

## Handmatig op Windows

Wine is geen Windows. Dit moet je op een echte pc nalopen, het liefst naast de C#-versie.

### Fase 2: vastleggen

- [ ] Bij het opstarten verschijnt de melding "Snapr (Rust) draait – druk op … om een screenshot te maken".
- [ ] `PrintScreen` start een screenshot. Of, als Windows de toets voor het Knipprogramma gebruikt: er verschijnt een waarschuwing met uitleg.
- [ ] Draait de C#-versie al met dezelfde sneltoets, dan geeft de versie die als tweede start de melding "Sneltoets niet beschikbaar".
- [ ] Na de sneltoets reageert de overlay meteen op `Esc` en `Enter`, zonder eerst te klikken (toetsenbordfocus).
- [ ] Linksklik op het tray-icoon start een screenshot; het menu (rechtsklik) heeft *Screenshot maken* en *Afsluiten*.
- [ ] Slepen selecteert een gebied; het maatlabel klopt met wat er geplakt wordt.
- [ ] Klik zonder slepen = de hele monitor onder de muis. `Enter` = alle schermen. `Esc` of rechtsklik = annuleren.
- [ ] Alt+Tab tijdens de overlay annuleert (maar niet tijdens het slepen).
- [ ] Plakken werkt in Paint, Word, een browser (bijv. Gmail) en Slack/Teams. Kleuren kloppen en het beeld staat niet op zijn kop.
- [ ] **Twee schermen**, ook met verschillende schaal (100% + 150%) en met het tweede scherm links van of boven het hoofdscherm: de overlay dekt beide schermen precies, de hint staat op het scherm met de muis, en een klik pakt het juiste scherm.
- [ ] Explorer herstarten (Taakbeheer › Windows Verkenner › Opnieuw starten): het tray-icoon komt terug.
- [ ] Geen zichtbare vertraging bij slepen op een groot of 4K-scherm.

### Fase 3: editor

- [ ] Na het kiezen van een gebied opent de editor gecentreerd op het scherm met de muis, met de afbeelding 1:1 (scherp, niet geschaald). De editor heeft een donkere titelbalk en het Snapr-icoon.
- [ ] De editor heeft meteen toetsenbordfocus: `Esc` sluit zonder eerst te klikken (ook na `Enter` in de overlay).
- [ ] Een screenshot van een heel 4K-scherm past in het venster (verkleind, max. 92% van het werkgebied); de toolbar blijft bruikbaar.
- [ ] Pijlen tekenen in alle vijf kleuren (knoppen én toetsen `1`–`5`); dikte, kop, witte rand en schaduw zien eruit als in de C#-versie, ook op 125%/150%.
- [ ] `Ctrl+Z`, `Ctrl+Y`, `Ctrl+Shift+Z` en de knoppen; die zijn grijs als er niets ongedaan te maken is.
- [ ] Hover-effect op knoppen en kleurbolletjes; handje als cursor boven knoppen, kruisje boven de afbeelding.
- [ ] `Ctrl+C` zet het bewerkte beeld op het klembord ("Gekopieerd naar klembord").
- [ ] `Ctrl+S` opent de opslaan-dialoog in de opslagmap met naam `Snapr jjjj-mm-dd uu.mm.ss.png`; het bestand klopt.
- [ ] Sluiten (Esc, kruisje, Alt+F4) na een wijziging zet de bewerkte versie op het klembord; zonder wijziging blijft het klembord zoals het was.
- [ ] Venster vergroten/verkleinen: de afbeelding schaalt mee omlaag, nooit omhoog; niet kleiner dan 580×260.
- [ ] Editor naar een scherm met andere schaal slepen: toolbar en tekst schalen mee. (Afwijking: de C#-versie past dan ook de venstergrootte aan de inhoud aan; hier neemt het venster de door Windows voorgestelde grootte.)
