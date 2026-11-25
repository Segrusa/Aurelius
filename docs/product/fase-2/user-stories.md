> **Historical:** the Team chat, #general and the project channels were removed in migration 0045; this document describes Fase 2 as designed before that.

# User stories Fase 2: Ticketdiscussie en kanalen in de Team chat

- **Task:** AUR-MU-DEF-N (Nova, voor Orion)
- **Bron:** `docs/product/multi-user.md` §2 "Ticketdiscussie", "Team chat: kanalen", "Memory"; `docs/product/fase-1/README.md` (B1–B10 goedgekeurd); `docs/product/fase-1/user-stories.md`.
- **Status:** concept, wacht op review door Orion en akkoord van Sébastien.
- **Regel:** wat de bron niet vastlegt, staat als **aanname** (A-xx) of **open vraag** (OV-xx). Terminologie zoals in Fase 1 §0.

## Aannames

- **A-01: auteur vóór Fase 3.** Een bericht van een mens krijgt als auteur de actieve board member van de client (Fase 1, MU1-21). Een echte login komt in Fase 3.
- **A-02: Claude-login.** Alle runs gebruiken in Fase 2 nog de ene login van de server (Fase 1, A-05).
- **A-03: lusgrenzen.** Er komen geen nieuwe constanten. De ticketdiscussie en de kanalen gebruiken dezelfde `MAX_HOPS = 3`, `WAKING_POSTS_PER_RUN = 2` en `WAKING_POSTS_PER_WINDOW = 4` per `WAKE_WINDOW_MS` (1 uur) als de huidige Team chat, met één budget per employee over alle kanalen en tickets samen (OV-02).
- **A-04: clients.** Alles geldt voor desktop én mobiel (§4 Fase 2).

## User stories

Prioriteit: **M** = Must, **S** = Should, **C** = Could.

### Epic A: Ticketdiscussie

#### MU2-01: Eén discussie per ticket (M)
**Als** board member **wil ik** bij elk ticket één discussie, **zodat** alles over dat ticket op één plek staat.

- **AC1.** *Given* een nieuw ticket, *When* het wordt aangemaakt, *Then* bestaat er precies één conversatie van het type `ticket`, gekoppeld aan dat ticket.
- **AC2.** *Given* de ticketweergave (desktop en mobiel), *When* ik die open, *Then* zie ik de discussie met de berichten in volgorde, en komen nieuwe berichten live binnen.
- **AC3.** *Given* een ticket op Done, *When* ik de discussie open, *Then* kan ik ze lezen maar niet meer schrijven (OV-01).
- **AC4.** *Given* de lijst met gesprekken en de Team chat, *When* ik die bekijk, *Then* staan ticketdiscussies daar niet tussen. Je opent ze via het ticket.

#### MU2-02: Board members en employees schrijven in de discussie (M)
**Als** board member **wil ik** in de ticketdiscussie schrijven en de employees van het ticket daar zien antwoorden, **zodat** ik niet in de interne taakgesprekken hoef te zoeken.

- **AC1.** *Given* een open ticket, *When* ik een bericht stuur, *Then* staat het in de discussie met mijn naam als auteur (A-01).
- **AC2.** *Given* de eigenaar of een employee met een delegatie onder het ticket, *When* die in een run een tool gebruikt om naar het ticket te posten, *Then* staat het bericht in de discussie met de naam en de rol van die employee.
- **AC3.** *Given* een bericht van een board member zonder @vermelding, *When* het ticket in Doing staat, *Then* krijgt de eigenaar het als bericht in de hoofdtask (zoals een Owner-bericht nu), zonder een extra run te starten als de hoofdtask al loopt (OV-03).
- **AC4.** *Given* een employee die in een run van het ticket zit of door een @vermelding gewekt is, *When* de run start, *Then* krijgt die de recente discussie als context.

#### MU2-03: Delegaties, rapporten en kolomwijzigingen in de discussie (M)
**Als** board member **wil ik** in de discussie zien wat er onder het ticket gebeurt, **zodat** ik het verloop kan volgen zonder elke taak te openen.

- **AC1.** *Given* een ticket, *When* het van kolom wisselt, *Then* verschijnt een systeembericht "‹wie› verplaatste van ‹kolom› naar ‹kolom›" (wie = board member, employee of systeem).
- **AC2.** *Given* een task onder het ticket, *When* een employee delegeert, *Then* verschijnt "‹van› gaf ‹taak› aan ‹aan›", met een link naar de taak.
- **AC3.** *Given* een delegatie, *When* ze een rapport oplevert of faalt, *Then* verschijnt de samenvatting van het rapport (of de fout) met een link naar de taak.
- **AC4.** *Given* de keuze van de dispatcher, een plan of het eindrapport van de hoofdtask, *When* die er komt, *Then* verschijnt ze ook in de discussie.
- **AC5.** *Given* zo'n gespiegeld of systeembericht, *When* het verschijnt, *Then* wekt het niemand, ook niet als er een naam met @ in staat.

#### MU2-04: @vermelding wekt een employee, binnen de lusgrenzen (M)
**Als** board member **wil ik** met @Naam een employee in de discussie roepen, **zodat** ik snel een vraag aan de juiste collega kan stellen.

- **AC1.** *Given* een bericht van een board member met @Naam, *When* het verstuurd wordt, *Then* start een run van die employee op hop 0, en komt het antwoord als bericht in dezelfde discussie.
- **AC2.** *Given* een employee die in een run met @Naam naar de discussie post, *When* het bericht verstuurd wordt, *Then* wordt Naam gewekt op hop + 1. Op `MAX_HOPS` (3) wekt het niemand meer.
- **AC3.** *Given* een run die al `WAKING_POSTS_PER_RUN` (2) wekkende posts deed, of een employee met `WAKING_POSTS_PER_WINDOW` (4) wekkende posts in het laatste uur, *When* die nog een post met @ doet, *Then* wordt de post geplaatst maar wekt ze niemand, en ziet de employee dat in het antwoord van de tool.
- **AC4.** *Given* een gewekte employee die niet de eigenaar is, *When* die antwoordt, *Then* wijzigt dat de kolom of de hoofdtask niet. Werk geven kan alleen via delegatie of een nieuw ticket.
- **AC5.** *Given* een @vermelding van een naam die niet bestaat of gearchiveerd is, *When* het bericht verstuurd wordt, *Then* wekt het niemand en toont de client dat.

#### MU2-05: Plan goedkeuren of terugsturen in de discussie (M)
**Als** board member **wil ik** een plan goedkeuren of terugsturen vanuit de discussie, **zodat** de knop met tekstveld uit Fase 1 (B2) niet meer nodig is.

- **AC1.** *Given* een ticket in Plan review, *When* het plan klaar is, *Then* staat het als bericht in de discussie met de acties **Goedkeuren** en **Terugsturen**.
- **AC2.** *Given* dat plan, *When* ik op Goedkeuren klik, *Then* gaat het ticket naar Doing in de uitvoeringsmodus van MU1-14, en staat de goedkeuring met mijn naam in de discussie.
- **AC3.** *Given* dat plan, *When* ik Terugsturen kies, *Then* is mijn feedback (niet leeg) een gewoon bericht in de discussie, krijgt de eigenaar die en komt er een nieuw plan.
- **AC4.** *Given* Fase 2, *When* ik de ticketweergave open, *Then* is het aparte tekstveld van B2 weg. Een snelkoppeling op het ticket mag blijven, maar die schrijft altijd in de discussie.

#### MU2-06: Terug naar Doing en Done vanuit de discussie (M)
**Als** board member **wil ik** een ticket in Review terugsturen met feedback in de discussie, **zodat** de feedback bij de rest van het verloop staat.

- **AC1.** *Given* een ticket in Review, *When* ik "Terug naar Doing" kies (in de discussie, op het ticket of door te slepen), *Then* moet ik een bericht schrijven (niet leeg), staat dat in de discussie en gaat het naar de eigenaar (MU1-16 AC2).
- **AC2.** *Given* een ticket in Review, *When* ik het op Done zet, *Then* staat "‹naam› zette op Done" in de discussie. De regels van B6 gelden ongewijzigd.

#### MU2-07: Ticketdiscussie-tools voor employees (S)
**Als** employee **wil ik** de discussie van mijn ticket lezen en erin posten, **zodat** ik vragen kan stellen aan de board members.

- **AC1.** *Given* een run onder een ticket (hoofdtask of delegatie) of een run gewekt vanuit de discussie, *When* de tools worden opgebouwd, *Then* zijn er tools om de discussie te lezen en erin te posten. Daarbuiten zijn die tools er niet.
- **AC2.** *Given* een post met @Naam via die tool, *When* die verstuurd wordt, *Then* gelden de lusgrenzen van MU2-04.

### Epic B: Kanalen in de Team chat

#### MU2-08: Algemeen kanaal en één kanaal per project (M)
**Als** board member **wil ik** een algemeen kanaal en per project een kanaal, **zodat** gesprekken over een project niet door elkaar lopen.

- **AC1.** *Given* de Team chat, *When* ik die open, *Then* zie ik het kanaal **Algemeen** en één kanaal per actief project, met de laatste activiteit.
- **AC2.** *Given* een nieuw project, *When* het wordt aangemaakt, *Then* bestaat het kanaal meteen. Bij een gearchiveerd project is het kanaal alleen te lezen.
- **AC3.** *Given* een kanaal, *When* ik een bericht met @Naam stuur, *Then* werken het wekken en de lusgrenzen zoals in MU2-04, en antwoordt de employee in hetzelfde kanaal.
- **AC4.** *Given* een run gewekt in een projectkanaal, *When* ze start, *Then* draait ze in de context van dat project (map en project-memory). In Algemeen is dat de company-context.
- **AC5.** *Given* de huidige Team chat, *When* de kanalen er komen, *Then* wordt die het kanaal Algemeen. De project-assistent en de gesprekken met een employee blijven zoals ze zijn.

#### MU2-09: Employees posten in het juiste kanaal (M)
**Als** employee **wil ik** kunnen kiezen in welk kanaal ik post, **zodat** mijn bericht bij het juiste project staat.

- **AC1.** *Given* `post_to_group_chat`, *When* een employee die gebruikt zonder kanaal, *Then* gaat het bericht naar het kanaal van de huidige run (het kanaal dat de run wekte, anders het kanaal van het project van de run, anders Algemeen).
- **AC2.** *Given* een kanaal als parameter, *When* het kanaal niet bestaat of alleen te lezen is, *Then* weigert de tool met een duidelijke fout.
- **AC3.** *Given* `read_group_chat`, *When* een employee die gebruikt, *Then* kan die een kanaal kiezen, met dezelfde standaard als AC1.

### Epic C: Ticket vanuit een chat

#### MU2-10: Ticket aanmaken vanuit een bericht (M)
**Als** board member **wil ik** van een bericht in een chat meteen een ticket maken, **zodat** een idee uit een gesprek niet verloren gaat.

- **AC1.** *Given* een bericht in een kanaal, een project-assistent of een gesprek met een employee, *When* ik "Ticket maken" kies, *Then* opent het ticketformulier, voorgevuld met een titel (eerste regel), het bericht als beschrijving en het project van het kanaal of gesprek.
- **AC2.** *Given* dat formulier, *When* ik bevestig, *Then* staat het ticket in de Backlog met mij als aanmaker. Er start niets vanzelf (Fase 1, §2).
- **AC3.** *Given* het nieuwe ticket, *When* het bestaat, *Then* staat in de chat een verwijzing naar het ticket en begint de ticketdiscussie met een verwijzing naar het bronbericht.
- **AC4.** *Given* een employee, *When* die in een chat werk ziet voor een ticket, *Then* kan die zelf geen ticket aanmaken (Fase 1 OV-05). De employee stelt het voor, en een board member maakt het met AC1 (OV-05).

#### MU2-11: "Give work" maakt een ticket (S, wacht op OV-04)
**Als** board member **wil ik** dat "Give work" een ticket maakt, **zodat** er geen werk buiten het Board ontstaat (Fase 1 B7, Pixel P-02).

- **AC1.** *Given* "Give work" met een gekozen employee en een opdracht, *When* ik verstuur, *Then* ontstaat een ticket met mij als aanmaker, die employee als eigenaar, de gekozen modus, en de opdracht als beschrijving. Het ticket start meteen (zoals MU1-12: zelf een eigenaar kiezen).
- **AC2.** *Given* "Give work" zonder project, *When* ik verstuur, *Then* vraagt de client om een project (tickets hebben altijd een project).
- **AC3.** *Given* zo'n ticket, *When* het rapport er is, *Then* gaat het naar Review en geldt de reviewpoort zoals bij elk ticket.

### Epic D: Memory

#### MU2-12: Samenvatting na Done uit de ticketdiscussie (M)
**Als** board member **wil ik** dat een afgerond ticket een samenvatting van het hele ticket in de project-memory zet, **zodat** ook de beslissingen uit de discussie en de delegaties bewaard blijven.

> Keuze van de fase: **Fase 2.** In Fase 1 (MU1-18) gebeurt de extractie al bij Done, maar alleen op het gesprek van de hoofdtask. Pas met de ticketdiscussie is er één bron die het hele ticket bevat. Deze story vervangt de bron van MU1-18; de AC's van MU1-18 blijven gelden.

- **AC1.** *Given* een ticket dat op Done gaat, *When* het gesloten is, *Then* schrijft het systeem precies één samenvatting naar de project-memory, op basis van de ticketvelden, de discussie en de rapporten van de tasks (de bestaande extractie, met een andere bron).
- **AC2.** *Given* de samenvatting, *When* ik ze bekijk, *Then* verwijst ze naar het ticket (id en titel) en staat ze als systeembericht in de discussie.
- **AC3.** *Given* de tasks onder het ticket, *When* ze eindigen, *Then* is er geen dubbele extractie: per ticket komt er één samenvatting, niet één per task (OV-06).
- **AC4.** *Given* een mislukte extractie (bijvoorbeeld door quotum), *When* die mislukt, *Then* blijft het ticket op Done, is de fout zichtbaar in de discussie en kan een board member opnieuw proberen. *(Neemt MU1-18 AC4 over, dat uit Fase 1 is doorgeschoven omdat protocol 4 er geen veld voor had.)*

## Open vragen (escalatie naar Orion)

| ID | Vraag | Voorstel |
|---|---|---|
| **OV-01** | Mag je in de discussie nog schrijven als het ticket op Done staat? | Nee, alleen lezen, net als het ticket (MU1-16 AC1). Vervolgwerk wordt een nieuw ticket dat naar dit ticket verwijst |
| **OV-02** | Delen de ticketdiscussies en de kanalen één budget voor wekkende posts per employee, of heeft elk een eigen budget? | Eén budget per employee (4 per uur) over alles samen. Dat is de veiligste vorm van RULE 08 en vraagt geen nieuwe constanten |
| **OV-03** | Wat is de relatie tussen de ticketdiscussie en het gesprek van de hoofdtask? Wordt de discussie het enige gesprek van de eigenaar, of blijft de hoofdtask een eigen gesprek hebben? | De hoofdtask houdt zijn eigen gesprek (minder impact op de task-engine). Berichten van board members gaan van de discussie naar de hoofdtask, en de antwoorden en rapporten van de eigenaar worden gespiegeld naar de discussie. Titan bevestigt in de impactanalyse |
| **OV-04** | Maakt "Give work" vanaf Fase 2 een ticket (open punt B7)? | Ja (MU2-11): een ticket dat meteen start met de gekozen employee. Wie een snel gesprek wil zonder ticket, gebruikt het gesprek met een employee of de project-assistent. Schedules blijven taken zonder ticket |
| **OV-05** | Mogen employees in Fase 2 zelf tickets aanmaken (bijvoorbeeld vanuit een kanaal)? | Nee. `created_by_user` moet een board member zijn en in Fase 3 bepaalt die het Claude-abonnement. Een employee stelt een ticket voor in de chat, een board member maakt het in één klik (MU2-10) |
| **OV-06** | Blijft de extractie per task bestaan voor tasks onder een ticket? | Nee voor tasks onder een ticket (alleen de ticketsamenvatting bij Done). Ja voor tasks zonder ticket (chat, schedules), zoals nu |
| **OV-07** | Kan je ook een board member met @ vermelden, en krijgt die dan een melding? | Ja, als markering in de discussie of het kanaal. Een melding per board member komt pas in Fase 3, als de server weet wie er ingelogd is |
| **OV-08** | Kan een board member een gewekte employee in de discussie werk laten doen dat de kolom wijzigt? | Nee (MU2-04 AC4). Alleen de eigenaar en de task-engine wijzigen de kolom. Ander werk gaat via delegatie of een nieuw ticket |
