# Technische impactanalyse Guidelines

*AUR-GL-00-T · Titan (Tech Lead) · 2026-10-31 · basis: `main` 56ea3e5 (protocol 5) + `feat/aur-mu-04` @ e2060f0 (Fase 4, nog niet gemerged) · bron: [README.md](README.md) B1–B7*

Dit is alleen een analyse. Er is geen code gewijzigd, en er is niets gebouwd of getest.

## 0. Samenvatting
- **Migratie 0023**: Fase 4 neemt 0022 (`0022_vault.sql`). Er komen vijf tabellen: `guidelines`, `guideline_versions` (append-only), `guideline_links`, `guideline_proposals` en `run_guidelines`, en de seed (§1).
- **Protocol: additief binnen 5**, feature `guidelines`. Geen nieuwe major (§2).
- **Plek**: een gelabeld blok in de **systeemtekst**, direct na de identiteit en de instructies (`plan.rs:257-262`). Het telt als verplichte context in de Context Builder. Bij een hervatte sessie krijgt de run de bevroren systeemtekst van die sessie. Is de set guideline-versies veranderd, dan start de run een **nieuwe sessie**. Zo klopt B6 ook met `--resume` (§3).
- **Tokens**: 24k tekens ≈ 6–8k tokens. Dat is te veel naast het invoerbudget van 27,2k tokens. Mijn voorstel is **16k tekens per run** (D-GL-03, §4).
- **CLAUDE.md** blijft de bron voor repo-feiten (bouwen en testen in deze repo). Guidelines beschrijven hoe het team werkt. Er hoeft niets te verhuizen (§5).
- **Departments komen niet terug** (§6). **`propose_guideline`** krijgt een eigen tabel en eigen methodes, los van `proposals` (§7).
- **Schatting: ≈ 15,5 d**, met een doorlooptijd van ongeveer 1,5 week (§9).

## 1. Datamodel en migratie (0023)
De migratie wordt `migrations/0023_guidelines.sql`, plus een regel in `crates/storage/src/migrate.rs` en `schema23_tests.rs` met `open_until(23)`, zoals bij 0018, 0019 en 0021. Alle tabellen zijn `STRICT`.

1. **`guidelines`**: `id`, `company_id` → `company(id)`, `current_version`, `created_by_user`, `updated_by_user` → `users(id)`, `created_at_ms`, `updated_at_ms`, `archived_at_ms`. De titel en de tekst staan in de versie, zodat elke versie volledig is.
2. **`guideline_versions`**, append-only met de triggers `no_update`/`no_delete`, zoals `vault_item_versions` in 0022. Kolommen: `guideline_id`, `version` CHECK > 0, `title` CHECK lengte 1–120, `text` CHECK `length(text) BETWEEN 1 AND 8000`, `source` CHECK IN (`seed`,`created`,`edited`,`proposal`,`restored`), `proposal_id`, `restored_from`, `by_user`, `created_at_ms`, met PK(`guideline_id`,`version`).
   - De tekst staat **inline** en niet in `blobs`, zoals de instructies dat wel doen. Een guideline is klein (≤ 8k), de CHECK bewaakt B4 in de database zelf, en blob-opkuis is niet nodig. SQLite telt met `length()` tekens, net als `chars().count()` in Rust.
   - De unieke titel geldt per company, onder de niet-gearchiveerde guidelines (NOCASE). De daemon controleert dat bij create en update, want een index over twee tabellen kan niet.
3. **`guideline_links`**: `guideline_id`, `employee_id` → `employees(id)` NULL, `project_id` → `projects(id)` NULL, met CHECK "precies één van beide" (B2, geen combinatie). Verder `created_by_user` en `created_at_ms`. Uniek via twee partiële indexen: (`guideline_id`,`employee_id`) WHERE employee_id IS NOT NULL, en idem voor `project_id`. Daarbij een index op `employee_id` en een op `project_id` voor de opbouw van de run.
4. **`guideline_proposals`** (§7): `id`, `guideline_id` (NULL = een nieuwe guideline), `base_version`, `proposed_by` → `employees(id)`, `title`, `text` (CHECK ≤ 8000), `reason` (≤ 2000), `evidence_json` (json_valid), `status` CHECK IN (`pending`,`approved`,`rejected`,`withdrawn`), `decision_note`, `decided_by_user`, `origin_conversation_id`, `run_id`, `created_at_ms`, `decided_at_ms`. Index op (`status`,`created_at_ms`), op (`guideline_id`,`status`) en op (`proposed_by`,`status`).
5. **`run_guidelines`** (B6): `run_id` → `runs(id)`, `guideline_id`, `version`, `source` CHECK IN (`employee`,`project`), `chars`, met PK(`run_id`,`guideline_id`). Ze bewaart welke versies een run echt in zijn systeemtekst had. Het gaat om een paar rijen per run. Zonder foreign key op de versie, zoals `vault_audit`, zodat de opkuis van runs niet blokkeert.
6. **Seed (B7)**: de startset uit [seed.md](seed.md) als `INSERT … SELECT`, met `source = 'seed'`, alleen `WHERE NOT EXISTS (SELECT 1 FROM guidelines)` en alleen als er een company is. Koppelingen gaan **op naam**: `INSERT INTO guideline_links SELECT … FROM employees WHERE name = 'Forge' AND archived_at_ms IS NULL`, en voor de projectguidelines `FROM projects WHERE name = 'Aurelius'`. Op een andere installatie zonder die namen komen de guidelines er wel, maar ongekoppeld. Dat doet geen kwaad (D-GL-06).

## 2. Protocol: binnen 5, additief
| Wijziging | Waar | Oordeel |
|---|---|---|
| Feature `guidelines` | `protocol/src/rpc.rs` `feature`, `FEATURES` | additief |
| read `guideline.list { archived? }` → `GuidelineView { id, title, version, chars, links: [{employee_id?, project_id?}], updated_by, updated_at_ms, archived }`, zonder tekst | `protocol/src/method.rs` | additief in 5 |
| read `guideline.get { id, version? }` → met tekst | `method.rs` | additief |
| read `guideline.versions { id }`, newest first (zoals `employee.instruction_versions`) | `method.rs` | additief |
| read `guideline.effective { employee_id, project_id? }` → de set die een run zou krijgen, in volgorde, met `total_chars` en `limit_chars` (voorbeeld en B4 in de UI) | `method.rs` | additief |
| read `guideline.proposals { status? }` | `method.rs` | additief; **niet** in `proposal.list`, zie §7 |
| writes `guideline.create { title, text }`, `guideline.update { id, title?, text? }` met `expected_version`, `guideline.archive`/`guideline.restore { id }`, `guideline.restore_version { id, version }`, `guideline.link`/`guideline.unlink { id, employee_id \| project_id }`, `guideline.decide_proposal { proposal_id, approve, title?, text?, note? }` | `protocol/src/write.rs`, domain-commando's in een nieuwe `crates/domain/src/guideline.rs` | additief in 5 |
| Rechten: writes alleen voor een board member (`require_board_member`, `application/src/ticket.rs:27`), apps krijgen `NOT_FOR_APPS` op de writes (`protocol/src/app.rs`), reads mogen; remote: reads = Read, writes = TaskControl (`protocol/src/remote.rs`) | `app.rs`, `remote.rs` | additief |
| Events `guideline.created/updated/archived/restored/linked/unlinked { id, title, version, by }`, `guideline.proposed { proposal_id, guideline_id?, by_employee }`, `guideline.proposal_decided { proposal_id, approved }` | `crates/domain/src/events.rs` | additief (oude 5-clients slaan onbekende events over) |
| Foutcodes `guideline_too_large { chars, limit, employee?, project? }` (B4) en `guideline_title_taken`; `version_conflict` bestaat al | `crates/domain/src/error.rs`, `protocol` | additief |
| `ContextView.items`: een item `kind = "guideline"` per guideline (context-inspector) | `domain/src/views.rs` | additief (`kind` is al tekst) |
| Agent-tool `propose_guideline` | `agent-bridge/src/mcp.rs`, `protocol/src/bridge.rs` (`EmployeeTaskOp::ProposeGuideline`), `daemon/src/api/bridge.rs` | intern |
| gen-dart | `clients/mobile/lib/data/protocol/*` | in een eigen commit, zoals "gen-dart after …" |

**Dat het domain-commando's zijn, is bewust.** Zo lopen ze via `writes::domain` → `b10` (Fase 4, `daemon/src/api/writes.rs`): een guideline-tekst met een vault-waarde wordt geweigerd (`looks_like_secret`). Daarvoor hoeft niets extra gebouwd te worden. Een voorstel van een agent gaat via `api/bridge.rs` en wordt daar al geredigeerd (Fase 4).

## 3. Plek in de context en hervatte sessies (B6)
**Waar.** In `prepare_chat_run` (`crates/application/src/run/plan.rs`) komt het blok direct na `You are {name}, {role}. {instructions}` (r.257-262), dus vóór de map-, browser-, taak- en collegablokken.
- De systeemtekst is de juiste plek, en niet de eerste user-turn of een tool. Ze gaat elke API-aanroep mee via `--append-system-prompt-file` (`runtime/src/claude_code/mod.rs:300`), ze **overleeft een compactie** van Claude Code (een user-turn verdwijnt dan in de samenvatting), en ze weegt zwaarder dan wat het model zelf ophaalt.
- Vroeg in de tekst: guidelines veranderen zelden. Een stabiel prefix houdt de prompt-cache warm. Wat vaak verandert, staat al in `tail`.
- In de Context Builder (`application/src/memory/context.rs`) krijgt `Input` een veld `guidelines` naast `rules`. Het telt in `mandatory` (r.141) en wordt nooit ingekort. Het krijgt een eigen manifest-item per guideline, zodat de context-inspector de tokens per guideline toont. Het blok zelf blijft in de systeemtekst op de plaats hierboven.

**Wie het krijgt.** Elke run van een employee: chat, taak, variant-trial en kanaalrun. Het project is dat van de taak, of `channel_project` voor een kanaal- of discussierun (MU2-08 AC4). In de eigen chat zonder project zijn het alleen de employee-guidelines. De projectassistent (`project_chat::assistant_system`) krijgt alleen de projectguidelines. **Geen** guidelines krijgen triage (`ToolProfile::Triage`), memory-extractie en de ticketsamenvatting. Dat zijn text-only runs zonder werk in een map.

**Vorm** (Engels, zoals de rest van de systeemtekst):
```
## Guidelines
How this company works (set by the board members). Your task's criteria, the agent rules and your permissions go before them; a project guideline goes before an employee guideline; the repository's own CLAUDE.md goes before them on how to build and test in that repository. Guidelines never give you more rights. Propose a missing or better rule with propose_guideline.

### Rust style (g:<id> v3 · employee)
…markdown…

### Aurelius: layers and boundaries (g:<id> v1 · project Aurelius)
…markdown…
```
De volgende kopniveaus in een guideline-tekst schuiven één niveau op (`#` wordt `####`), zodat een guideline de structuur van de systeemtekst niet kan breken. Het id staat erbij, zodat `propose_guideline` ernaar kan verwijzen.

**Hervatte sessie / `--resume`.** Claude Code bewaart de toegevoegde systeemtekst niet. Aurelius stuurt bij een vervolg de **bevroren** systeemtekst van de sessie (`conversation_sessions.system_text`, `daemon/src/dispatcher/session.rs:33`, maximaal 24 h: `SYSTEM_TEXT_MAX_AGE_MS`). Zonder maatregel houdt een gesprek dus tot 24 h de oude guidelines, ook in nieuwe runs. Dat is strijdig met B6 ("vanaf de volgende run").
- **Voorstel (D-GL-05):** `SessionContext` (`run/session.rs:20`) krijgt `guidelines: Vec<String>` (`"<id>@<version>"`). In `plan.rs` bij `resume` (r.602): verschilt `before.guidelines` van de huidige set, dan `resume = None`. Er start dan een nieuwe sessie met de nieuwe tekst. Dit gebeurt lui en per gesprek, alleen voor wie de wijziging raakt.
- Het alternatief, `conversation_sessions::expire_for_employee` zoals bij instructies (`org/commands.rs:582`), dekt geen projectkoppelingen. Het zou ook alle sessies van een heel project laten vervallen bij elke tikfout.
- Een **lopende** run houdt zijn tekst vanzelf: het proces is met die tekst gestart. B6 klopt dus.
- `run_guidelines` wordt geschreven in dezelfde transactie als `claim_session` (`run/session.rs:409`): bij een vervolg de versies van de sessie (die door de check gelijk zijn aan de huidige), bij een nieuwe sessie de huidige.
- Een slash-command (`command_turns`) verplaatst de geschiedenis naar de systeemtekst. De guidelines blijven daar staan, dus er verandert niets.

## 4. Tokenkost en de limiet van B4
- Aurelius schat tokens als tekens / 4 (`storage/src/memory.rs:75`). Voor markdown met code is dat optimistisch: reken op 3–4 tekens per token. 24.000 tekens ≈ **6.000–8.000 tokens**, 16.000 tekens ≈ 4.000–5.300.
- **Budget**: `input_budget()` = 32.000 − 15 % = **27.200 tokens** (`memory/context.rs:11-28`). `rules` + `tail` + de vraag zijn verplicht. Past dat niet, dan **faalt de run** (`ContextTooLarge`). Een managerrun zoals de mijne heeft vandaag naar schatting 4–5k tokens aan regels (instructies, guidance, collega's en organigram) plus het projectoverzicht in `tail`, en daarnaast een vaste 4.000 voor memory (`MEMORY_BUDGET_TOKENS`). Met 6–8k guidelines blijft er voor de geschiedenis nog ongeveer 8–10k over, en een ruime taakomschrijving kan de run laten falen.
- **Geld**: per nieuwe sessie wordt het blok één keer in de cache geschreven. Daarna gaat het elke API-aanroep van de tool-loop mee als cache-read (≈ 10 % van de gewone prijs). Een run met 30 aanroepen betaalt het blok dus ruwweg 1,25 + 30 × 0,1 ≈ 4 keer. Bij 6k tokens is dat ≈ 25k tokenequivalenten per run, bij de seed (de grootste set is Bridge in Aurelius: 7.505 tekens ≈ 2,1k tokens) ≈ 9k. Een nieuwe sessie na een wijziging (§3) kost één keer extra de volledige herbouw van de geschiedenis.
- **Oordeel**: 8.000 tekens per guideline is **akkoord**: genoeg voor een stijlgids, en klein genoeg om te reviewen. Per run stel ik **16.000 tekens** voor (D-GL-03): dat is in tokens gelijk aan het memory-budget, en de seed past er ruim in (de grootste set is Bridge in Aurelius met 7.505 tekens, zie [seed.md](seed.md)).
- **Controle**: bij `link`, `update`, `restore_version` en `decide_proposal` rekent de daemon de som uit voor **elke** combinatie die kan voorkomen: (employee-set van elke employee + projectset van elk actief project) en ook de projectset alleen. Bij ≈ 15 employees × 10 projecten is dat triviaal. Gaat het daarboven, dan volgt `guideline_too_large` met de combinatie die overloopt. Het vangnet bij de run (bv. na een `project.restore` of een nieuw project) volgt Nova (OV-02, GL-12 AC3): de run start toch, met de guidelines die passen. Eerst vallen employee-guidelines weg, van achteren naar voren, want project gaat voor (B3). De run zegt dat in de kopregel van het blok, in het manifest, en met een melding aan de board members. Weigeren zou werk blokkeren voor iets wat de schrijfcontrole al hoort te vangen.

## 5. Verhouding tot CLAUDE.md
- **Wat Claude Code laadt**: de run start met de projectmap als cwd (`folders[0]`). Claude Code laadt zelf `CLAUDE.md` uit die map en de bovenliggende mappen, plus geneste `CLAUDE.md`'s als het daar bestanden leest, en de user-`CLAUDE.md` uit `CLAUDE_CONFIG_DIR`. Dat is de loginmap per board member (`runtime/src/claude_code/login.rs:18`). Aurelius ziet die tekst niet en telt hem niet mee in het budget.
- **Inhoud vandaag**: de `CLAUDE.md` van de repo (1.756 tekens) gaat alleen over worktrees, `CARGO_TARGET_DIR`, één build aan het einde en opkuis. Dat zijn repo-feiten die ook gelden voor een mens of een losse Claude Code buiten Aurelius.
- **Overlap**: alleen met de seed-guideline "Worktrees, build en opkuis". Die verwijst daarom naar CLAUDE.md en voegt alleen toe wat team-afspraak is (de opkuisstap van AUR-OPS-01, rapporten in `target/<taak-id>/`, de suite op de achtergrond, commits per subfix). Ze herhaalt CLAUDE.md niet.
- **Voorrang**: Claude Code zet CLAUDE.md in een user-turn met "may or may not be relevant". Ons blok staat in de systeemtekst en weegt dus zwaarder. Een conflict los je daarom niet op met een voorrangsregel, maar door de inhoud te splitsen: **CLAUDE.md = hoe bouw en test je in deze repo; guidelines = hoe het team werkt.** De kopregel van het blok (§3) zegt expliciet dat CLAUDE.md voorgaat voor bouwen en testen in die repo, omdat dat het meest specifiek is (in lijn met B3).
- **Verhuizen**: niets. Wel twee aandachtspunten. (1) CLAUDE.md is Windows-getint (`worktree.ps1`, `C:/…`), terwijl de server op Linux draait. Dat is een idee voor later, buiten deze scope. (2) De user-`CLAUDE.md` in een loginmap zou alleen gelden voor runs onder de login van die board member. Dat is niet voorspelbaar, dus documenteren: "gebruik guidelines, niet de CLAUDE.md van je login".

## 6. Departments als koppeling: nee
- Departments zijn weg sinds protocol 4: `0018_board.sql:150-170` dropt `departments` en het memory-scope `department`, en `docs/product/multi-user.md` §1 zegt: "Het organigram loopt enkel via `reports_to`." Een koppeling aan departments zou een concept terugbrengen dat de Owner net heeft geschrapt, met migratie, protocol en UI erbij.
- Een derde niveau maakt ook B3 zwaarder (employee < department < project) en de check van §4 groter.
- Wat het oplost (dezelfde regels voor Forge, Bridge en Nexus), lost B1 al op: één guideline met drie koppelingen. In de UI volstaat **meervoudig koppelen** (meerdere employees tegelijk aanvinken). Dat is voor Pixel.
- Kanttekening: de geïnstalleerde daemon is ouder dan main en zet nog "You are in the department …" in de prompt. Na de update naar main valt dat weg. Guidelines mogen er niet op steunen.

## 7. `propose_guideline` naast `propose_instructions`
- **Tool** (`agent-bridge/src/mcp.rs`, naast r.148): `propose_guideline { guideline?, title, text, reason, evidence[] }`. `guideline` is een id of titel uit het eigen blok; leeg betekent een nieuwe guideline. `text` is de volledige nieuwe markdown. Het bewijs is verplicht, zoals bij instructies (`proposal::validate_evidence`, 1–8 refs). In alle modi, ook Auto, beslist een board member. De agent krijgt geen antwoord nu.
- **Wie**: elke employee (B5), dus breder dan `propose_instructions` (alleen voor zichzelf of directe reports). Een agent kan alleen een wijziging voorstellen aan guidelines die in zijn eigen blok staan, of een nieuwe. Zo kan hij geen regels herschrijven die hij nooit kreeg.
- **Limieten**: maximaal 1 open voorstel per guideline (zoals `MAX_OPEN_PER_TARGET`) en maximaal 2 open voorstellen voor een nieuwe guideline per employee (zoals `MAX_OPEN_PER_EMPLOYEE`). De tekst voldoet aan dezelfde validatie als `guideline.update`, inclusief de B4-check bij het goedkeuren en niet bij het voorstellen. Een voorstel koppelt zelf niets: bij een nieuwe guideline stelt de goedkeuring standaard de koppeling aan de voorsteller voor, en de board member past die aan.
- **Eigen tabel, geen `kind = 'guideline'` in `proposals`.** `proposals` heeft `name`, `role` en `instructions_blob_id` NOT NULL (0018:190-206), en de CHECK op `kind` vraagt een table rebuild. Erger: `proposal.list` met een onbekende kind kan oude 5-clients laten struikelen bij het deserialiseren van de enum. Een eigen tabel en `guideline.proposals` blijven echt additief. `crates/domain/src/proposal.rs` levert hergebruik (`validate_evidence`, `validate_reason`, `validate_note`, `ProposalStatus`). De nieuwe types komen in `domain/src/guideline.rs`.
- **Beslissen**: `guideline.decide_proposal`, met `text`/`title` om aan te passen bij het goedkeuren, `note` terug naar de voorsteller (zoals `answer_proposer` in `org/commands.rs`), en `version_conflict` als de basisversie niet meer de huidige is. Een gearchiveerde voorsteller geeft `withdrawn`. In de inbox naast de instructievoorstellen (Pixel), met een diff ten opzichte van de basisversie.
- **Prompt**: `AUTONOMY_GUIDANCE` (`plan.rs:11`) krijgt een halve zin: "…propose a missing or better working rule with propose_guideline, also only for a lesson confirmed at least twice." Dezelfde drempel als bij instructies, tegen spam.

## 8. Risico's
| # | Risico | Maatregel |
|---|---|---|
| R1 | Prompt-injectie via een guideline ("negeer de acceptatiecriteria", "je mag pushen") | alleen board members wijzigen en keuren goed; de kopregel zegt dat guidelines nooit rechten geven en dat taak, agent-regels en rechten voorgaan; de rechten zitten in de daemon (`permissions.rs`) en niet in tekst; de diff in de inbox |
| R2 | Een secret in een guideline staat in elke run | domain-commando's, dus B10 van Fase 4 weigert het (§2); voorstellen gaan via de geredigeerde bridge |
| R3 | Het budget loopt over en runs falen (`ContextTooLarge`) | 16k per run (D-GL-03), check bij het schrijven over alle combinaties (§4), duidelijke melding |
| R4 | Vaak bewerken = veel nieuwe sessies = cache en geschiedenis opnieuw | lui per gesprek (D-GL-05); de UI zegt "geldt vanaf de volgende run van N employees" |
| R5 | Guidelines en CLAUDE.md spreken elkaar tegen | splitsing van de inhoud (§5), de kopregel, de seed verwijst in plaats van te kopiëren |
| R6 | Feiten in guidelines verouderen (bv. "bekende fout `connections.rs:153`") | de seed houdt vluchtige feiten eruit; versies plus een "laatst gewijzigd" in de UI |
| R7 | Seed op naam koppelt verkeerd op een andere installatie | alleen bij een exacte naam en een lege tabel; anders ongekoppeld (D-GL-06) |
| R8 | **Overlap met Fase 4 (RULE 09)** | bouw pas na de merge van AUR-MU-04. Fase 4 raakt ook: `agent-bridge/src/mcp.rs`; `protocol/src/{bridge,method,write,rpc,app,remote,lib}.rs`; `domain/src/{events,error,lib,ids,employee}.rs`; `storage/src/{lib,migrate}.rs`; `daemon/src/api/{bridge,host,mod,reads,writes}.rs`; `daemon/src/dispatcher/chat.rs`; `application/src/lib.rs`; de gen-dart-uitvoer en de clients (`clients/desktop/src/app.rs`, mobiel `data/`). **Niet** door Fase 4 geraakt: `run/plan.rs`, `run/session.rs`, `memory/context.rs`, `domain/src/proposal.rs`, `org/commands.rs`, `daemon/src/dispatcher/session.rs` |
| R9 | Voorstellen-spam door agents | de drempel "twee keer bevestigd" in de prompt, plus de limieten van §7 |

### Per B-punt
| # | Oordeel |
|---|---|
| B1 | **Akkoord.** De bibliotheek met inline versies (§1). |
| B2 | **Akkoord**, met invulling **D-GL-01**: het project van een run is dat van de taak, of het kanaal- of discussieproject (MU2-08 AC4). De eigen chat zonder project krijgt alleen employee-guidelines. De projectassistent alleen projectguidelines. Triage en text-only runs geen. |
| B3 | **Akkoord**, met aanvulling **D-GL-02**: in de kopregel ook "CLAUDE.md van de repo gaat voor bij bouwen en testen", en "guidelines geven nooit rechten". Volgorde: employee, dan project, en binnen elk niveau alfabetisch. Het meest specifieke staat zo het laatst. |
| B4 | **Alternatief D-GL-03**: 8.000 per guideline akkoord, maar **16.000 per run** in plaats van 24.000 (§4). De check gebeurt bij elke write die de som verandert, over alle combinaties employee × project (ook "zonder project"). Bij de run is er een vangnet zonder weigering, zoals Nova voorstelt in OV-02: eerst vallen employee-guidelines weg, en dat wordt altijd gemeld. |
| B5 | **Akkoord**, met **D-GL-04**: een eigen tabel `guideline_proposals` en een eigen read (geen `kind` in `proposals`). Elke employee, maar alleen voor guidelines in het eigen blok of een nieuwe. Bewijs verplicht. 1 open per guideline, 2 nieuwe per employee. |
| B6 | **Akkoord**, met **D-GL-05**: de versieset in `SessionContext`. Is die anders, dan start er een nieuwe sessie (anders houdt een hervatte sessie tot 24 h de oude tekst). Een snapshot in `run_guidelines`. |
| B7 | **Akkoord**, met **D-GL-06**: de seed in 0023, alleen als de tabel leeg is, met koppelingen op exacte naam (employees, project "Aurelius"). Repo-specifieke regels als **projectguideline** van Aurelius, algemene als employee-guideline (zie [seed.md](seed.md)). |

## 9. Schatting
| Wie | Werk | Dagen |
|---|---|---|
| Schema | 0023 (5 tabellen, triggers, seed) + `storage/src/guidelines.rs` (CRUD, versies, koppelingen, effectieve set, som per combinatie, voorstellen, `run_guidelines`) + `schema23_tests` | 1,5 |
| Forge | `domain/src/guideline.rs` (validatie, limieten, commando's) + application-commando's (create/update/archive/restore/restore_version/link/unlink/decide) met events en B4-check | 1,5 |
| Forge | protocol (reads, writes, events, fouten, feature, rechten app/remote), gen-dart | 1 |
| Forge | het blok in `plan.rs`, `Input.guidelines` + manifest in `context.rs`, de versieset in `SessionContext` + resume-check, `run_guidelines` in `claim_session` | 1,5 |
| Nexus | `propose_guideline`: `mcp.rs`, `bridge.rs` (protocol + daemon api), limieten, `AUTONOMY_GUIDANCE` | 1 |
| Flux | desktop: bibliotheek, editor met tekenteller en de som per employee, versies/diff/terugzetten, koppelen vanuit guideline, employee en project, voorstellen in de inbox | 3 |
| Bridge | mobiel: lijst, lezen, eenvoudige editor, koppelen, voorstel goedkeuren of afwijzen met diff | 2,5 |
| Sentinel | review van injectie, rechten, B10 op guideline-tekst | 0,5 |
| Pulse | E2E: een run krijgt het blok en de snapshot klopt, een wijziging betekent een nieuwe sessie bij de volgende run en niet in de lopende, voorstel → goedkeuren, de B4-weigering | 1,5 |
| Trace | review server en clients | 1 |
| Titan | coördinatie, seed definitief maken, integratie | 0,5 |
| **Totaal** | | **≈ 15,5 d** (doorlooptijd ≈ 1,5 week) |

## 10. Bouwvolgorde
1. **Na de merge van AUR-MU-04** naar main (R8), op een branch `feat/aur-gl-01` vanaf die main. Schema doet 0023 + storage, parallel daarmee Forge `domain/src/guideline.rs`. Sentinel kijkt kort naar de kopregel en de rechten vóór de code.
2. Forge bevriest het protocol (additief op 5) → gen-dart. **Vanaf hier Flux en Bridge**, en parallel Nexus met `propose_guideline` (andere bestanden dan Forge in stap 3).
3. Forge: het contextblok + de sessiecheck + de snapshot. Dit raakt alleen bestanden die Fase 4 niet raakte.
4. Pulse E2E, Trace review per tak, Sentinel controle. Daarna de integratie door Titan. Merge en push alleen met akkoord van de Owner.

## 11. Afstemming met [user-stories.md](user-stories.md) en [ui-ontwerp.md](ui-ontwerp.md)
| Punt | Nova / Pixel | Titan |
|---|---|---|
| OV-01, GL-08: department als koppeldoel | Nova: department als extra koppeldoel (Could) | **Nee** (§6). Departments bestaan niet meer op main sinds 0018 en protocol 4. GL-08 wordt Won't. |
| P-54: groeperen per department in de koppellijst | Pixel: een driestandenvakje per department; alleen koppelingen per employee worden opgeslagen | Het idee klopt (alleen koppelingen per employee), maar er is geen department om op te groeperen. Mijn voorstel: groeperen op **leidinggevende** (`reports_to`, bv. "Reports to Titan · 4 of 9"). Dat is hetzelfde patroon met data die er wel is. |
| OV-02: vangnet bij de run | Nova: de run start met wat past, met een melding | **Akkoord**, overgenomen in §4. Eerst vallen employee-guidelines weg. |
| OV-08 / A-03: geen nieuwe sessie bij een wijziging | Nova: de volgende run krijgt het nieuwe blok, zonder nieuwe sessie; is dat technisch niet mogelijk, dan een nieuwe sessie | **Een nieuwe sessie, maar alleen voor de gesprekken die de wijziging raakt** (D-GL-05, §3). Het kan technisch wel: het nieuwe blok gaat als notitie in de volgende turn, zoals nieuwe memories (`session::changes`). Maar dan staat de oude versie nog in de bevroren systeemtekst en de nieuwe alleen in een turn. De oude weegt zwaarder, en na een compactie kan de notitie verdwijnen terwijl de oude systeemtekst blijft. Dan klopt B6 niet meer betrouwbaar, en een run is niet te reviewen tegen "de versies die hij kreeg". A-03 moet dus worden: "een guideline-wijziging start een nieuwe sessie bij de volgende run van de employees die ze raakt". |
| 24.000 in GL-12, de 80 %-waarschuwing en de rode grens bij Pixel | volgt B4 | De getallen volgen D-GL-03 (16.000) als Orion die keuze maakt. De mechaniek (de waarschuwing vanaf 80 % en het vakje dat vooraf uitgeschakeld is) blijft gelijk. `guideline.effective` (§2) levert de cijfers. |
