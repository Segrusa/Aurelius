# Feature set: employees als aparte bots, naar het model van OpenBot (vastgesteld 2026-11-08)

Status: vastgesteld op 8 november 2026 door Sébastien. De beslissingen staan in §6, de uitwerking in [bots/](bots/README.md). De kolom "Voorstel" hieronder is dus wat we bouwen.

## Context

Sébastien wil dat onze employees echte, aparte bots worden zoals in OpenBot. Dit document legt vast
welke ideeën we daaruit overnemen, wat we al hebben en wat we bewust niet overnemen.

Met OpenBot bedoelen we [regnull/openbot](https://github.com/regnull/openbot), het open-source
platform voor "a team of AI bots that use tools, keep long-term memory, and hand work to each other"
(gelezen op commit `b67aeac` van 2026-11-03). De andere projecten met dezelfde naam (een nanobot-fork,
een Android-robot, een Go-prototype) zijn niet bedoeld.

**Belangrijkste vaststelling.** OpenBot-bots zijn geen aparte processen. Alles draait in één
Python-daemon (FastAPI, LangGraph, SQLite). Wat een bot "apart" maakt is het actormodel: een eigen
mailbox, een eigen worker die één run tegelijk doet, eigen configuratie (model, tools, approvals),
eigen geheugen en scratchpad, en een eigen `@handle`. Aurelius heeft ongeveer twee derde van die
bouwstenen al. Wat ontbreekt is de eenheid: bij ons zit het verspreid over tickets, tasks,
conversations en jobs.

## 1. Begrippen

| OpenBot | Betekenis | Aurelius nu |
|---|---|---|
| **Actor** | Iedereen die berichten stuurt of ontvangt: een bot, de mens (`@you`) of een extern systeem via webhook. Eén rij in `actors` | Employee, board member (`users`) en app key zijn drie losse begrippen |
| **Mailbox / inbox item** | De wachtrij van een actor. Drie soorten items: `message`, `question`, `resume` | Jobs-tabel per soort werk (chat run, task, triage, memory); Inbox alleen voor mensen |
| **Thread** | Eén taak met zijn context: deelnemers, een default bot (thread lead) en een werkmap | Ticket met discussie, task, conversation (employee-chat, Team chat) |
| **Run** | Eén uitvoering van de agent-loop van één bot in één thread, gewekt door één of meer inbox items. Eenheid van planning, kosten en annuleren | Chat run / task run via de dispatcher |
| **Hop** | Teller op bot-naar-bot berichten; een menselijk bericht reset hem; bij 20 stopt de keten | Ping-pong-limiet in de Team chat én de ticket-discussies (RULE 08, `MAX_HOPS` in `domain/group_chat.rs`, geteld over alle discussies samen) |
| **Memory** | Per bot, over alle threads heen, alleen "hoe doe ik mijn werk" | `memory_items` met scopes global/project/employee |

## 2. Runtime: één mailbox per employee

| # | Feature | OpenBot | Aurelius nu | Voorstel |
|---|---|---|---|---|
| R1 | **Mailbox als enige ingang** | Alles wat een bot wekt is een inbox item: @mention, antwoord op een vraag, webhook, gepland bericht | Ticket-toewijzing, @mention, heartbeat, webhook en recurring task hebben elk een eigen pad | Eén `employee_inbox` waar al die paden in uitmonden; de dispatcher leest alleen nog daaruit |
| R2 | **Eén run tegelijk per bot** | Eén worker per bot; pakt het oudste item en voegt alle wachtende items van dezelfde thread samen tot één run (coalescen); wisselt tussen threads; globale semafoor (`MAX_CONCURRENT_RUNS`, standaard 4) | Load limits op de dispatcher, geen garantie per employee | Worker per employee; coalescen per ticket of chat |
| R3 | **Parkeren bij een vraag** | Zolang een bot in een thread op een antwoord wacht (`waiting_human`), blijven nieuwe items voor die thread in de wachtrij | Ticket blijft sinds #26 in Doing bij een vraag, maar nieuwe berichten starten wel een run | Items voor een geparkeerde thread blijven `queued` tot het `resume`-item verwerkt is |
| R4 | **Hop-teller overal** | `Message.hop`: 0 voor een mens, max(triggers)+1 voor een bot; bij de limiet een systeembericht per thread tot een mens weer spreekt | De limieten gelden al voor Team chat en ticket-discussies, maar de hop staat alleen op wat een run in een discussie post | Hop op elk bericht in ticket-discussie, Team chat en delegatie |
| R5 | **Vastgehouden handoff** | Heeft de zender nog ongelezen items in de thread, dan wordt zijn @mention niet afgeleverd maar bewaard; het platform bezorgt hem ongewijzigd zodra de zender bij is. Voorkwam twee reviews van één PR | Bestaat niet | Overnemen in de router; systeembericht "handoff vastgehouden" in de discussie |
| R6 | **Thread is de enige waarheid** | Een bot onthoudt niets van een thread tussen runs; elke run wordt opnieuw opgebouwd uit de berichten (24k tokens of 80 berichten, nieuwste eerst). Checkpoint van de run wordt na afloop verwijderd | Claude Code-sessie wordt hervat met `--resume` per conversation | Hybride: `--resume` binnen een ticket, herbouw bij een nieuwe thread; herbouw altijd beschikbaar als terugval na een crash |
| R7 | **Zelf-planning in de thread** | Tool `schedule_message(delay, content, to)`: een bot stuurt zichzelf of een collega later een bericht ("check de CI over 10 min"). De thread telt als actief zolang er iets gepland staat | Heartbeat (30 s tick, opt-in, quiet hours, tokenbudget) en recurring tasks | `schedule_message` toevoegen aan de bridge; heartbeat blijft voor "kijk rond zonder aanleiding" |
| R8 | **Activity log** | Elke bezorging, wachtrijstap en run-stap als rij zonder foreign keys, doorzoekbaar per thread, bot of run | Logging, geen doorzoekbaar log in de UI; per ticket is er wel een Activity-tab | Tabel `activity_log` plus een pagina |

## 3. Samenwerking: de promptregels

De systeemprompt van OpenBot is een template (`system_prompt.md`) met vaste blokken, in deze volgorde:
identiteit (naam, `@handle`, beschrijving), "hoe dit platform werkt", de eigen instructies letterlijk,
het roster van andere bots met hun beschrijving, tot acht relevante herinneringen, scratchpad,
todo-lijst per thread en optioneel de `AGENTS.md`/`CLAUDE.md` van de werkmap. Het stabiele deel staat
vooraan voor prompt caching, het wisselende deel achteraan.

De regels die het laten werken:

| # | Regel | Hoe OpenBot het formuleert | Aurelius nu |
|---|---|---|---|
| P1 | **Eindig elke reply met een handoff** | "End every reply with a handoff to whoever should act next: start a line with their handle, e.g. `@bob - over to you, do this and that`." Alleen de eerste `@handle` in een reply wekt een bot; latere handles zijn verwijzingen. Een reply zonder mention beëindigt het gesprek | `DISCUSSION_FIRST_GUIDANCE` vraagt om `@responsible` bij een vraag, geen vaste handoff-regel |
| P2 | **Thread lead** | Elke thread heeft een default bot. Die ziet de hele thread, plant welke bots in welke volgorde nodig zijn, en is de fallback: "If you are ever unsure who should act next, hand off to the thread lead instead of guessing." | Orion als triage-dispatcher wijst tickets toe, is daarna geen lead meer |
| P3 | **Scoped view voor gedelegeerden** | Een bot die niet de lead is en niet alleen is, ziet alleen berichten aan hem gericht plus zijn eigen antwoorden; de prompt zegt hoeveel andere berichten er zijn en noemt `read_history` en `recall_messages` om ze op te halen | Iedereen ziet de hele ticket-discussie |
| P4 | **Zelfstandige handoffs** | Uit de chief-of-staff prompt: "Other bots do not see this conversation... Every hand-off must therefore be self-contained: the goal, the paths or PR number and branch involved, the acceptance criteria, and what to report back. Never write 'see above'." | Delegatie via `delegate_task` met vrije tekst |
| P5 | **Triggers gemarkeerd** | Berichten die deze run wekten staan als `[naam] (new): ...`. Een trigger van vóór de eigen laatste reply krijgt "(new, arrived before your last reply; it may already be handled)": eerst vergelijken met je vorige antwoord, en zo ja in één zin zeggen dat het al gedaan is | Geen markering |
| P6 | **Rolprompts klein en operationeel** | Chief of staff: geen file tools, delegeert in zijn eerste reply, onderzoekt zelf niets. Reviewer: draait geen tests ("QA does that once"), labelt BLOCKER/NIT. QA: merged pas na `ask_human` en groene CI. Engineer: lange prompt met operating loop (understand, plan, implement, verify, report), playbooks per taaksoort en een shared-workspace-regel: nooit andermans wijzigingen terugdraaien | Instructies per employee zijn vrije tekst van de gebruiker; `AUTONOMY_GUIDANCE` is generiek |
| P7 | **Roster met beschrijving** | Het veld Description heet in de UI "Shown to other bots so they know when to hand work to this one" en komt in elke prompt als `- @handle (Naam): beschrijving` | Lijst van collega's in `run/plan.rs` (`colleague_lines`) met de eerste instructieregel, die daar niet voor bedoeld is |
| P8 | **Signatuur op externe systemen** | Elke comment op GitHub, Linear enz. wordt getekend als "OpenBot - `@handle`" | Geen regel |
| P9 | **Bots maken bots** | De lead heeft `create_bot`, `read_bot_instructions`, `update_bot_instructions` | Proposals: employee stelt collega voor, mens keurt goed. Dit houden we |

P1, P2, P4, P5, P7 en P8 staan als platformblok vooraan in de systeemtekst (`run/plan.rs`). P1 is bij ons
scherper dan bij OpenBot (9 november, [bots/README.md](bots/README.md) B7): wakker maken gaat alleen met de
expliciete tool `hand_off`; een `@Naam` in wat een employee schrijft wekt niemand. P4 staat ook als vaste tekst
in de delegatie-tool; P6 wordt een set startprofielen (zie §4, C6). P3 is beslist: scoped in de
prompt, volledig in de UI (§6).

## 4. Configuratie, geheugen en buitenwereld

| # | Feature | OpenBot | Aurelius nu | Voorstel |
|---|---|---|---|---|
| C1 | **Model per bot** | `provider`, `model`, `model_settings` (effort, temperature, max tokens, max model calls, timeout, permission) per bot; `auto` volgt de serverinstelling | Model, provider en effort per run (`ResolvedRunConfig`). Server-side kan het al per employee via `config_overrides` (`connection.set_defaults` op employee-scope); `runtime_selection` in `config_json` wordt hiervoor niet gebruikt | UI op de bestaande overrides: provider, model, effort per employee, met Inherit als standaard |
| C2 | **Tools per bot met approval-vinkje** | Elke tool aan of uit, en per ingeschakelde tool "needs approval"; MCP-servers per bot toegekend, tools heten `server__tool` en laden on demand | Tool profile per run (`tooling.rs`), MCP-servers van de gebruiker voor iedereen, plugins per employee; alle bridge-tools omzeilen approval via `MEMORY_TOOLS` | Toollijst en `approval_tools` in `EmployeeConfig.tool_policy`; MCP-servers per employee aanvinken |
| C3 | **Toegangsniveau** | Read-only of Read and edit; voor de Claude Code-runtime vertaald naar `--restricted --strict-mcp-config --tools Read,Grep,Glob[,Edit,Write,NotebookEdit]` | Permission modes Ask/AcceptEdits/Auto/Plan op de conversation | Niveau per employee naast de mode; expliciete toollijst per niveau meegeven aan `claude -p` |
| C4 | **Repository-instructies** | Vinkje "load repository instructions": `AGENTS.md`/`CLAUDE.md` van de werkmap in de prompt (max 32k) | Claude Code leest `CLAUDE.md` zelf | Niets te doen zolang we via Claude Code draaien |
| C5 | **Geheugen met scope-regel** | Per bot, over alle threads; alleen "how the bot does its job"; na elke run een uitgestelde reflectie (30 s debounce, per bot en thread) die alleen bot-niveau kennis extraheert; acht herinneringen op gelijkenis met de triggertekst in de prompt; `manage_memory` en `search_memory` als tools | `memory_items` met scopes, extractie- en consolidatiejobs, `remember`/`recall` | De scope-regel letterlijk overnemen in de extractieprompt en in `remember`: feiten die alleen in één ticket gelden horen in de discussie, niet in het geheugen |
| C6 | **Startprofielen** | Catalogus van opt-in bots: chief_of_staff, engineer, reviewer, qa, frontend_designer, elk met instructiebestand, toollijst en icoon | Geen catalogus; `fresh_start.rs` kopieert bestaande employees en maakt Orion niet aan | Catalogus van employee-profielen in de repo, kiesbaar bij "Nieuwe employee" |
| C7 | **Scratchpad per bot** | Eén tekst per bot, altijd in de prompt, bijgewerkt via de `scratchpad`-tool; in de UI zichtbaar via een notitie-icoon op de run card | Bestaat niet | Tabel `employee_scratchpads` plus tool in de bridge |
| C8 | **Todo-lijst per thread** | `todo_write` per bot per thread; na samenvatting van een lange run wordt de lijst opnieuw in de samenvatting gezet | Claude Code heeft eigen todo's binnen een sessie | Alleen nodig als we R6 (herbouw) kiezen |
| C9 | **Externe actoren** | Een extern systeem is een actor met `webhook_url`; krijgt elk bericht in zijn threads als POST met HMAC-SHA256 (`X-OpenBot-Signature`) en retries, of pollt de REST API en bevestigt items | Gesigneerde webhooks met outbox, app keys, `aurelius-mcp`, agent functions | App key wordt een actor: kan deelnemen aan een ticket-discussie en wordt gemend met `@handle` |
| C10 | **Chatkanalen** | Telegram ingebouwd (bot token, webhook); Slack en Discord op de roadmap | Geen; mobile en web via de gateway | Telegram als eerste extern kanaal, als externe actor (C9) |
| C11 | **Usage en kosten** | Per run tokens in/uit, cache, reasoning, kosten en model calls; opgeteld per thread | Usage per run bestaat grotendeels (analyse-linear §4.7) | Odometer per ticket en per board member in de UI (zie §5) |

## 5. UI

| # | Onderdeel | OpenBot | Aurelius nu | Voorstel |
|---|---|---|---|---|
| U1 | **Run card** | Onder elk botbericht een ingeklapte kaart: statusbadge (pulserend bij running), aantal tool calls en mislukte calls, tokens, Cancel-knop; opengeklapt een transcript met `$ tool(args)`-regels en het resultaat eronder; notitie-icoon opent de scratchpad | Runs zichtbaar in de task-weergave; in de discussie staat het resultaat al als `Reported`-regel, zonder kaart | Run card in ticket-discussie en Team chat, op die regels |
| U2 | **Interrupt card** | "X is waiting for you" met antwoordveld, of per tool call approve/reject met één knop "Submit decisions"; dezelfde kaart staat in de Inbox | Approvals en vragen via de Inbox sinds #29 | Eén component voor vraag en approval, in discussie én Inbox |
| U3 | **Waiting indicator** | Onder de thread-titel: "@eng — waiting to pick up this thread (behind 2 items)", uit `waiters.updated` over SSE | Geen | Toont wie in de wachtrij staat voor dit ticket en hoe ver |
| U4 | **Thread odometer** | Calls, tokens in, tokens uit, kosten per thread in de kop | Geen | Per ticket, plus per board member in een overzicht |
| U5 | **Inbox** | Twee secties: "Waiting for you" (vragen, direct beantwoordbaar) en "Unread messages" gegroepeerd per thread met Mark read en Mark all read | Persoonlijke Inbox met één-klik-antwoorden (#29); "Needs you" bestaat al | Groeperen per ticket kan client-side; "Wacht op jou" bovenaan |
| U6 | **Bot-pagina** | Tabs Inbox, Memory, Settings; in de sidebar elke bot met een actief/idle-dot; Composer met `@`-autocomplete die het roster met beschrijving toont | Employees-scherm en employee-pagina (`manager.rs`) | Tab Inbox (wachtrij van de employee) en Memory toevoegen; status-dot in de lijst |
| U7 | **Scheduled** | Pagina met alle geplande berichten en hun uitkomst | Recurring tasks in de task-UI | Eén pagina voor recurring tasks, heartbeats en `schedule_message` |
| U8 | **Living Office** | 3D-kantoor met een "Openling" per rol en statusbubbels: idle, waiting, thinking, working, coding, testing, blocked, human_review, reporting, completed, resting, sleeping. Bij hen nog demo-data | Geen | Later; het statusmodel (U8) is wel nu al bruikbaar voor de status-dot van U6 |

## 6. Beslissingen (8 november 2026)

1. **Mailbox: vervanger.** `employee_inbox` vervangt de jobs-tabel als wachtrij voor werk met een employee. Job-soorten zonder employee (`ticket.triage`, `memory.*`) en de projectassistent blijven in `jobs`.
2. **Scoped view (P3): ja, alleen in de prompt.** Een niet-lead krijgt in zijn prompt alleen de berichten aan hem en zijn eigen antwoorden, plus hoeveel andere er zijn en met welke tool hij ze leest. In de UI blijft de hele discussie leesbaar voor iedereen, zoals `multi-user.md` wil.
3. **Credentials per employee: nee.** Provider, model, effort en toegangsniveau zijn per employee; runs blijven op de Claude-login van het board member (zie `fase-1/spike-claude-logins.md`).

## 7. Bewust niet overgenomen

- **Geen login of multi-user** bij OpenBot: één operator, geen accounts. Wij houden board members met eigen login.
- **`run_shell` zonder sandbox** draait als de serveruser met volledige toegang. Wij houden de bridge als MCP-server en de permission modes van Claude Code.
- **Sessie weggooien na elke run** (R6 zuiver). Te duur met Claude Code; we kiezen de hybride.
- **Bots die bots aanmaken zonder goedkeuring** (P9). Proposals blijven.

## 8. Volgorde

Grootte is een eerste inschatting (S/M/L); de uitwerking staat in [bots/impactanalyse.md](bots/impactanalyse.md).

| # | Wat | Grootte |
|---|---|---|
| 1 | Promptregels P1, P2, P4, P5, P7, P8 en de scoped view P3 in `plan.rs` en de delegatie-tool | S |
| 2 | Model, effort en toegangsniveau per employee (C1, C3) | M |
| 3 | Geheugen-scope-regel in extractie en `remember` (C5) | S |
| 4 | Run card, interrupt card en odometer in de discussie (U1, U2, U4) | M |
| 5 | Mailbox per employee met worker, coalescen en parkeren (R1 t/m R3) | L |
| 6 | Hop-teller en vastgehouden handoff overal (R4, R5) | M |
| 7 | `schedule_message`, scratchpad, waiting indicator, Scheduled-pagina (R7, C7, U3, U7) | M |
| 8 | Tools en MCP per employee met approval-vinkje (C2) | M |
| 9 | Startprofielen (C6) | S |
| 10 | Externe actoren en Telegram (C9, C10) | L |
| 11 | Activity log (R8), bot-pagina met Inbox en Memory (U6) | M |
| 12 | Living Office (U8) | L |

## Bronnen

- [regnull/openbot](https://github.com/regnull/openbot): `backend/openbot/runtime/system_prompt.md`, `bots/instructions/*.md`, `runtime/router.py`, `runtime/prompt.py`, `docs/architecture.md`, `frontend/src/components/{RunCard,InterruptCard,WaitingIndicator}.tsx`, `frontend/src/pages/{InboxPage,BotEditorPage}.tsx`, `frontend/src/lib/office/status.ts`.
- Aurelius: `crates/domain/src/employee.rs`, `crates/application/src/run/plan.rs`, `crates/daemon/src/dispatcher/mod.rs`, `crates/agent-bridge/src/mcp.rs`, `docs/product/multi-user.md`, `docs/product/samenwerking/analyse-linear.md`.
