# Analyse: van Aurelius naar een eigen Linear

- **Ticket:** #6 · **Auteur:** Nova (Business Analyst) · **Datum:** 2026-11-02
- **Basis:** code op `main` @ `53cb385` (domain, protocol, storage, clients) en `docs/product/`, plus de documentatie van Linear.
- **Status:** voorstel. V1 en V2 zijn beslist door Sebastien (hoofdstuk 7); de rest is niet beslist en niets is gebouwd. De bevindingen komen uit het lezen van de code; ik heb de software niet gedraaid.

## 1. De conclusie

**Aurelius is vandaag een werkplaats waar mensen werk uitbesteden aan AI-employees. Linear is een plek waar mensen samen werk plannen en volgen.** De technische basis is sterk genoeg voor die stap. Het datamodel en de workflow zijn er nog niet op gebouwd.

Het verschil zit in één punt: **een ticket is nu een werkopdracht aan een AI, geen werkitem van een team.**

- Een ticket heeft geen menselijke verantwoordelijke. Er is alleen een aanmaker en een employee als eigenaar (`crates/domain/src/ticket.rs:132-138`).
- Naar Todo slepen start altijd de dispatcher en een run. Andere verplaatsingen volgen het werk en worden geweigerd (`ticket.rs:288-291`). Dat blijft zo: AI is de enige uitvoerder (V2).
- Meldingen gaan over runs (goedkeuring, mislukt, resultaat), niet over tickets of personen (`crates/storage/src/timeline.rs:306-337`).

**Mijn aanbeveling:** bouw niet eerst cycles of roadmaps. Maak eerst de samenwerking per persoon goed (hoofdstuk 5, fase A): een menselijke verantwoordelijke die het werk van de AI opvolgt, een persoonlijke Inbox en "My issues". Zonder die drie blijft de rest een bord dat één iemand bedient.

**Waarom dit de moeite waard is:** Linear behandelt agents als een toevoeging aan menselijk werk. Aurelius heeft al een organigram, delegatie, plan review, een testpoort, guidelines, memory en een vault per run. Dat is een echt onderscheid, en het blijft overeind als de menselijke kant erbij komt.

## 2. Hoe Aurelius nu werkt

| Laag | Wat er is |
|---|---|
| Server | Eén daemon met SQLite op een eigen pc, bereikbaar via Tailscale. Eén company per server. |
| Clients | Desktop (Rust/GPUI) en mobiel (Flutter). Geen webclient. Daarnaast `aurelius-mcp` en app-keys voor externe apps. |
| Mensen | Board members met login, sessies per apparaat en een eigen Claude-login. Eén rol: iedereen mag alles. |
| AI | Employees met `reports_to`, instructies, memory, guidelines, plugins en vault-toegang. |
| Werk | Ticket → hoofdtask → delegaties, elk met een eigen gesprek en runs. |
| Bord | Zeven vaste kolommen voor de hele company: Backlog, Todo, Doing, Plan review, Testing, Review, Done. Alleen naam en volgorde zijn aan te passen. |
| Praten | Team chat: algemeen kanaal, per project, per ticket. Threads, reacties, @mentions, ongelezen per persoon. |
| Overig | Ideas, notes, terugkerende taken, usage en trace, artifacts, terminal en diff per gesprek. |

**Wat al goed staat voor samenwerking:**
- Alle writes lopen via één pipeline met idempotency-sleutel, receipt en event. Clients kunnen events vanaf een `seq` terugspelen en live volgen. Dit is de juiste basis voor een activity feed en realtime bijwerken.
- Tickets hebben optimistische versies (`expected_version`), dus gelijktijdige bewerkingen overschrijven elkaar niet stil.
- Discussies hebben per persoon een leesstand en een mention-telling (`migrations/0020_discussion_reads.sql`).
- De toetsenbordbediening en het commandopalet (Ctrl+K) zitten al in de desktop, en dat is de kern van hoe Linear aanvoelt.
- De review blijft bij mensen: alleen een board member zet een ticket op Done, en er wordt niets automatisch gemerged.

## 3. Vergelijking met Linear

Legenda: ✅ aanwezig · 🟡 deels · ❌ niet gevonden in de code.

| Onderwerp | Linear | Aurelius | |
|---|---|---|---|
| Teams | Primaire eenheid; elk team heeft eigen workflow, triage en cycles | Geen. Het Aurelius-project lijkt hier het meest op (map, kanaal, vault, memory, dispatcher) | ❌ |
| Projecten | Groep issues rond een uitkomst, met milestones | Geen tegenhanger. Een Aurelius-project heeft geen doel, datum of lead | ❌ |
| Initiatieven | Groepeert projecten onder een strategisch doel | Geen | ❌ |
| Cycles | Tijdvakken per team met rollover en capaciteit | Geen | ❌ |
| Status per team | Eigen geordende statussen per team | Zeven vaste kolommen voor de hele company | 🟡 |
| Menselijke assignee | Mens is assignee; een agent is gedelegeerde en de mens blijft verantwoordelijk | Alleen employee-eigenaar | ❌ |
| Agents | Assigneren of @mention; guidance per workspace en team | Veel rijker: organigram, delegatie, plan review, testpoort | ✅ |
| Triage | Inbox met accept, decline, duplicate, snooze en regels | Ideas komen het dichtst bij, maar worden een task en geen ticket (`crates/domain/src/idea.rs:119`) | 🟡 |
| Inbox | Persoonlijk: abonnementen, snooze, reminders, eigen leesstand | Geen ticket-events of mentions; leesstand gedeeld (zie 4.2) | 🟡 |
| Views | Opgeslagen, gedeelde lijsten en borden met filters, ook via URL | Niet-opgeslagen filters in de client (`clients/desktop/src/screens/board.rs:473`) | 🟡 |
| Relaties, sub-issues | Blocks, related, duplicate, parent/child | Geen. Delegaties zijn interne tasks | ❌ |
| Labels | Beheerde labels | Vrije tekst per ticket, zonder beheer of hernoemen | 🟡 |
| Schatting | Punten, gebruikt voor capaciteit | Geen | ❌ |
| Git | Koppeling met branches en PR's | Branch staat alleen als tekst in het rapport (`crates/application/src/task/testing.rs:78`) | 🟡 |
| Zoeken | Over het hele werk | Titel, beschrijving en nummer. FTS bestaat alleen voor memory | 🟡 |
| Bereikbaarheid | Web, desktop, mobiel, deelbare URL's | Desktop en mobiel, geen web, geen deep links | 🟡 |
| Rollen | Admins en members; agents kunnen niets beheren | Eén rol | ❌ |
| Inzichten | Grafieken en snapshots per cycle | Alleen usage en kosten | 🟡 |

## 4. Bevindingen voor samenwerking

### 4.1 De kernkloof: mensen komen niet voor op het werk
- **Geen assignee.** Niemand kan zien wat van wie is. De aanmaker is niet hetzelfde: die blijft aanmaker en de runs lopen standaard op diens Claude-login, ook als een ander het ticket opvolgt (`ticket.rs:160-163`).
- **Geen menselijk werkpad: bewust.** Elk ticket dat start, gaat naar een employee. Handwerk (een klantgesprek, een ontwerp) hoort volgens V2 niet op het bord; de menselijke rol is opdracht geven, beslissen en reviewen.
- **Een agent is geen volwaardige deelnemer.** Employees kunnen geen ticket maken of aanpassen: alleen `assign_ticket`, `post_to_discussion` en voor de tester `report_test_result` raken een ticket (`crates/agent-bridge/src/mcp.rs`). Wat ze ontdekken, gaat naar Ideas of naar een task. Ook `aurelius-mcp` kent alleen `create_task`.

### 4.2 Meldingen werken niet per persoon
- 🐞 **Waarschijnlijke bug, los op te lossen:** `mark_all_read_for` zet `read_at_ms` op de gedeelde rij van notificaties "voor iedereen" (`crates/storage/src/timeline.rs:498-500`). Als één board member alles gelezen markeert, is het volgens de SQL voor iedereen gelezen. Dit is uit de code afgeleid en nog niet gereproduceerd.
- **Geen melding bij wat er voor een team toe doet.** `notification_for` kent geen enkel `ticket.*`-event. Een ticket dat in Review of Plan review komt, een terugsturing en een @mention van een board member geven dus geen Inbox-rij en dus geen push. Een mention is alleen een badge in de kanalenlijst (`crates/storage/src/discussions.rs:341`).
- **Geen abonnementen.** `channel_members` bestaat, maar er is geen "volgen" of "dempen" per ticket, en geen snooze of reminder. Een deadline is alleen een veld: ik vond geen herinnering.

### 4.3 Het bord schaalt niet mee met meerdere projecten of mensen
- De workflow is voor de hele company dezelfde. Twee projecten kunnen niet verschillend werken.
- Filters staan in de client en zijn niet opslaan- of deelbaar. Ik vond geen lijstweergave, bulkacties of templates.
- Geen status **Cancelled**: terug naar Backlog betekent annuleren, waardoor "nog niet gepland" en "afgevoerd" door elkaar lopen.
- Ticketnummers lopen per company (`#12`). Linear-achtige sleutels (`AUR-12`) maken verwijzen en git-koppeling makkelijker.

### 4.4 Discussie en kennis
- Berichten zijn niet te bewerken of te verwijderen; alleen een bericht in de wachtrij kan weg. Voor een team dat samen schrijft, is dat een gemis.
- Zoeken over discussies ontbreekt, dus besluiten in een thread zijn later moeilijk terug te vinden.
- Ticketbeschrijvingen worden als geheel overschreven, zonder versiegeschiedenis of samen bewerken. Een versieconflict wordt wel opgevangen.

### 4.5 Toegang en vertrouwen
Dit is prima voor een klein vertrouwd team, maar niet voor iemand van buiten:
- Eén rol: elke board member kan wachtwoorden van anderen resetten, sessies intrekken en secrets inzien. Dat laatste wordt wel gelogd.
- Alle runs draaien als dezelfde OS-user. Logins van anderen zijn afgeschermd met deny-regels; echte sandbox-isolatie staat nog als follow-up op de lijst.
- Iedereen kan zijn eigen ticket accepteren: er is geen vier-ogen-regel.
- Nieuwe collega's hebben een Tailscale-toegang en een door iemand doorgegeven wachtwoord nodig. Er is geen uitnodiging.

### 4.6 Beschikbaarheid
De server draait op één pc. Staat die uit, dan staat het hele team stil, en niet alleen de AI. Voor een Linear-vervanger is een altijd-aan server waarschijnlijk nodig. Er is ook geen offline modus in de clients: ze zijn een venster op de server (`docs/mobile.md`).

### 4.7 Kosten
Runs lopen op de Claude-login van de aanmaker, of van wie het overneemt met `ticket.use_my_login`. Als het quotum op is, wacht de run. In een team moet dus zichtbaar zijn wie voor welk werk betaalt. Usage per ticket en per persoon is de ontbrekende stap, en die data is er al grotendeels.

## 5. Voorgestelde roadmap

De grootte is een eerste inschatting van mij (S/M/L). Titan moet de echte schatting en impactanalyse maken.

### Fase A: samenwerken per persoon (eerst)
| # | Wat | Grootte |
|---|---|---|
| A0 | Bug: leesstand per persoon (4.2) | S |
| A1 | Menselijke verantwoordelijke, los van de employee-eigenaar (volgt op en reviewt, voert niet uit) | M |
| A2 | Volgen van tickets, automatisch bij aanmaken, toewijzen en mention | M |
| A3 | Persoonlijke Inbox: ticket-events en mentions, ook als push | M |
| A4 | "My issues": wat van mij is, wat ik volg, wat op mij wacht | S |
| A5 | Deep links naar ticket en bericht | S |
| A6 | Berichten bewerken en verwijderen, met zichtbare historie | S |

### Fase B: een echte issue tracker
| # | Wat | Grootte |
|---|---|---|
| B1 | Status Cancelled (het handmatige werkpad vervalt door V2) | S |
| B2 | Triage: Ideas en binnenkomende tickets met accept, decline, duplicate, snooze | M |
| B3 | Employees mogen tickets maken en bijwerken, ook via `aurelius-mcp` | M |
| B4 | Relaties: blocks, related, duplicate, parent/child | M |
| B5 | Lijstweergave, serverfilters, opgeslagen en gedeelde views | L |
| B6 | Beheerde labels, templates, bulkacties | M |
| B7 | Volledig zoeken over tickets en discussies | M |
| B8 | Git: branch en PR als veld op het ticket, ticketsleutel in branchnaam | M |

### Fase C: plannen
| # | Wat | Grootte |
|---|---|---|
| C1 | Outcome-projecten met doel, lead, datum, milestones en voortgang | L |
| C2 | Cycles met rollover. Eerst nagaan of dit nog nut heeft: met AI als uitvoerder is de capaciteit het quotum, niet mensuren | L |
| C3 | Roadmap of tijdlijn | M |
| C4 | Inzichten, inclusief iets wat Linear niet heeft: AI-kosten en doorlooptijd per ticket | M |

### Fase D: platform (afhankelijk van de besluiten in hoofdstuk 7)
| # | Wat | Grootte |
|---|---|---|
| D1 | Webclient | L |
| D2 | Teams met eigen workflow | L |
| D3 | Sandbox-isolatie per run (rollen en uitnodigingen vervallen voorlopig door V1) | M |
| D4 | Altijd-aan hosting | M |
| D5 | Ticket-webhooks (nu vier events, geen voor tickets: `crates/domain/src/webhook.rs:59`), publieke API, import uit Linear | M |

## 6. User stories voor fase A

Dit zijn voorstellen; de aannames staan in hoofdstuk 7.

**US-A0 Eigen leesstand.** Als board member wil ik dat mijn Inbox alleen voor mij gelezen wordt.
- Als A alles gelezen markeert, blijft dezelfde melding voor B ongelezen.
- Ongelezen-tellers op desktop en mobiel volgen dit.
- `dismiss` en `dismiss_all` werken ook alleen voor de persoon zelf.

**US-A1 Verantwoordelijke.** Als board member wil ik per ticket één verantwoordelijke persoon, zodat duidelijk is wie het werk van de AI opvolgt en reviewt.
- Een ticket heeft nul of één verantwoordelijke (in de tekst hieronder: assignee), los van de employee-eigenaar en de aanmaker. Die persoon voert het ticket niet uit.
- Starten vanaf Todo blijft een run bij een employee starten; de assignee verandert daar niets aan.
- Elke board member kan dit instellen, wijzigen en wissen, op elk moment behalve op Done.
- De wijziging staat in de activity met wie en wanneer.
- Het bord toont de assignee en kan erop filteren.
- De assignee verandert niets aan welke Claude-login de run gebruikt.
- Een uitgeschakelde board member blijft zichtbaar op diens tickets en is niet meer te kiezen.

**US-A2 Volgen.** Als board member wil ik tickets volgen, zodat ik alleen updates krijg over wat mij aangaat.
- Ik volg automatisch als ik het ticket maak, assignee word of in de beschrijving of discussie genoemd word.
- Ik kan handmatig volgen en ontvolgen; na ontvolgen volgt pas weer een automatisch volgen bij een nieuwe mention of toewijzing.
- Iemand die alleen het kanaal leest, volgt niet automatisch.

**US-A3 Persoonlijke Inbox.** Als volger wil ik een melding bij wat mijn aandacht nodig heeft.
- Melding bij: toewijzing aan mij, @mention van mij, ticket in Plan review of Review, terugsturen, ticket mislukt of wacht op mijn login.
- Een eigen actie geeft geen melding aan mezelf.
- Een klik opent het ticket op de juiste plek.
- Push volgt dezelfde regels op mobiel.
- Herhaalde updates op één ticket worden samengevoegd tot één rij (aanname).
- De melding bevat geen secretwaarde: de tekst gaat door de vault-redactor.

**US-A4 My issues.** Als board member wil ik één overzicht van mijn werk.
- Tabs: *Toegewezen*, *Gemaakt door mij*, *Ik volg*, *Wacht op mij* (Plan review en Review van mijn tickets).
- Sortering op prioriteit en daarna op laatste update; Done staat standaard uit.
- Het overzicht is op desktop en mobiel gelijk en werkt ook met filter per project.

**US-A5 Deep links.** Als board member wil ik een ticket of bericht kunnen delen met een link.
- "Kopieer link" op ticket en bericht.
- De link opent de juiste plek in de client, ook na inloggen.
- Zonder rechten of bij een onbekend ticket toont de client een duidelijke melding.
- De link bevat geen sessietoken.

**US-A6 Berichten bewerken.** Als auteur wil ik mijn bericht kunnen corrigeren.
- Alleen de auteur kan bewerken of verwijderen.
- Een bewerkt bericht toont "bewerkt"; een verwijderd bericht laat een regel achter en de vorige tekst blijft in de audit.
- Een bewerking wekt geen employees opnieuw en start geen run. Een nieuwe mention in een bewerking wekt ook niemand (aanname, vanwege de lusgrenzen van RULE 08).
- Berichten van employees en system lines zijn niet te bewerken.

**Randgevallen voor Titan en Pixel:**
- Oudere clients moeten blijven werken: nieuwe velden optioneel, nieuwe events achter een feature (protocol 5).
- Een mention van een employee wekt die employee en geeft geen Inbox-melding aan die employee. Een mention van een board member wekt niemand.
- Wat met volgers van een gearchiveerd of Done ticket? Voorstel: geen meldingen meer, behalve bij heropenen.

## 7. Open beslissingen

| # | Vraag | Waarom het uitmaakt | Mijn aanname |
|---|---|---|---|
| **V1** | Voor wie is dit: eigen team, of ooit een product voor anderen? | Bepaalt rollen, hosting, multi-tenant en beveiliging (fase D). | **Beslist (Sebastien, 2026-11-02): eigen team.** Multi-tenant, rollen en uitnodigingen vallen buiten scope |
| **V2** | Moeten mensen ook zelf tickets uitvoeren, of blijft AI altijd de uitvoerder? | Bepaalt of een handmatig werkpad nodig is en hoe zwaar de workflow verandert. | **Beslist (Sebastien, 2026-11-02): alleen AI voert uit.** Geen handmatig werkpad; de ticket-engine blijft zoals ze is |
| V3 | Is een Aurelius-project een Linear-team? | Dan komt er een nieuw begrip voor outcome-projecten. Het is een naamkeuze met gevolgen voor elke UI. | Ja; het nieuwe begrip krijgt een andere naam, zoals "Epic" |
| V4 | Is een webclient nodig? | Grootste los bouwblok. Zonder web geen deelbare links buiten de apps. | Niet voor fase A–C |
| V5 | Draait de server altijd-aan? | Nu stopt alles als de pc uit is. | Voor fase A niet nodig, daarna wel |
| V6 | Vier-ogen-regel bij Review? | Wijkt af van "iedereen mag alles" in `docs/product/multi-user.md`. | Instelbaar per project, standaard uit |

Fase A kan zonder de overige besluiten beginnen.

## 8. Risico's
- **Twee modellen door elkaar.** Assignee, employee-eigenaar, aanmaker en login-user zijn vier "wie"-velden op één ticket. Zonder duidelijk ontwerp (Pixel) wordt dat verwarrend.
- **Meldingsmoeheid.** Agents produceren veel events. Alleen de lijst in US-A3 geeft een melding; run-updates niet.
- **Scope.** Een volledige Linear is groot. De winst zit niet in functies overnemen maar in een bord waarop mensen beslissen en AI uitvoert.
- **Protocol en migraties.** Alles kan additief binnen protocol 5.

## Bronnen
De Linear-kolommen voor model, triage, cycles, views, inbox en agents komen uit de documentatie hieronder. Relaties, labels, git en rollen komen uit mijn algemene productkennis en heb ik niet nagekeken.

- [Conceptual model](https://linear.app/docs/conceptual-model)
- [AI Agents](https://linear.app/docs/agents-in-linear)
- [Assign and delegate issues](https://linear.app/docs/assigning-issues)
- [Triage](https://linear.app/docs/triage)
- [Cycles](https://linear.app/docs/use-cycles)
- [Custom views](https://linear.app/docs/custom-views)
- [Inbox](https://linear.app/docs/inbox)
