# Technische impactanalyse Fase 1: Company, Board en Tickets

*AUR-MU-00-T2 · Titan (Tech Lead) · 2026-10-30 · basis: `main` 4694dd7 (worktree `target/wt-mu-00`), productdefinitie `docs/product/multi-user.md`*
*Aangevuld in AUR-MU-00-T3 (2026-10-30): board members en onboarding in Fase 1 (§4b, onderdeel S5), volgens multi-user.md §3 "Onboarding (beslist)", Nova MU1-20 t/m MU1-22, OV-12, OV-13 en Pixel §11 / P-07. De vroegere aanname "`created_by_user` = de principal van het apparaat, NULL toegelaten, geen `users`-tabel" vervalt.*

Dit document is alleen een analyse. Er is geen code gewijzigd. Bestandsverwijzingen gelden voor 4694dd7.

---

## 0. Samenvatting van de adviezen

| # | Beslissing | Advies |
|---|---|---|
| A1 | Hoe werkt de "schone werkruimte" technisch? | **Een nieuwe database met een eenmalige import** (de "fresh start"), naar het voorbeeld van de bestaande v0.6-import (`crates/storage/src/import.rs`, `upgrade_legacy`). **Geen** migratie die tabellen leegmaakt. |
| A2 | De migratieketen | De keten 0001–0017 blijft **ongewijzigd** (de checksums staan vast). Er komt een nieuwe migratie **0018** die het Fase 1-schema neerzet. De fresh start draait alle migraties op een lege database en importeert daarna. |
| A3 | Backup | De oude `data/` en `blobs/` worden **in hun geheel opzij gezet** (`pre-v0.11/`, nooit automatisch opgeruimd), bovenop de bestaande `pre-update`-backup. Vooraf kan een dry-run met een rapport. Terugdraaien kan door de mappen terug te zetten en de oude build te draaien. |
| A4 | De relatie tussen kolom en status | De **taakstatus blijft de waarheid van de engine**. De **kolom is een eigen veld van het ticket**, dat de engine bijwerkt op één plek (`task_line::set_status`). Een board member verplaatst kaarten met expliciete commando's. |
| A5 | De reviewpoort | Alleen de **hoofdtask van een ticket** blijft in `Review` staan. Subtasks (delegaties) en taken zonder ticket lopen zoals nu meteen door naar `Succeeded`, zodat delegaties ongewijzigd blijven. `complete()` wordt gesplitst in *rapporteren* en *aanvaarden*. |
| A6 | Plan review | Een plan van een ticket wordt **niet** als live approval van 30 minuten in de inbox gehouden. De run stopt na het plan, de taak wacht met `waiting_reason = 'plan_review'` en de goedkeuring hervat dezelfde Claude-sessie met een bericht en een nieuwe modus. |
| A7 | Dispatcher | Er komt een **aparte, korte dispatcher-run** met één tool (`assign_ticket`), los van het organigram. Die is **niet** gebouwd op `hand_over_task`, omdat hand-over het organigram volgt en Orion dan alleen aan Atlas of peers kan overdragen. |
| A8 | Protocol | **PROTOCOL_MAJOR van 3 naar 4.** Methodes en velden voor departments verdwijnen. Een oude client krijgt dan een duidelijke melding "Update needed" in plaats van stille parse-fouten. |
| A9 | Board members in Fase 1 | **Een tabel `users` in 0018** (gebruikersnaam + argon2id-hash), de methodes `user.create`, `user.list`, `user.whoami` en `user.use` (met een **eenmalige wachtwoordcontrole** bij het kiezen, OV-13), en een **actieve board member per verbinding** (`Peer.user` → `CommandContext.user_id`). Daardoor is **`tickets.created_by_user` NOT NULL** met een FK naar `users`. De clients tonen een verplichte gate (aanmaken of kiezen) voordat er iets anders bereikbaar is. Dit is **identificatie, geen authenticatie**: sessies en de koppeling van apparaten blijven Fase 3 (§4b). |

Totale schatting: **ongeveer 42 ontwikkeldagen** (was 36; +6 d voor S5, zie §4b en §5). Met Schema, Forge, Flux en Bridge parallel is dat **3,5 à 4,5 weken doorlooptijd**, inclusief review door Trace, een korte review door Sentinel en QA door Pulse.

---

## 1. Migraties en de schone werkruimte (crates/storage, migrations/)

### 1.1 Huidige situatie
- `crates/storage/src/migrate.rs`: de migraties hebben een versie en een checksum (`schema_migrations.checksum`). Een gewijzigde migratie geeft `MigrationChecksum`. **Een bestaande migratie kan dus niet aangepast worden.** `rebuild: true` draait met foreign keys uit en controleert daarna `foreign_key_check`.
- **0014** (`0014_company.sql` + `company_convert.rs`): maakt de impliciete company. Managers worden employees, en de migratie maakt `departments`, `employees.department_id`, `proposals.department_id` en het scope-type `department`. `company_convert::convert` maakt afdelingen uit de oude projecten.
- **0017** (`0017_group_chat.sql`): voegt het conversatietype `group` toe en **maakt in de migratie zelf** de Team chat-conversatie aan (unieke index `one_group_conversation`). `Store::open` maakt een ontbrekende group-rij opnieuw aan (AUR-BUG-03).
- Backup: `backup::before_migration` (aangeroepen in `crates/daemon/src/lib.rs:245`) maakt een `pre-update-<ts>`-backup (`VACUUM INTO` plus kopie van `blobs/`, manifest met sha256). Er worden er 3 bewaard.
- Voorbeeld van een import: bij v0.6 → v0.7 zette `import::upgrade_legacy` de oude map `data/` en `blobs/` opzij als `v0.6-backup/`, maakte een nieuwe database en importeerde projecten, managers, memories, ideeën en terugkerende taken in **één transactie**. Memory kreeg daarbij één audit-event als bron (`import.rs:396`).

### 1.2 Waarom niet "een migratie die tabellen leegmaakt"
1. **`events` is append-only**: de triggers `events_no_update` en `events_no_delete` (0014) blokkeren het. Vrijwel alles verwijst naar `events(id)`: `messages.source_event_id`, `memory_sources.event_id`, `retrieval_chunks.source_event_id`, `config_overrides.source_event_id`, `file_grants`, `jobs`, `approvals`, `notifications`, `shares` en `usage_records`. Leegmaken betekent de triggers droppen en een volgorde van ongeveer 25 tabellen correct leegmaken, met grote kans op verweesde of kapotte referenties.
2. **Blobs op schijf** (`blobs/`) van gesprekken, artifacts, rapporten en bijlagen blijven staan. Daarvoor is een aparte GC nodig, anders blijft er gevoelige inhoud achter.
3. **FTS** (`chunk_fts`), `conversation_sessions` (Claude Code-sessies), `run_traces`, `webhook_outbox` en `app_key_threads` moeten allemaal meegenomen worden. 0015 laat zien hoeveel werk dat al is voor één conversatietype.
4. **Terugdraaien** kan alleen met een volledige restore. Bij een import blijft het origineel gewoon onaangeroerd staan.
5. De gegevens die we **behouden** zijn klein en goed afgebakend (employees, projecten, memory en configuratie). Selectief kopiëren is eenvoudiger en beter te testen dan selectief wissen.

### 1.3 Advies: een nieuwe database met import ("fresh start")

**Mechanisme** (nieuwe module `crates/storage/src/fresh_start.rs`, naar het voorbeeld van `import.rs`):

1. **Detectie bij het opstarten** (`daemon/src/lib.rs`, vóór `Store::open`, net als `upgrade_legacy`): bestaat er een database met `MAX(version) BETWEEN 14 AND 17`, dan is het een protocol 3-werkruimte en volgt de fresh start. Een database ≤ 13 wordt eerst **op een kopie** naar 17 gemigreerd, met de bestaande keten. De keten blijft daarvoor nodig.
2. **Backup en opzij zetten**:
   - de bestaande `before_migration`-backup (`pre-update-<ts>`) zoals nu;
   - daarna gaan `data/` en `blobs/` met een rename naar `<root>/pre-v0.11/` (de naam volgt de release). Bestaat die map al, dan wordt er geweigerd met een duidelijke melding, net als bij `v0.6-backup`. Identiteitsbestanden (`host_id`, `local_principal_id`, `environment`) worden **gekopieerd**, zodat de installatie en de koppelingen dezelfde blijven.
3. **Nieuwe database**: `Store::open` op een lege `data/` draait 0001–0018. Op een lege database zijn 0014 en `company_convert` een no-op, en 0017 maakt een lege Team chat. Dat is precies wat we willen: de Team chat wordt niet overgenomen. 0018 legt het Fase 1-schema neer (§1.4).
4. **Import in één transactie** (de oude database alleen-lezen openen, alles eerst lezen en dan schrijven, zoals bij `import.rs`). Overgenomen wordt:

| Tabel(len) | Overnemen | Opmerking |
|---|---|---|
| `scopes` (global, project, employee) | ja, met dezelfde ids | `department`-scopes **niet** |
| `projects` + description-blob | ja, met dezelfde ids, `folder` en `status` | projectmappen op schijf blijven onaangeroerd |
| `employees` + identity- en instructions-blobs, `config_json`, `folder`, `title`, `reports_to`, `status` | ja, met dezelfde ids | `department_id` vervalt. `reports_to` eerst NULL en daarna gezet (FK op zichzelf) |
| `employee_instruction_versions` | ja | de geschiedenis van de instructies (nodig voor `propose_instructions` en `base_version`) |
| `memory_items` (status active, proposed, superseded, pinned) + content-blobs | ja | **department-scope → global (company) scope**, zie 1.5. `forgotten` en `rejected` **niet** |
| `retrieval_chunks` / `chunk_fts` | opnieuw opbouwen met `memory::index` | geen embeddings kopiëren |
| `memory_sources` | één audit-event `memory.imported` als bron | zoals in `import.rs:396` |
| `config_overrides` (run-standaarden per scope) | ja, behalve die van de department-scope | `source_event_id` → het import-event |
| `settings` (onder andere `follow_org_chart`) | ja | |
| `connections` | ja | het secret blijft in de OS-keychain (`secret_ref`) |
| `employee_plugins`, `project_plugins` | ja | |
| `heartbeats` | ja, maar **uitgeschakeld** importeren | een heartbeat maakt werk buiten het bord. Opnieuw aanzetten doet de board member |
| `remote_clients`, `push_registrations` | ja | gekoppelde telefoons hoeven niet opnieuw te koppelen. In Fase 3 worden ze aan een user gekoppeld |
| `app_keys` | ja (zonder `app_key_threads` en `webhook_outbox`) | externe integraties blijven werken |
| `notes` (projectnotities) | ja | dit is projectkennis, geen werk |
| `ideas` | alleen `open`, met `task_id = NULL` | worden in Fase 1 geen tickets. Een ticket maken uit een idee is een latere optie |
| `recurring_tasks`, `agent_functions` | ja, maar **gepauzeerd** | die maken taken buiten het bord. Ze omzetten naar tickets is een open punt (§7) |
| `tasks`, `runs`, `conversations`, `messages`, `approvals`, `artifacts*`, `attachments`, `shares`, `run_*`, `usage_records`, `jobs`, `notifications`, `proposals`, `events` (geschiedenis), Team chat | **nee** | blijven alleen in `pre-v0.11/` |

5. **Rapport en controle**: het importrapport (aantallen per soort plus wat overgeslagen is) gaat naar de log en als notificatie naar de clients. Er is een `--dry-run` via de bestaande binary `aurelius-import` (uitbreiden met een subcommando `fresh-start`), zodat Sébastien het rapport ziet **vóór** de echte overgang.
6. **Claude Code-sessies**: de oude sessiebestanden in `~/.claude` blijven staan maar worden niet meer gebruikt. `conversation_sessions::claim_root` werkt op de nieuwe database. Opruimen is optioneel (een idee voor later).

### 1.4 Migratie 0018: het Fase 1-schema (`rebuild: true`)
Draait in de praktijk alleen op een lege database, maar wordt toch correct als rebuild geschreven, zodat testfixtures en een eventuele andere volgorde veilig blijven.

1. **`company`**: `id`, `scope_id` (verwijst naar de bestaande global scope: company-memory = global memory), `name`, `dispatcher_employee_id` (de standaard-dispatcher, NULL = nog niet gekozen), `version`, `created_at_ms` en `updated_at_ms`. Er is precies één rij (unieke index op een constante, zodat er later meerdere companies mogelijk zijn zonder schemawijziging van de rest).
2. **`users`** (de board members, zie §4b.1 voor de kolommen): direct na `company`, omdat `tickets.created_by_user` ernaar verwijst.
3. **`board_columns`**: `id`, `company_id`, `kind` CHECK IN (`backlog`,`todo`,`doing`,`plan_review`,`review`,`done`), `name`, `position`, `version`. Het gedrag hangt aan `kind`, niet aan de naam: namen en volgorde zijn instelbaar, de zes soorten liggen vast (§7, open punt). Er komen zes standaardrijen.
4. **`tickets`**: `id`, `company_id`, `project_id`, `number` (doorlopend per company, voor weergave), `title`, `description_blob_id`, `column_id`, `position` (REAL, voor de volgorde binnen een kolom), `priority`, **`created_by_user` TEXT NOT NULL REFERENCES users(id)** (de actieve board member van de verbinding die het ticket maakte, §4b.3), `owner_employee_id`, `run_json` (modus en model), `labels_json`, `attachments_json`, `deadline_ms`, `root_task_id` (de huidige hoofdtask), `review_feedback` (laatste), `version`, `created_at_ms`, `updated_at_ms`, `started_at_ms`, `done_at_ms` en `archived_at_ms`.
5. **`tasks`**: rebuild met
   - `ticket_id TEXT REFERENCES tickets(id)`: gezet op de hoofdtask **en geërfd door alle delegaties eronder** (in `insert_part`). Daardoor vind je snel "de gekoppelde tasks" en in Fase 3 de Claude-login van de aanmaker voor elke run;
   - `waiting_reason` CHECK uitgebreid met `'plan_review'`.
6. **`projects.dispatcher_employee_id`** (NULL = de standaard van de company).
7. **Departments verwijderen**: `employees` en `proposals` opnieuw opbouwen zonder `department_id`, `scopes` opnieuw opbouwen met CHECK `kind IN ('global','project','employee')`, `DROP TABLE departments` en de index `employees_department` verwijderen.
8. **Seed**: de company-rij (naam uit de instellingen of "Company"), de zes kolommen en `company.dispatcher_employee_id` = de employee die "Orion" heet als die bestaat (dit gebeurt in de import, omdat de database bij 0018 nog leeg is). **Geen** users: die bestaan in de oude database niet, dus na de upgrade start elke client in de onboarding (§4b.4).

`MIGRATIONS` krijgt `Migration { version: 18, …, rebuild: true, then: None }`. `LAST_PROTOCOL_2_VERSION` blijft 13.

### 1.5 Department-memory naar company-memory
In de import (niet in SQL, omdat het om een mapping gaat): voor elke `memory_items`-rij met een scope van type `department` wordt `scope_id` de global scope. Daarnaast:
- `locator_json` krijgt `"from_department": "<naam>"`, zodat de herkomst zichtbaar blijft in de memory-inspector;
- **dubbels** (dezelfde `content_hash` als een bestaand global-item): het oudste houden en het andere als `superseded` markeren met `supersedes_id`. Niets wordt weggegooid;
- de aantallen per afdeling komen in het importrapport.

### 1.6 Wat er met 0014 en 0017 gebeurt
- **Beide blijven byte-voor-byte ongewijzigd** (checksum). Ze blijven nodig om een oudere database (≤ 13 of 14–16) eerst op een kopie naar 17 te brengen voordat er geïmporteerd wordt.
- **0014**: de tabel `departments` en de kolom `department_id` bestaan dus kort, tot 0018 ze verwijdert. `company_convert.rs` blijft bestaan (alleen relevant voor oude databases). De eigenlijke "company" uit 0014 (één organigram, employees zonder project) blijft het model. 0018 maakt die alleen expliciet met een tabel.
- **0017**: de Team chat-conversatie (`kind='group'`) die deze migratie maakt, is in de nieuwe database **leeg**: de Team chat wordt niet overgenomen. De herstelcode in `Store::open` blijft. In Fase 2 (kanalen) wordt de group-conversatie het algemene kanaal. Daarvoor is dan een migratie nodig die de index `one_group_conversation` aanpast.

### 1.7 Backup en terugweg
1. **Vóór de installatie**: een handmatige backup (`BackupCreate`, de knop bestaat op desktop) en de dry-run van de fresh start met het rapport.
2. **Automatisch bij de start**: de `pre-update`-backup (zoals nu) **en** de volledige oude map in `pre-v0.11/`, die nooit automatisch gepruned wordt.
3. **Terugdraaien**: de daemon stoppen, `data/` en `blobs/` vervangen door die uit `pre-v0.11/` en de vorige build installeren. Dat wordt beschreven in de release-notes.
4. **Risico**: een restore van een **oude** backup (schema 17) via de restore-functie in de nieuwe build start bij de volgende start opnieuw een fresh start, en het werk in de nieuwe werkruimte wordt dan opzij gezet in plaats van gewist. Zet bij `request_restore` daarom een waarschuwing als `manifest.schema_version < 18`.

### 1.8 Bestanden, risico's en schatting (onderdeel 1)
- **Bestanden**: `migrations/0018_board.sql` (nieuw), `crates/storage/src/{migrate.rs, fresh_start.rs (nieuw), import.rs (gedeelde helpers), backup.rs, lib.rs (Store::open), bin/aurelius-import.rs}`, `crates/daemon/src/lib.rs` (opstartvolgorde), `crates/storage/src/tests.rs` plus een nieuwe testfixture "database van protocol 3 met alles erin".
- **Risico's**: (1) een vergeten tabel met nuttige configuratie, opgevangen met een tabel-voor-tabel-checklist in een test die faalt bij een nieuwe tabel zonder beslissing; (2) een FK-volgorde in de import (`reports_to`), opgevangen met een tweede pass en `foreign_key_check`; (3) memory waarvan `locator_json` naar verdwenen taken of gesprekken wijst (de clients moeten een ontbrekende bron netjes tonen); (4) schijfruimte: de oude map blijft volledig staan.
- **Schatting**: 0018 1,5 d · fresh-start-import + dry-run + rapport 2,5 d · backup/opstartvolgorde + tests 1 d = **5 d (Schema)**.

---

## 2. Protocol en storage

### 2.1 Wat verdwijnt (departments)
| Laag | Wat verdwijnt |
|---|---|
| domain | `crates/domain/src/department.rs` (volledig), `DepartmentId` (`ids.rs`), `MemoryScope::Department` (`memory.rs:105`), `SetEmployeeDepartment` (`employee.rs`), `department` in `ProposeColleague`/`proposal.rs`, de `department_*`-varianten in `CommandResult` (`command.rs:95-98`), de events `department.created/updated/archived` en `employee.department_set` (`events.rs:45-49`), de department-velden in `views.rs` (33 vermeldingen) en `org.rs` |
| storage | `departments.rs` (volledig), `department_of` in `employees.rs`, `department_read_scopes` en de department-tak in `memory.rs`, `proposals.rs` (department_id), `timeline.rs`, `company_convert.rs` blijft (alleen voor oude databases) |
| application | `org/department.rs` (volledig), de department-paden in `org/commands.rs`, `org/employee.rs`, `memory/commands.rs`, `memory/memory_context.rs`, de groepering per afdeling in `task/tasks.rs` (`colleague_lines`) |
| prompts | `run/plan.rs:58` (`CHAT_GUIDANCE`: "…or your department's (remember with scope employee or department)"), `plan.rs:402-404` ("You are in the department …"), `colleague_lines` (de koppen "No department:") |
| agent-bridge | `mcp.rs:131` (`propose_colleague.department`), `mcp.rs:295` (de enum `scope` zonder `department`), `mcp.rs:302` (de beschrijving van recall) |
| protocol | `Method::DepartmentList`, `department_id` in `EmployeeList`, `TaskList`, `MemoryList`, `MemoryContext` en `ProposalList` (`method.rs`), `ResponseBody::Departments`/`DepartmentsSnapshot` (`rpc.rs:298`), de commando's `CreateDepartment`/`UpdateDepartment`/`ArchiveDepartment`/`SetEmployeeDepartment` (`remote.rs:246-249`, `app.rs:37,115`), de department-velden in `CompanyGet` |
| client (Rust) | `crates/client/src/lib.rs:655-671` (`departments()`, de parameter `employees(department_id)`) |

### 2.2 Wat erbij komt
**Domain-types** (nieuw `crates/domain/src/ticket.rs`): `TicketId`, `BoardColumnId`, `ColumnKind`, `TicketView`, `BoardView` (kolommen met kaarten), `CreateTicket`, `UpdateTicket`, `MoveTicket`, `StartTicket`, `AssignTicket`, `ApprovePlan`, `RevisePlan`, `AcceptTicket`, `SendBackTicket`, `ArchiveTicket`, `UpdateBoardColumns`, `SetDispatcher`. `CompanyView` krijgt `id`, `name` en `dispatcher_employee_id`.

**Protocol** (`crates/protocol/src/method.rs`, `write.rs`, `rpc.rs`, `events.rs`):

| Soort | Naam | Werking |
|---|---|---|
| read | `board.get { project_id? }` | kolommen + kaarten (gefilterd per project of alle projecten) |
| read | `ticket.get { ticket_id }` | ticket + hoofdtask + gekoppelde tasks (via `tasks.ticket_id`) + de laatste feedback |
| read | `ticket.list { project_id?, column?, query? }` | lijst/zoeken (mobiel, Needs you) |
| write | `ticket.create` | in Backlog (of meteen gestart met `start: true`) |
| write | `ticket.update` | titel, beschrijving, prioriteit, labels, deadline (ms sinds 1970, vóór het jaar 10000), modus/model. De eigenaar **niet**: die alleen via `ticket.assign` |
| write | `ticket.move { ticket_id, column_id, before_ticket_id? }` | slepen: boven de kaart `before_ticket_id` (geen: onderaan). Een kaart boven zichzelf in de eigen kolom blijft staan (geen event). Onbekende velden (het oude `position`) worden geweigerd, zoals bij alle ticket-/boardcommando's. Naar Todo = Start. Binnen een kolom = alleen de volgorde. Board members mogen Done **niet** via move bereiken (alleen via `ticket.accept`), en vanuit Review terug naar Doing alleen via `ticket.send_back` |
| write | `ticket.start { ticket_id, employee_id? }` | de knop Start. Met een employee wordt de dispatcher overgeslagen |
| write | `ticket.assign { ticket_id, employee_id?, reason? }` | eigenaar kiezen vóór de hoofdtask loopt (Backlog, of Todo zolang de hoofdtask Draft is) → `ticket.dispatched`. `employee_id: null` = terug naar "de dispatcher kiest", **alleen in Backlog** → `ticket.updated { changed: ["owner"] }` (geen `ticket.dispatched`: niemand werd gekozen) |
| write | `ticket.approve_plan { ticket_id, decision: approve|approve_edits, feedback? }` | Plan review → Doing |
| write | `ticket.revise_plan { ticket_id, feedback }` | het plan opnieuw maken (blijft in Doing tot het nieuwe plan er is) |
| write | `ticket.accept { ticket_id, note? }` | Review → Done (alleen board members) |
| write | `ticket.send_back { ticket_id, feedback }` (feedback verplicht) | Review → Doing |
| write | `ticket.cancel { ticket_id }` | hoofdtask annuleren → Backlog |
| write | `ticket.archive { ticket_id }` | uit het bord halen (Done of Backlog) |
| write | `board.columns_update { columns[] }` + `expected_version` | naam/volgorde. De boardversie = de hoogste `version` van de kolommen (ze wijzigen samen); een oude versie geeft `version_conflict`. Antwoord `board_columns_updated { version }`; het event bevat de kolommen (id, soort, naam, positie) |
| write | `company.update { name?, dispatcher_employee_id? }` | |
| write | `project.update` + `dispatcher_employee_id` | per project |
| events | `ticket.created`, `ticket.updated`, `ticket.moved {from,to,by}`, `ticket.started`, `ticket.dispatched {employee, by, reason}`, `ticket.plan_review`, `ticket.review`, `ticket.accepted`, `ticket.sent_back`, `ticket.cancelled`, `board.columns_updated` | `EventKind` + Dart |

- **Rechten** (`remote.rs`, `app.rs`): ticket- en boardcommando's vallen onder dezelfde capability als taken maken. `ticket.accept` en `ticket.send_back` worden **niet** toegestaan voor app keys met `audience: management` en niet voor employees (de bridge), zodat de reviewpoort echt bij een mens ligt. Beide `match`-blokken zijn exhaustief, dus de compiler dwingt af dat elke methode een keuze krijgt.
- **Employees** krijgen in Fase 1 **geen** eigen tool om kaarten te verplaatsen. De engine verplaatst ze automatisch tot Review (§3). Een `move_ticket`-tool hoort beter bij de ticketdiscussie (Fase 2).
- **`created_by_user`**: ~~de principal van de sessie~~ (vervallen in T3). Het is de **actieve board member van de verbinding** (`CommandContext.user_id`, §4b.3), NOT NULL. Het staat in `TicketView` en, samen met de uitvoerder van elke kolomwijziging, in de event-metadata (`ctx`). Ter correctie: de principal was ook geen bruikbare auteur, want `api/writes.rs:31` zet voor **elke** verbinding (lokaal én remote) `shared.principal_id`, dus alle apparaten waren dezelfde "auteur".
- **Dart**: `cargo xtask gen-dart` genereert `clients/mobile/lib/data/protocol/{models,methods}.dart` opnieuw. De drift-test in xtask vangt een vergeten generatie op.

### 2.3 Storage
Nieuw `crates/storage/src/tickets.rs` (CRUD, `board()`, `move_to(column, position)`, `tickets_of_task`, `ticket_of_task` via `tasks.ticket_id`), `company.rs` (de rij plus de dispatcher) en `board_columns`-functies. In `tasks.rs`: `insert_task(…, ticket_id)`, een `task_views`-JOIN voor `ticket_id`/`number`.

### 2.4 Schatting (onderdeel 2)
Departments verwijderen (domain/storage/application/prompts/bridge/protocol + tests aanpassen): **2,5 d (Forge)**. Ticket-domain, storage en protocol + rechten + gen-dart + events: **3 d (Forge)**. **Risico's**: veel tests verwijzen naar `department_id: None` (ongeveer 40 daemon-tests met één vermelding, bijna altijd een parameter `TaskList`/`EmployeeList`). Dat is mechanisch, maar het moet in één commit om conflicten te vermijden (RULE 09).

---

## 3. Task-engine

### 3.1 Huidige situatie (vastgesteld in de code)
- `can_transition` (`crates/domain/src/task.rs:59`) kent al `Running→Review`, `Review→Succeeded` en `Review→Queued`. **Het wordt echter alleen afgedwongen in `task_transition`** (`application/src/task/tasks.rs:567`, gebruikt door `daemon/src/tasks.rs:141`). Alle andere statuswijzigingen lopen via `task_line::set_status`, dat **niet** controleert. Zo doet `complete()` `Waiting→Review`, wat volgens `can_transition` niet mag. Daarnaast zijn er twee rechtstreekse `UPDATE`s: `tasks.rs:470` (terug naar draft) en `agent_functions.rs:468` (alleen in een test).
- `complete()` (`task_runs.rs:223`) bewaart het rapport en loopt dan **in één transactie** `Queued→Running→Review→Succeeded`. Review is dus een doorgangsstatus. Daarna volgen de memory-extractie, het vrijgeven van afhankelijke taken en `after_ended` (het resultaat naar de parent of de origin-conversatie).
- `on_run_finished` (`task_runs.rs:78`) bepaalt het verloop: failed, stopped → Waiting(input), kinderen open → Waiting(dependency), ongeziene resultaten → nieuwe run, anders `complete()`.
- `inherit_mode` (`task_runs.rs:807`): Plan- en Ask-modus worden **niet** doorgegeven aan delegaties, Auto en AcceptEdits wel.
- Plan: `ExitPlanMode` wordt een approval (`claude.ExitPlanMode`) met **TTL 30 minuten** (`approvals.rs:18`). De run blijft zolang leven en wacht in `daemon/src/permissions.rs::plan_decision`. Approve schakelt de sessie naar Auto of AcceptEdits. Bij expiry geldt een deny.

### 3.2 `can_transition`
Voorstel voor de nieuwe matrix (`**…**` = nieuw):
```
Draft→Queued · Queued→Running · Running→Review · Review→Succeeded · Review→Queued
(Queued|Running|Review)→Waiting · Waiting→Queued
**Waiting→Review**            (rapport na wachten op collega's, zoals complete() nu al doet)
**Waiting(plan_review)→Queued** (zit al in Waiting→Queued; de reden telt in de engine, niet in de matrix)
**Queued|Waiting→Draft**      (ticket terug naar Todo zonder eigenaar; vervangt de rauwe UPDATE in tasks.rs:470)
_→Failed · _→Cancelled
```
En belangrijker: **`task_line::set_status` gaat `can_transition` controleren** (in debug als `debug_assert!`, in release een log met `warn` en een event `task.invalid_transition`, maar de wijziging gaat wel door, zodat een bestaande, ongeziene volgorde niet plots blokkeert). Zo wordt de matrix echt de bron van waarheid voordat de reviewpoort erop rust. De test `domain/src/task.rs:258` wordt uitgebreid.

### 3.3 `complete()` met een echte Review
Splitsen in twee functies:
1. **`report(tx, st, task, text)`**: rapport opslaan (zoals nu, regels 224-238) en `→Review`. Daarna:
   - **is de taak de hoofdtask van een ticket** (`parent IS NULL AND ticket_id IS NOT NULL`)? Dan **stopt het hier**: event `task.review`, het ticket gaat naar de kolom Review, en er komt een notificatie "Needs you: review" (bestaande notificatie-infrastructuur). Er is geen extractie, er worden geen afhankelijke taken vrijgegeven en er is geen `after_ended`;
   - **anders** (een delegatie of een taak zonder ticket): meteen `accept()`. **Zo blijven delegaties exact zoals nu.**
2. **`accept(tx, st, task, note)`**: `Review→Succeeded`, `task.succeeded` (`reason: "reported"` voor automatisch, `"accepted"` + de actor voor een board member), memory-extractie (dit is meteen de "samenvatting naar project-memory na Done" uit de productdefinitie), afhankelijke taken vrijgeven en `after_ended`. Aangeroepen door `ticket.accept`.

**Terugsturen** (`ticket.send_back`): `Review→Queued`, de feedback als bericht in de taakconversatie (`thread::post_returning`, met speaker de board member), `→Running` en `run_for(...)`, hetzelfde patroon als `continue_when_done`. De ticketkolom wordt Doing en `tickets.review_feedback` wordt gezet. De Claude-sessie van de conversatie loopt door (`conversation_sessions`), dus het model heeft zijn eigen werk nog in de context.

**Aandachtspunten in de bestaande code:**
- `is_terminal()` blijft gelijk (Review is niet terminal). `open_from`, `live_run` en `link_run` werken dus al met Review.
- **Een bericht in de conversatie van een taak in Review**: `link_run` koppelt de run aan de taak en `on_run_finished` roept opnieuw `report()` aan. Het ticket blijft in Review met een nieuw rapport. Dat is goed gedrag ("een vraag tussendoor"), maar het moet zo gedocumenteerd worden. Een expliciete terugsturing gebeurt alleen via `ticket.send_back`.
- **Een sequentiële employee** (`task_line`): Review is niet Running, dus de beurt komt vrij en de volgende taak start. Dat is gewenst: de employee wacht niet op de board.
- **Escalatie** (`org::escalate`) bij failed of stopped blijft gelijk. "Needs you" krijgt er een categorie *Review* bij.
- **Variants** (`task/variants.rs`): een ticket met varianten blijft buiten scope van Fase 1. Een variant overnemen blijft een gebruikersactie. Er wordt niets automatisch gemerged (RULE 07), dus Done doet **geen** git-actie.

### 3.4 Kolom en taakstatus
**Het principe**: de taakstatus is de waarheid voor de engine, de kolom is wat de board ziet. De engine verplaatst de kaart op **één plek**: een hook `ticket_line::on_status(tx, task, new_status, reason)`, aangeroepen vanuit `task_line::set_status` (het enige knelpunt voor statuswijzigingen) en alleen actief voor de **hoofdtask** van een ticket.

| Kolom | Hoofdtask | Wie verplaatst ernaartoe |
|---|---|---|
| Backlog | geen hoofdtask (of de vorige is `Cancelled`) | een board member (`create`, `cancel`) |
| Todo | `Draft` (wacht op de dispatcher) of `Queued` (wacht op de beurt of de job) | een board member (Start of slepen) |
| Doing | `Running` of `Waiting(input/dependency/quota/approval/…)` | de engine (start), een board member (`send_back`, `approve_plan`) |
| Plan review | `Waiting(plan_review)` | de engine |
| Review | `Review` | de engine (`report()`) |
| Done | `Succeeded` | **alleen een board member** (`ticket.accept`) |
| — | `Failed` | het ticket blijft in Doing met een badge "Failed" en "Needs you". De board member kan opnieuw starten (een nieuwe hoofdtask, de oude blijft gekoppeld via `ticket_id`) of annuleren (→ Backlog) |

Een handmatige verplaatsing door een board member (`ticket.move`) wordt omgezet naar een taakactie: naar Todo = Start (een hoofdtask maken), naar Backlog = de hoofdtask annuleren. Andere verplaatsingen die niet bij de taakstatus passen (bijvoorbeeld Doing → Review met de hand) worden geweigerd met een duidelijke fout.

### 3.5 Delegaties blijven ongewijzigd
- `delegate_task` en `hand_over_task` maken of verplaatsen tasks zoals nu (`employee_action`, `insert_part`). De enige toevoeging: `insert_part` neemt `ticket_id` over van de parent (één kolom extra).
- Subtasks hebben een `parent`, dus `report()` doet meteen `accept()`. Resultaten gaan zoals nu naar de parent (`after_ended` → `post_result` → `continue_when_done`).
- **Hand-over van de hoofdtask**: `tasks.employee_id` verandert. `tickets.owner_employee_id` volgt mee via de hook (event `ticket.updated`). De kolom blijft gelijk.
- **Test-eis**: de bestaande `crates/daemon/tests/delegation.rs`, `subagents.rs` en `tasks.rs` blijven groen **zonder** aanpassingen, behalve het weghalen van `department_id`.

### 3.6 Plan review
**Het probleem met de huidige aanpak voor tickets**: een ExitPlanMode-approval houdt een levend Claude Code-proces vast en verloopt na 30 minuten met een deny. Een board member reviewt soms pas uren later, en een herstart van de daemon maakt de approval ongeldig.

**Voorstel**: een plan van een ticket is een **mijlpaal van de taak, geen live vraag**.
1. Het ticket heeft modus Plan, dus de conversatie van de hoofdtask start in `AgentMode::Plan` (zoals nu via `RunSettings`).
2. De run roept `ExitPlanMode` aan. `permissions.rs::ask` ziet dat de run bij de **hoofdtask van een ticket** hoort en maakt **geen inbox-approval**. Het plan wordt geplaatst in de conversatie (`task_threads.rs:214` doet dat al), en het antwoord aan Claude is een deny met de tekst *"Your plan is handed in for review by the board. Stop now; you will be resumed when it is decided."* De run eindigt dan normaal.
3. `on_run_finished`: de conversatie staat nog in Plan **en** er is een plan ingediend, dus `Waiting('plan_review')`, de kolom wordt Plan review en er komt een notificatie. Dit gebeurt **voordat** `complete()` aan bod komt. Zonder ingediend plan maar met een eindantwoord in Plan-modus (het model sloeg ExitPlanMode over) geldt hetzelfde: het eindantwoord is dan het plan.
4. **Goedkeuren** (`ticket.approve_plan`): de conversatie gaat naar `Auto` of `AcceptEdits` (zoals `PlanDecision::Approve/ApproveEdits` nu doet). Er komt een bericht "Plan approved by …, carry it out." met eventuele feedback, dan `Waiting→Queued→Running` en `run_for`. Dezelfde sessie gaat verder. De kolom wordt Doing.
5. **Herzien** (`ticket.revise_plan`): de modus blijft Plan, de feedback komt als bericht, er volgt een nieuwe run en de kolom wordt Doing tot het nieuwe plan er is.
6. **Afwijzen** = `ticket.cancel` (→ Backlog).
7. **Zonder Plan-modus** wordt Plan review overgeslagen (er is geen ExitPlanMode).
8. **`mode_choice`** (management kiest zelf de modus): als het ticket de modus open laat, kiest de eigenaar met `set_mode` zoals nu. Kiest die Plan, dan volgt dezelfde Plan review.
9. **Delegaties tijdens de planfase**: Plan wordt niet doorgegeven (`inherit_mode`). Een subtask met een eigen ExitPlanMode blijft via de **inbox** lopen zoals nu (geen ticket-hoofdtask). Dat verandert niet.

Dit hergebruikt de modusmachinerie en `conversation_sessions`. Het enige nieuwe is één tak in `permissions.rs::ask` en één in `on_run_finished`. De bestaande inbox-flow voor plannen buiten tickets blijft ongewijzigd (`daemon/tests/plans.rs` en `sessions.rs` blijven groen).

### 3.7 De dispatcher-run
**Trigger**: een ticket komt in Todo **zonder** eigenaar. Er wordt een hoofdtask in `Draft` gemaakt (met de titel, de beschrijving als goal, de modus en het model van het ticket) en een job `ticket.dispatch`.

**Werking**:
1. Bepaal de dispatcher: `projects.dispatcher_employee_id` ?? `company.dispatcher_employee_id`. Is die er niet of is die gearchiveerd, dan blijft het ticket in Todo met "Needs you: choose an employee" (geen run).
2. **Een korte run** van de dispatcher in een eigen, verborgen conversatie per ticket (er komt een nieuw conversatietype `dispatch`, of de verborgen hoofdchat van de employee zoals bij de Team chat; voorkeur voor een eigen type, zodat het werk zichtbaar en auditeerbaar is in het ticket). Systeemtekst: het ticket, de lijst van collega's (`tasks::colleague_lines`, zonder afdelingen) en hun load (`load/`). Tools: **alleen** `assign_ticket { employee, reason }` plus `recall` (bijvoorbeeld "wie deed dit vorige keer"). Modus Plan, geen bestands- of shelltools, een kort budget (bijvoorbeeld max_turns 3).
3. `assign_ticket` controleert dat de employee bestaat, actief is en niet de dispatcher zelf is (tenzij die de enige kandidaat is). Daarna `task_line::set_employee`, `Draft→Queued`, de task-job wordt aangemaakt (dezelfde weg als `assign_task`, `tasks.rs:420`), `tickets.owner_employee_id` wordt gezet en er volgt het event `ticket.dispatched {employee, reason, by: dispatcher}`. De gekozen employee start dan via de bestaande `task.run`-job (→ Doing).
4. **Fallback**: eindigt de run zonder `assign_ticket` of met een fout, dan is er één nieuwe poging met de tekst "Choose with assign_ticket". Daarna blijft het ticket in Todo met "Needs you". Er is geen lus (RULE 08).
5. **Het organigram geldt niet** voor `assign_ticket`: de dispatcher handelt namens de board. Dat is bewust anders dan `hand_over_task` (`org.rs:165`: alleen naar de boss of peers), waardoor Orion via hand-over niet aan bijvoorbeeld Forge zou kunnen geven. Na de toewijzing volgen delegaties door de eigenaar het organigram weer zoals nu.
6. **Een board member die zelf kiest** (`ticket.start { employee_id }` of de eigenaar vooraf invult): er is geen dispatcher-run.
7. **Naamgeving**: in `crates/daemon/src/dispatcher/` bestaat al een "dispatcher" (de run- en job-dispatcher). Noem de nieuwe code daarom `triage` of `ticket_dispatch`, zodat er geen verwarring ontstaat.
8. **Fase 3**: de dispatcher-run gebruikt de Claude-login van de aanmaker van het ticket (via `tasks.ticket_id` → `tickets.created_by_user`), net als de rest van het ticket.

### 3.8 Bestanden, risico's en schatting (onderdeel 3)
- **Bestanden**: `crates/domain/src/task.rs` (matrix + tests), `crates/application/src/task/{task_runs.rs, task_line.rs, tasks.rs, task_threads.rs, ticket_line.rs (nieuw), triage.rs (nieuw)}`, `crates/application/src/run/plan.rs` (prompts voor de dispatcher en het ticket), `crates/daemon/src/{permissions.rs, tasks.rs, dispatcher/mod.rs (de nieuwe job-kind)}`, `crates/agent-bridge/src/mcp.rs` (`assign_ticket`), en nieuwe daemon-tests `tests/board.rs` (het volledige pad), `tests/plan_review.rs` en `tests/triage.rs`.
- **Risico's**: (1) `set_status` gaat controleren en daardoor kunnen bestaande ongeziene overgangen boven water komen (daarom eerst warn in release); (2) Plan review op basis van een deny: het gedrag van Claude Code na een deny op ExitPlanMode moet met `real_claude_plan.rs` bevestigd worden (het model moet echt stoppen en niet in Plan verder schrijven); (3) races tussen `ticket.send_back` en een run die nog loopt (vermijden met `live_run` en een `fenced_task`, zoals in de bestaande code); (4) de kosten en het quotum van de dispatcher-run per ticket (beperkt met het kleine budget en zonder bestandstools).
- **Schatting**: matrix + controle 0,5 d · `report`/`accept` + send_back + kolomhook 2 d · Plan review 1,5 d · dispatcher 1,5 d · tests (het volledige pad, delegaties ongewijzigd, plan, dispatcher, races) 1,5 d = **7 d (Forge)**.

---

## 4. Clients

### 4.1 Desktop (`clients/desktop`, gpui; de verbinding via `crates/client`)
**Wat verdwijnt**: `screens/org.rs` (56 vermeldingen: afdelingen beheren, de head, de memory van de afdeling), department-delen in `app.rs` (87), `backend.rs` (36), `screens/employees.rs` (19), `tasks.rs` (16, filter per afdeling), `dashboard.rs` (16), `overlays.rs` (14, onder andere "nieuwe afdeling"), `keymap.rs` (6), en kleine stukken in `projects.rs` en `plugins.rs`. Het organigram blijft, alleen via `reports_to`.

**Wat erbij komt**:
- **Het bord** (een nieuw scherm `screens/board.rs`): kolommen naast elkaar, kaarten (titel, project, prioriteit, eigenaar, labels, deadline, badges voor live run, Failed en Needs you), een filter per project, Start op een kaart en **slepen tussen kolommen**. gpui heeft `on_drag`/`drag_over`/`on_drop` (gebruikt in `resize.rs:67` en `group_chat.rs:41`), maar nog niet voor slepen tussen lijsten. Reken op een kleine spike. Als terugval is er een menu "Move to …".
- **De ticketweergave** (paneel of scherm): velden, de gekoppelde tasks (boom), de conversatie van de hoofdtask (hergebruik van `screens/chat.rs`), knoppen per kolom: *Start*, *Approve plan / Approve edits / Revise*, *Accept (Done) / Send back with feedback*, *Cancel*.
- **Een nieuw ticket** (overlay): titel, beschrijving, project, prioriteit, eigenaar (optioneel), modus/model, labels, deadline en bijlagen (hergebruik van de bestaande uploadflow van taken).
- **Instellingen**: kolomnamen en volgorde, de standaard-dispatcher (company) en de dispatcher per project.
- **Needs you** en het dashboard: nieuwe categorieën *Plan review* en *Review*.
- **Schatting**: departments eruit 1,5 d · bord + slepen 3,5 d · ticketweergave/aanmaken/review 2,5 d · instellingen + Needs you 0,5 d = **8 d (Flux)**. `app.rs` (8.285 regels) is het conflictrisico: één eigenaar tegelijk.

### 4.2 Flutter (`clients/mobile`, Flutter 3.47.9 in `~/development/flutter`)
**Wat verdwijnt**: `features/company_org/department_page.dart` en `department_memory.dart` (volledig), department-delen in `company_org_page.dart`, `employee_widgets.dart`, `employee_page.dart`, `features/company/company_bloc.dart`, `data/company.dart`, `data/commands.dart`, `data/aurelius_repository.dart`, `features/tasks/tasks_page.dart`, `project/new_task_page.dart`, `app/router.dart`, en de tests `protocol_test.dart`, `company_org_test.dart`, `company_at_work_test.dart`, `widgets_test.dart` en `support/builders.dart`. `data/protocol/models.dart` (214) en `methods.dart` (41) worden **gegenereerd**.

**Wat erbij komt**:
- **Het bord**: op een telefoon één kolom tegelijk met tabs of een horizontale `PageView`, met een teller per kolom. Verplaatsen met **lang indrukken + slepen** (`LongPressDraggable`/`DragTarget`, `ReorderableList` wordt al gebruikt in `projects_page.dart`) **en** altijd een actie "Move to …" in het menu van de kaart (toegankelijkheid, kleine schermen).
- **De ticketpagina**: velden, de gekoppelde tasks, de conversatie van de hoofdtask (hergebruik van `features/chat/chat_view.dart`) en de review-acties zoals op desktop.
- **Een nieuw ticket** (een variant van `new_task_page.dart`), en de instellingen voor de dispatcher (company en project).
- **Schatting**: departments eruit 1,5 d · bord 3 d · ticketpagina + aanmaken + review 2,5 d · instellingen 0,5 d = **7,5 d (Bridge)**.

### 4.3 Compatibiliteit tussen client en server
- **De handshake** controleert alleen de major (`rpc.rs:21`, mobiel `connection.dart:313`, `methods.dart:8`). Er zijn **geen minors**. Toevoegingen binnen een major worden aangekondigd via `HelloResult::features`.
- **Advies: PROTOCOL_MAJOR 4.** Het verwijderen van `department.list`, `department_id` in methodes en views en de department-commando's is een **breaking change**. Een oude client op major 3 zou anders `unknown_method` (zie AUR-BUG-03: `reject_unparsed`) of parse-fouten op gewijzigde views krijgen, zonder duidelijke melding. Met major 4 tonen beide clients de bestaande, duidelijke melding "Update needed … Update the app" (`incompatible_message`).
- **Een nieuwe client tegen een oude server** krijgt "Update the server". De desktop wordt samen met de server geïnstalleerd (dezelfde build), dus het probleem zit vooral bij **de mobiele app en desktops die met een andere server verbinden** (`screens/servers.rs`).
- **De volgorde bij de uitrol**: eerst de server (met de fresh start), dan de desktop (dezelfde installatie), dan de mobiele app via de store of een sideload. Tussendoor toont de oude mobiele app "Update needed". Dat is acceptabel, want Sébastien is in Fase 1 de enige gebruiker.
- **Gekoppelde apparaten** blijven gekoppeld (`remote_clients` wordt geïmporteerd). Er is dus geen nieuwe pairing nodig na de update van de app. Wel komt elke client na de upgrade eerst in de onboarding of de keuze van een board member (§4b.4).
- **App keys en webhooks**: `ticket.*`-events gaan **niet** standaard naar webhooks (die blijven `manager.replied` en andere bestaande events). Het management-kanaal (`app.rs`) mag tickets lezen maar niet accepteren of terugsturen.
- **Features binnen major 4**: Fase 2 (discussie, kanalen) en Fase 4 (vault) kunnen als `features` binnen major 4 worden aangekondigd. Fase 3 (login) is waarschijnlijk opnieuw een major (auth in de handshake).

---

## 4b. Board members en onboarding, Fase 1-minimum (onderdeel S5)

*Toegevoegd in AUR-MU-00-T3. Bron: multi-user.md §2 "Toegang" en §3 "Onboarding (beslist)", Nova MU1-20, MU1-21, MU1-22, OV-12, OV-13 en de tabel "Scheiding onboarding en login", Pixel §11 en P-07/P-08.*

**De grens.** Fase 1 bouwt alleen wat nodig is om elke ticketactie een echte board member als auteur te geven: een tabel `users`, board members aanmaken, een actieve board member per client en een gate in de clients. **Fase 1 is identificatie, geen authenticatie.** Sessies, uitloggen, een apparaat koppelen aan een user, auteurschap van elk bericht en een Claude-login per user blijven Fase 3.

### 4b.1 De tabel `users` (in 0018)
Advies: **in 0018**, en niet in een aparte migratie. 0018 is toch nieuw en draait alleen op een lege database (§1.3), en `tickets.created_by_user` krijgt meteen een FK. Met een aparte 0019 zou `tickets` eerst zonder FK aangemaakt worden of opnieuw opgebouwd moeten worden.
```sql
CREATE TABLE users (
  id TEXT PRIMARY KEY,                               -- UserId (uuid v7, zoals de andere ids)
  company_id TEXT NOT NULL REFERENCES company(id),
  username TEXT NOT NULL CHECK(length(username) BETWEEN 3 AND 32),
  password_hash TEXT NOT NULL,                       -- PHC-string van argon2id ($argon2id$v=19$m=…,t=…,p=…$salt$hash)
  created_by_user TEXT REFERENCES users(id),         -- NULL = aangemaakt in de onboarding (geen actieve board member)
  version INTEGER NOT NULL DEFAULT 0,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX users_username ON users(username COLLATE NOCASE);
```
- **Uniek per server**, hoofdletterongevoelig (OV-12). Er is één company per server, en een login in Fase 3 hoeft dan geen company te kiezen. De tekenset (letters, cijfers, `.`, `_`, `-`) wordt in het domain gecontroleerd (`Username::parse`), niet in SQL, zodat de foutmelding duidelijk is.
- **Bewust niet in Fase 1**: `disabled_at_ms` en de wachtwoordreset (OV-14, Fase 3). Die komen met `ALTER TABLE … ADD COLUMN` zonder rebuild. Een user wordt nooit verwijderd (deactiveren), zodat `created_by_user` altijd geldig blijft.
- **De fresh start** importeert geen users (die bestaan niet). De backup en de restore nemen de tabel mee zoals elke andere. Een restore van een schema-17-backup leidt opnieuw tot de onboarding (zie het risico in §1.7).

**Wachtwoordopslag**
- De crate `argon2` (RustCrypto, met `password-hash`): **argon2id**, parameters volgens OWASP (m = 19 MiB, t = 2, p = 1), een willekeurige salt per user (`OsRng`), en de PHC-string als waarde. De parameters staan in de string, dus later sterker maken kan zonder migratie (opnieuw hashen bij de volgende succesvolle controle). Nu zit er geen argon2 in `Cargo.lock` (er is alleen een transitieve `pbkdf2`). Het is dus een nieuwe dependency: `cargo deny`/licentie controleren (MIT/Apache).
- Hashen en controleren gebeurt in `spawn_blocking`: argon2 kost ongeveer 20–50 ms CPU en mag de async-runtime niet blokkeren.
- **Het wachtwoord mag nooit in de database of de logs komen, ook niet indirect.** Twee concrete valkuilen in de huidige code:
  1. `request_hash` (`application/src/lib.rs:152`) is een **SHA-256 zonder salt** over het volledige commando en komt in `command_receipts.request_hash`. Zit het wachtwoord in het domain-commando, dan staat er een snel te kraken hash van in de database. 
  2. Elke write wordt een **append-only event** (`events_no_update`/`events_no_delete`). Een wachtwoord of hash in een event-payload kan je niet meer wissen.
  
  Daarom: de protocolhandler hasht **vóór** de domain-laag. Het domain-commando is `CreateUser { username }` en de hash gaat als apart argument naar `storage::users::insert` in dezelfde transactie. Het event `user.created` bevat alleen `{ user_id, username, by }`. De protocoltypes met een wachtwoord (`UserCreateParams`, `UserUseParams`) krijgen een eigen `Debug` die `password: "<redacted>"` toont, zodat ook `tracing` het niet kan lekken. Een test controleert dat het wachtwoord nergens terechtkomt: na `user.create` zoeken we in de volledige database-dump en de log naar de tekst van het wachtwoord.

### 4b.2 Protocolmethodes (binnen protocol 4)
| Soort | Naam | Werking | Actieve board member nodig? |
|---|---|---|---|
| read | `user.list` | `[UserView { id, username, created_at_ms, created_by_user }]`, **nooit** de hash | nee (de keuze "Who's working here?") |
| read | `user.whoami` | `{ board_members_exist: bool, user: Option<UserView> }` voor deze verbinding | nee |
| write | `user.create { username, password }` | valideert (OV-12: 3–32 tekens, wachtwoord ≥ 12 tekens), hasht en maakt de user. **Een verbinding zonder actieve board member wordt meteen die nieuwe user** (MU1-20 AC2: "kom ik in de app"). Een verbinding met een actieve board member blijft zichzelf (MU1-22: iemand anders aanmaken). Fouten: `username_taken`, `invalid_username`, `weak_password` | **nee** (zie hieronder) |
| write | `user.use { user_id, password }` | de **eenmalige wachtwoordcontrole** (OV-13, Pixel P-08): controleert de hash en zet de actieve board member van deze verbinding. Fout: `wrong_password` (dezelfde fout voor een onbekende user), met een kleine vertraging en een limiet per verbinding (5 pogingen per minuut) | nee |
| hello | `HelloParams.user_id: Option<UserId>` | de client geeft de board member mee die hij onthouden heeft. Bestaat die user, dan wordt hij actief **zonder** wachtwoord (de controle gebeurde al eenmalig bij `user.use`) | — |
| hello | `HelloResult.board_members_exist: bool`, `HelloResult.user: Option<UserView>` | de client weet meteen bij het verbinden of hij de onboarding, de keuze of de app toont, zonder extra round-trip | — |
| events | `user.created { user_id, username, by }` | live, zodat een open "Who's working here?" of de instellingen meteen bijwerken | — |

- **Waarom `user.create` geen actieve board member vraagt**: MU1-21 AC2 en Pixel §11.2 laten "New board member" toe vanuit het keuzescherm, dus zonder actieve board member. Het maakt ook MU1-20 AC4 (twee clients tegelijk een eerste board member) eenvoudig: de unieke index beslist, en bij verschillende namen lukken beide. Een aparte "bootstrap"-regel ("alleen als er nog geen users zijn") zou die race juist doen mislukken. Het voegt geen nieuw risico toe, want zonder sessies kan iedereen in de tailnet zich toch al voor een ander uitgeven (risico R2).
- **"Wie ben ik" zit op de verbinding**: `Peer` (`daemon/src/api/mod.rs:49`) krijgt `user: Option<UserId>`, gezet door `hello`, `user.create` en `user.use`. `CommandContext` (`application/src/lib.rs:33`) krijgt `user_id: Option<UserId>`, gevuld in `api/writes.rs:31`. `ctx.metadata()` neemt `user_id` op, zodat elk event van een board member de uitvoerder heeft (MU1-15 AC4, MU1-14 AC3, MU1-21 AC3).
- **Wisselen** ("Switch board member…", Pixel §11.2) is een nieuwe `user.use` op dezelfde verbinding. Er is geen uitloggen in Fase 1.
- **De rechten** (`remote.rs`, `app.rs`): `user.*` mag voor de eigen clients (lokaal en gekoppelde apparaten). App keys mogen `user.list` lezen maar geen `user.create` of `user.use` doen. Een app heeft geen board member.

### 4b.3 `created_by_user` NOT NULL
- Elke write die een board member als auteur vastlegt, vraagt `ctx.user_id`. Zonder actieve board member geeft de server **`board_member_required`**: `ticket.create`, `ticket.update`, `ticket.move`, `ticket.start`, `ticket.approve_plan`, `ticket.revise_plan`, `ticket.accept`, `ticket.send_back`, `ticket.cancel`, `ticket.archive`, `board.columns_update` en `company.update`. Dat is één controle in de ticket-commandhandler (`require_board_member(&ctx)`), niet per methode verspreid.
- `ticket.create` zet `created_by_user = ctx.user_id`. Niemand kan het wijzigen (MU1-07: "Niet te wijzigen"). `ticket.update` heeft het veld niet.
- **Wie anders tickets kan maken**: niemand in Fase 1. Employees niet (Nova OV-05), en app keys ook niet (die hebben geen board member). Die krijgen `board_member_required` en blijven taken gebruiken. Er is dus geen pad waarop een ticket zonder aanmaker ontstaat, en NOT NULL is zonder uitzondering af te dwingen.
- **Bestaande gegevens**: er is geen backfill nodig, want de fresh start neemt geen tickets of taken over (§1.3).
- De **uitvoerder** van een kolomwijziging is een board member (`ctx.user_id`) of een employee (de engine, de dispatcher). Dat komt in `ticket.moved { from, to, by: Actor }` met `Actor::BoardMember(UserId) | Actor::Employee(EmployeeId)`, en niet als kolom in `tickets`.
- **Niet in Fase 1**: andere writes (taken, chatberichten, approvals, instellingen) vragen op de server nog geen board member. Ze nemen `user_id` wel mee in de event-metadata als die er is. Een strikte servercontrole op elke methode heeft zonder sessies geen veiligheidswaarde (R2) en zou ongeveer 40 daemon-testbestanden raken. Die komt in Fase 3, samen met de sessies.

### 4b.4 De gate in de clients
Beide clients beslissen na `hello` op basis van `board_members_exist` en `user`:

| Situatie | Scherm (Pixel §11) |
|---|---|
| `board_members_exist = false` | **"Create the first board member"** (§11.1/§11.3). Verder niets bereikbaar, behalve stoppen en een andere server kiezen (MU1-20 AC1) |
| er zijn board members, `user = None` | **"Who's working here?"** (§11.2): `user.list`, kiezen + wachtwoord (`user.use`), of "New board member" (`user.create`) (MU1-21 AC2) |
| `user = Some` | de app. De naam staat in de server picker ("as sebastien") of bovenaan More (MU1-21 AC4) |
| `user.created` terwijl de onboarding open is (een andere client was eerst) | het scherm blijft werken. Bij een andere naam lukt het gewoon (MU1-20 AC4/AC5) |
| een onthouden `user_id` bestaat niet meer (bijvoorbeeld na een restore) | terug naar de keuze |

- **Onthouden**: de client bewaart **alleen de `user_id`** per server, nooit het wachtwoord. Desktop: naast de serverinstellingen (`screens/servers.rs` en het bestaande configbestand per server). Mobiel: per gekoppelde server in de bestaande opslag van de koppeling (`secure storage`, naast de credential van het apparaat).
- **Desktop** (`clients/desktop`): een nieuw scherm `screens/onboarding.rs` (aanmaken + kiezen + wachtwoordprompt), een gate in `app.rs` vóór de sidebar en de sneltoetsen (zoals het huidige servers- en koppelscherm), "Switch board member…" in de server picker (`sidebar.rs`) en **Board members** (lijst + nieuw, MU1-22) in een nieuw `screens/board_members.rs`. In `crates/client`: `user_list`, `user_whoami`, `user_create` en `user_use`, plus `user_id` in `hello`.
- **Mobiel** (`clients/mobile`): `features/onboarding/` (aanmaakpagina, keuzelijst, wachtwoord-sheet), een redirect in `app/router.dart` naar `/onboarding` (route uit Pixel §2) zolang er geen actieve board member is, de board member bovenaan More (`features/more/`), en `/settings/board-members`. De protocoltypes komen uit gen-dart.
- **Validatie** staat in beide clients als hulp (de ✓-regels van Pixel), maar **de server beslist**. De client toont de foutcode van de server bij het juiste veld.

### 4b.5 Aansluiting op `remote_clients` en Fase 3
- **Fase 1 verandert `remote_clients` niet.** De koppeling van een apparaat blijft de authenticatie van het **apparaat** (`credential_verifier`, `remote_sessions`). De actieve board member staat er los van, per verbinding.
- **Fase 3 bouwt verder op dezelfde tabel `users`.** De ids blijven gelijk, dus alle `created_by_user` en alle `by` in de events blijven geldig. Wat erbij komt:
  1. `user_sessions` (een token per user en apparaat, met een verloopdatum, naar het voorbeeld van `remote_sessions`) en `remote_clients.user_id` (nullable FK, het apparaat van een user);
  2. **`HelloParams.user_id` zonder bewijs wordt geweigerd**: de onthouden `user_id` van Fase 1 wordt dan "log opnieuw in als sebastien" (het wachtwoord één keer, daarna het sessietoken). Dezelfde schermen, dus Pixel §11 blijft bruikbaar;
  3. de servercontrole `board_member_required` op **alle** user-methodes (niet alleen tickets), met een allowlist voor `hello` en `user.*`;
  4. `claude_logins(user_id, config_dir, …)` uit de spike (`spike-claude-logins.md` §3.2): ticketruns zoeken via `tasks.ticket_id` → `tickets.created_by_user` de login van de aanmaker. Daar is in Fase 1 dus al alles voor klaar;
  5. de lokale IPC-verbinding (de desktop op de server zelf): mag die vertrouwd blijven als "de eigenaar van de machine", of moet ook die inloggen? Dat is een open punt voor Fase 3 (§7).
- **Protocol**: Fase 1 voegt alles toe binnen major 4. Fase 3 verandert de betekenis van `HelloParams.user_id` (weigeren zonder bewijs) en wordt daarom waarschijnlijk major 5, zoals al in §4.3 staat.

### 4b.6 Risico's
| # | Risico | Ernst | Maatregel |
|---|---|---|---|
| R1 | **Wachtwoordopslag**: een hash van het wachtwoord lekt via `command_receipts.request_hash` (SHA-256 zonder salt), een append-only event, de tracing van requests of een crashrapport | hoog (onomkeerbaar door de append-only events) | hashen vóór de domain-laag, geen wachtwoord in commando's of events, een redacting `Debug`, en de "zoek het wachtwoord in de dump en de log"-test (§4b.1). Sentinel reviewt de parameters en de redactie |
| R2 | **Zich uitgeven voor een ander**: zonder sessies accepteert de server `HelloParams.user_id` en `user.create` van iedereen die de server bereikt. Iemand in de tailnet met een gekoppeld apparaat (of een aangepaste client) kan tickets maken, accepteren of terugsturen op naam van een andere board member. De eenmalige wachtwoordcontrole beschermt alleen de **gewone** client, niet het protocol | middel in Fase 1 (Sébastien is de enige gebruiker; de tailnet en de koppeling van apparaten blijven de echte toegangscontrole) | **expliciet aanvaarden tot Fase 3** en zo vermelden in de release-notes en de UI ("This device remembers you. Full sign-in comes later.", Pixel §11.2). Geen gevoelige functie (vault, Claude-login per user) vóór Fase 3. Een audit is mogelijk, want elk event heeft `client_id` (het apparaat) en `user_id` |
| R3 | **Raden van wachtwoorden** via `user.use` | laag (tailnet) | een limiet per verbinding en een vertraging, dezelfde fout voor een onbekende user en een fout wachtwoord |
| R4 | **CPU** van argon2 (19 MiB, ~30 ms per poging) | laag | `spawn_blocking`, en de limiet uit R3 |
| R5 | **Twee eerste board members tegelijk** (MU1-20 AC4) | laag | de unieke index beslist. Een test met twee verbindingen |
| R6 | **De restore van een oude backup** zonder users | laag | de client valt terug op de onboarding of de keuze (§4b.4) |
| R7 | **Uitbreiding van de scope**: de verleiding om "toch even" sessies te bouwen | middel (planning) | de grens uit Nova's tabel aanhouden. Alles uit §4b.5 is Fase 3 |

### 4b.7 Bestanden, migratiestappen en schatting (onderdeel S5)
- **Bestanden**:
  - storage (Schema): `migrations/0018_board.sql` (de tabel `users` + de FK op tickets), `crates/storage/src/users.rs` (nieuw: `insert`, `by_username`, `get`, `list`, `count`), tests in `storage/src/tests.rs`;
  - server (Forge): `crates/domain/src/{ids.rs (UserId), user.rs (nieuw: Username, UserView, CreateUser), command.rs, events.rs}`, `crates/application/src/{lib.rs (CommandContext.user_id + metadata), user.rs (nieuw)}`, de ticket-commandhandler (`require_board_member`), `crates/protocol/src/{method.rs, write.rs, rpc.rs (HelloParams/HelloResult), events.rs, remote.rs, app.rs}`, `crates/daemon/src/api/{mod.rs (Peer.user + de afhandeling van hello), writes.rs}`, `crates/client/src/lib.rs`, de dependency `argon2` in `Cargo.toml`, gen-dart, en een nieuwe `crates/daemon/tests/board_members.rs`;
  - desktop (Flux): `clients/desktop/src/{app.rs (gate), backend.rs, screens/onboarding.rs (nieuw), screens/servers.rs (onthouden + wisselen), screens/sidebar.rs (de server picker "as …"), screens/board_members.rs (nieuw; er is nog geen algemeen instellingenscherm, dus de plaats van "Settings → Board members" stemt Flux af met Pixel)}`;
  - mobiel (Bridge): `clients/mobile/lib/{features/onboarding/ (nieuw), app/router.dart (redirect), data/aurelius_repository.dart, features/pairing/ (de opslag per gekoppelde server), features/more/ (de board member bovenaan + Board members)}` + widgettests.
- **Migratiestappen**: alleen 0018 (een tabel `users` en een unieke index; `tickets.created_by_user` NOT NULL met FK). Geen backfill en geen import van users. De keten 0001–0017 blijft ongewijzigd.
- **Tests** (nieuw): onboarding op een lege server, `user.create` zonder actieve board member → actief, twee eerste board members tegelijk, `username_taken` hoofdletterongevoelig, `weak_password`, `user.use` met een fout wachtwoord en de limiet, `hello` met een onthouden en een onbekende `user_id`, `ticket.create` zonder board member → `board_member_required`, `created_by_user` en `by` in de events, een app key mag geen `user.create`, en het wachtwoord staat nergens in de database of de log.
- **Schatting**:

| Wie | Werk | Dagen |
|---|---|---|
| Schema | `users` in 0018 + `storage/users.rs` + tests | 0,5 |
| Forge | domain/application/protocol/daemon (`Peer.user`, `CommandContext.user_id`, `hello`, `user.*`, `board_member_required`), argon2 + redactie, gen-dart, `tests/board_members.rs` | 1,5 |
| Flux | desktop: onboarding, keuze + wachtwoord, onthouden/wisselen, Settings → Board members | 1,5 |
| Bridge | mobiel: idem + router-redirect + widgettests | 1,5 |
| Sentinel | review: argon2-parameters, redactie, OV-12-regels, R2 in de release-notes | 0,5 |
| Pulse | E2E van de onboarding op desktop en mobiel (deel van Q) | 0,5 |
| **Totaal S5** | | **6 d** |

---

## 5. Overzicht per onderdeel

| Onderdeel | Crates/bestanden | Migratiestappen | Belangrijkste risico's | Schatting | Eigenaar |
|---|---|---|---|---|---|
| **S1** Schema 0018 + fresh-start-import + backup | `migrations/0018_board.sql`, `storage/{migrate, fresh_start (nieuw), import, backup, lib}.rs`, `bin/aurelius-import.rs`, `daemon/src/lib.rs` | 0001–0017 ongewijzigd · 0018 company, users, board_columns, tickets (created_by_user NOT NULL → users), tasks.ticket_id + plan_review, projects.dispatcher, departments weg · bij de start: backup → opzij → nieuwe database → import → rapport | een vergeten configuratietabel · FK-volgorde · een restore van een oude backup · schijfruimte | 5 d | **Schema** |
| **S2** Departments verwijderen (server) | domain, storage, application (org, memory, task, run/plan prompts), agent-bridge, protocol, client | 0018 (rebuilds) | brede maar mechanische diff, veel tests | 2,5 d | **Forge** |
| **S3** Ticket-domain/storage/protocol | `domain/ticket.rs`, `storage/{tickets,company}.rs`, `protocol/{method,write,rpc,events,remote,app}.rs`, `client/src/lib.rs`, gen-dart | — | de rechten op accept en send_back · drift in gen-dart | 3 d | **Forge** |
| **S4** Engine: reviewpoort, kolomhook, Plan review, dispatcher | `domain/task.rs`, `application/task/{task_runs, task_line, tasks, ticket_line*, triage*}.rs`, `daemon/{permissions, tasks, dispatcher}.rs`, `agent-bridge/mcp.rs` | — | gedrag van Claude Code na een deny op ExitPlanMode · races · kosten van de dispatcher | 7 d | **Forge** |
| **S5** Board members en onboarding (Fase 1-minimum, §4b) | storage: `0018` (`users`), `storage/users.rs` · server: `domain/{ids,user*,command,events}.rs`, `application/{lib,user*}.rs`, `protocol/{method,write,rpc,events,remote,app}.rs`, `daemon/api/{mod,writes}.rs`, `client/src/lib.rs`, `argon2` · desktop: `app.rs` (gate), `screens/{onboarding*, board_members*, servers, sidebar}.rs` · mobiel: `features/{onboarding*, more, pairing}`, `app/router.dart` | 0018: tabel `users` + unieke index op `username COLLATE NOCASE`, FK vanuit `tickets.created_by_user`. Geen backfill, geen import | wachtwoord(-hash) lekt via `request_hash` of een append-only event · zich uitgeven voor een ander zonder sessies (aanvaard tot Fase 3) · scope-uitbreiding naar sessies | 6 d (Schema 0,5 · Forge 1,5 · Flux 1,5 · Bridge 1,5 · Sentinel 0,5 · Pulse 0,5) | **Forge** (server), Schema, Flux, Bridge, Sentinel |
| **C1** Desktop | `clients/desktop/src/{app, backend, keymap}.rs`, `screens/{org, employees, tasks, dashboard, overlays, board*, ticket*}.rs` | — | slepen tussen lijsten in gpui · conflicten in `app.rs` | 8 d | **Flux** |
| **C2** Mobiel | `clients/mobile/lib/{data, features/company_org, features/tasks, features/project, features/board*, features/ticket*}`, tests | — | slepen op een telefoon (terugval via een menu) · de testsuite (widgets_test) | 7,5 d | **Bridge** |
| **Q** End-to-end + review + QA | `daemon/tests/board.rs`, het pad Backlog→Done met delegaties, de upgrade-test van een kopie van de echte database | dry-run op een kopie van de echte database van Sébastien | — | 3 d | Pulse + Trace (Sentinel kort op de fresh start: secrets, achtergebleven blobs) |
| **Totaal** | | | | **≈ 42 d** (was 36 d; +6 d voor S5) | |

---

## 6. Voorgestelde bouwvolgorde en afhankelijkheden

```
Week 1   Schema: S1 (0018 incl. users + fresh start)    Forge: S2 (departments eruit, server+protocol)
            │                                              │
            └──────────────┬───────────────────────────────┘
Week 2                Forge: S3 + S5-server (ticket- en user-protocol bevriezen; gen-dart)   ← Flux: C1a / Bridge: C2a departments eruit (vanaf S2)
                           │                                                                    Sentinel: review S5 (argon2, redactie)
Week 2-3              Forge: S4 (engine)    ║   Flux: S5-desktop (gate) → C1b bord + ticket   ║   Bridge: S5-mobiel (gate) → C2b bord + ticket
                           │                ║   (tegen het bevroren protocol, met de testserver)
Week 3-4½             Pulse: Q (onboarding, het volledige pad, delegaties, upgrade-dry-run)  →  Trace: review per tak  →  akkoord Sébastien
```
1. **S1 en S2 parallel**: verschillende bestanden. S1 schrijft alleen `migrations/` en de nieuwe storage-modules (inclusief de tabel `users` en `storage/users.rs` van S5), S2 de domain-, application- en protocolcode. Afspraak: alleen **Schema** raakt `migrate.rs` en `storage/src/tests.rs` (RULE 09).
2. **S3 en het serverdeel van S5 na S2, bij Forge, in één tak** (dezelfde bestanden `method.rs`, `rpc.rs`, `remote.rs`, `app.rs` en `api/writes.rs`; `CommandContext.user_id` is nodig voor `ticket.create`). Het eind daarvan is het **bevroren protocol 4**, met `user.*` en de nieuwe `hello`-velden: vanaf dan kunnen de clients aan de nieuwe schermen werken. Sentinel reviewt het S5-deel meteen, vóór de clients erop bouwen.
3. **S4 na S1 en S3** (heeft `tasks.ticket_id` en de ticket-storage nodig).
4. **C1a en C2a** (afdelingen eruit) kunnen starten zodra S2 gemerged is in de featurebranch. Na het bevroren protocol komt **eerst de gate van S5** (onboarding, keuze, wisselen) en **dan C1b en C2b**: zonder actieve board member kan een client geen ticket maken (`board_member_required`), dus de bordschermen hebben de gate nodig om te testen.
5. **Q** na S4 en de clients, inclusief de onboarding op desktop en mobiel (eerste board member, twee tegelijk, keuze met wachtwoord). Eerst de dry-run van de fresh start op een **kopie** van de echte data (nooit op de originele map).
6. Elke stap op een eigen featurebranch die aftakt van de vorige (zoals bij AUR-GC-01). Geen push of merge naar main zonder akkoord (RULE 07).
7. **Het kritieke pad** is Forge: S2 2,5 d + S3 3 d + S5-server 1,5 d + S4 7 d = 14 d, daarna Q en de review. Daarom de doorlooptijd van 3,5 à 4,5 weken (§0). Wil Orion het pad korter, dan kan Nexus het serverdeel van S5 nemen, maar alleen ná S3 in dezelfde tak (dezelfde bestanden, RULE 09). Dat scheelt dan weinig.

---

## 7. Open punten voor Orion, Nova en Pixel

1. **Kolommen instelbaar**: zijn de zes soorten vast (alleen naam en volgorde instelbaar, mijn voorstel), of moeten er eigen kolommen bij kunnen? Eigen kolommen hebben geen gedrag in de engine en zijn dus alleen "Doing-varianten".
2. **Terugkerende taken en agent functions**: die maken nu taken buiten het bord. Mijn voorstel voor Fase 1 is ze gepauzeerd importeren. Moeten ze later tickets maken (in Backlog of meteen in Todo)?
3. **Een failed ticket**: blijft in Doing met een badge (mijn voorstel), of gaat het terug naar Todo?
4. **Taken zonder ticket** (delegaties vanuit de Team chat, de project-assistent, app keys): die blijven in Fase 1 bestaan zonder reviewpoort. Moeten die in een latere fase ook een ticket krijgen? Dit raakt Fase 2 ("vanuit een chat een ticket aanmaken").
5. **De standaard-dispatcher**: als er geen employee "Orion" bestaat (een andere installatie), dan blijft hij leeg en kiest de board member. Is dat akkoord, of moet het de top van het organigram worden (`reports_to IS NULL`)?
6. **Risico R2 aanvaarden (§4b.6)**: in Fase 1 kan iedereen die de server bereikt zich zonder sessies voor een andere board member uitgeven. Mijn voorstel is dat Sébastien dit expliciet aanvaardt tot Fase 3, en dat de vault en de Claude-login per user pas na de sessies komen. Ik ga hiervan uit, tenzij Orion iets anders beslist.
7. **OV-13**: ik ga uit van Nova's voorstel en Pixel P-08 (een eenmalige wachtwoordcontrole bij het kiezen). Zonder die controle valt alleen `user.use` weg (−0,25 d), maar dan zijn de wachtwoorden in Fase 1 niet meer dan opslag.
8. **Fase 3, lokale verbinding**: blijft de desktop op de server zelf (lokale IPC) vertrouwd als "de eigenaar van de machine", of moet ook die inloggen (§4b.5)?

---

## 8. Baseline van de tests (main 4694dd7, worktree `target/wt-mu-00`)

Gedraaid op 2026-10-30 (Linux):

| Suite | Resultaat |
|---|---|
| `cargo test --workspace --no-fail-fast` | **767 passed, 1 failed, 14 ignored**. De fout: `crates/daemon/tests/connections.rs:153` `api_connection_needs_a_key_and_disabled_means_no_fallback`. Het aantal past bij de bekende ene Linux-fout uit AUR-BUG-04 (toen 757/1). Ik heb niet nagegaan of het dezelfde test is |
| `flutter test` (clients/mobile) | **398 passed** (een eerste run tijdens het compileren van cargo brak `widgets_test.dart` af door de belasting; de herhaling zonder belasting was volledig groen) |
| `flutter analyze` | **15 issues** (= de bekende baseline) |

Fase 1 moet deze baseline halen: hetzelfde of beter, plus de nieuwe tests van §3.8.
