# Productdefinitie: Aurelius voor meerdere gebruikers — Company, Board, Tickets (goedgekeurd 2026-10-30)

## Context
Aurelius gaat nu uit van één eigenaar per server: er is geen gebruikerstabel en geen login, er is één Claude Code-login op de server, er is geen board en de status `review` wordt meteen doorlopen. Sébastien wil dat meerdere board members ermee werken via een ticketgedreven kanban-workflow, met een discussie per ticket, kanalen in de Team chat, een vault en memory op meerdere niveaus. Deze versie verwerkt zijn antwoorden op de open vragen uit v1.

## 1. Datamodel

| Entiteit | Inhoud | Wat er nu is |
|---|---|---|
| **Company** | Expliciete tabel (één per server). Daaronder vallen de board members, employees, projecten, de inrichting van het bord, de vault en de memory van de company | Impliciet (0014). Krijgt nu een tabel |
| **Board member (user)** | Gebruikersnaam en wachtwoord. Er is één rol: iedereen mag alles zien en doen. Heeft een eigen Claude-connectie | Bestaat niet. Er zijn alleen gekoppelde apparaten (`remote_clients`) |
| **Employee** | Een AI-agent met hiërarchie (`reports_to`) en instructies | Bestaat. Afdelingen vervallen (zie hieronder) |
| ~~Afdeling~~ | **Vervalt.** Het organigram loopt enkel via `reports_to` | `departments`, `department_id`, het memory-scope `department` en "Department" in prompts en UI worden verwijderd |
| **Project** | Map, status, en daaronder tickets, tasks, vault, memory en een kanaal in de Team chat | Bestaat |
| ↳ **Ticket** | Een kaart op het bord, met een eigen discussie | Nieuw |
| ↳ **Tasks** | De interne werkeenheden onder een ticket (de hoofdtask plus de delegaties) | Bestaan |
| ↳ **Vault** | Items van het type tekst, url of secret | Nieuw |
| **Memory** | Op niveau van company, project en employee | Bestaat (scopes global/project/employee/department) |

## 2. Vastgelegde beslissingen

**Toegang**
- Er komt één company per server, met een expliciete `company`-tabel zodat meerdere companies later mogelijk blijven.
- Board members loggen in met gebruikersnaam en wachtwoord (gehasht met argon2). Een apparaat wordt aan een user gekoppeld. Elke actie en elk bericht krijgt een auteur.
- Er is één rol, *Board member*. Iedereen mag alles zien, tickets maken, reviewen en de vault beheren.
- Toegang blijft via **Tailscale** lopen. De server draait centraal en is via de tailnet bereikbaar.

**Claude-abonnementen: optie (a), elke board member zijn eigen**
- Elke board member koppelt zijn eigen Claude-login. Runs van een ticket, ook alle delegaties eronder, gebruiken de connectie van **de aanmaker van het ticket**.
- **Haalbaarheid:** dit lijkt technisch mogelijk. Claude Code kan per proces een eigen configuratie- en loginmap gebruiken (`CLAUDE_CONFIG_DIR`), of een token dat met `claude setup-token` gemaakt wordt (`CLAUDE_CODE_OAUTH_TOKEN`). De daemon start `claude` nu met een allowlist van omgevingsvariabelen (`crates/runtime/src/claude_code/mod.rs`). Die moet per run de map of het token van de juiste user meegeven. **Dit moet Titan eerst bevestigen met een spike** (twee logins naast elkaar op één machine).
- Is het quotum van de aanmaker op, dan wacht de run met de bestaande reden `quota`. Er is geen stille fallback naar het abonnement van iemand anders.
- Runs zonder ticket (Team chat, project-assistent) gebruiken de connectie van de board member die het bericht stuurde.

**Board en workflow**
- De kolommen worden op company-niveau ingesteld. Standaard: **Backlog · Todo · Doing · Plan review · Review · Done**.
- Een ticket wordt **pas gestart na een klik op "Start" of door het naar Todo te slepen**. Er is geen automatische pickup vanuit de Backlog.
- In Todo kiest de dispatcher, standaard **Orion** (per project in te stellen), een employee. Een board member kan ook zelf meteen een employee kiezen.
- Een ticket in **Plan-modus** stopt na het plan in *Plan review*. Na goedkeuring in de discussie gaat het naar Doing voor de uitvoering. Zonder Plan-modus wordt die kolom overgeslagen.
- Employees mogen een ticket verplaatsen tot en met Review. Alleen board members zetten het op **Done**, of terug naar Doing met feedback in de discussie.
- Done sluit het ticket. Er wordt **niets automatisch** gemerged, gepusht of gedeployed (RULE 07).
- Ticketvelden: titel, beschrijving, project, kolom, prioriteit, aanmaker, eigenaar (employee), modus en model, labels, bijlagen, deadline, discussie en gekoppelde tasks.

**Ticketdiscussie**
- Er komt één gesprek per ticket (een nieuw conversatietype `ticket`). Board members en employees schrijven erin. Delegaties, rapporten van subtasks en kolomwijzigingen verschijnen erin. Een @vermelding wekt een employee, met dezelfde lusgrenzen als de huidige Team chat (MAX_HOPS en limieten op wekkende posts).

**Team chat: kanalen**
- De Team chat krijgt een **algemeen kanaal** en **één kanaal per project**. Board members en employees spreken er allemaal in.
- De project-assistent en de gesprekken met een employee blijven bestaan. Vanuit een chat kan je een ticket aanmaken.

**Vault**
- De vault bestaat op **company-niveau en per project**. Items hebben het type tekst, url of secret.
- Secrets worden versleuteld opgeslagen. Ze verschijnen nooit in prompts, memory of discussies. Een run krijgt ze als omgevingsvariabele of via een tool. Er is een auditlog van wie wat las of wijzigde.
- **Alle employees mogen alle items lezen.** Er zijn geen toegangslijsten per item.

**Memory**
- Memory bestaat op **company-, project- en employee-niveau**. Na Done schrijft een ticket automatisch een samenvatting naar de project-memory, op basis van de bestaande extractie na een task.
- De memory per afdeling vervalt samen met de afdelingen. Wat er nu in staat, wordt bij de overgang naar company-memory verplaatst, zodat er geen kennis verloren gaat.

## 3. Resterende kleine aannames (tenzij je iets anders wil)
- **Onboarding (beslist):** zolang de server nog geen board members heeft, is de eerste stap in elke client verplicht een board member aanmaken (gebruikersnaam en wachtwoord). Andere toegang is er dan nog niet. Daarna kan elke board member nieuwe board members aanmaken.
- **Nieuwe versie, geen migratie van werk (beslist):** bestaande taken, gesprekken en Team chat worden niet overgenomen. De nieuwe versie start met een schone werkruimte. Daardoor hoeven we geen migratiepad voor oude taken of gesprekken naar tickets te bouwen en te testen.
- **Wel overgenomen (beslist):** employees (met instructies en `reports_to`), projecten en memory. **Afdelingen worden verwijderd.**

## 4. Volgorde
1. **Fase 0:** deze definitie vastleggen als `docs/product/multi-user.md` en de beslissingen in memory zetten. **Tegelijk** laat Titan een spike doen naar meerdere Claude-logins op één server (deze beslissing heeft het grootste risico).
2. **Fase 1, Company + Board + Tickets:** de company-tabel, afdelingen verwijderen, de kolommen van het bord inclusief Plan review, tickets boven op tasks, de knop Start en slepen naar Todo, de dispatcher en een echte reviewpoort. Het veld `created_by_user` zit er meteen in.
3. **Fase 2, Ticketdiscussie en kanalen in de Team chat:** desktop en mobiel.
4. **Fase 3, Multi-user:** users met login, apparaten per user, auteurschap en per user een Claude-connectie, op basis van de uitkomst van de spike.
5. **Fase 4, Vault:** voor company en project, versleuteld en met audit.

## 5. Uitvoering na goedkeuring
- Atlas legt de beslissingen vast met `remember` en schrijft `docs/product/multi-user.md`.
- Opdracht aan **Orion** (Task ID AUR-MU-00): (1) de spike voor Claude-logins per user via Titan, (2) Nova schrijft de user stories en Pixel het ontwerp van bord en ticket voor Fase 1, (3) Titan maakt een technische impactanalyse (migraties, protocol, clients).
- Er wordt pas gebouwd na het akkoord van Sébastien per fase. Er wordt niets gepusht of gemerged zonder akkoord.

## Verificatie
- Spike: twee board members met elk hun eigen Claude-login draaien tegelijk een run op dezelfde server. In de logboeken of het gebruik is te zien welk account elke run gebruikte.
- Fase 1 is klaar wanneer een ticket het volledige pad doorloopt: Backlog → Todo (slepen) → Doing → (Plan review) → Review → Done, met delegaties die ongewijzigd werken. Daarbij zijn de bestaande tests groen (cargo en flutter) en is er per fase een review door Trace en QA door Pulse.
