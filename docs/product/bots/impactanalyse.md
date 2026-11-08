# Technische impactanalyse: employees als bots

*2026-11-08 · basis `feat/astryx` · bron [README.md](README.md) B1-B6*

## 1. Promptregels (stap 1, gebouwd)

- **Platformblok:** `PLATFORM_RULES` in `crates/application/src/run/plan.rs`, helemaal vooraan de systeemtekst van een employee-run. Eén tekst voor iedereen, dus een gedeeld cache-prefix. Niet voor quick answers (geen tools) en niet voor de projectassistent.
- **Tokens:** ±480 tokens tegen een budget van 27.200; een test begrenst het op 500. De lijst van collega's groeit niet: een beschrijving is maximaal 160 tekens, net als de instructieregel die ze vervangt. De scoped view maakt de context kleiner.
- **Versie (B5):** `PLATFORM_RULES_VERSION` zit in de `profile` van `session_key` (`daemon/src/dispatcher/session.rs`). Een andere waarde start een nieuwe sessie. Een test pint de hash van de tekst aan de versie.
- **Eenmalig effect:** bij de eerste run na de update start elk gesprek van een employee één keer een nieuwe sessie.
- **Per thread:** de lead en de scoped view staan niet in het blok maar in het wekbericht (`wake_body`) of de tail van een ticket-run, omdat de hoofdchat van een employee nog veel threads bedient. Stap 5 (`employee_threads`) kan dat vereenvoudigen.
- **Scoped view:** `group_chat::scoped_view` beslist puur; `ThreadView` in `conversation/discussion.rs` leest maximaal 60 berichten en citeert er zoals eerder maximaal 20. Quick answers en de projectassistent krijgen alles.
- **Triggers (P5):** `trigger_mark` in `plan.rs`. Wat na het laatste antwoord komt is `(new)`; wat binnenkwam voor de run van dat antwoord eindigde (`runs.ended_at_ms`) krijgt de langere markering. De markering zit in de turn-tekst, niet in `session::item`, dus hervatten breekt niet.
- **Roster (P7):** `colleagues` leest `employees.description`, met terugval op de eerste instructieregel. Migratie `0033_employee_description.sql`, `schema33_tests.rs`.
- **P1 met een randvoorwaarde:** een antwoord dat naar de discussie gekopieerd wordt wekt niemand. De regel zegt daarom "post je handoff". Dat blijft zo tot stap 6.

## 2. Geheugen (stap 3, gebouwd)

`MEMORY_SCOPE_RULE` in `domain/src/memory.rs`, gebruikt door de extractieprompt (`daemon/src/memory.rs`) en de `remember`-tool (`agent-bridge/src/mcp.rs`). `EXTRACTION_VERSION` wordt `aurelius-extract-3`. De regel is tekst: er is geen controle in de server die een fact van één ticket weigert.

## 3. Latere stappen

| Stap | Plekken | Let op |
|---|---|---|
| 2 C1, C3 | `claude_code/mod.rs`, `tooling.rs`, `run/session.rs` | Toegangsniveau in `session_key`, net als de platformversie. `connection.set_defaults` draagt C1 al op employee-scope; de UI en een leesmethode zijn nog nodig |
| 4 U1, U2, U4 | `storage/src/usage.rs`, `views.rs`, `approval-card.tsx` | Nieuw: `ApprovalView.ticket_id`, `MessageView.awaiting_user`, `ticket_id` op `usage.get_totals`, `TokenUsage.model_calls`, usage per `login_user` |
| 5 mailbox | `employee_inbox`, `employee_threads`, `run_triggers`; `ticket_line::dispatched`, `triage::assign`, `discussion::wake`, `to_owner`, `employee.rs:463`, heartbeat, `make_task`, `agent_functions.rs:303`, `insert_part`, proposals, varianten, ideas | Worker per employee; run en config-/login-snapshot pas bij het claimen. Lease, `lease_epoch`, herstel en `load::admit` blijven. Run-snapshot en `run_guidelines` blijven. Notificaties, `task.*`-webhooks, `list_open_questions`, recurring tasks en functions moeten blijven werken. `run_triggers` kan `trigger_mark` vervangen |
| 6 R4, R5 | `discussion.rs`, `task_runs.rs` | Assumptie 1. Runresultaten blijven `Reported`; een vastgehouden handoff als `action`-string, geen nieuwe `SystemLineKind` |
| 7 | `crates/storage`, `mcp.rs`, `data.ts` | `schedule_message` en scratchpad in de bridge |
| 8 C2 | `permissions.rs`, `configs_for_run`, `mcp.rs` | `MEMORY_TOOLS` herzien |
| 9 C6 | `org/employee.rs`, `employee-dialog.tsx` | Idempotent zoals de guidelines-seed |
| 10 C9, C10 | `protocol/src/app.rs`, `webhooks.rs`, `crates/remote-gateway` | `NOT_FOR_APPS` opheffen voor discussie |
| 11 | `storage/src/timeline.rs`, `employee-page.tsx` | `activity_state` en instellingen op `EmployeeSummary` |
| 12 | nieuw scherm, `app.tsx` | three.js als enige nieuwe dependency |

## 4. Risico's

| # | Risico | Maatregel |
|---|---|---|
| R1 | De mailbox raakt elk wekpad en kan de login of het quota-parkeren stil verliezen | Login uit het item, tests op `queued.rs` en `group_chat.rs` als sjabloon, `journey.e2e.ts` |
| R2 | Een niet-lead mist iets en wordt verrast | De notitie noemt het aantal en de tool. Tot 60 berichten terug |
| R3 | Een bevroren sessie houdt oude rollen of beschrijvingen tot 24 uur | Geldt ook vandaag voor de lijst van collega's. De platformregels zelf zijn versiegebonden |
| R4 | Parallelle runs en "ordinary, individual usage" | Eén run per employee (R2); limieten op wekkende posts blijven |
