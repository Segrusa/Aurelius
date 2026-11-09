# Seed: startset guidelines (B7)

*AUR-GL-00-T · Titan (Tech Lead) · 2026-10-31 · bronnen: `CLAUDE.md`, de code op `main` 56ea3e5 (`Cargo.toml`, `rust-toolchain.toml`, de crate-indeling, `migrations/`, `protocol/src/rpc.rs`, `clients/mobile`), de review- en QA-rapporten van Fase 4 (`target/aur-mu-04/r1-review.md`, `qa3-rapport.md`) en de teamafspraken in memory (AUR-OPS-01, lessen uit AUR-MU-01 t/m 04)*

Deze guidelines komen één keer via een eenmalige, idempotente seedstap bij de eerste start na migratie 0023, alleen als de bibliotheek leeg is; niet als inhoud van de migratie (README B7, boven impactanalyse §1.6, D-GL-06). Daarna zijn het gewone guidelines die een board member kan aanpassen. De tekst is Engels, net als de rest van de systeemtekst, de code en de commits. Een review-oordeel blijft in het Nederlands (GOEDGEKEURD / NIET GOEDGEKEURD), zoals vandaag.

**Keuzes**
- **Algemeen** (hoe het team werkt, in elk project) → koppeling aan **employees**. **Repo-specifiek** (de lagen en het protocol van Aurelius) → koppeling aan het **project Aurelius** (B2). Zo krijgt een employee in een ander project de Aurelius-regels niet.
- Er staan **geen vluchtige feiten** in (bv. de bekende testfout `connections.rs:153`): die verouderen (R6) en horen in de taak of het rapport.
- **Geen herhaling** van CLAUDE.md (worktree aanmaken, `CARGO_TARGET_DIR`) en van de agent-regels (RULE 01–10, RULE 04-rapport): die staan al in de context.
- Groottes zijn gemeten in tekens van de markdown-tekst (wat B4 telt). Tokens ≈ tekens / 3,5.

## Overzicht
*G3 (Desktop app, gpui-kit) is uit de startset gehaald: de GPUI-client bestaat niet meer. Bestaande installaties houden hun rij.*

| # | Titel | Niveau | Gekoppeld aan | Tekens |
|---|---|---|---|---|
| G1 | Rust style | employee | Forge, Bridge, Nexus, Schema, Sentinel, Trace, Titan | 1.477 |
| G2 | Flutter and Dart (mobile app) | employee | Bridge, Flux | 790 |
| G4 | Worktrees, builds and cleanup | employee | Forge, Flux, Bridge, Nexus, Schema, Sentinel, Pulse, Orbit, Trace, Titan | 1.188 |
| G5 | Tests and evidence | employee | Forge, Flux, Bridge, Nexus, Schema, Sentinel, Pulse | 849 |
| G6 | Code review | employee | Trace, Sentinel, Pulse, Titan | 977 |
| P1 | Aurelius: layers and boundaries | project | project Aurelius | 1.114 |
| P2 | Aurelius: protocol and migrations | project | project Aurelius | 1.344 |

**Som per run**: zie de tabel onderaan. De grootste set (Sentinel in het project Aurelius) is 6.949 tekens, of ≈ 2.000 tokens. Dat is minder dan de helft van de 16.000 van D-GL-03, zodat er ruimte blijft voor projectregels van andere repo's en voor voorstellen van het team.

---

## G1 — Rust style
*Gekoppeld aan: Forge, Bridge, Nexus, Schema, Sentinel, Trace, Titan. Bron: `Cargo.toml` (edition 2024, `[workspace.dependencies]`), `rust-toolchain.toml`, de stijl van `crates/*`, de les over `#[must_use]` (AUR-MU-01-S4b).*

~~~markdown
- Edition 2024; the toolchain is pinned in `rust-toolchain.toml`. Never change it in a feature branch.
- Format with rustfmt. Clippy gets no new warnings in the code you touched.
- Write code that reads like the code around it: same naming, idioms and comment density.
- Every `pub` item gets a `///` doc comment that says what it is for and why, in plain English. Inside a function, comment only what the code does not already say.
- Limits and fixed values are named consts next to their type (`*_MAX`, `*_MS`), not literals.
- Errors are `thiserror` types (`DomainError`, `StorageError`, `AppError`). No `unwrap`/`expect` outside tests, unless an invariant makes failure impossible; then the message says which invariant.
- A value that must not be dropped gets `#[must_use]` on its type: on a function it does not catch `f()?;`.
- Serde: `rename_all = "snake_case"`. A new field on an existing wire type is `#[serde(default)]`, with `skip_serializing_if` when an absent value means "as before".
- Prefer small pure functions that are easy to test; keep IO at the edges.
- Tests: `#[cfg(test)] mod tests` in the file, a sibling `*_tests.rs` module when they grow large, `crates/<crate>/tests/` for integration tests. A test's name is a sentence of what it proves (`a_refused_update_writes_no_previous_value`).
- Identifiers, comments, logs, commit messages and UI text are English.
- No new dependency without the Tech Lead; use what `[workspace.dependencies]` already has.
~~~

## G2 — Flutter and Dart (mobile app)
*Gekoppeld aan: Bridge, Flux. Bron: `clients/mobile/pubspec.yaml`, `lib/features/*`, `analysis_options.yaml`, de lessen uit AUR-MU-01-C2c/C2d en QA2.*

~~~markdown
- The app is in `clients/mobile`. State per feature with flutter_bloc (`<area>_bloc.dart` and `<area>_page.dart` in `lib/features/<area>/`), value types with equatable, navigation with go_router.
- Talk to the server only through `lib/data/aurelius_repository.dart` and the generated protocol. Check the server's features before you show a feature.
- A bloc that refreshes on events does so only while its tab is visible (TickerMode); otherwise it marks itself stale and refreshes when shown.
- A failure is never an endless "Loading…": show what failed and a Try again.
- UI text is English and uses the same words as the desktop app.
- `flutter analyze` reports no more issues than on the base branch, and `flutter test` is green. Flutter is in `~/development/flutter/bin` (not on PATH).
~~~

## G4 — Worktrees, builds and cleanup
*Gekoppeld aan: Forge, Flux, Bridge, Nexus, Schema, Sentinel, Pulse, Orbit, Trace, Titan. Bron: `CLAUDE.md` (waarnaar verwezen wordt, zonder de tekst te herhalen), AUR-OPS-01 (de opkuisstap), de les "commit elke subfix" en de les over een suite van meer dan 10 minuten.*

~~~markdown
The repository's CLAUDE.md says how to make a worktree and how to build in it; follow it. On top of that:
- One worktree and one branch per task (`feat/<task>` or `fix/<task>`), from the base your task names. Never work in the main checkout.
- While you work, run only targeted tests (`cargo test -p <crate> <filter>`). Run the full suite once at the end, in the background (`nohup … > target/<task-id>/suite.log 2>&1 &`), and poll it: it takes longer than 10 minutes.
- Commit every finished sub-fix with its tests right away, so an interruption loses nothing. Never merge or push without the Owner's approval.
- Commit messages are English: what changed and why, ending with the task id, e.g. "vault: fail closed when the redactor cannot be filled (AUR-MU-04-F6)".
- Never touch another session's worktree or target folder, `~/.claude`, the system keychain, the installed daemon, or the live data folder (`~/.local/share/Aurelius`, read only).
- When the task is done: reports, logs and screenshots go to `target/<task-id>/`; remove review and QA worktrees (only with a clean `git status`) and run `git worktree prune`; delete your own build folder; put `du -sh target` in your report.
~~~

## G5 — Tests and evidence
*Gekoppeld aan: Forge, Flux, Bridge, Nexus, Schema, Sentinel, Pulse. Bron: de reviews van Fase 4 (mutatiecontrole in R1, het gat in de daemon-test van m3), de QA-rapporten en RULE 06.*

~~~markdown
- Every fix comes with a test that fails without it. For a critical line, prove it: break the line, watch the test fail, put it back.
- Test what the acceptance criterion says through the layer that uses it (the daemon API, not only storage).
- Report test results as "N passed, N failed, N ignored" against the baseline of the branch you started from, with the path of the log. Name every failure that was already there; a known failure is not a pass.
- Never claim that a test, build or check ran without showing its output.
- Remove temporary probe tests before you finish; leave the worktree clean.
- Run tests with the real Claude CLI (`real_claude_*`) only when the task asks for them.
- QA tests the real flows end to end on desktop and mobile, with screenshots in `target/<task-id>/`. A finding has steps, expected, actual and a screenshot.
~~~

## G6 — Code review
*Gekoppeld aan: Trace, Sentinel, Pulse, Titan. Bron: de vorm van `target/aur-mu-04/r1-review.md`, `sec1-review.md` en `qa3-rapport.md`.*

~~~markdown
- Scope: an exact commit range (`git diff <base>..<head>`) and the task's acceptance criteria. Read the code; do not only run it.
- Verdict first: GOEDGEKEURD or NIET GOEDGEKEURD, with the number of blockers, majors and minors.
- Blocker: data loss, a security hole, a broken build or main flow. Major: a criterion not met, or a real bug with a reproducible effect. Minor: anything else worth fixing.
- Each blocker or major gives: where (`file:line`), what goes wrong, the proof (a test, a probe or a mutation), the effect, and a concrete fix with the test that should come with it.
- Minors go in one table: where, finding, fix.
- A section "Checks done" with the suite numbers and the log path, and a coverage list for each point you were asked to look at.
- The reviewer does not change the branch: a probe is a throwaway test, removed afterwards.
- The report goes in `target/<task-id>/<review-id>.md`. Questions for the Tech Lead go under "Open points", not as findings.
~~~

## P1 — Aurelius: layers and boundaries
*Gekoppeld aan: project Aurelius. Bron: de crate-indeling in `Cargo.toml`, `application/src/lib.rs` (`commit`, receipts), `run/plan.rs`, Trace R1 m1/m2 (SQL hoort in storage), de redactie van Fase 4.*

~~~markdown
The crates point one way: domain → storage → application → daemon; protocol is the wire, runtime runs Claude Code.
- domain: pure types, validation and limits. No IO, no SQL.
- storage: all SQL lives here (rusqlite). The application and the daemon call storage functions; they never query tables themselves.
- application: `CommandService`. A write is a command that commits in one transaction with its events and receipt (idempotency); side effects follow the commit via `EventNotice`.
- daemon: the API (reads, writes, host, bridge), the dispatcher (runs) and permissions, as a thin layer over the application.
- protocol: wire types with `JsonSchema`; the mobile Dart models are generated from them.
- agent-bridge: the MCP tools a run sees. Each tool maps to an `EmployeeTaskOp` or `EmployeeMemoryOp` that the daemon checks; a tool call is never trusted.
- The system text of a run is built only in `application/src/run/plan.rs`; what changes often goes in its tail.
- What a board member types or a run produces passes the vault redactor before it is stored or sent. Never log params that can carry a secret.
~~~

## P2 — Aurelius: protocol and migrations
*Gekoppeld aan: project Aurelius. Bron: `protocol/src/rpc.rs` (`PROTOCOL_MAJOR`, `FEATURES`, `quotes_no_params`), `app.rs`, `remote.rs`, `xtask` gen-dart, `migrations/0011`–`0022`, `schema18/19/21_tests.rs`, de werkwijze van S3/S5 en Fase 4 (vóór de pipeline).*

~~~markdown
Protocol
- Stay inside the current major (5): add, never change or remove. A new capability gets a feature in `FEATURES` (`rpc.rs`); clients hide what the server lacks.
- New fields are optional with a default; old clients skip unknown events.
- Every new method gets its rights in `app.rs` (apps: `NOT_FOR_APPS` on writes by default) and `remote.rs`; a board-member write calls `require_board_member`.
- A method whose params may carry a secret or password is handled before the command pipeline (no value in `request_hash`, events or receipts) and is listed in `quotes_no_params`.
- After a protocol change, run `cargo xtask gen-dart` and commit the generated files on their own ("gen-dart after …"). Never edit `clients/mobile/lib/data/protocol/*` by hand.

Migrations
- Take the next free number across all open branches. Never edit a migration that is on main.
- Tables are STRICT, with a CHECK for every enum and length. History and audit tables are append-only (`no_update`/`no_delete` triggers).
- To change a CHECK or NOT NULL, rebuild the table in the same migration (`x_new`, INSERT … SELECT, DROP, RENAME, indexes again).
- Each migration gets a `schemaNN_tests.rs` that opens a database at the previous version with data (`open_until`) and checks the upgrade.
- A migration that drops data needs the Tech Lead and the Owner first.
~~~

---

## Som per run (gemeten)
| Employee | Employee-guidelines | Tekens | + P1 + P2 (in project Aurelius) |
|---|---|---|---|
| Forge | G1, G4, G5 | 3.514 | 5.972 |
| Flux | G2, G4, G5 | 2.827 | 5.285 |
| Bridge | G1, G2, G4, G5 | 4.304 | 6.762 |
| Nexus | G1, G4, G5 | 3.514 | 5.972 |
| Schema | G1, G4, G5 | 3.514 | 5.972 |
| Sentinel | G1, G4, G5, G6 | 4.491 | **6.949** (grootste) |
| Pulse | G4, G5, G6 | 3.014 | 5.472 |
| Orbit | G4 | 1.188 | 3.646 |
| Trace | G1, G4, G6 | 3.642 | 6.100 |
| Titan | G1, G4, G6 | 3.642 | 6.100 |
| Orion, Nova, Pixel, Atlas | (geen) | 0 | 2.458 |

Gemeten met `awk` over de tekst tussen de `~~~markdown`-hekken (tekens inclusief regeleinden).

**Kanttekening bij B2.** Omdat er geen combinatie employee × project is, krijgen ook Orion, Nova en Pixel in het project Aurelius P1 en P2 (2.458 tekens, ≈ 700 tokens per run), terwijl ze geen code schrijven. Ik aanvaard dat: het is klein, en de lagen en het protocol helpen ook bij het schrijven van een impactanalyse of user stories. Wordt dit later een probleem, dan verhuizen P1 en P2 naar employee-koppelingen (Forge, Bridge, Nexus, Schema, Sentinel, Trace, Titan). Daarvoor is geen nieuw model nodig.
