# User stories Fase 1: Company, Board en Tickets

- **Task:** AUR-MU-00-N (Nova, voor Orion)
- **Bron:** `docs/product/multi-user.md` (goedgekeurd door Sébastien op 2026-10-30). Paragraafverwijzingen als §2 "Board en workflow" slaan op dat document.
- **Status:** concept, wacht op review door Orion en akkoord van Sébastien.
- **Regel:** waar de bron iets niet vastlegt, staat het als **aanname** (A-xx) of als **open vraag** (OV-xx). Er is niets bijbedacht als beslissing.

---

## 0. Terminologie (vast, ook voor Pixel)

| Term | Betekenis | Niet gebruiken |
|---|---|---|
| **Company** | De ene organisatie op een server (tabel `company`) | Organisatie, workspace |
| **Board member** | Een mens die met Aurelius werkt (tabel `users`). Er is één rol | Owner, gebruiker, admin |
| **Employee** | Een AI-agent met instructies en `reports_to` | Agent, manager, medewerker |
| **Board** | Het kanbanbord van de company | Kanban, takenbord |
| **Column** / kolom | Een kolom op het Board. De standaardnamen worden altijd in het Engels en exact zo geschreven: **Backlog**, **Todo**, **Doing**, **Plan review**, **Review**, **Done** | "To do", "In progress", "Klaar" |
| **Ticket** | Een kaart op het Board. Het ticket staat boven op tasks | Kaart, issue, story |
| **Task** | Een interne werkeenheid onder een ticket: de **hoofdtask** plus de **delegaties** | Subticket |
| **Aanmaker** | De board member die het ticket maakte (`created_by_user`) | Auteur, eigenaar |
| **Eigenaar** | De employee die het ticket uitvoert (`owner_employee`) | Assignee, toegewezene |
| **Dispatcher** | De employee die in Todo een eigenaar kiest. Standaard Orion | Router, planner |
| **Start** | De knop die een ticket uit de Backlog naar Todo zet | Run, Go |
| **Modus** | De bestaande `AgentMode`: Ask, Accept edits, Auto, Plan | Permissiemodus |
| **Company-memory** | De bestaande memory-scope `global`, vanaf nu zo genoemd in de UI | Globale memory |

---

## 1. Rollen

- **Board member:** een mens. Mag alles zien en doen (§2 "Toegang").
- **Employee:** een AI-agent. Werkt tickets uit en mag ze verplaatsen tot en met Review.
- **Dispatcher:** een employee in de rol van dispatcher (standaard Orion).
- **Systeem:** de Aurelius-server (daemon), voor automatische overgangen.

---

## 2. Aannames (gelden tenzij Orion of Sébastien anders beslist)

- **A-01: ticketkolom en taskstatus.** De kolom is de bron van waarheid voor het Board. De hoofdtask volgt die kolom: Todo, dan `queued` en `running` in Doing, dan `review` in Review, dan `succeeded` in Done. Terug naar Doing gebruikt de bestaande overgang `review → queued`. Titan bevestigt de mapping in de impactanalyse.
- **A-02: kolommen hebben een vaste functie.** De zes standaardkolommen hebben elk een vaste functie in de workflow (start, uitvoering, planpoort, reviewpoort, afgesloten). "Op company-niveau instellen" betekent hier: de weergavenaam en de volgorde aanpassen, en kolommen zonder functie toevoegen of verwijderen. Een kolom met een functie verwijderen kan niet. Zie OV-02.
- **A-03: Fase 1 heeft nog geen ticketdiscussie.** Die komt in Fase 2. De goedkeuring in Plan review en de feedback bij "terug naar Doing" gebeuren in Fase 1 daarom via een knop en een tekstveld op het ticket. Die tekst gaat als bericht naar het gesprek van de hoofdtask. Zie OV-01.
- **A-04: clients.** Fase 1 geldt voor de desktop en voor mobiel, omdat de onboarding "in elke client" moet werken. Zie OV-10.
- **A-05: Claude-login.** In Fase 1 gebruiken alle runs nog de ene Claude-login van de server. De login per aanmaker komt in Fase 3.

---

## 3. User stories

Prioriteit volgens MoSCoW: **M** = Must, **S** = Should, **C** = Could, **W** = Won't (niet in Fase 1).

### Epic A: Company en organigram

#### MU1-01: Company-tabel (M)
**Als** board member **wil ik** dat alle gegevens van de server onder één expliciete company vallen, **zodat** later meerdere companies mogelijk zijn zonder herontwerp.

- **AC1.** *Given* een nieuwe of geüpgradede server, *When* de server start, *Then* bestaat er precies één rij in `company`.
- **AC2.** *Given* de company, *When* ik de board members, employees, projecten, de Board-inrichting en de company-memory opvraag, *Then* verwijzen ze allemaal naar die company (direct of via hun project).
- **AC3.** *Given* een server met al een company, *When* iemand een tweede company probeert aan te maken (via API of migratie), *Then* wordt dat in Fase 1 geweigerd met een duidelijke fout. Het schema laat een tweede company wel toe; alleen de applicatie houdt het op één.
- **AC4.** *Given* de company, *When* een client de company opvraagt, *Then* krijgt hij minstens de id en de naam terug.

#### MU1-02: Afdelingen verwijderen, het organigram loopt via reports_to (M)
**Als** board member **wil ik** een organigram dat alleen op `reports_to` steunt, **zodat** er één bron van waarheid is voor de hiërarchie.

- **AC1.** *Given* de nieuwe versie, *When* het schema wordt bekeken, *Then* bestaan de tabel `departments`, de kolom `employees.department_id`, de kolom `proposals.department_id` en het scope-type `department` niet meer.
- **AC2.** *Given* employees met `reports_to`, *When* ik het organigram open, *Then* toont het de hiërarchie alleen op basis van `reports_to`, zonder groepering per afdeling.
- **AC3.** *Given* een employee die vroeger in een afdeling zat, *When* ik de employee bekijk na de upgrade, *Then* zijn de naam, de rol, de instructies en `reports_to` ongewijzigd.
- **AC4.** *Given* een employee die een collega voorstelt (`propose_colleague`), *When* het voorstel wordt gemaakt, *Then* vraagt het geen afdeling meer, alleen `reports_to`.

#### MU1-03: Department-memory gaat naar company-memory (M)
**Als** board member **wil ik** dat de memory van afdelingen naar company-memory gaat, **zodat** er bij het verwijderen van afdelingen geen kennis verloren gaat.

- **AC1.** *Given* N memory-items met scope `department` voor de upgrade, *When* de upgrade klaar is, *Then* bestaan er N extra items in company-memory met dezelfde inhoud.
- **AC2.** *Given* een verplaatst item, *When* ik het bekijk, *Then* is de naam van de oude afdeling als herkomst te zien (bijvoorbeeld als label of prefix), zodat de context bewaard blijft. *(Aanname: de vorm kiest Titan.)*
- **AC3.** *Given* de upgrade, *When* die klaar is, *Then* is er geen memory-item meer met scope `department`, en zijn er buiten die verplaatsing geen memory-items verloren (het aantal na = het aantal ervoor).
- **AC4.** *Given* een employee in een run, *When* `recall` wordt aangeroepen, *Then* worden de verplaatste items gevonden via company-memory.

#### MU1-04: "Department" verdwijnt uit prompts en UI (M)
**Als** board member **wil ik** nergens meer afdelingen zien, **zodat** de UI en de agents hetzelfde model gebruiken.

- **AC1.** *Given* een run van een employee, *When* de systeemprompt wordt opgebouwd, *Then* staat er geen "department", "afdeling" of afdelingsnaam in. De org-chart-tekst in de prompt groepeert dan op `reports_to` (bijvoorbeeld "You report to …", "Peers …").
- **AC2.** *Given* de desktop en de mobiele client, *When* ik door alle schermen ga (organigram, employee, instellingen, memory), *Then* is er geen veld, filter of label "Department" of "Afdeling" meer.
- **AC3.** *Given* de MCP-tools en het protocol, *When* ze worden bekeken, *Then* hebben ze geen parameter of veld meer dat een afdeling verwacht.
- **AC4.** *Given* de bestaande tests, *When* de afdelingstests worden verwijderd of herschreven, *Then* zijn `cargo test` en `flutter test` groen (met de bekende baseline).

### Epic B: Board en kolommen

#### MU1-05: Standaardkolommen van het Board (M)
**Als** board member **wil ik** een Board met de kolommen Backlog, Todo, Doing, Plan review, Review en Done, **zodat** ik de stand van elk ticket in één oogopslag zie.

- **AC1.** *Given* een company zonder eigen kolominrichting, *When* ik het Board open, *Then* zie ik in deze volgorde: **Backlog · Todo · Doing · Plan review · Review · Done**.
- **AC2.** *Given* het Board, *When* ik een project kies als filter, *Then* zie ik alleen de tickets van dat project, in dezelfde kolommen. *(Aanname: het Board is per company en kan per project gefilterd worden; zie OV-03.)*
- **AC3.** *Given* een ticket, *When* het van kolom wisselt (door een mens, een employee of het systeem), *Then* zien alle verbonden clients die wijziging live, zonder te verversen.

#### MU1-06: Kolommen instellen op company-niveau (M)
**Als** board member **wil ik** de kolommen van het Board voor de hele company instellen, **zodat** het Board past bij hoe wij werken.

- **AC1.** *Given* de instellingen van de company, *When* ik een kolom hernoem of de volgorde wijzig en opsla, *Then* zien alle clients en alle projecten de nieuwe inrichting.
- **AC2.** *Given* een kolom met een vaste functie (A-02), *When* ik die probeer te verwijderen, *Then* wordt dat geweigerd met een uitleg.
- **AC3.** *Given* een kolom zonder functie die nog tickets bevat, *When* ik die probeer te verwijderen, *Then* wordt dat geweigerd tot de kolom leeg is. *(Aanname.)*
- **AC4.** *Given* een hernoemde kolom met een functie, *When* een ticket de workflow doorloopt, *Then* werkt de functie (start, planpoort, reviewpoort, sluiten) gewoon, omdat die op de functie steunt en niet op de naam.
- **AC5.** *Given* een company, *When* ik "Standaard herstellen" kies, *Then* krijg ik weer de zes standaardkolommen. *(Could, zie de prioriteit hieronder.)*

> Prioriteit: AC1–AC4 zijn **M**. AC5 is **C**. De precieze reikwijdte van "instellen" hangt af van OV-02.

### Epic C: Tickets

#### MU1-07: Ticket aanmaken (M)
**Als** board member **wil ik** een ticket aanmaken met alle velden uit de productdefinitie, **zodat** een employee precies weet wat er moet gebeuren.

Velden (§2 "Board en workflow"):

| Veld | Verplicht | Standaardwaarde / regel |
|---|---|---|
| Titel | ja | max. 160 tekens (zoals `TITLE_MAX` van tasks) |
| Beschrijving | nee | vrije tekst (markdown) |
| Project | ja | een bestaand, actief project |
| Kolom | ja | **Backlog** bij aanmaken |
| Prioriteit | ja | standaard "normaal". De schaal staat in OV-06 |
| Aanmaker (`created_by_user`) | ja | automatisch de huidige board member. **Niet** te wijzigen |
| Eigenaar (employee) | nee | leeg, of meteen gekozen (MU1-11) |
| Modus | ja | standaard de modus van het project of de server (bestaand gedrag) |
| Model | nee | leeg = het standaardmodel |
| Labels | nee | nul of meer vrije tekstlabels |
| Bijlagen | nee | bestanden, zoals de bestaande bijlagen bij tasks |
| Deadline | nee | een datum (met tijd optioneel) |
| Gekoppelde tasks | — | automatisch gevuld (MU1-08) |

- **AC1.** *Given* een board member op het Board, *When* die een ticket aanmaakt met een titel en een project, *Then* staat het ticket in **Backlog** en is `created_by_user` die board member.
- **AC2.** *Given* het formulier, *When* de titel leeg is of geen project is gekozen, *Then* wordt het ticket niet aangemaakt en is te zien welk veld ontbreekt.
- **AC3.** *Given* een ticket zonder `created_by_user`, *When* het via een API of tool aangemaakt wordt, *Then* weigert de server het. *(`created_by_user` is NOT NULL vanaf Fase 1.)*
- **AC4.** *Given* een nieuw ticket in Backlog, *When* er tijd verstrijkt, *Then* start er geen run en wordt er geen task aangemaakt (zie MU1-09).
- **AC5.** *Given* een bestaand ticket, *When* een board member een veld behalve de aanmaker wijzigt, *Then* wordt de wijziging opgeslagen en live getoond. Modus en model zijn alleen te wijzigen zolang er geen run van de hoofdtask loopt. *(Aanname.)*

#### MU1-08: Ticket boven op tasks (M)
**Als** board member **wil ik** dat een ticket zijn tasks bundelt, **zodat** ik vanuit het ticket de hoofdtask en alle delegaties kan volgen.

- **AC1.** *Given* een ticket dat start, *When* de eigenaar bekend is, *Then* maakt het systeem precies één **hoofdtask** aan met de titel, de beschrijving (als brief), het project, de modus, het model en de bijlagen van het ticket, gekoppeld aan het ticket.
- **AC2.** *Given* een hoofdtask die delegeert, *When* er een delegatie (subtask) ontstaat, *Then* is die ook aan het ticket gekoppeld, en zie ik in het ticket de boom van gekoppelde tasks met hun status.
- **AC3.** *Given* een ticket met tasks, *When* de delegaties lopen, *Then* werken ze ongewijzigd ten opzichte van nu (delegate_task, hand_over_task, message_task, rapportage volgens RULE 04).
- **AC4.** *Given* een ticket, *When* ik een gekoppelde task open, *Then* zie ik het bestaande taskgesprek.

#### MU1-09: Starten met Start of door naar Todo te slepen (M)
**Als** board member **wil ik** zelf bepalen wanneer een ticket start, **zodat** er niets onverwacht gaat lopen of quotum verbruikt.

- **AC1.** *Given* een ticket in Backlog, *When* ik op **Start** klik, *Then* gaat het naar **Todo**.
- **AC2.** *Given* een ticket in Backlog, *When* ik het naar **Todo** sleep, *Then* gebeurt precies hetzelfde als bij Start.
- **AC3.** *Given* tickets in Backlog, *When* de server herstart, de scheduler draait of er een employee vrij is, *Then* wordt geen enkel ticket uit Backlog opgepakt. **Er is geen automatische pickup.**
- **AC4.** *Given* een employee, *When* die een ticket uit Backlog naar Todo (of verder) probeert te zetten, *Then* wordt dat geweigerd. Alleen een board member start een ticket. *(Afgeleid uit "pas gestart na een klik op Start of slepen naar Todo"; zie OV-05.)*
- **AC5.** *Given* een ticket in Todo zonder eigenaar, *When* het in Todo komt, *Then* wordt de dispatcher gewekt (MU1-10). Met een eigenaar start de hoofdtask meteen (MU1-12).

#### MU1-10: Dispatcher kiest een employee (M)
**Als** board member **wil ik** dat een dispatcher een geschikte employee kiest voor een gestart ticket, **zodat** ik niet zelf hoef te weten wie wat doet.

- **AC1.** *Given* een project zonder eigen instelling, *When* een ticket zonder eigenaar in Todo komt, *Then* krijgt **Orion** de opdracht een eigenaar te kiezen.
- **AC2.** *Given* de dispatcher, *When* die een eigenaar kiest, *Then* wordt het ticketveld *Eigenaar* gevuld, ontstaat de hoofdtask (MU1-08) en gaat het ticket naar **Doing**.
- **AC3.** *Given* de dispatcher, *When* die een eigenaar kiest, *Then* volgt de keuze de bestaande regels van het organigram (`follow_org_chart`). *(Aanname: de dispatcher mag alle actieve employees kiezen als die regel uit staat.)*
- **AC4.** *Given* een dispatcher die geen keuze kan maken, *When* hij dat meldt, *Then* blijft het ticket in Todo met een zichtbare reden, en kan een board member zelf een eigenaar kiezen.

#### MU1-11: Dispatcher per project instellen (M)
**Als** board member **wil ik** per project een andere dispatcher kunnen instellen, **zodat** een project met een eigen lead zelf zijn werk verdeelt.

- **AC1.** *Given* de projectinstellingen, *When* ik een andere actieve employee als dispatcher kies en opsla, *Then* gaan nieuwe tickets van dat project in Todo naar die employee.
- **AC2.** *Given* een project met een eigen dispatcher, *When* ik de instelling leegmaak, *Then* is Orion weer de dispatcher.
- **AC3.** *(Voorlopig, OV-04)* *Given* een projectdispatcher die gearchiveerd is, *When* er daarna een ticket in Todo komt, *Then* gaat de opdracht naar Orion. Bestaat Orion ook niet (of is die gearchiveerd), dan blijft het ticket in Todo met de melding "Kies een eigenaar".

#### MU1-12: Board member kiest zelf meteen een employee (M)
**Als** board member **wil ik** bij het aanmaken of starten zelf een eigenaar kiezen, **zodat** het ticket zonder de dispatcher meteen bij de juiste employee komt.

- **AC1.** *Given* een ticket met een gekozen eigenaar, *When* ik op Start klik of naar Todo sleep, *Then* wordt de dispatcher **niet** gewekt, ontstaat de hoofdtask bij die employee en gaat het ticket naar **Doing**.
- **AC2.** *Given* een ticket in Todo dat op de dispatcher wacht, *When* ik zelf een eigenaar kies, *Then* gaat de opdracht naar die employee en vervalt de keuze van de dispatcher. *(Aanname.)*
- **AC3.** *Given* de eigenaarkeuze, *When* ik de lijst open, *Then* zie ik alleen actieve employees.

### Epic D: Workflow, Plan review en de reviewpoort

#### MU1-13: Plan review in Plan-modus (M)
**Als** board member **wil ik** dat een ticket in Plan-modus na het plan stopt in Plan review, **zodat** ik het plan kan goedkeuren voor er iets verandert.

- **AC1.** *Given* een ticket met modus **Plan**, *When* de eigenaar het plan heeft opgeleverd, *Then* gaat het ticket naar **Plan review** en wordt er niets uitgevoerd.
- **AC2.** *Given* een ticket in Plan review, *When* ik het ticket open, *Then* zie ik het plan (het rapport van de planrun).
- **AC3.** *Given* een ticket met een andere modus dan Plan (Ask, Accept edits, Auto), *When* het de workflow doorloopt, *Then* komt het **nooit** in Plan review. Het gaat van Doing rechtstreeks naar Review.
- **AC4.** *Given* een ticket in Plan review, *When* een employee het naar Doing probeert te zetten, *Then* wordt dat geweigerd. Alleen een board member keurt goed.

#### MU1-14: Plan goedkeuren of terugsturen (M)
**Als** board member **wil ik** een plan goedkeuren of met feedback terugsturen, **zodat** de uitvoering alleen start met een plan dat ik goed vind.

- **AC1.** *Given* een ticket in Plan review, *When* ik op **Goedkeuren** klik, *Then* gaat het ticket naar **Doing** en voert de eigenaar het goedgekeurde plan uit, in de uitvoeringsmodus van OV-07.
- **AC2.** *Given* een ticket in Plan review, *When* ik feedback geef en het plan terugstuur, *Then* blijft de modus Plan, krijgt de eigenaar de feedback en komt er een nieuw plan, waarna het ticket weer in Plan review staat. *(Aanname: de bron noemt alleen de goedkeuring.)*
- **AC3.** *Given* een goedkeuring of feedback, *When* die wordt gegeven, *Then* wordt opgeslagen welke board member het deed en wanneer. *(Basis voor de ticketdiscussie in Fase 2; zie A-03.)*

#### MU1-15: Employees verplaatsen tickets tot en met Review (M)
**Als** employee **wil ik** mijn ticket naar Review zetten als het werk klaar is, **zodat** een board member het kan beoordelen.

- **AC1.** *Given* een ticket in Doing waarvan de hoofdtask klaar is, *When* de eigenaar zijn rapport oplevert, *Then* gaat het ticket naar **Review** en staat de hoofdtask op `review` (niet meer meteen doorlopen naar `succeeded`, zoals nu).
- **AC2.** *Given* een employee, *When* die een ticket naar **Done** probeert te zetten (via een tool, een API of een statuswijziging), *Then* weigert de server dat met een duidelijke fout.
- **AC3.** *Given* een employee, *When* die een ticket verplaatst tussen kolommen tot en met Review, *Then* is dat toegestaan binnen de workflowregels (bijvoorbeeld niet uit Backlog, zie MU1-09 AC4, en niet uit Plan review, zie MU1-13 AC4).
- **AC4.** *Given* elke kolomwijziging, *When* die gebeurt, *Then* wordt vastgelegd wie (board member of employee) ze deed en wanneer.

#### MU1-16: Alleen een board member zet op Done of terug naar Doing (M)
**Als** board member **wil ik** de enige zijn die een ticket afrondt of terugstuurt, **zodat** er een echte reviewpoort is.

- **AC1.** *Given* een ticket in Review, *When* ik het naar **Done** zet (knop of slepen), *Then* is het ticket gesloten, staat de hoofdtask op `succeeded` en is het ticket alleen nog te lezen. *(Heropenen: zie OV-08.)*
- **AC2.** *Given* een ticket in Review, *When* ik het terug naar **Doing** zet, *Then* moet ik feedback invullen (niet leeg), gaat de feedback naar de eigenaar, start de hoofdtask opnieuw (`review → queued`) en komt het ticket na het nieuwe rapport weer in Review.
- **AC3.** *Given* een ticket in Review, *When* ik het zonder feedback naar Doing probeer te zetten, *Then* wordt dat geweigerd met de vraag om feedback.
- **AC4.** *(Voorlopig, OV-09)* *Given* een ticket in Review met een delegatie die nog loopt, *When* ik het op Done zet, *Then* vraagt de client om een bevestiging. Pas na die bevestiging worden de lopende delegaties geannuleerd en gaat het ticket op Done.

#### MU1-17: Niets automatisch mergen, pushen of deployen (M)
**Als** board member **wil ik** dat Done niets aan de buitenwereld verandert, **zodat** merges, pushes en deploys altijd een bewuste beslissing blijven (RULE 07).

- **AC1.** *Given* een ticket dat op Done gaat, *When* het systeem het afsluit, *Then* voert het geen `git merge`, `git push`, PR-aanmaak of deploy uit, en start het daar geen run voor.
- **AC2.** *Given* een ticket in Done, *When* ik het ticket bekijk, *Then* zie ik eventuele branches of commits uit het rapport als informatie, zonder knop die automatisch merget.
- **AC3.** *Given* de bestaande RULE 07-goedkeuringen, *When* een employee tijdens Doing wil mergen of pushen, *Then* gelden dezelfde regels en goedkeuringen als nu.

#### MU1-18: Memory-samenvatting na Done (M)
**Als** board member **wil ik** dat een afgerond ticket automatisch een samenvatting in de project-memory zet, **zodat** de kennis uit het ticket bewaard blijft.

- **AC1.** *Given* een ticket dat op Done gaat, *When* het gesloten is, *Then* schrijft het systeem precies één samenvatting naar de **project-memory** van het ticketproject, op basis van de bestaande extractie na een task.
- **AC2.** *Given* de samenvatting, *When* ik ze bekijk, *Then* verwijst ze naar het ticket (id en titel).
- **AC3.** *Given* een ticket dat nooit op Done kwam, *When* het in een andere kolom blijft, *Then* is er geen ticketsamenvatting. *(De bestaande extractie per task blijft zoals ze is.)*
- **AC4.** *Given* een mislukte extractie (bijvoorbeeld door quotum), *When* die mislukt, *Then* blijft het ticket op Done en is de fout zichtbaar. Er kan opnieuw geprobeerd worden. *(Aanname.)*

### Epic E: Nieuwe versie en overgang

#### MU1-19: Schone werkruimte bij de nieuwe versie (M)
**Als** board member **wil ik** dat de nieuwe versie met een schone werkruimte start, **zodat** er geen migratiepad nodig is van oude taken en gesprekken naar tickets.

- **AC1.** *Given* een server met de vorige versie, *When* de nieuwe versie start, *Then* zijn er geen taken, gesprekken (taskgesprekken, gesprekken met employees, project-assistent) of Team chat-berichten uit de vorige versie zichtbaar.
- **AC2.** *Given* de upgrade, *When* die klaar is, *Then* zijn de employees (naam, rol, titel, instructies en `reports_to`), de projecten en de memory (company, project, employee, met de department-memory volgens MU1-03) wel overgenomen.
- **AC3.** *Given* de upgrade, *When* die start, *Then* wordt eerst een back-up van de oude database gemaakt, die na de upgrade nog bestaat. *(Aanname: het weggooien van taken en gesprekken is onomkeerbaar; zie RULE 07 en OV-11.)*
- **AC4.** *Given* de nieuwe versie, *When* ik het Board open, *Then* is het leeg en heeft het de standaardkolommen.
- **AC5.** *Given* de Team chat, *When* ik die na de upgrade open, *Then* is die leeg en werkt hij.

### Epic F: Onboarding van de eerste board member

> **Grens Fase 1 / Fase 3.** Fase 1 bevat alleen wat nodig is om `created_by_user` betrouwbaar te vullen: een tabel `users`, het aanmaken van een board member en een client die weet "wie ben ik". De echte **login** (sessies, het controleren van het wachtwoord bij elke verbinding, een apparaat koppelen aan een user, auteurschap van elk bericht en een Claude-connectie per user) is **Fase 3**. Zie de tabel na MU1-22.

#### MU1-20: Eerste board member verplicht aanmaken (M)
**Als** eerste persoon die een client met een nieuwe server verbindt **wil ik** eerst een board member aanmaken, **zodat** elk ticket een aanmaker heeft.

- **AC1.** *Given* een server zonder board members, *When* ik een client (desktop of mobiel) open, *Then* zie ik alleen het scherm "Board member aanmaken" (gebruikersnaam en wachtwoord). Het Board, de Team chat en de andere schermen zijn niet bereikbaar.
- **AC2.** *Given* dat scherm, *When* ik een gebruikersnaam en een wachtwoord invul en bevestig, *Then* wordt de board member aangemaakt, met het wachtwoord gehasht met **argon2** (nooit als platte tekst opgeslagen of gelogd), en kom ik in de app.
- **AC3.** *Given* een lege gebruikersnaam, een gebruikersnaam die al bestaat of een wachtwoord dat niet aan de minimale eisen voldoet (OV-12), *When* ik bevestig, *Then* wordt er niets aangemaakt en zie ik de reden.
- **AC4.** *Given* dat twee clients tegelijk een eerste board member aanmaken, *When* beide bevestigen, *Then* lukken beide alleen als de gebruikersnamen verschillen. Daarna is de onboarding voor alle clients voorbij.
- **AC5.** *Given* dat er al minstens één board member bestaat, *When* een nieuwe client verbindt, *Then* ziet die het verplichte aanmaakscherm niet meer en gaat hij door naar MU1-21.

#### MU1-21: Actieve board member in een client (Fase 1-minimum) (M)
**Als** board member **wil ik** dat mijn client weet wie ik ben, **zodat** mijn tickets en acties mijn naam als aanmaker krijgen.

- **AC1.** *Given* een client die net een board member aanmaakte, *When* ik daarna een ticket aanmaak, *Then* is `created_by_user` die board member.
- **AC2.** *Given* een client op een server met board members, *When* de client nog geen actieve board member heeft, *Then* kies ik een bestaande board member (of maak ik er een aan) voordat ik verder kan. De client onthoudt die keuze. *(Fase 1: dit is een **identificatie**, geen authenticatie. Of het wachtwoord hier al gecontroleerd wordt, staat in OV-13.)*
- **AC3.** *Given* een actieve board member, *When* ik een ticket start, goedkeur, terugstuur of op Done zet, *Then* wordt die board member als uitvoerder vastgelegd (MU1-14 AC3, MU1-15 AC4).
- **AC4.** *Given* de client, *When* ik de actieve board member bekijk, *Then* zie ik zijn naam (bijvoorbeeld in de header of de instellingen).

#### MU1-22: Board member maakt een andere board member aan (S)
**Als** board member **wil ik** nieuwe board members aanmaken, **zodat** collega's hun eigen tickets kunnen maken.

- **AC1.** *Given* een actieve board member, *When* die een nieuwe board member aanmaakt met een gebruikersnaam en een wachtwoord, *Then* bestaat die board member en kan hij als aanmaker van tickets dienen.
- **AC2.** *Given* een gebruikersnaam die al bestaat, *When* ik die opnieuw probeer aan te maken, *Then* wordt dat geweigerd.

> Prioriteit S: de bron zegt "Daarna kan elke board member nieuwe board members aanmaken", maar legt niet vast of dat in Fase 1 of Fase 3 komt. Zonder deze story kan je in Fase 1 alleen via het eerste aanmaakscherm board members toevoegen.

**Scheiding onboarding en login**

| Onderdeel | Fase 1 | Fase 3 |
|---|---|---|
| Tabel `users` (gebruikersnaam, argon2-hash, company) | ✅ | — |
| Verplicht aanmaakscherm zolang er geen board member is, in elke client | ✅ (MU1-20) | — |
| `created_by_user` NOT NULL op tickets, uitvoerder bij kolomwijzigingen en goedkeuringen | ✅ (MU1-07, MU1-15, MU1-21) | — |
| Actieve board member per client kiezen of onthouden (identificatie) | ✅ (MU1-21) | vervangen door een echte login |
| Andere board members aanmaken | S (MU1-22) | ✅ als het niet in Fase 1 zit |
| Inloggen met wachtwoordcontrole, sessies, uitloggen | — | ✅ |
| Een apparaat (`remote_clients`) koppelen aan een user | — | ✅ |
| Auteurschap van elke actie en elk bericht (Team chat, gesprekken) | — | ✅ |
| Claude-connectie per board member, runs op de login van de aanmaker, `quota` zonder fallback | — | ✅ (na de spike) |
| Wachtwoord wijzigen of resetten, board member verwijderen | — | ✅ (open, OV-14) |

---

## 4. Traceerbaarheid (beslissing → story)

| # | Beslissing in `multi-user.md` | Story-ID's |
|---|---|---|
| T-01 | §1, §2 "Toegang": één company per server, expliciete `company`-tabel, later uitbreidbaar | MU1-01 |
| T-02 | §1: afdeling vervalt, het organigram loopt enkel via `reports_to`; `departments` en `department_id` worden verwijderd | MU1-02 |
| T-03 | §2 "Memory": department-memory gaat bij de overgang naar company-memory | MU1-03 |
| T-04 | §1: "Department" verdwijnt uit prompts en UI | MU1-04 |
| T-05 | §2 "Board": kolommen op company-niveau, standaard Backlog · Todo · Doing · Plan review · Review · Done | MU1-05, MU1-06 |
| T-06 | §1, §2: ticket boven op tasks (hoofdtask + delegaties) | MU1-08 |
| T-07 | §2: ticketvelden titel, beschrijving, project, kolom, prioriteit, aanmaker, eigenaar, modus en model, labels, bijlagen, deadline, gekoppelde tasks | MU1-07, MU1-08 (de discussie: Fase 2) |
| T-08 | §4 Fase 1: het veld `created_by_user` zit er meteen in | MU1-07, MU1-21 |
| T-09 | §2: start pas na Start of slepen naar Todo, geen automatische pickup uit Backlog | MU1-09 |
| T-10 | §2: in Todo kiest de dispatcher, standaard Orion, een employee | MU1-10 |
| T-11 | §2: de dispatcher is per project in te stellen | MU1-11 |
| T-12 | §2: een board member kan zelf meteen een employee kiezen | MU1-12 |
| T-13 | §2: Plan-modus stopt in Plan review; zonder Plan-modus wordt die kolom overgeslagen | MU1-13 |
| T-14 | §2: na goedkeuring gaat het ticket naar Doing voor de uitvoering | MU1-14 |
| T-15 | §2: employees verplaatsen tot en met Review | MU1-15 |
| T-16 | §2: alleen board members zetten op Done, of terug naar Doing met feedback | MU1-16 |
| T-17 | §2: Done sluit het ticket, niets automatisch mergen, pushen of deployen (RULE 07) | MU1-16, MU1-17 |
| T-18 | §2 "Memory": na Done een automatische samenvatting naar de project-memory | MU1-18 |
| T-19 | §3: nieuwe versie, geen overname van taken, gesprekken en Team chat; een schone werkruimte | MU1-19 |
| T-20 | §3: wel overgenomen: employees (instructies, `reports_to`), projecten, memory; afdelingen verwijderd | MU1-19, MU1-02, MU1-03 |
| T-21 | §3: onboarding: zolang er geen board members zijn, eerst een board member aanmaken in elke client | MU1-20, MU1-21 |
| T-22 | §3: daarna kan elke board member nieuwe board members aanmaken | MU1-22 (S, OV-15) |
| T-23 | §2 "Toegang": wachtwoord gehasht met argon2 | MU1-20 AC2 |
| T-24 | §2 "Toegang": één rol, iedereen mag alles | Alle stories (geen rolcontrole tussen board members) |
| T-25 | Verificatie: het volledige pad Backlog → Todo → Doing → (Plan review) → Review → Done, met delegaties die ongewijzigd werken | MU1-08 AC3, MU1-09, MU1-13 t/m MU1-16 |

---

## 5. Buiten scope van Fase 1

**Fase 2: Ticketdiscussie en kanalen in de Team chat**
- Het conversatietype `ticket`, één gesprek per ticket, met board members en employees.
- Delegaties, rapporten van subtasks en kolomwijzigingen als berichten in de discussie.
- @vermeldingen in de ticketdiscussie die een employee wekken (MAX_HOPS en limieten op wekkende posts).
- De goedkeuring in Plan review en de feedback "terug naar Doing" via de discussie (in Fase 1 via een knop en een tekstveld, A-03).
- De Team chat met een algemeen kanaal en één kanaal per project.
- Een ticket aanmaken vanuit een chat.

**Fase 3: Multi-user**
- Een echte login (wachtwoordcontrole, sessies), apparaten per user, auteurschap van elke actie en elk bericht.
- Een Claude-connectie per board member, ticketruns (en delegaties) op de login van de aanmaker, `quota` zonder stille fallback, runs zonder ticket op de login van de afzender. Dit hangt af van de spike (`spike-claude-logins.md`).
- Zie ook de tabel na MU1-22.

**Fase 4: Vault**
- De vault op company-niveau en per project, items van het type tekst, url of secret, versleuteld, met een auditlog. Secrets nooit in prompts, memory of discussies.

**Niet in deze productdefinitie (niet bouwen)**
- Meerdere companies per server (alleen het schema laat het toe).
- Meerdere rollen of rechten per board member.
- Automatische merges, pushes of deploys bij Done.

---

## 6. Open vragen (escalatie naar Orion)

| ID | Vraag | Waarom | Voorstel (aanname) |
|---|---|---|---|
| **OV-01** | De bron laat de goedkeuring van Plan review en de feedback "terug naar Doing" **in de discussie** gebeuren, maar de ticketdiscussie komt pas in Fase 2. Hoe gebeurt dat in Fase 1? | Tegenstrijdigheid in de volgorde (§2 tegenover §4) | A-03: een knop en een verplicht tekstveld op het ticket. De tekst gaat naar het gesprek van de hoofdtask. In Fase 2 verhuist dat naar de discussie |
| **OV-02** | Wat betekent "op company-niveau instellen" precies: hernoemen, volgorde, toevoegen, verwijderen? Mogen Todo, Plan review, Review en Done verwijderd worden, terwijl de workflow erop steunt? | Niet vastgelegd | A-02: kolommen met een functie zijn vast (alleen te hernoemen en te verschuiven), extra kolommen kunnen erbij |
| **OV-03** | Is er één Board per company met een projectfilter, of één Board per project? | §1 hangt tickets onder een project; §2 stelt de kolommen in op company-niveau | Eén Board per company, te filteren per project |
| **OV-04** | Wat gebeurt er als de dispatcher (Orion of de projectdispatcher) niet bestaat of gearchiveerd is? | Orion is een gewone employee, geen systeemrol | Terugvallen op Orion. Bestaat die ook niet, dan blijft het ticket in Todo met een melding "kies een eigenaar" |
| **OV-05** | Mogen employees zelf tickets aanmaken of starten (bijvoorbeeld voor vervolgwerk)? `created_by_user` moet een board member zijn | Niet vastgelegd; idee "vanuit een chat een ticket" is Fase 2 | Nee in Fase 1. Employees gebruiken `add_idea` of delegaties |
| **OV-06** | Welke prioriteitsschaal hebben tickets (bijvoorbeeld Laag/Normaal/Hoog/Urgent)? Tasks hebben nu een geheel getal | Niet vastgelegd; Pixel heeft het nodig | Laag · Normaal · Hoog · Urgent, standaard Normaal |
| **OV-07** | In welke modus gaat een ticket na de plangoedkeuring naar Doing? De bron zegt "naar Doing voor de uitvoering", maar niet in welke modus | Plan voert niets uit | De board member kiest de uitvoeringsmodus bij het goedkeuren (standaard die van het project). Het ticketveld Modus blijft dan Plan als historiek, of wordt aangepast; dat beslist Titan samen met Pixel |
| **OV-08** | Kan een ticket op Done heropend worden? | Niet vastgelegd; tasks kunnen niet heropend worden (een nieuwe gekoppelde task) | Nee in Fase 1. Voor vervolgwerk maak je een nieuw ticket |
| **OV-09** | Wat gebeurt er met delegaties die nog lopen als een board member het ticket op Done zet? En wat als de hoofdtask `failed` of `cancelled` wordt: in welke kolom staat het ticket dan? | Niet vastgelegd | Done kan pas als er geen taak onder het ticket meer loopt (of na een expliciete bevestiging die ze annuleert). Bij `failed` blijft het ticket in Doing met een zichtbare fout en de knop "Opnieuw" |
| **OV-10** | Moet Fase 1 zowel de desktop als mobiel dekken, of eerst één client? | §4 noemt de clients alleen voor Fase 2 | Beide (A-04), omdat de onboarding "in elke client" moet werken |
| **OV-11** | Betekent "schone werkruimte" een nieuwe database met import van employees, projecten en memory, of een migratie die taken en gesprekken verwijdert? Wat gebeurt er met notities, artifacts, ideeën, schedules, plugins en instructievoorstellen van projecten? En mag de oude data in een back-up bewaard blijven? | Niet vastgelegd; onomkeerbaar (RULE 07) | Een back-up van de oude database, dan een migratie. Wat niet tot taken, gesprekken of de Team chat behoort, blijft. Titan werkt dit uit in de impactanalyse, Sébastien beslist |
| **OV-12** | Welke minimale eisen gelden voor het wachtwoord en de gebruikersnaam? | Niet vastgelegd | Een gebruikersnaam van 3–32 tekens (letters, cijfers, `.`, `_`, `-`), hoofdletterongevoelig uniek. Een wachtwoord van minstens 12 tekens. Sentinel valideert |
| **OV-13** | Controleert de client in Fase 1 al het wachtwoord als je een bestaande board member kiest, of is dat helemaal Fase 3? | De opdracht zegt "echte login in Fase 3"; zonder controle kan iedereen in de tailnet zich voor een andere board member uitgeven | Fase 1: kiezen zonder sessie, met een eenmalige wachtwoordcontrole bij het kiezen (goedkoop, omdat de hash er al is). Sessies en apparaatkoppeling in Fase 3 |
| **OV-14** | Kan een board member verwijderd of gedeactiveerd worden, en kan een wachtwoord gereset worden? Wat gebeurt er met zijn tickets (`created_by_user`)? | Niet vastgelegd | Fase 3. Tickets houden hun aanmaker (deactiveren, niet verwijderen) |
| **OV-15** | Komt "board members aanmaken door een board member" in Fase 1 of in Fase 3? | §3 legt het vast, §4 noemt het in geen enkele fase | In Fase 1 als Should (MU1-22) |
| **OV-16** | Kunnen er in Fase 1 nog tasks bestaan zonder ticket (bijvoorbeeld uit de Team chat, de project-assistent of een terugkerende schedule)? | §2 noemt "runs zonder ticket", maar het is niet duidelijk of die tasks aanmaken | Ja: tasks zonder ticket blijven bestaan en verschijnen niet op het Board. Alleen een ticket start via het Board |
| **OV-17** | Krijgen employees een tool om een ticket te verplaatsen (bijvoorbeeld terug naar Todo), of loopt dat alleen via de status van hun task? | MU1-15 AC3 laat het open | Fase 1: alleen via de taskstatus (Doing → Review). Een tool komt eventueel in Fase 2 |

---

## 7. Afhankelijkheden

- **Pixel (AUR-MU-00-P):** het Board en de ticketweergave. Gebruikt de terminologie uit §0, de velden uit MU1-07, de knoppen Start, Goedkeuren, "Terug naar Doing" (met verplichte feedback) en Done, en het onboardingscherm uit MU1-20/21.
- **Titan (AUR-MU-00-T2):** de impactanalyse. Bevestigt A-01 (de mapping van kolom naar taskstatus), de migratie voor MU1-02/03/19 en OV-11.
- **Titan (AUR-MU-00-T1):** de spike. Raakt Fase 1 niet, maar bepaalt Fase 3.
