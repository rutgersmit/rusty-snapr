# Testen

## Automatisch

- `cargo test --lib`: de kern (instellingen, sneltoetsen, pixelator, editorlogica, overlay-logica, PNG). Draait op elk OS.
- `cargo test --test golden`: tekent een vaste scène met alle soorten annotaties en vergelijkt die met `tests/golden/scene.png`. Na een bedoelde wijziging: `SNAPR_UPDATE_GOLDEN=1 cargo test --test golden`.
- `cargo run --release --example timings`: ruwe tijden van de zware stappen op 4K-formaat.
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

### Fase 4: pixeleren, nummers, selecteren

- [ ] Pixeleren (`P` of `B`): tekst wordt onleesbaar; de blokken blijven stabiel tijdens het slepen; twee regio's krijgen elk een eigen ruispatroon.
- [ ] Nummers (`N`): elke klik plaatst het volgende nummer; slepen verfijnt de plek; na ongedaan maken komt het nummer terug. 10 en hoger worden breder.
- [ ] Nummers zien eruit als in de C#-versie (Segoe UI Bold, witte tekst, donkere tekst op geel).
- [ ] Klik op een pijl of nummer selecteert (blauw gestippeld kader) en verplaatst hem, ook met een ander gereedschap actief. Pixelregio's zijn niet te selecteren.
- [ ] Uiteinden van een geselecteerde pijl verslepen (handje als cursor); verplaatsen kan niet buiten de afbeelding.

### Markeren

- [ ] Markeren (`H`): een bibberige sleepbeweging over een regel tekst geeft één strakke, horizontale streep over het midden van de regel; de tekst blijft donker en leesbaar.
- [ ] Een duidelijk schuine streep blijft schuin (maar recht); een echte bocht wordt gevolgd zonder het gebibber.
- [ ] De markeerstift begint geel en onthoudt zijn eigen kleur; de kleur van pijlen en nummers verandert niet mee.
- [ ] Een markering is te selecteren en te verplaatsen; een rechte markering ook aan de uiteinden te verlengen. Ongedaan maken werkt.
- [ ] `Delete`/`Backspace` verwijdert de selectie; `Esc` deselecteert eerst en sluit pas daarna.
- [ ] Het selectiekader komt niet mee bij kopiëren of opslaan.
- [ ] Tooltips op alle toolbarknoppen, in de taal van Windows.

### Fase 5: instellingen

- [ ] Tray › *Instellingen…* opent het venster (een tweede keer: hetzelfde venster naar voren). Donkere titelbalk, zelfde indeling als de C#-versie.
- [ ] Sneltoets opnemen: klik in het veld, druk bijv. `Ctrl+Shift+S`, `Alt+F9`, `Win+Shift+X` en `PrintScreen`. Letters en cijfers zonder Ctrl/Alt/Win worden genegeerd. Alt-combinaties openen geen systeemmenu.
- [ ] Zolang het venster open is, doet de sneltoets niets; na *Opslaan* werkt de nieuwe sneltoets meteen, na *Annuleren* de oude.
- [ ] *PrintScreen*-knop zet de sneltoets terug.
- [ ] *Bladeren…* (of klik op het pad) kiest een map; een lang pad wordt in het midden ingekort.
- [ ] "Starten met Windows": na opslaan staat `RustySnapr` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; uitvinken haalt hem weg; na opnieuw inloggen start Snapr (Rust).
- [ ] "Direct kopiëren" uit: na een capture blijft het klembord ongemoeid, en sluiten van de editor kopieert niet.
- [ ] `Esc` annuleert, `Enter` slaat op (ook tijdens het opnemen); `Tab` stopt het opnemen.

### Fase 6: afwerking

- [ ] *Afsluiten* in het tray-menu met een open, bewerkte editor: de bewerkte versie staat daarna op het klembord.
- [ ] `Enter` in de overlay op twee 4K-schermen: de editor opent vlot, en tekenen en verslepen blijven vloeiend.
- [ ] Een fout veroorzaken is lastig; controleer in elk geval dat `%AppData%\RustySnapr\snapr.log` niet ontstaat bij normaal gebruik.
- [ ] Vergelijk opstarttijd en geheugengebruik (Taakbeheer) met de C#-versie.
