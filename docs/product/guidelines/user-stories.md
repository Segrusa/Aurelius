# User stories Guidelines

- **Task:** AUR-GL-00-N (Nova, voor Orion)
- **Bron:** `docs/product/guidelines/README.md` (B1–B7, concept van Orion), de vraag van de Owner ("guidelines per employee, vooral coding guidelines") en het bestaande patroon van instructies met versies en `propose_instructions`.
- **Status:** goedgekeurd door Atlas volgens de README (B1–B7). Rechtgezet bij de start van AUR-GL-01: grens per run 16.000 (GL-07, GL-12), A-03/GL-11 AC4 volgen B6 (nieuwe sessie), GL-08 is Won't (B2), de seed is een stap bij de eerste start (B7). Bij een verschil gaat de README voor.
- **Regel:** wat de bron niet vastlegt, staat als **aanname** (A-xx) of **open vraag** (OV-xx). Waar ik een B-voorstel anders zou doen, staat dat als OV met een alternatief.

## Aannames

- **A-01: na Fase 4.** De bouw (AUR-GL-01) start na de merge van AUR-MU-04. Elke wijziging heeft dus een board member (Fase 3) of een employee met een run als auteur.
- **A-02: guideline.** Een guideline heeft een **titel** (uniek binnen de company, hoofdletterongevoelig), een **tekst** in markdown, een optionele **korte beschrijving** (alleen voor de UI, niet in de run) en een **versienummer**. Titel en tekst samen vormen een versie.
- **A-03: verschil met instructies.** Instructies blijven ongewijzigd: één tekst per employee, met versies en `propose_instructions`. Guidelines komen ernaast en vervangen niets. Een wijziging van de geldende guidelines start bij de **volgende run een nieuwe sessie**, alleen voor de gesprekken die ze raakt (B6, rechtgezet; OV-08 is niet overgenomen).
- **A-04: geschiedenis volgt het instructiepatroon.** Elke wijziging maakt een nieuwe versie. Terugzetten maakt ook een nieuwe versie (de geschiedenis wordt nooit herschreven). Goedkeuren op een verouderde basisversie wordt geweigerd (`version_conflict`), net zoals bij instructievoorstellen.
- **A-05: welke runs.** Alleen runs van een employee krijgen het guideline-blok: taak-, gesprek-, ticket- en projectassistent-runs. De triage-run van de dispatcher (`ToolProfile::Triage`) en interne jobs (extractie, samenvatting, consolidatie) krijgen het niet, om tokens te sparen.
- **A-06: run zonder project.** Een run zonder project (bijvoorbeeld een company-gesprek of de groepschat) krijgt alleen de guidelines die aan de employee gekoppeld zijn.
- **A-07: realtime.** Wijzigingen aan de bibliotheek, koppelingen en voorstellen komen als events, zodat alle clients meteen bijwerken (zoals de rest van het protocol).

## User stories

Prioriteit: **M** = Must, **S** = Should, **C** = Could. **W** (Won't) staat niet als story maar in de OV's.

### Epic A: Bibliotheek (B1)

#### GL-01: Guideline aanmaken en bekijken (M)
**Als** board member **wil ik** guidelines op company-niveau aanmaken, **zodat** werkregels zoals de Rust-afspraken op één plek staan en niet per employee gekopieerd worden.

- **AC1.** *Given* de instellingen van de company, *When* ik Guidelines open, *Then* zie ik alle actieve guidelines met titel, beschrijving, huidige versie, aantal tekens, aan wie of wat ze gekoppeld zijn, en wie ze laatst wijzigde en wanneer.
- **AC2.** *Given* de bibliotheek, *When* ik een guideline aanmaak met titel en tekst, *Then* bestaat ze als versie 1 zonder koppelingen. Ze verschijnt meteen in alle clients en staat in de geschiedenis met mij als auteur.
- **AC3.** *Given* een titel die al bestaat (ook bij een ander hoofdlettergebruik), *When* ik die gebruik, *Then* wordt dat geweigerd met de melding dat de titel al bestaat.
- **AC4.** *Given* een lege titel, een lege tekst of een tekst boven 8.000 tekens (GL-12), *When* ik opsla, *Then* wordt dat geweigerd met de reden en het aantal tekens.
- **AC5.** *Given* een employee in een run, *When* die een guideline probeert aan te maken of te wijzigen buiten `propose_guideline` om, *Then* kan dat niet (B5).

#### GL-02: Guideline wijzigen met versies en geschiedenis (M)
**Als** board member **wil ik** een guideline wijzigen en elke vorige versie terugzien, **zodat** ik weet wat er veranderde, wanneer en door wie.

- **AC1.** *Given* een guideline, *When* ik titel of tekst wijzig en opsla, *Then* ontstaat een nieuwe versie (n+1). De vorige versies blijven ongewijzigd.
- **AC2.** *Given* de geschiedenis van een guideline, *When* ik ze open, *Then* zie ik per versie het nummer, de datum, de auteur (board member, of "voorstel van ‹employee›, goedgekeurd door ‹board member›", of "seed") en de reden als die er is.
- **AC3.** *Given* twee versies, *When* ik ze vergelijk, *Then* toont de client de verschillen in de tekst (diff).
- **AC4.** *Given* twee board members die dezelfde guideline tegelijk bewerken, *When* de tweede opslaat terwijl de eerste al een nieuwe versie maakte, *Then* wordt dat geweigerd (`version_conflict`) en toont de client de nieuwe versie, zodat er niets stil overschreven wordt.
- **AC5.** *Given* een opslag zonder wijziging in titel en tekst, *When* ik opsla, *Then* ontstaat er geen nieuwe versie.

#### GL-03: Een vorige versie terugzetten (M)
**Als** board member **wil ik** een eerdere versie terugzetten, **zodat** ik een slechte wijziging snel ongedaan maak.

- **AC1.** *Given* de geschiedenis, *When* ik bij versie k "Terugzetten" kies en bevestig, *Then* ontstaat een nieuwe versie n+1 met de titel en tekst van versie k. De geschiedenis vermeldt "teruggezet naar versie k".
- **AC2.** *Given* een versie k waarvan de titel intussen door een andere guideline gebruikt wordt, *When* ik ze terugzet, *Then* wordt dat geweigerd met die reden.
- **AC3.** *Given* een terugzetting die de limiet zou overschrijden (GL-12), *When* ik ze bevestig, *Then* wordt ze geweigerd zoals een gewone wijziging.

#### GL-04: Guideline archiveren (M)
**Als** board member **wil ik** een guideline uitschakelen zonder haar geschiedenis te verliezen, **zodat** ik kan nagaan welke regels een oude run kreeg.

- **AC1.** *Given* een guideline, *When* ik ze archiveer en bevestig, *Then* krijgt geen enkele nieuwe run ze nog. De client toont vooraf aan welke employees en projecten ze gekoppeld is.
- **AC2.** *Given* een gearchiveerde guideline, *When* ik de bibliotheek open, *Then* staat ze apart (filter "gearchiveerd"), met geschiedenis en koppelingen nog leesbaar.
- **AC3.** *Given* een gearchiveerde guideline, *When* ik ze herstel, *Then* gelden de koppelingen weer vanaf de volgende run. Het herstel wordt geweigerd als het de limiet overschrijdt (GL-12).
- **AC4.** *Given* een gearchiveerde guideline, *When* ik ze definitief verwijder, *Then* vraagt de client een bevestiging. Runs die een versie ervan kregen, tonen nog titel en versienummer (OV-07).

### Epic B: Koppelen (B2)

#### GL-05: Koppelen aan employees (M)
**Als** board member **wil ik** een guideline aan een of meer employees koppelen, **zodat** bijvoorbeeld Forge, Bridge en Nexus dezelfde Rust-regels krijgen.

- **AC1.** *Given* een guideline, *When* ik er employees aan koppel of van loskoppel, *Then* geldt dat vanaf hun volgende run (GL-11) en staat het in de geschiedenis van de guideline (wie, wanneer, welke employee).
- **AC2.** *Given* een employee, *When* ik de pagina van die employee open, *Then* kan ik daar ook guidelines koppelen en loskoppelen, en zie ik ze naast de instructies, apart ervan.
- **AC3.** *Given* een koppeling waardoor de limiet per run overschreden zou worden (GL-12), *When* ik ze opsla, *Then* wordt ze geweigerd met de combinatie die te groot wordt.
- **AC4.** *Given* een gearchiveerde employee, *When* ik de koppelingen van een guideline bekijk, *Then* staat die employee er nog, gemarkeerd als gearchiveerd.

#### GL-06: Koppelen aan projecten (M)
**Als** board member **wil ik** een guideline aan een project koppelen, **zodat** projectregels ("deze repo gebruikt X") gelden voor iedereen die in dat project werkt.

- **AC1.** *Given* een guideline, *When* ik ze aan een project koppel, *Then* krijgt elke run met dat project ze, ongeacht de employee (B2: employee **of** project).
- **AC2.** *Given* een project, *When* ik de instellingen van het project open, *Then* zie ik de gekoppelde guidelines en kan ik er koppelen en loskoppelen.
- **AC3.** *Given* de UI, *When* ik koppel, *Then* is er geen combinatie employee × project mogelijk (B2).
- **AC4.** *Given* een gearchiveerd project, *When* ik zijn guidelines bekijk, *Then* zijn de koppelingen nog leesbaar en los te koppelen.

#### GL-07: Overzicht van wat een employee effectief krijgt (S)
**Als** board member **wil ik** per employee zien welke guidelines een run krijgt, **zodat** ik de kost en de inhoud begrijp voordat ik iets koppel.

- **AC1.** *Given* de pagina van een employee, *When* ik "Guidelines in een run" open, *Then* zie ik de lijst in de volgorde van GL-10 voor "zonder project" en, via een keuzelijst, voor elk actief project. Bij elke guideline staat waarom ze erbij zit (employee of project).
- **AC2.** *Given* dat overzicht, *When* ik het bekijk, *Then* zie ik per combinatie het totaal aantal tekens tegenover de limiet van 16.000 (GL-12), met een waarschuwing vanaf 80 %.

#### GL-08: Koppelen aan een department (W — Won't, README B2)
> **Rechtgezet:** niet gebouwd. Departments bestaan niet meer sinds 0018; de UI groepeert op leidinggevende (`reports_to`) om snel meerdere employees aan te vinken. De tekst hieronder blijft alleen als historiek.

**Als** board member **wil ik** een guideline aan een department koppelen (bv. Engineering), **zodat** een nieuwe developer de coding guidelines meteen krijgt zonder dat ik eraan moet denken.

- **AC1.** *Given* een guideline gekoppeld aan een department, *When* een employee van dat department een run start, *Then* krijgt die de guideline zoals bij een koppeling aan de employee.
- **AC2.** *Given* een employee die van department wisselt, *When* die een volgende run start, *Then* gelden de guidelines van het nieuwe department.
- Zie **OV-01**: beslist in README B2: nee.

### Epic C: Het blok in de run (B3, B4, B6)

#### GL-09: Guidelines als gelabeld blok in de run (M)
**Als** employee **wil ik** de guidelines die voor mij gelden in mijn context krijgen, **zodat** ik ze volg zonder ze te moeten zoeken.

- **AC1.** *Given* een run van een employee (A-05), *When* de context wordt opgebouwd, *Then* bevat die één gelabeld blok "Guidelines", gescheiden van de instructies. Het blok bevat elke geldende guideline met titel, versienummer en niveau (employee of project).
- **AC2.** *Given* een guideline die zowel aan de employee als aan het project van de run gekoppeld is, *When* het blok wordt opgebouwd, *Then* staat ze er één keer in, op het projectniveau (OV-03).
- **AC3.** *Given* een employee zonder geldende guidelines, *When* een run start, *Then* is er geen blok (ook geen lege kop).
- **AC4.** *Given* het blok, *When* het wordt opgebouwd, *Then* begint het met een vaste kop die de voorrang uitlegt (GL-10 AC2), zodat het model die niet hoeft te raden.

#### GL-10: Volgorde en voorrang (M)
**Als** board member **wil ik** een voorspelbare volgorde en voorrang, **zodat** ik weet welke regel wint bij een conflict.

- **AC1.** *Given* het blok, *When* het wordt opgebouwd, *Then* staan eerst de employee-guidelines en daarna de project-guidelines, binnen elk niveau alfabetisch op titel (B3).
- **AC2.** *Given* de kop van het blok, *When* een employee die leest, *Then* staat daar: (1) de taak met haar acceptatiecriteria, de agent-regels en de rechten gaan altijd voor; (2) bij een conflict tussen guidelines wint een project-guideline van een employee-guideline.
- **AC3.** *Given* dezelfde gegevens, *When* twee runs starten, *Then* is het blok byte voor byte gelijk (stabiele volgorde, ook goed voor de prompt-cache).

#### GL-11: Vastgezette versie per run (M)
**Als** reviewer **wil ik** weten welke versie van elke guideline een run kreeg, **zodat** ik zijn werk tegen de juiste regels beoordeel.

- **AC1.** *Given* een run die gestart is, *When* een board member intussen een geldende guideline wijzigt, koppelt, loskoppelt of archiveert, *Then* verandert het blok van die run niet. De wijziging geldt vanaf de volgende run (B6).
- **AC2.** *Given* een run, *When* ze start, *Then* bewaart Aurelius per guideline in het blok het guideline-id en het versienummer.
- **AC3.** *Given* een afgelopen of lopende run, *When* ik de details van de run open, *Then* zie ik welke guidelines (titel en versie) ze kreeg, en kan ik de tekst van precies die versie openen.
- **AC4.** *Given* een taak met meerdere runs (bv. na een bericht van de Owner of een `message_task`), *When* een nieuwe run start, *Then* krijgt die de versies die op dat moment gelden. Het rapport of de run-details tonen het als de versies verschilden van de vorige run van die taak, en die run start dan een nieuwe sessie (B6).

#### GL-12: Limiet op de grootte (M)
**Als** board member **wil ik** een harde grens op de grootte van guidelines, **zodat** de tokenkost van elke run voorspelbaar blijft.

- **AC1.** *Given* een guideline, *When* de tekst langer is dan 8.000 tekens, *Then* wordt opslaan, terugzetten of goedkeuren geweigerd, met het aantal tekens en de grens (B4).
- **AC2.** *Given* een schrijfactie die het totaal kan doen stijgen (koppelen, tekst verlengen, terugzetten, herstellen uit het archief, een voorstel goedkeuren, de seed), *When* daardoor voor minstens één combinatie van employee en project (inclusief "zonder project") het blok meer dan 16.000 tekens zou bevatten, *Then* wordt de actie geweigerd. De melding noemt de employee, het project en het totaal (OV-02).
- **AC3.** *Given* een run waarvan het blok toch boven de grens uitkomt (bv. na een verlaging van de grens), *When* de run start, *Then* start ze toch. Het blok bevat dan alleen de guidelines die passen, in de volgorde van GL-10 maar met project-guidelines eerst, en de run-details en de board member krijgen een melding welke guidelines wegvielen. Er wordt nooit midden in een guideline afgeknipt.
- **AC4.** *Given* de editor, *When* ik typ, *Then* zie ik het aantal tekens tegenover 8.000, en bij het opslaan het grootste totaal over de combinaties waar de guideline in zit.

### Epic D: Voorstellen en goedkeuren (B5)

#### GL-13: Een guideline voorstellen met `propose_guideline` (M)
**Als** employee **wil ik** een nieuwe guideline of een wijziging voorstellen, **zodat** een regel die ik mis of die niet klopt bij een board member terechtkomt.

- **AC1.** *Given* een run, *When* de employee `propose_guideline` gebruikt met een titel, de volledige tekst, een reden en minstens één bewijs (een taak of memory, zoals bij `propose_instructions`), *Then* ontstaat een voorstel dat in de inbox van de board members komt. De employee krijgt alleen te horen dat het voorstel er is, geen beslissing.
- **AC2.** *Given* een bestaande guideline, *When* de employee een wijziging voorstelt, *Then* verwijst het voorstel naar de guideline en naar de versie waarop het gebaseerd is (de versie uit het blok van zijn run, of uit GL-15).
- **AC3.** *Given* een nieuw voorstel, *When* de employee koppelingen meegeeft (employees en/of projecten), *Then* zijn die deel van het voorstel. Zonder koppelingen stelt Aurelius voor: de voorsteller zelf, en het project van de run als dat er is (OV-04).
- **AC4.** *Given* een guideline met al een open voorstel, *When* een employee er nog een indient, *Then* wordt dat geweigerd met de melding dat er al een open voorstel is (één open voorstel per guideline). Voor nieuwe guidelines heeft elke employee hooguit twee open voorstellen.
- **AC5.** *Given* een voorstel dat de grens van 8.000 tekens overschrijdt, *When* het wordt ingediend, *Then* wordt het meteen geweigerd met de reden, zodat de employee het kan inkorten.
- **AC6.** *Given* elke modus, ook Auto, *When* een voorstel bestaat, *Then* wordt het nooit automatisch toegepast.
- **AC7.** *Given* een voorsteller die gearchiveerd wordt voor er een beslissing is, *When* dat gebeurt, *Then* krijgt het voorstel de status "ingetrokken".

#### GL-14: Voorstel goedkeuren of afwijzen (M)
**Als** board member **wil ik** een voorstel beoordelen, aanpassen en goedkeuren of afwijzen, **zodat** alleen regels die ik verantwoord in de runs komen.

- **AC1.** *Given* een voorstel in de inbox, *When* ik het open, *Then* zie ik de voorsteller, de reden, het bewijs (klikbaar), de voorgestelde koppelingen en, bij een wijziging, de diff tegenover de basisversie.
- **AC2.** *Given* een voorstel, *When* ik het goedkeur (eventueel met aangepaste tekst, titel of koppelingen), *Then* ontstaat een nieuwe guideline (versie 1) of een nieuwe versie, met in de geschiedenis "voorstel van ‹employee›, goedgekeurd door ‹board member›".
- **AC3.** *Given* een voorstel op basisversie k terwijl de guideline intussen op versie n > k staat, *When* ik goedkeur, *Then* wordt dat geweigerd (`version_conflict`). De client toont de diff met de huidige versie en laat me de tekst aanpassen en dan goedkeuren.
- **AC4.** *Given* een voorstel, *When* ik het afwijs met een optionele notitie, *Then* krijgt de voorsteller de beslissing met de notitie in zijn volgende run of als bericht in het gesprek van zijn taak (zoals bij `propose_instructions`).
- **AC5.** *Given* een goedkeuring die de limiet zou overschrijden (GL-12), *When* ik goedkeur, *Then* wordt ze geweigerd met de reden, en blijft het voorstel open.
- **AC6.** *Given* een employee in een run, *When* die een voorstel probeert goed te keuren, *Then* kan dat niet. Alleen board members beslissen.

#### GL-15: Een guideline lezen vanuit een run (S)
**Als** employee **wil ik** guidelines lezen die niet in mijn blok staan, **zodat** ik een wijziging kan voorstellen of de regels van een collega of ander project ken.

- **AC1.** *Given* een run, *When* de employee `list_guidelines` gebruikt, *Then* krijgt die titel, beschrijving, huidige versie en koppelingen van de actieve guidelines, zonder de tekst.
- **AC2.** *Given* een titel of id, *When* de employee `read_guideline` gebruikt, *Then* krijgt die de huidige tekst en het versienummer.
- **AC3.** *Given* een guideline die gelezen wordt, *When* dat gebeurt, *Then* verandert het blok van de run niet (GL-11) en geldt de gelezen tekst niet als regel voor die run.

### Epic E: Clients

#### GL-16: Bewerken op desktop (M)
**Als** board member **wil ik** guidelines op desktop beheren, **zodat** ik lange regels comfortabel kan schrijven.

- **AC1.** *Given* desktop, *When* ik de bibliotheek open, *Then* kan ik guidelines zoeken, aanmaken, bewerken (markdown met voorbeeld), archiveren, herstellen, de geschiedenis met diff bekijken, terugzetten en koppelen.
- **AC2.** *Given* een voorstel in de inbox op desktop, *When* ik het open, *Then* kan ik het bekijken, aanpassen, goedkeuren en afwijzen (GL-14).
- **AC3.** *Given* niet-opgeslagen wijzigingen in de editor, *When* ik wegnavigeer, *Then* vraagt de client of ik ze wil weggooien.

#### GL-17: Bekijken, bewerken en beslissen op mobiel (S)
**Als** board member **wil ik** guidelines op mobiel kunnen lezen, kort aanpassen en voorstellen afhandelen, **zodat** een voorstel niet wacht tot ik aan mijn bureau zit.

- **AC1.** *Given* mobiel, *When* ik de bibliotheek open, *Then* kan ik guidelines zoeken, lezen (gerenderde markdown), aanmaken, bewerken (platte markdown, zonder voorbeeld naast de tekst), archiveren en koppelen.
- **AC2.** *Given* een voorstel op mobiel, *When* ik het open, *Then* zie ik de diff, en kan ik goedkeuren of afwijzen met een notitie. De tekst aanpassen voor ik goedkeur mag ook.
- **AC3.** *Given* de geschiedenis op mobiel, *When* ik ze open, *Then* kan ik versies lezen, vergelijken met de vorige en terugzetten (met een bevestiging).

### Epic F: Seed (B7)

#### GL-18: Startset met de huidige afspraken (M)
**Als** board member **wil ik** dat de bibliotheek start met de afspraken die het team vandaag al volgt, **zodat** de guidelines vanaf dag één nut hebben.

- **AC1.** *Given* een company zonder guidelines, *When* de feature voor het eerst actief wordt, *Then* bevat de bibliotheek de startset uit `seed.md` (Rust, Flutter, worktrees en build) als gewone guidelines, versie 1, met als auteur "seed".
- **AC2.** *Given* de startset, *When* ze wordt aangemaakt, *Then* krijgt ze de koppelingen uit `seed.md` aan de bestaande employees (op naam). Een employee die niet bestaat wordt overgeslagen en gemeld, zonder dat de seed faalt.
- **AC3.** *Given* een company die al guidelines heeft, of een seed die al eerder liep, *When* de server opnieuw start of een update krijgt, *Then* gebeurt er niets (de seed is idempotent en loopt één keer).
- **AC4.** *Given* een seed-guideline, *When* een board member ze wijzigt, archiveert of verwijdert, *Then* gaat dat zoals bij elke andere guideline, en zet een volgende update ze niet terug.
- **AC5.** *Given* de startset, *When* ze wordt aangemaakt, *Then* respecteert ze de limieten van GL-12. Past ze niet, dan is dat een fout in `seed.md` die voor de bouw opgelost moet worden.

## Open vragen (escalatie naar Orion)

| ID | Vraag | Voorstel |
|---|---|---|
| **OV-01** (B2) | Koppelen aan een **department** naast employees en projecten? Titans impactanalyse noemt departments. | Ik ben het eens met B2 en wil geen employee × project, maar ik stel een **department als extra koppeldoel** voor (GL-08, Could). Coding guidelines horen vaak bij een department (Engineering): een nieuwe developer krijgt ze dan meteen, en wie vergeet koppelen, mist geen regels. Het model blijft eenvoudig: een guideline geldt als ze aan de employee, zijn department **of** het project gekoppeld is. Niveau in de volgorde: department, employee, project. Wordt het niet gekozen, dan blijft GL-08 Won't |
| **OV-02** (B4) | B4 zegt "de koppeling weigert" boven 16.000 tekens. Maar het totaal hangt af van employee **en** project, en het kan ook stijgen door een wijziging, terugzetting, herstel uit het archief, goedkeuring of de seed. | **Alternatief:** de controle geldt voor **elke** schrijfactie die het totaal kan doen stijgen (GL-12 AC2), en per combinatie employee × project inclusief "zonder project", niet alleen bij het koppelen. Bij de start van een run is er een vangnet zonder weigering: de run start met de guidelines die passen, met een melding (GL-12 AC3). **Reden:** anders kan een tekstwijziging bij Forge ongemerkt een run boven de grens duwen, en een run weigeren blokkeert werk. De cijfers 8.000 en 16.000 wachten op Titans §context |
| **OV-03** (B3) | Een guideline die zowel aan de employee als aan het project gekoppeld is: twee keer in het blok, of één keer, en waar? | Eén keer, op het projectniveau (het specifieke niveau, B3). Ze telt één keer mee voor de limiet |
| **OV-04** (B5) | Mag een agent in een voorstel koppelingen meegeven aan **andere** employees of projecten? `propose_instructions` beperkt zich tot jezelf en je directe ondergeschikten. | Ja, want een board member keurt toch alles goed en kan de koppelingen aanpassen. Standaard zijn de koppelingen de voorsteller zelf en het project van de run (GL-13 AC3). Verder eens met B5, met dezelfde vangrails als `propose_instructions`: bewijs verplicht, één open voorstel per guideline, twee open nieuwe voorstellen per employee, nooit automatisch, ook niet in Auto |
| **OV-05** (B7) | B7 stelt voor de seed **in de migratie** te zetten. | **Alternatief:** geen inhoud in de schemamigratie, maar een **eenmalige, idempotente seedstap** bij de eerste start na de migratie, alleen als de bibliotheek leeg is, met de tekst en de koppelingen uit `seed.md` (GL-18). **Reden:** (1) een migratie staat vast in elke installatie en kan je achteraf niet corrigeren, terwijl de afspraken over Rust, Flutter en worktrees specifiek zijn voor dit team; (2) de seedstap kan koppelen op employeenaam en ontbrekende employees melden in plaats van te falen; (3) "alleen als de bibliotheek leeg is" voorkomt dubbele guidelines. Het resultaat is hetzelfde als in B7: gewone guidelines die een board member kan aanpassen |
| **OV-06** (B7) | De afspraken staan vandaag ook in `CLAUDE.md` van de repo, en die leest Claude Code in elke run in die repo. Na de seed staan ze dus twee keer in de context (dubbele tokens, en een risico dat ze uit elkaar lopen). | De seed neemt de afspraken over, en daarna worden ze uit `CLAUDE.md` gehaald, of `CLAUDE.md` verwijst er alleen nog naar. Wie beslist en wie `CLAUDE.md` aanpast, hoort bij Titans impactanalyse en het bouwplan |
| **OV-07** (B1) | Verwijderen of alleen archiveren? Runs verwijzen naar versies (B6). | Archiveren als standaard (GL-04). Definitief verwijderen alleen van een gearchiveerde guideline. De run-details houden titel en versienummer, maar de tekst is dan weg. Alternatief: nooit definitief verwijderen. Mijn voorkeur is het alternatief, omdat het volume klein is en de reviewbaarheid van B6 dan volledig blijft |
| **OV-08** (B6) | Een gesprek of taak hervat soms dezelfde Claude-sessie. Een instructiewijziging start een nieuwe sessie, maar wat doet een guideline-wijziging? | Eens met B6 (vastgezet per run). Een guideline-wijziging start **geen** nieuwe sessie (A-03): de volgende run krijgt het nieuwe blok, en de kop zegt dat dit blok de vorige versies vervangt. De run-details tonen het verschil (GL-11 AC4). Titan bevestigt of dit technisch kan met hervatte sessies. Kan het niet, dan valt de keuze terug op een nieuwe sessie zoals bij instructies |
| **OV-09** | Subagents die een employee via de Task-tool van Claude Code start, krijgen de systeemprompt van de employee niet, en dus ook de guidelines niet. | Buiten de scope van deze stories, maar het is een risico voor coding guidelines: een subagent die code schrijft, volgt ze dan niet. Titan beoordeelt dat in de impactanalyse. Het minimum is een regel in de kop van het blok: "geef de relevante guidelines mee als je een subagent code laat schrijven" |
| **OV-10** (B4) | Geldt de limiet in tekens of in tokens? | Tekens (eenvoudig en uit te leggen in de UI). Titan rekent in §context de verhouding uit (≈ 4 tekens per token voor Engelse tekst, minder voor code) |

## Dekking van de B-punten

| B | Stories | OV |
|---|---|---|
| B1 Bibliotheek | GL-01–GL-04, GL-16, GL-17 | OV-07 |
| B2 Koppeling project | GL-05–GL-07 | OV-01 (department) |
| B3 Volgorde en voorrang | GL-09, GL-10 | OV-03 |
| B4 Limiet | GL-12 | OV-02, OV-10 |
| B5 Voorstellen en goedkeuren | GL-13–GL-15 | OV-04 |
| B6 Lopende run | GL-11 | OV-08 |
| B7 Seed | GL-18 | OV-05, OV-06 |
