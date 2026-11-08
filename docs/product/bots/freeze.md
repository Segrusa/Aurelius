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

Wat later bijkomt, wordt hier per stap toegevoegd voordat de client erop bouwt.
