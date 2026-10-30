# User stories Fase 4: Vault

- **Task:** AUR-MU-DEF-N (Nova, voor Orion)
- **Bron:** `docs/product/multi-user.md` §1 (Vault onder Company en Project), §2 "Vault", §2 "Toegang" (iedereen mag de vault beheren).
- **Status:** concept, wacht op review door Orion en akkoord van Sébastien.
- **Regel:** wat de bron niet vastlegt, staat als **aanname** (A-xx) of **open vraag** (OV-xx).

## Aannames

- **A-01: na Fase 3.** Fase 4 bouwt op de login en het auteurschap van Fase 3: elke wijziging en elke lezing heeft een board member of een employee met een run als bron.
- **A-02: item.** Een item heeft een naam (uniek binnen zijn niveau), een type (**tekst**, **url** of **secret**), een waarde, een optionele beschrijving en optioneel een naam voor de omgevingsvariabele.
- **A-03: beheer.** Board members beheren de vault (§2 "Toegang"). Employees lezen alleen (OV-03).

## User stories

Prioriteit: **M** = Must, **S** = Should, **C** = Could.

### Epic A: Items beheren

#### MU4-01: Vault op company-niveau (M)
**Als** board member **wil ik** items voor de hele company bewaren, **zodat** gedeelde gegevens (zoals een staging-URL of een API-key) op één plek staan.

- **AC1.** *Given* de instellingen van de company, *When* ik de vault open, *Then* zie ik alle company-items met naam, type, beschrijving en wie ze laatst wijzigde. Secrets zijn gemaskeerd.
- **AC2.** *Given* de vault, *When* ik een item aanmaak, wijzig of verwijder, *Then* is dat meteen zichtbaar in alle clients en staat het in het auditlog (MU4-09).
- **AC3.** *Given* een naam die op dat niveau al bestaat, *When* ik die opnieuw gebruik, *Then* wordt dat geweigerd.

#### MU4-02: Vault per project (M)
**Als** board member **wil ik** items per project bewaren, **zodat** gegevens van één project niet tussen de rest staan.

- **AC1.** *Given* een project, *When* ik de vault van dat project open, *Then* zie ik de items van het project en, apart, de company-items.
- **AC2.** *Given* een project-item met dezelfde naam (of omgevingsvariabele) als een company-item, *When* een run in dat project de items krijgt, *Then* wint het project-item (OV-02).
- **AC3.** *Given* een gearchiveerd project, *When* ik de vault ervan open, *Then* zijn de items nog te lezen en te verwijderen.

#### MU4-03: Types tekst, url en secret (M)
**Als** board member **wil ik** per item een type kiezen, **zodat** gewone gegevens leesbaar blijven en secrets beschermd zijn.

- **AC1.** *Given* een item van het type **tekst** of **url**, *When* ik het bekijk, *Then* zie ik de waarde. Een url wordt gevalideerd en is klikbaar.
- **AC2.** *Given* een item van het type **secret**, *When* ik het bekijk, *Then* is de waarde gemaskeerd. Tonen of kopiëren is een expliciete actie die in het auditlog komt.
- **AC3.** *Given* een secret, *When* ik het type wil wijzigen naar tekst of url, *Then* vraagt de client een bevestiging, omdat de waarde dan in prompts terecht kan komen (OV-06).

#### MU4-04: Versleutelde opslag (M)
**Als** board member **wil ik** dat de waarden versleuteld opgeslagen zijn, **zodat** een kopie van de database geen secrets prijsgeeft.

- **AC1.** *Given* de database, *When* iemand ze leest zonder de sleutel, *Then* staat geen enkele secretwaarde er leesbaar in (OV-01 voor tekst en url).
- **AC2.** *Given* de sleutel, *When* de server start, *Then* staat die buiten de database, alleen leesbaar voor de daemon-user (0600), en nooit in logs, events of back-ups van de database (OV-04).
- **AC3.** *Given* een restore van een back-up, *When* de sleutel ontbreekt of niet past, *Then* werkt de rest van Aurelius, en toont de vault per secret "niet te ontsleutelen" in plaats van een fout of rommel.

### Epic B: Geheimhouding en levering aan runs

#### MU4-05: Secrets nooit in prompts, memory, discussies of events (M)
**Als** board member **wil ik** dat een secret nooit in de tekst van Aurelius belandt, **zodat** het niet lekt via een gesprek, memory of de timeline.

- **AC1.** *Given* een run, *When* de systeemprompt en de context worden opgebouwd, *Then* staat er geen secretwaarde in, alleen de naam, het type en de omgevingsvariabele.
- **AC2.** *Given* het aanmaken of wijzigen van een secret, *When* het commando verwerkt wordt, *Then* staat de waarde niet in het commando, de receipt, de `request_hash` of het event. Het event bevat alleen het item-id, de naam en de actor.
- **AC3.** *Given* een run-output, een bericht, een rapport of een memory-item dat letterlijk een secretwaarde bevat, *When* het opgeslagen of getoond wordt, *Then* is de waarde vervangen door `‹secret: naam›` (OV-05).
- **AC4.** *Given* `remember`, *When* een employee een secretwaarde probeert op te slaan, *Then* wordt die vervangen zoals in AC3 en bewaart memory alleen de verwijzing.

#### MU4-06: Levering als omgevingsvariabele (M)
**Als** employee **wil ik** een item als omgevingsvariabele in mijn run krijgen, **zodat** ik een tool of script kan gebruiken zonder de waarde te zien.

- **AC1.** *Given* een item met een naam voor de omgevingsvariabele, *When* een run start in het project (of company-breed), *Then* krijgt het `claude`-proces van die run die variabele, naast de bestaande allowlist.
- **AC2.** *Given* andere processen (de daemon, andere runs), *When* ze lopen, *Then* krijgen ze die variabele niet. Ze komt ook niet in de debug-output of de trace van de run.
- **AC3.** *Given* een levering aan een run, *When* die gebeurt, *Then* komt er per item een regel in het auditlog (employee + run).
- **AC4.** *Given* een naam die met een gereserveerde variabele botst (`PATH`, `HOME`, `ANTHROPIC_*`, `CLAUDE_*` en de allowlist), *When* ik die instel, *Then* wordt dat geweigerd.

#### MU4-07: Levering via een tool (M)
**Als** employee **wil ik** met een tool zien welke items er zijn en ze gebruiken, **zodat** ik weet wat ik in mijn werk kan inzetten.

- **AC1.** *Given* een run, *When* de employee `vault_list` gebruikt, *Then* krijgt die naam, type, niveau, beschrijving en omgevingsvariabele van de items, zonder secretwaarden.
- **AC2.** *Given* een item van het type tekst of url, *When* de employee `vault_get` gebruikt, *Then* krijgt die de waarde, en komt de lezing in het auditlog.
- **AC3.** *Given* een secret, *When* de employee het via een tool nodig heeft, *Then* krijgt de run de waarde **niet in de context** maar op een manier die buiten het model blijft (OV-05; voorstel: een tijdelijk bestand van de run met 0600, waarvan de tool alleen het pad teruggeeft). De lezing komt in het auditlog.

#### MU4-08: Alle employees mogen lezen (M)
**Als** board member **wil ik** geen toegangslijsten per item, **zodat** de vault eenvoudig blijft (§2 "Vault").

- **AC1.** *Given* een employee in een run, *When* die items opvraagt, *Then* kan die de company-items en de items van elk project lezen, zonder lijst per item. Standaard toont `vault_list` de company-items en het project van de run (OV-03).
- **AC2.** *Given* een employee, *When* die een item probeert aan te maken, te wijzigen of te verwijderen, *Then* wordt dat geweigerd (A-03).

### Epic C: Audit en clients

#### MU4-09: Auditlog van lezen en wijzigen (M)
**Als** board member **wil ik** zien wie wat las of wijzigde, **zodat** ik misbruik of een lek kan nagaan.

- **AC1.** *Given* elke wijziging (aanmaken, wijzigen, verwijderen, type wijzigen) en elke lezing (tonen of kopiëren door een board member, levering als omgevingsvariabele, `vault_get` of de secret-tool), *When* die gebeurt, *Then* komt er een regel met tijd, item, actie en wie: de board member, of de employee met de run (en het ticket als dat er is).
- **AC2.** *Given* het auditlog, *When* ik het bekijk per item of voor de hele vault, *Then* zie ik die regels, en nooit een waarde.
- **AC3.** *Given* het auditlog, *When* iemand een regel probeert te wijzigen of te verwijderen, *Then* kan dat niet (append-only). Ook een verwijderd item houdt zijn historiek.

#### MU4-10: Vault op desktop en mobiel (S)
**Als** board member **wil ik** de vault op desktop en mobiel gebruiken, **zodat** ik ook onderweg een url of gegeven kan opzoeken.

- **AC1.** *Given* desktop of mobiel, *When* ik de vault van de company of een project open, *Then* kan ik de items zien, aanmaken, wijzigen en verwijderen.
- **AC2.** *Given* een secret op mobiel, *When* ik het wil tonen of kopiëren, *Then* vraagt de client eerst een bevestiging en komt het in het auditlog.

#### MU4-11: Item wijzigen of verwijderen terwijl runs lopen (S)
**Als** board member **wil ik** weten wat een wijziging doet met lopende runs, **zodat** ik een secret veilig kan vervangen.

- **AC1.** *Given* een run die een item al als omgevingsvariabele kreeg, *When* ik het item wijzig of verwijder, *Then* houdt die run de oude waarde tot ze eindigt. Nieuwe runs krijgen de nieuwe waarde of niets.
- **AC2.** *Given* het verwijderen van een item met een omgevingsvariabele, *When* ik bevestig, *Then* toont de client dat runs die variabele niet meer krijgen.

## Open vragen (escalatie naar Orion)

| ID | Vraag | Voorstel |
|---|---|---|
| **OV-01** | Worden alleen secrets versleuteld, of alle waarden (ook tekst en url)? | Alle waarden versleuteld (één mechanisme, geen fout bij het wisselen van type). Alleen secrets worden gemaskeerd en uit tekst geweerd |
| **OV-02** | Wint een project-item van een company-item met dezelfde naam of omgevingsvariabele? | Ja, het project-item wint. De UI toont dat het company-item in dat project overschreven is |
| **OV-03** | Mogen employees items aanmaken of wijzigen (bijvoorbeeld een url die ze ontdekken)? En betekent "alle items lezen" ook de items van andere projecten? | Employees alleen lezen; voor nieuwe items stellen ze het voor aan een board member. Ja, alle projecten (zoals de bron zegt), maar `vault_list` toont standaard alleen company + het project van de run |
| **OV-04** | Waar staat de sleutel, en hoe gaat een back-up of een verhuizing van de server? | Een sleutelbestand in de datamap van de daemon (0600), apart van de database en niet in de gewone back-up. De UI toont bij het eerste secret dat je de sleutel apart moet bewaren. Sentinel valideert |
| **OV-05** | De bron zegt "nooit in prompts" én "via een tool". Een tool die een secret teruggeeft, zet het in de context van het model (en in de transcript van Claude). Hoe leveren we een secret via een tool? | De tool geeft nooit de waarde terug, maar zet ze in een tijdelijk bestand van de run (0600, weg na de run) en geeft alleen het pad. Daarnaast vervangt Aurelius bekende secretwaarden in alles wat het opslaat (MU4-05 AC3). Restrisico: een employee kan het bestand toch lezen en uitprinten; de transcript in de Claude-map van de board member valt buiten Aurelius. Sentinel beoordeelt |
| **OV-06** | Mag een secret van type wijzigen naar tekst of url? | Ja, met een bevestiging en een auditregel (MU4-03 AC3) |
| **OV-07** | Hoe lang blijft het auditlog bewaard? | Onbeperkt in Fase 4 (het volume is klein); opschonen komt later als het nodig is |
