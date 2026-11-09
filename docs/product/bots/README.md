# AUR-BOTS-00: Employees als aparte bots

*2026-11-08 · branch `feat/astryx` · vraag van Sébastien · uitwerking van [../bots-feature-set.md](../bots-feature-set.md), die de feature-id's (R1, P1, C2, U4 …) definieert*

| Document | Inhoud |
|---|---|
| [user-stories.md](user-stories.md) | BOT-01 t/m BOT-30, Given/When/Then, per stap |
| [ui-ontwerp.md](ui-ontwerp.md) | Wat de schermen krijgen, in Astryx; ontwerpkeuzes P-200 en verder |
| [impactanalyse.md](impactanalyse.md) | Per stap: code, migraties, protocol, tokens, risico's |
| [freeze.md](freeze.md) | Protocol-freeze: wat er bij mag binnen protocol 5 en hoe |

**Vraag:** employees worden echte, aparte bots zoals in OpenBot: een eigen mailbox, een worker die één run tegelijk doet, eigen instellingen en geheugen, een eigen `@Naam`.
**Scope:** de twaalf stappen van §8 van de feature set, elk server, protocol en React-client in Astryx.
**Buiten scope:** nieuwe schermen in `clients/mobile`. De oude GPUI-client is inmiddels verwijderd, zie [Verwijderde functies](#verwijderde-functies). Wat we bewust niet overnemen staat in §7 van de feature set.
**Protocol:** additief binnen 5, zie [freeze.md](freeze.md).

## Beslissingen

Deze tabel gaat voor op de andere documenten in deze map.

| # | Vraag | Besluit | Reden |
|---|---|---|---|
| B1 | Mailbox: vervanger of laag | **Vervanger.** `employee_inbox` is de wachtrij voor werk met een employee. Soorten zonder employee (`ticket.triage`, `memory.*`) en de projectassistent blijven in `jobs` | Geen twee waarheden. De run-snapshot blijft: bevroren `login_user`, quota-parkeren per login, `run_guidelines` bij het claimen |
| B2 | Scoped view (P3) | **Scoped in de prompt van een niet-lead, volledig in de UI** | Goedkopere delegatie en zelfstandige handoffs, terwijl de discussie leesbaar blijft voor iedereen (`multi-user.md`) |
| B3 | Credentials per employee | **Provider, model, effort en toegangsniveau per employee; runs blijven op de Claude-login van het board member** | Een eigen login per employee botst met de voorwaarden (`fase-1/spike-claude-logins.md`) |
| B4 | Wie is de lead (P2) | **Ticket: de Responsible (een board member); zonder actieve Responsible de owner. Kanaal: de projectdispatcher, anders de CEO; General: de CEO** | Sluit aan op B3 en de routering van `fase-a`: de Responsible beslist al over het ticket |
| B5 | Hervatte sessies en de promptregels | **Het platformblok heeft een versie in de sessiesleutel: een nieuwe versie start een nieuwe sessie**, zoals guidelines B6 | Een hervatte sessie houdt tot 24 uur de bevroren systeemtekst; een notitie overleeft compactie niet |
| B6 | Geheugen (C5) | **Eén regel, in de extractie én in `remember`:** feiten die alleen in één ticket of taak gelden horen in de discussie | Anders vult het geheugen zich met status die morgen niet meer klopt |
| B8 | Praten met een employee (9 nov) | **Eén blijvend gesprek per board member per employee, naast de hoofdchat, via de mailbox met thread `chat:<conversation>` op de login van dat board member.** Een ticket uit zo'n gesprek wordt door de server gekoppeld; komt het in Review, dan schrijft dezelfde employee een kort bericht in dat gesprek | Praten moet vanzelf gaan, zonder een nieuw soort werk: tickets blijven gewone tickets, en een chat is nooit de ouder van een ticket en wacht er nooit op |
| B7 | Handoff (vervangt de wekregel van P1, 9 nov) | **Een handoff is een expliciete actie:** de bridge-tool `hand_off(to, message)` schrijft een inbox-item met zijn hop en staat in de discussie als gewoon bericht "Flux handed over to Bo". Een `@Naam` in wat een employee schrijft is alleen een verwijzing en wekt niemand; een antwoord zonder handoff beëindigt het gesprek. Een `@Naam` van een board member wekt nog steeds, en `@username` van een board member meldt die nog steeds. De handoff-regel staat zo in het platformblok; het platformblok krijgt er ook een schrijfregel bij (gewone zinnen, geen ritueel bevestigen, geen verplichte slotregel) | Tekst die toevallig een naam noemt mag niemand wekken; een handoff blijft controleerbaar en wordt door R5 vastgehouden als geheel |

## Aannames die je kunt omdraaien

1. `MAX_HOPS` gaat van 3 naar 8 voor ticket-threads, anders stopt een handoff-keten te vroeg.
2. `quick_answer` blijft als begrensde uitzondering op één run per employee.
3. Een run die op een approval wacht telt niet mee voor het employee-slot, wel voor `max_runs`; zijn thread staat geparkeerd.
4. Conversations zonder employee (projectassistent) blijven via `jobs` lopen.
5. `schedule_message` bevriest de login van de plannende run; externe actoren buiten een ticket krijgen geen run. Geen stille terugval.
6. Bouwen en testen één keer per stap.
7. De CSP krijgt twee style-hashes (Astryx-fase).

## Volgorde en stand

| # | Stap | Stand |
|---|---|---|
| 1 | P1, P2, P4, P5, P7, P8 plus scoped view P3 | gebouwd |
| 2 | C1 UI op de overrides, C3 toegangsniveau | gebouwd |
| 3 | C5 scope-regel | gebouwd |
| 4 | U1, U2, U4 | gebouwd |
| 5 | Mailbox, worker, samenvoegen, parkeren, hybride sessies (R1-R3, R6) | gebouwd |
| 6 | R4 hop op elk bericht, R5 vastgehouden handoff, `hand_off` (B7) | gebouwd |
| 7 | R7, C7, U3, U7 | gebouwd |
| 8 | C2 tools en MCP per employee | gebouwd |
| 9 | C6 startprofielen | gebouwd |
| 10 | C9 externe actoren, C10 Telegram | vervallen (besluit gebruiker) |
| 11 | R8 activity log, U6 | gebouwd |
| 12 | U8 Living Office | vervallen (besluit gebruiker) |
| 13 | Gesprekken met employees (B8) | gebouwd |

**Risico's:** de mailbox raakt elk wekpad, en de login- en quota-regels zijn de plek waar dat stil mis kan gaan. Parallelle runs vergroten het risico op "ordinary, individual usage" uit de spike. Eén lange branch over client, server en protocol zonder tussentijdse merge.

## Verwijderde functies

*2026-11-09 · besluit van Sébastien.* Om de app zo eenvoudig mogelijk te maken zijn deze functies verwijderd. De reden is telkens dezelfde: nul rijen in de productiedatabase en geen scherm in de React-client.

| Wat | Wat er weg is |
|---|---|
| Oude GPUI-desktopclient | `clients/desktop` en `third_party/gpui-base`; Electron (`clients/electron` met `clients/ts/app`) vervangt hem. De iconen staan nu in `clients/electron/assets`. Seed-richtlijn `g3-desktop-gpui-kit` zit niet meer in de startset; bestaande installaties houden hun rij. |
| Notities, Shares | `note.*`, `share.*`. `task.discuss` draaide op Shares en is daarmee ook weg, net als de knop "Discuss in Chat" op mobiel. |
| Taakvarianten | `task.variants`, `task.choose_variant`, `variants.*`, `probe_worktree` en de varianten-afhandeling in mailbox en dispatcher. |
| Taakafhankelijkheden | `depends_on` bij taken. Wachtreden `dependency` blijft bestaan voor delegaties. |
| Oude artifacts | `artifact.*`; staat los van Claude Design. Het taakrapport via `write_artifact_file` blijft. |
| Plugins | `plugin.*`, `employee_plugins`, `project_plugins`. |
| Heartbeats | De periodieke wekker per employee. Voor wat regelmatig terugkomt zijn er terugkerende taken en geplande berichten. |
| App keys, API keys, agent functions | Sleutels voor externe apps en functies met hun aanroepen. Omdat `aurelius-mcp` alleen met een app key werkt, zijn ook die crate, de Docker-image, `/v1/mcp`, `/v1/functions`, `/v1/calls` en het management channel (`ask_management`) weg. API keys van een connection (de sleutel voor een provider) blijven. |
| Webhooks | `webhook_outbox` en de levering van `task.*`-events. |
| Terminal | `terminal.*`. |
| Git-weergave | `git.get_diff` en `git.get_status`, met de git-chip in de mobiele chat. |

Er is geen tabel gedropt en geen migratie aangepast: de ongebruikte lege tabellen en kolommen (zoals `created_by_app_key_id`) blijven staan, en `PROTOCOL_MAJOR` blijft 5. Oude events van verwijderde soorten worden bij het lezen overgeslagen.
