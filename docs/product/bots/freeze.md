# Protocol-freeze: employees als bots

*2026-11-08 · `PROTOCOL_MAJOR` blijft 5 · gaat voor op de impactanalyse waar ze verschillen; de README gaat voor op allebei*

## Regels

1. Alles additief: een optioneel veld of `#[serde(default)]`, met `skip_serializing_if` waar het hoort.
2. Named structs (de generator crasht op inline objecten).
3. Een nieuwe methode: `Method`/`Command` + `COMMAND_METHODS`, een `feature::X`, rechten in `remote.rs` en `app.rs`, `areasOf` in de client.
4. **Nooit** een nieuwe waarde in een enum zonder `#[serde(other)]`: de Flutter-app gooit dan het hele antwoord weg.
5. Geen nieuwe enum-waarde in een snapshot, geen nieuwe `SystemLineKind` of `MessageRole`; een nieuwe `EventKind` alleen achter `accepts`/`hides_kind`. Runresultaten blijven `SystemLineKind::Reported`.
6. Na elke wijziging `cargo xtask gen-ts` én `gen-dart` draaien en committen; de drift-test moet slagen.
7. `clients/desktop` moet blijven compileren.

## Vastgelegd

| Stap | Wire | Oordeel |
|---|---|---|
| 1 | `EmployeeSummary.description?`, `employee.create.description?`, `employee.update.description?` (geen veld: blijft; leeg: wist). Fout `out_of_range` boven 160 tekens | additief; geen nieuwe feature-vlag: een oudere server laat het veld weg en negeert het |
| 2 | `EmployeeSummary.access_level?` (weggelaten bij `inherit`), `employee.create.access_level?`, `employee.update.access_level?` (geen veld: blijft), enum `AccessLevel` met `#[serde(other)] unknown`, feature `employee_access`. Een `update` met dit veld vraagt op afstand de rechten van Approve. C1 gebruikt het bestaande `connection.list` (lezen) en `connection.set_defaults` (schrijven), die al per employee werken | additief; de feature-vlag bestaat omdat de client geen niveau mag tonen dat een oudere server negeert |
| 3 | Geen | |
| 4 | Feature `run_cards`. `MessageView.run?` (`RunBrief`: `status` als string, `tool_calls`, `failed_calls`), `MessageView.awaiting_user?` (weggelaten als false), `MessageView.run_id` nu ook op posts en `Reported`-regels. `ApprovalView.ticket_id?`, `ApprovalView.discussion_id?`. Nieuwe lees-methode `run.get_activity {run_id}` (`RunActivity`, Read op afstand, niet voor apps). `usage.get_totals.ticket_id?`; `TokenUsage.model_calls` (`#[serde(default)]`, 0 = onbekend); `UsageSnapshot.by_user?` (`UsageUserRow`) | additief; `status` is geen enum en dus geen enum-waarde erbij; `run.get_activity` is lezen, zonder `COMMAND_METHODS`. Migratie 35 (`usage_records.model_calls`) |
| 5 | Geen. `MessageView.queued`, `run.queued`, `run.get` en Send now blijven zoals ze waren en worden gevoed uit `employee_inbox`; een run-id bestaat al zolang het item wacht, de `runs`-rij pas vanaf het moment dat de worker het oppakt | |
| 6 | Geen wireveld. De nieuwe `SystemLine.action` `handoff_held` (op een `limit`-regel, de tekst zegt alles; een oudere client toont `text`) en de bridge-operatie `hand_off` (`EmployeeTaskOp::HandOff`, bridge-protocol, niet zichtbaar voor clients). Hops staan alleen in `meta_json` en op `employee_inbox.hop`, en het item met `state` `held` wacht in dezelfde tabel | additief |
| 13 | Feature `employee_chats`. Nieuw commando `employee.open_chat {employee_id}` (resultaat `employee_chat_opened`, Chat op afstand, niet voor apps) en lees-methode `employee.chats` (`EmployeeChatsSnapshot`, `EmployeeChatView`). `chat.send` en `chat.retry` werken in zo'n gesprek, alleen voor het eigen board member; `conversation.get` antwoordt anderen `not_found`. `MessageView.update` krijgt de vrije tekst `ticket_note`; `ticket_update` wordt door de server uit `conversation.get` gefilterd. Bridge-operatie `tell_responsible` | additief; `update` is geen enum, geen nieuwe `EventKind`. Migratie 37 (`conversations.board_member_id`, `chat_tickets`) |

Wat later bijkomt, wordt hier per stap toegevoegd voordat de client erop bouwt.
