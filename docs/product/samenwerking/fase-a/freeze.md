# Fase A: protocol-freeze en bouwafspraken

*AUR-FA-T · Titan · 2026-11-02 · basis `feat/aur-fa` (A0 + docs) · gaat voor op de impactanalyse waar ze verschillen; de README gaat voor op allebei.*

## 1. De drie punten uit de README

**P-90: de feedback van een beslissing is herkenbaar zonder nieuwe kolom. De regel blijft.**
`decision_feedback` (`conversation/discussion.rs:785`) schrijft het event met dedupe `cmd:<command_id>:feedback`, en `messages.source_event_id` wijst ernaar. Dat geldt ook voor bestaande rijen. Storage leest dit in één helper `is_decision_feedback(message)`.
Afwijking: de client krijgt één veld `MessageView.editable` en verbergt *Edit* en *Delete* als dat false is. De tooltip *"This message is the feedback of a decision"* vervalt, want dan zou er een tweede veld bij moeten.

**B2: *Plan to review* vervangen ontneemt oudere clients geen actie.**
- Een plan van de hoofdtaak van een ticket is nu al geen approval (`plan_review.rs`, `hand_in`) en geeft vandaag dus geen *Plan to review*-melding. Er verdwijnt niets; `ticket.plan_review` krijgt een nieuwe rij.
- `approval.requested` met `claude.ExitPlanMode` (delegaties, gesprekken, taken zonder ticket) blijft zoals het is, inclusief de push-acties (`push.rs:34`).
- `Result ready` en `Task failed` van de hoofdtaak van een ticket zijn alleen rijen om te openen. Een oudere client ziet in plaats daarvan de nieuwe rij met `severity` `info` of `warning`, in Results of Failed, en beslist op het bord zoals nu.

**B6: `ticket_needs` kost ≈ 1 d (0,5 desktop, 0,5 mobiel), bovenop de 44 d. De mijlpalen verschuiven niet.**
- Eén server-veld: `TicketView.responsible_disabled`, zodat een client geen ledenlijst nodig heeft.
- Desktop: `ticket_needs` blijft zeggen wat de kaart toont (knop, statusregel). Nieuw is `ticket_needs_me` voor ◆, de filter *Needs you*, de badge en *Waiting on me*.
- Mobiel: hetzelfde rond `TicketState.needsYou`.
- De regel, op beide clients gelijk en met dezelfde tests:
  ```
  needs_me(t, me) = needs(t) is er
                    EN (needs == login: me == login_user
                        anders: responsible ontbreekt OF responsible_disabled OF responsible == me)
  ```

## 2. Wire (additief binnen protocol 5)

### Features en accepts
Constanten in `rpc.rs::feature`: `ticket_responsible`, `ticket_follow`, `personal_inbox`, `my_tickets`, `message_locate`, `message_edits`.
**Ze gaan pas in `FEATURES` als hun server-deel klaar is** (door de engineer van dat deel), zodat een client nooit een stub aanroept.
Client-features in `method.rs::client_feature`: `ticket_follow`, `message_edits`.

### Commands (`Command`, allemaal `NOT_FOR_APPS`, `require_board_member`)
| Method | Struct | Velden |
|---|---|---|
| `ticket.set_responsible` | `SetTicketResponsible` (`deny_unknown_fields`) | `ticket_id`, `responsible_user_id: Option<UserId>` (none = niemand) |
| `ticket.follow` | `FollowTicket` | `ticket_id`, `on: bool` |
| `message.edit` | `EditMessage` | `message_id`, `text`, `confirm_secret` (default false) |
| `message.delete` | `DeleteMessage` | `message_id` |
| `ticket.create` | bestaand `CreateTicket` | nieuw `responsible_user_id: Option<UserId>`, `#[serde(default)]`; ontbreekt = niemand (oude clients) |

`message.edit` staat in de `match` van `b10` (`writes.rs`) en in `quotes_no_params`. Geen enkele method krijgt een nieuwe `ErrorCode`.

### Reads (niet voor apps; in `remote.rs` als de andere ticket-reads)
| Method | Params | Antwoord |
|---|---|---|
| `ticket.followers` | `ticket_id` | `TicketFollowersView { followers: [FollowerView { user_id, username, disabled, reason: String, followed_at_ms }] }` |
| `ticket.my_tickets` | `project_id?`, `include_done?` | `MyTicketsView { tickets: [TicketView], following: [TicketId] }` |
| `message.locate` | `message_id` | `MessageLocation { message_id, conversation_id, ticket_id?, ticket_number?, thread_root? }`; alleen `group` en `ticket`, anders `NotFound` |
| `message.history` | `message_id` | `MessageHistoryView { versions: [MessageVersionView { text, replaced_at_ms }], deleted: bool }`, nieuwste eerst, via `redact::text`; bij `deleted` is `versions` **altijd leeg** |

**`ticket.my_tickets`** leest de kijker uit de sessie, nooit uit een param. De tickets zijn de unie van: `responsible = ik`, `created_by = ik`, ik volg, en open tickets met `login_user = ik` of zonder actieve responsible. Done en gearchiveerd alleen met `include_done`. Sortering op prioriteit en daarna `updated_at_ms`. Limiet `MY_TICKETS_MAX = 500`.
De client maakt de vier tabs en tellers uit dit ene antwoord. *Waiting on me* is `needs_me` uit §1, dus dezelfde functie als de kaart; de server rekent ◆ niet opnieuw uit.

### Velden (allemaal `#[serde(default, skip_serializing_if = …)]`)
- `TicketView`: `responsible_user_id: Option<UserId>`, `responsible_username: Option<String>`, `responsible_disabled: bool`.
- `MessageView`: `edited_at_ms: Option<i64>`, `deleted_at_ms: Option<i64>`, `editable: bool` (per lezer, zoals `reactions.mine`).
- `NotificationView`: `ticket_id`, `ticket_number`, `conversation_id`, `message_id`, `reason: Option<String>`, `merged_count: u32`, `needs_you: bool`.
- `reason` is tekst, geen enum: `responsible`, `mention`, `plan_review`, `review`, `sent_back`, `failed`, `login`. Een client toont een onbekende waarde als gewone rij.
- `system_action::RESPONSIBLE = "responsible"` op de bestaande `SystemLineKind::Moved`. Geen nieuwe `SystemLineKind`.

### Events
| Kind | Status | Oudere client |
|---|---|---|
| `ticket.updated` met `changed: ["responsible"]` | bestaand | ververst het bord |
| `ticket.follow_changed` | **nieuw**, `hides_kind` tenzij `accepts` `ticket_follow` | krijgt hem niet, ook niet in de replay |
| `message.edited` | **nieuw**, `hides_kind` tenzij `accepts` `message_edits` | krijgt hem niet, ziet de nieuwe tekst bij de volgende `conversation.get` |
| `message.deleted` | **bestaat al** (`events.rs:63`, wachtrij-bericht) en oudere clients kennen hem; `metadata.queued = false` onderscheidt | kent het event; controleer dat de handler tegen een niet-wachtrij-bericht kan (test) |

**Regel R1:** geen nieuwe enumwaarde in een snapshot. Test met de oude enum, zoals `channels_tests.rs:29`.

### Linkvorm (A5a, beide clients gelijk)
`aurelius://<host_id>/ticket/<ticket_id>` en `aurelius://<host_id>/message/<message_id>`. Strikt parsen: scheme, host_id, precies één soort, geldig id; alles anders is *"This isn't a valid Aurelius link."*. Een link navigeert alleen.

### Vastgelegd door N1 (`feat/aur-fa-proto`)
- Rechten op een apparaat: `ticket.set_responsible` en `ticket.follow` `TaskControl`; `message.edit` en `message.delete` `Chat`; de vier reads `Read`. Apps nooit.
- `CommandResult`: `ticket.set_responsible` geeft `TicketChanged`; nieuw zijn `TicketFollowChanged`, `MessageEdited`, `MessageDeleted`.
- Tot een serverdeel klaar is: `Validation` met *"This server does not do this yet."*. Wie een deel afmaakt, zet de feature in `FEATURES`, haalt hem uit de lijst in `only_message_locate_is_advertised_until_the_server_parts_are_built` en verwijdert de stub.
- Client: `crates/client` kreeg alleen `message_locate`; de andere reads en writes voegt Flux toe. Dart-velden zijn nullable (`?? false`, `?? 0`). Mobiel: `EventKind.messageEdited` hoort in `isChat`.

## 3. Migratie 0029 (geen rebuild, gooit niets weg)
0029 is op 2026-11-02 op geen enkele branch of remote in gebruik.
- `tickets.responsible_user_id TEXT REFERENCES users(id)`.
- `ticket_followers(ticket_id, user_id, reason, followed_at_ms)`, `STRICT, WITHOUT ROWID`, PK (ticket, user), `CHECK(reason IN ('created','responsible','mentioned','manual'))`; backfill: de aanmaker volgt zijn niet-gearchiveerde tickets (`created`).
- `notifications`: `ticket_id`, `conversation_id`, `message_id`, `reason` (`CHECK(reason IS NULL OR length(reason) <= 32)`), `merged_count INTEGER NOT NULL DEFAULT 0 CHECK(merged_count >= 0)`. `notifications` is niet `STRICT`-gewijzigd: `ADD COLUMN` volstaat. Indexen op (`user_id`, `ticket_id`) en (`user_id`, `conversation_id`). `conversation_id` staat er extra (t.o.v. de impactanalyse), omdat een mention buiten een ticket per kanaal samenvoegt.
- `messages.edited_at_ms`, `messages.deleted_at_ms`.
- `schema29_tests.rs` via `open_until(28)` met tickets, berichten en meldingen.

## 4. A3-regels die de server vastlegt
**Ontvangers**
| Event | Reason | Voor wie | `needs_you` |
|---|---|---|---|
| `ticket.updated` `changed: responsible` | `responsible` | de nieuwe responsible | nee |
| bericht met `metadata.mentioned_users` | `mention` | de genoemde board members; ook buiten een ticket | nee |
| `ticket.plan_review`, `ticket.review` | `plan_review`, `review` | volgers + responsible | responsible; zonder actieve responsible alle volgers |
| `ticket.sent_back` | `sent_back` | volgers + responsible | nee |
| `task.failed` hoofdtaak ticket | `failed` | volgers + responsible | als hierboven |
| login nodig | `login` | alleen de login-user (bestaande rij) | ja |

- Nooit aan de actor. Nooit aan een uitgeschakelde board member. Nooit berichttekst in een rij of push.
- B2: voor de hoofdtaak van een ticket geen `Result ready` en geen shared `Task failed`. Taken zonder ticket en approvals veranderen niet.
- `needs_you` wordt bij het lezen berekend uit de huidige stand, zodat het een overdracht volgt.
- `severity` `warning` of `info`, nooit `action` (alleen de bestaande login-rij). `task_id` is de hoofdtaak als die er is. `title` is de volledige zin (*"Marie made you responsible"*), zodat een oudere client hem ook kan tonen.

**Samenvoegen:** één ongelezen rij per (user, ticket), of (user, kanaal) buiten een ticket. Elke nieuwe gebeurtenis zet `created_at_ms` op nu en `merged_count + 1`. De kop is de belangrijkste reden: login > failed > review/plan_review > mention > sent_back > responsible. **Push:** voor de eerste rij en altijd voor `responsible` en `mention`; de pushregel volgt de impactanalyse B5.

**Gate:** in `notify`, op de stand ná de wijziging (test per event). Geen rij op Done of gearchiveerd. `STILL_RELEVANT` verbergt ticketrijen van gesloten tickets, `review`/`plan_review` zodra het ticket daar weg is en `failed` zodra de hoofdtaak niet meer failed is (B12).

**Afwijking van P-83:** geen gebeurtenissenlijst per rij. `+n` opent de tab Activity van het ticket. Reden: staat in geen user story en vraagt een extra tabel en read.

**A0-punt:**
- *Nieuwe board member:* een shared rij van vóór `users.created_at_ms` telt voor die persoon als gelezen, behalve `severity = 'action'` die nog relevant is (een open approval hoort iemand wel te zien). In `read_by`, zonder schrijven.
- *Groei:* `timeline::prune_notifications` verwijdert rijen ouder dan `NOTIFICATION_KEEP_MS` (90 d), tenzij het een nog relevante `action`-rij is. Reads en dismissals gaan mee via `ON DELETE CASCADE`; het event blijft. Draait bij de start van de daemon en daarna dagelijks. Dit raakt geen audit: de events blijven.

## 5. A6-regels
- Bewerken: nieuwe blob, event `message.edited` met de **vorige** blob als payload. Verwijderen: vaste tekst *"This message was deleted."*, event `message.deleted` met de vorige blob. Metadata alleen `message_id` en `by`, plus bij `message.deleted` `queued: false`. Geen tekst in metadata. Metadata komt niet op de wire (`protocol/src/events.rs:9`); clients kijken alleen naar de kind en lezen opnieuw.
- `editable` = auteur is de lezer, `kind` `group` of `ticket`, geen systeemregel, niet in de wachtrij, niet verwijderd, geen beslissingsfeedback, discussie niet alleen-lezen. De command controleert exact dezelfde helper.
- Roept `board_message` nooit aan: tests `an_edit_with_a_new_mention_wakes_nobody`, `a_deleted_message_starts_no_run`. Geen `mentioned_users`, geen volgen, geen rij.
- Dialoogtekst (B3), letterlijk op beide clients: *"Delete this message? A line stays that says it was deleted. The text stays in the audit log and backups, and employees that read it keep it. This doesn't erase a secret: rotate it and put it in the vault."* Bewerkhint: *"Editing wakes no one. An employee that already read this keeps the original. This doesn't erase a secret: rotate it and put it in the vault."*

## 6. Eigenaars (RULE 09) en volgorde
| Stap | Wat | Wie | Branch |
|---|---|---|---|
| N1 | Freeze: types, commands (stub), reads, rechten, gen-dart; `message.locate` volledig | Nexus | `feat/aur-fa-proto` |
| S1 | Migratie 0029, `schema29_tests.rs`, storage-basis | Schema | `feat/aur-fa-schema` |
| F1 | A1, A2 | Forge | `feat/aur-fa-engine` |
| F2 | A3, A0-punt | Forge | idem |
| F3 | A4 (read), A6 | Forge | idem |
| D1 / M1 | A1, A2, A5a, B6 | Flux / Bridge | `feat/aur-fa-desktop`, `feat/aur-fa-mobile` |
| D2 / M2 | A3 | Flux / Bridge | idem |
| D3 / M3 | A4, A6 | Flux / Bridge | idem |
| O1, D4 / M4 | A5b | Orbit, Flux / Bridge | `feat/aur-fa-ops` |
| Slot | Sentinel, Trace, Orbit E2E, één fixronde | | |

Forge is eigenaar van `crates/**` behalve wat Nexus en Schema in hun stap raken, Schema van `migrations/**` en `crates/storage/**` tot S1 gemerged is, Flux van `clients/desktop/**`, Bridge van `clients/mobile/**` behalve `lib/data/protocol/*` (alleen uit gen-dart), Orbit van `scripts/**`. Alleen Titan merget naar `feat/aur-fa`.
