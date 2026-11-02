# Technische impactanalyse fase A: samenwerken per persoon

*AUR-FA-DEF-T · Titan (Tech Lead) · 2026-11-02 · basis: `main` 53cb385 (protocol 5, schema 27) · bron: `docs/product/samenwerking/analyse-linear.md` hoofdstuk 5 en 6 · besluiten: V1 eigen team, V2 alleen AI voert uit, de ticket-engine blijft zoals ze is*

Dit is alleen een analyse. Er is geen code gewijzigd en niets gedraaid: de bevindingen komen uit het lezen van de code. Bestandsverwijzingen gelden voor `main` 53cb385. A0 (`AUR-FA-A0`) loopt apart en staat hier alleen als afhankelijkheid.

## 0. Samenvatting
- **Protocol: additief binnen 5 kan, maar een feature alleen is niet genoeg** (§5.1). Een feature zegt wat de server kan. Een oudere client leest die niet, dus een nieuwe `EventKind` moet achter `HelloParams.accepts` en `hides_kind`. In een snapshot mag geen enkele nieuwe enumwaarde komen. Nieuwe meldingssoorten worden een tekstveld, geen enum.
- **A3 is groter dan gedacht.** De meldingstabel gaat uit van één rij per event (`timeline.rs:380`, `timeline.rs:513-520`). Per persoon ontvangen vraagt nieuwe logica voor ontvangers, push en samenvoegen. A3 is 8 d, dus L en geen M.
- **A5 is geen S** (§5.2): 8 d. Linux en Windows geven in GPUI geen URL door en de desktop kent geen single-instance. Daarom splits ik A5 in A5a (links binnen de app, 3,5 d) en A5b (eigen scheme op OS-niveau, 4,5 d). A3 heeft alleen A5a nodig.
- **A6 heeft geen nieuwe audittabel nodig** (§5.3): de vorige tekst gaat als payload in een append-only event. Dat is ook de grens: bewerken is geen wissen.
- **Heropenen bestaat niet** (§5.5): Orions randgeval geeft technisch geen probleem, maar de uitzondering kan vandaag nooit optreden.
- **Schatting: ≈ 44 d werk, doorlooptijd ≈ 3 weken** (§6). Migratie: 0029, want A0 lijkt 0028 te nemen (§1).

## 1. Datamodel en migratie (0029, één bestand, geen rebuild)
**Nummer:** 0027 is het hoogste op `main` en op alle open branches. In de worktree van A0 staat al een `schema28_tests`, dus A0 neemt waarschijnlijk 0028. Fase A neemt 0029; controleer dat opnieuw bij de freeze. Eén migratie voor heel fase A, met `schema29_tests.rs` (`open_until(28)` met tickets, berichten en meldingen).

Alles is additief en gooit geen data weg: `ALTER TABLE … ADD COLUMN` met nullable kolommen vraagt geen rebuild.

| Wijziging | Voor | Opmerking |
|---|---|---|
| `tickets.responsible_user_id TEXT REFERENCES users(id)` | A1 | Nullable; bestaande tickets krijgen niemand |
| `ticket_followers(ticket_id, user_id, reason, followed_at_ms)` | A2 | `STRICT, WITHOUT ROWID`, PK (ticket, user), `CHECK(reason IN ('created','responsible','mentioned','manual'))` |
| Backfill: de aanmaker volgt zijn niet-gearchiveerde tickets | A2 | `INSERT … SELECT` in de migratie |
| `notifications.ticket_id`, `notifications.reason`, `notifications.merged_count` | A3 | `reason` met `CHECK` op lengte, zodat een nieuwe soort geen rebuild vraagt; index op (`user_id`, `ticket_id`) |
| `messages.edited_at_ms`, `messages.deleted_at_ms` | A6 | Geen nieuwe tabel, zie §5.3 |

**Ontvolgen is de rij verwijderen.** Er is geen tombstone nodig: automatisch volgen gebeurt alleen bij een aanmaak, toewijzing of nieuwe mention, en dat is precies de regel uit US-A2. `channel_members` hergebruik ik niet: daar zit de Owner altijd in (`ticket_channel.rs:29-31`) en lezers mogen niet automatisch volgen.

**Dedupe:** `notifications.dedupe_key` is `event:{id}` (`timeline.rs:380`) en wordt `event:{id}:{user}` voor rijen per persoon. Oude rijen blijven geldig.

## 2. Dekking per user story
Protocolrechten gelden voor alle nieuwe writes: `require_board_member` (`application/src/ticket.rs:31`), `NOT_FOR_APPS` (`protocol/src/app.rs:111`) en `Cap(Chat)` of de ticketgroep in `protocol/src/remote.rs:255-256`, `:329`.

### US-A1 Verantwoordelijke
| Laag | Wat | Waar |
|---|---|---|
| domain | `TicketView.responsible_user_id`, `responsible_username` (optioneel); `SetTicketResponsible`; `CreateTicket.responsible` als `Option` | `domain/src/ticket.rs:118`, `:217` |
| storage | Kolom in `SELECT` en `list`, `set_responsible`, plus `disabled`-check | `storage/src/tickets.rs:315-331`, `:334` |
| application | Command in één transactie; geweigerd op Done en gearchiveerd; event `ticket.updated` met `changed: ["responsible"]`, zoals de owner al doet | `application/src/ticket.rs:165`, `:242`; `domain/src/ticket.rs:320-322` |
| application | Systeemregel met bestaande `SystemLineKind::Moved` plus een nieuwe `system_action` | `domain/src/views.rs:771`, `ticket_channel.rs:68` |
| protocol | `ticket.set_responsible`, feature `ticket_responsible` | `protocol/src/write.rs`, `rpc.rs:344` |
| desktop (Flux) | Keuzelijst, kaart, filter naast owner en creator | `screens/ticket.rs`, `screens/board.rs:473` |
| mobiel (Bridge) | Idem | `features/board/ticket_page.dart`, `ticket_card.dart`, `board_page.dart` |
| migratie | `responsible_user_id` | §1 |

Geen wijziging in de engine, de dispatcher of `login_user`. **Naam:** op de wire `responsible`, niet `assignee`, omdat `ticket.assign` en `AssignTicket` al de employee-eigenaar kiezen (`domain/src/ticket.rs:325`). Een tweede "assign" zou een blijvende bron van bugs zijn. Het label in de UI is aan Pixel.

**Een eigen command, geen veld op `UpdateTicket`:** wissen vraagt anders een `Option<Option<…>>`, en een veld op een `deny_unknown_fields`-struct wordt door een oudere server geweigerd als de client het altijd meestuurt (`domain/src/ticket.rs:253`, les uit AUR-T3-F1).

### US-A2 Volgen
| Laag | Wat | Waar |
|---|---|---|
| storage | `follow`, `unfollow`, `followers`, `is_following` | nieuw `storage/src/followers.rs` |
| application | `ticket.follow { ticket_id, on }`; automatisch volgen in dezelfde transactie als de oorzaak | `ticket.rs:444` (aanmaak), `:540` (beschrijving), `discussion.rs:861` en `:988` (mention) |
| domain | Event `ticket.follow_changed`, verborgen voor oudere clients | `domain/src/events.rs:161-184`, `daemon/src/api/subscription.rs:177` |
| protocol | Read `ticket.followers`; `TicketView.following` alleen via de nieuwe read, want `TicketView` is niet per kijker | `protocol/src/method.rs:79` |
| clients | Knop en lijst volgers | `screens/ticket.rs`, `ticket_page.dart` |
| migratie | `ticket_followers` plus backfill | §1 |

**Valkuil in de beschrijving:** `update_ticket` overschrijft de hele tekst. Alleen namen die **nieuw** zijn (nieuw min oud) laten volgen, anders volgt iemand die ontvolgde weer na elke tikfout van een ander. Mentions komen uit `discussions::mentions` (`discussions.rs:346`).

### US-A3 Persoonlijke Inbox
| Laag | Wat | Waar |
|---|---|---|
| storage | `notify` maakt rijen per ontvanger; `notification_for` krijgt ticket-armen | `timeline.rs:306`, `:349`, aanroep in `storage/src/events.rs:73` |
| storage | Samenvoegen per (user, ticket), gate op gesloten tickets, `STILL_RELEVANT` | `timeline.rs:379-381`, `:467` |
| storage | `notification_at_seq` en `push_for_seq` geven een lijst; ze gaan nu uit van één rij | `timeline.rs:513-520`, `storage/src/remote.rs:191-195` |
| application | `metadata.mentioned_users` op het message-event, want de tekst zit in een blob en `notify` leest die niet | `discussion.rs:747-753`, `:306`, `:861`, `:988` |
| gateway | Eén push per ontvanger; extra routing in `data`; redactor over `subject` | `remote-gateway/src/lib.rs:550-578`, `push.rs:30-50` |
| protocol | `NotificationView`: optioneel `ticket_id`, `ticket_number`, `conversation_id`, `message_id`, `merged_count`, `reason: Option<String>`; feature `personal_inbox` | `domain/src/views.rs:1423-1433` |
| desktop | Rijen, klik op ticket in plaats van alleen task | `clients/desktop/src/app.rs:315`, `:4522` |
| mobiel | Rijen, tik, push-routing | `inbox_page.dart:288`, `inbox_sections.dart:47`, `router.dart:213-232` |
| migratie | Kolommen op `notifications` | §1 |

**Afhankelijkheid:** A0 wijzigt dezelfde functies in `timeline.rs`, dus A0 moet in de basis zitten.

**Regels:**
- **Geen berichttekst in een melding.** `subject` blijft `#N titel`, zoals nu (`timeline.rs:433`). Daarmee hoeft de tekst van een mention niet door de redactor, en gaat ze ook niet naar Apple. Een titel is bij intake al gecontroleerd (`b10`, `writes.rs:67`). `subject` gaat bij verzenden nog één keer door `redact::string`, voor een waarde die later aan de vault is toegevoegd; dat doen de webhooks ook (`webhooks.rs:425-426`).
- **Eigen actie:** uitsluiten op `metadata.user_id` of `metadata.by`. Een uitgeschakelde board member is geen ontvanger.
- **Mentions:** in elke discussie (kanaal, ticket, thread), niet in een taakgesprek. Buiten een ticket is er geen volger nodig. Een mention van een employee geeft geen Inbox-rij.
- **Verversen:** mobiel ververst de Inbox niet op chat-events (`companyChanges`, `aurelius_repository.dart:62`) en een mention komt juist als chat-event. De client moet dat bij `personal_inbox` wel doen. Desktop idem (`backend.rs:3045-3080`).
- **Android heeft geen push** (`push.rs:58-60`). Zie B8.

### US-A4 My issues
| Laag | Wat | Waar |
|---|---|---|
| storage | Eén query met tabs, sortering op prioriteit en daarna update; Done standaard uit | `storage/src/tickets.rs:306`, `:334` |
| protocol | Read `ticket.my_issues { tab, project?, include_done? }`, feature `my_issues` | `protocol/src/method.rs:83`, `app.rs:56`, `remote.rs:115` |
| daemon | Lezer uit de sessie (zoals `reads.rs:652`), nooit uit een parameter | `daemon/src/api/reads.rs` |
| clients | Nieuw scherm, gelijk op beide | `clients/desktop/src/app.rs:217` (`NavItem`), `router.dart:82-101` |
| migratie | Geen; leest §1 | |

Geen clientfilter op het bord: dat bevat geen volgers en geen gearchiveerde tickets.

### US-A5 Deep links
Volledig in §5.2. Crates: `protocol` (linkformaat, read `message.locate`), `daemon` (`discussion.rs:398` `message_place` wordt een read). Clients: zie §5.2. Geen migratie. Feature `message_locate`.

### US-A6 Berichten bewerken
| Laag | Wat | Waar |
|---|---|---|
| domain | `EditMessage { message_id, text, confirm_secret }`, `DeleteMessage`; `MessageView.edited_at_ms`, `deleted_at_ms` (optioneel); events `message.edited`, `message.deleted` | `domain/src/group_chat.rs`, `views.rs:593`, `events.rs` |
| storage | Blob wisselen met bestaande `update_message`; `message.history` uit `events` | `conversations.rs:275-282` |
| application | Nieuw command dat `board_message` nooit aanroept | `discussion.rs:713`, `:861` |
| daemon | In de b10-lijst; `hides_kind` voor de nieuwe events | `writes.rs:72-89`, `subscription.rs:177` |
| protocol | `message.edit`, `message.delete`, read `message.history`; feature en client-feature `message_edits` | `write.rs`, `method.rs:552`, `app.rs:111`, `remote.rs:255` |
| clients | Menu, bewerkveld, "bewerkt", historie | `screens/discussion.rs`, `features/chat/chat_view.dart` |
| migratie | Twee kolommen | §1 |

Weigeren als: niet de auteur (`message_author_user`, `discussions.rs:358`), geen `kind` `group` of `ticket`, systeemregel, of discussie alleen-lezen (`discussion.rs:80-88`). Geen nieuwe `ErrorCode`.

## 3. Protocol en rechten: alle wijzigingen
| Wijziging | Soort | Oudere client |
|---|---|---|
| `ticket.set_responsible`, `ticket.follow`, `message.edit`, `message.delete` | Writes, `NOT_FOR_APPS` | Roept ze nooit aan |
| `ticket.followers`, `ticket.my_issues`, `message.locate`, `message.history` | Reads, rechten in `app.rs` en `remote.rs`; `my_issues` niet voor apps (geen board member) | Roept ze nooit aan |
| `TicketView`, `MessageView`, `NotificationView`: nieuwe velden | Optioneel, `skip_serializing_if` | Negeert ze |
| `CreateTicket.responsible` | `Option`, zodat de Dart-builder het weglaat | Geen effect |
| `ticket.follow_changed`, `message.edited`, `message.deleted` | Nieuwe `EventKind`, verborgen via `accepts` | Krijgt ze niet |
| `ticket.updated` met `changed: ["responsible"]` | Bestaand event, nieuwe metadata | Ververst het bord |
| `system_action::RESPONSIBLE` | Bestaande `SystemLineKind` plus tekst | Toont de grijze regel |
| Features `ticket_responsible`, `ticket_follow`, `personal_inbox`, `my_issues`, `message_locate`, `message_edits` | `FEATURES` (`rpc.rs:344`) | Geen effect |
| Client-features `ticket_follow`, `message_edits` | `client_feature` (`method.rs:552`) | Noemt ze niet, krijgt de events niet |

Na de freeze: `cargo xtask gen-dart` in een eigen commit. Geen enkele method draagt een wachtwoord; `message.edit` kan een geheim bevatten en loopt daarom door `b10` vóór de pipeline, zodat `request_hash` en receipt het nooit bevatten (`writes.rs:62-67`). `quotes_no_params` (`rpc.rs:102`) krijgt `message.edit` erbij, zodat een parsefout de tekst niet herhaalt.

## 4. Lagen
Alle SQL in `storage`. Elke write is een command dat in één transactie commit met events en receipt. Meldingen en automatisch volgen horen in die transactie, niet erna: `events::append` roept `notify` al in de transactie aan (`storage/src/events.rs:73`), dus een melding en haar oorzaak kunnen niet uit elkaar lopen. Push blijft een neveneffect na de commit (`remote-gateway/src/lib.rs:550`).

## 5. De vijf punten

### 5.1 Protocol: wat betekent dit voor de meldingssoorten van A3?
**Een feature is niet voldoende. Er zijn drie verschillende gevallen.**

| Wat | Oudere client bij een onbekende waarde | Gevolg |
|---|---|---|
| `SystemLineKind` of `MessageRole` in `MessageView` | De Dart-enum gooit (`models.dart:18065-18067`), Rust-serde ook (`views.rs:712`, geen `other`). **De hele `conversation.get` faalt** | Nooit een nieuwe waarde. Bestaande kind plus `system_action`, zoals AUR-CHAT-001 |
| `EventKind` in `Event` | Het frame valt weg: mobiel `connection.dart:518-525`, remote desktop `transport.rs:132-138`. Geen crash, wel een gemist event en een waarschuwing per frame | Alleen achter `accepts` en `hides_kind` (`subscription.rs:177`), voorbeeld `server.renamed` en `message.reacted`; ook in de replay (`:284`) |
| `NotificationView.severity`, `title`, `target_type` | Vrije tekst (`views.rs:1423-1433`, `models.dart:13170-13179`). **Geen fout** | Een nieuwe soort is hier additief, met twee valkuilen hieronder |

Waarom een feature niet helpt: `FEATURES` gaat van server naar client. Een oudere client kent de nieuwe feature niet en stuurt ook niets terug. De server moet dus weten wat de client kan, en dat is `HelloParams.accepts`. **Feature voor wat de client mag tonen en aanroepen, `accepts` voor wat de server mag sturen.**

**Ontwerp voor A3:**
1. **Geen nieuwe `EventKind` voor meldingen.** Ze komen uit bestaande events: `ticket.plan_review`, `ticket.review`, `ticket.sent_back` (`events.rs:171-182`), `ticket.updated` met `changed`, `task.failed`, `message.user_committed`, `task.posted`. De login-melding bestaat al. Er komt dus ook geen `notification.created`.
2. **`reason` is een `Option<String>`, geen enum.** Anders maken we dezelfde val opnieuw: de generator maakt van een enum een `fromJson` die gooit, en dan breekt de volgende soort de hele `notification.list`. De clients tonen bij een onbekende `reason` een gewone rij.
3. **Valkuil severity:** oud mobiel laat `action` weg uit de lijst (`inbox_sections.dart:47`) maar telt hem wel in de badge (`inbox_bloc.dart:258-261`). Dat geeft een badge zonder rij. **Nieuwe rijen krijgen daarom `warning` of `info`, nooit `action`**; een nieuwe client groepeert op `reason`.
4. **Valkuil klik:** oude clients openen alleen `task_id` (`inbox_page.dart:288`, `clients/desktop/src/app.rs:4522`). Vul `task_id` met de hoofdtaak als die er is. Zonder hoofdtaak: geen tik op mobiel en de melding "no task to open" op desktop. Dat is lelijk maar werkt.
5. **Push** kent de clientversie niet. Extra sleutels in `data` negeert een oude app; zonder `task_id` opent ze `/inbox` (`router.dart:226-231`).

Oude clients zien de nieuwe rijen dus wel, ook als ze niet klikbaar zijn. Filteren op `accepts` zou een push naar een lege Inbox geven.

**Follow-up, niet in fase A:** de enums in de generator en in serde een `unknown`-tak geven, zodat dit probleem op termijn verdwijnt. Dat helpt pas voor clients vanaf dat moment, dus het haalt de regels hierboven niet weg.

### 5.2 A5: wat kost een eigen link-scheme? De maat S klopt niet
**Vorm:** `aurelius://<host_id>/ticket/<ticket_id>` en `aurelius://<host_id>/message/<message_id>`. `host_id` is nodig omdat beide clients meerdere servers kennen (`HelloResult.host_id`, `rpc.rs:147`; `host.dart`, `servers.rs:156`). De link bevat geen token. Een link is onbetrouwbare invoer: strikt parsen, ids valideren, alleen navigeren en nooit een actie uitvoeren.

**Wat er al is:** route `/ticket/:tid` en `/conversation/:cid` (`router.dart:115`, `:149`), `ticket.get`, en `conversation.get` geeft alle berichten (`method.rs:29`), dus geen paginering voor een anker.

**Wat ontbreekt:**

| Onderdeel | Bevinding | Kost |
|---|---|---|
| Server | `message.locate`: conversatie, ticket, threadroot; `message_place` is nu `pub(crate)` (`discussion.rs:398`) | 0,5 d (Nexus) |
| **A5a** desktop | Kopieer link; navigeren naar ticket en bericht; scrollen naar een bericht bestaat niet (alleen `scroll_to_bottom`, `ticket_discussion.rs:83`); `aurelius://` in markdown openen, want andere schemes worden nu bewust geweigerd (`markdown.rs:144-147`); plakveld in Ctrl+K | 1,5 d (Flux) |
| **A5a** mobiel | Idem; scroll naar bericht ontbreekt in `chat_view.dart` | 1,5 d (Bridge) |
| **A5b** desktop-installers | Windows: `HKCU\Software\Classes\aurelius` (`install-production.ps1:71-76` en de uninstall). Linux: `MimeType=x-scheme-handler/aurelius` en `Exec=… %u` (`install-linux.sh:124-133`). macOS: `CFBundleURLTypes` (`install-mac.sh:72`) | 1 d (Orbit) |
| **A5b** desktop-app | **GPUI geeft op Linux en Windows geen URL door**: `on_open_urls` wordt opgeslagen maar nooit aangeroepen en `register_url_scheme` is `unimplemented` (gpui-pre 0.3.7, `linux/platform.rs:451`, `:793`; `windows/platform.rs:669`, `:994`). Alleen macOS werkt (`macos/platform.rs:1488`). Er is ook geen single-instance: `main.rs` opent altijd een venster. Dus: argv lezen, naar het draaiende proces doorgeven via een lokale socket of named pipe, en de link bewaren tot na inloggen en een serverwissel | 2 d (Flux) |
| **A5b** mobiel | Android: `intent-filter` met `VIEW`/`BROWSABLE` (manifest heeft alleen `MAIN`, `AndroidManifest.xml:26-29`). iOS: `CFBundleURLTypes` ontbreekt. Plugin `app_links` is een nieuwe dependency; die keur ik goed | 0,5 d (Bridge) |
| **A5b** mobiel na inloggen | De router hoort bij één host (`app.dart:53`) en `boardMemberRedirect` stuurt naar `/onboarding` en vergeet het doel (`onboarding.dart:24-33`). Go_routers eigen deeplinks gaan dus naar de verkeerde host. Nodig: een handler boven `HostApp` die de link bewaart, van host wisselt en pas bij `BoardMemberStep.ready` navigeert, ook bij een koude start en bij `changePassword` | 1 d (Bridge) |
| Sentinel | Parser, argv, socket | onderdeel van §6 |

**Foutmeldingen (US-A5):** een onbekend ticket geeft `NotFound` en een duidelijke melding. Een aparte rechtenfout is er niet, want er is één rol (V1). Wel: niet ingelogd (bewaren tot na inloggen), of geen koppeling met die `host_id` ("Je bent niet met deze server verbonden").

**Totaal 8 d.** In-app 3,5 d, OS-niveau 4,5 d. Dat is L, niet S.

**Product-kanttekening:** zonder webclient (V4) werkt zo'n link alleen op een toestel met Aurelius en een koppeling met die server. Andere apps maken van een eigen scheme vaak geen klikbare link; dat weet ik uit algemene kennis en heb ik niet per app nagekeken. De meeste winst zit dus in links binnen Aurelius en in meldingen, en dat is A5a. Zie B1.

### 5.3 A6: audit, redactor, append-only en RULE 08
**Audit zonder nieuwe tabel.** `events` is al append-only (triggers, `0014_company.sql:201-204`) en heeft `payload_blob_id`. De eerste versie van een bericht staat daar al (`discussion.rs:753`). Dus:
- **Bewerken:** nieuwe blob in `messages.content_blob_id` (`update_message`, `conversations.rs:275-282`), event `message.edited` met als payload de **vorige** blob. `messages` is een projectie en heeft geen no-update-trigger.
- **Verwijderen:** `content_blob_id` wijst naar één vaste tekst ("This message was deleted."), event `message.deleted` met de vorige blob. **Dit is bewust.** Elke lezer (employees via `latest_in`, de samenvatting na Done, de mention-telling in `unread`, zoeken, oudere clients) leest die tekst vanzelf, zonder filter per leespad. Een filter vergeten kan dus geen tekst lekken.
- **Lezen:** `message.history` leest de events van die conversatie, nieuwste eerst. Bij weinig bewerkingen en één conversatie is geen extra index nodig.
- Blobs worden niet opgeruimd (geen `DELETE FROM blobs` gevonden) en `content_blob_keys` kent de event-payloads al (`conversations.rs:790-800`). Een aparte tabel met triggers zou juist botsen met verwijderpaden.

Metadata bevat nooit tekst, alleen `message_id` en `by`. De timeline toont payloads niet.

**Redactor:**
- De nieuwe tekst gaat door `b10` en valt onder `confirm_secret` (`writes.rs:67`, `:72-89`). Zonder regel in die `match` weigert hij altijd; die regel is dus een acceptatiecriterium.
- De vorige tekst is al door de redactor gegaan toen ze werd opgeslagen. Hetzelfde geldt niet voor een waarde die **later** in de vault komt: `message.history` gaat daarom bij het uitgeven door `redact::text`, zoals `webhooks.rs:425-426`.

**Waar het niet samengaat, en dat moet eerlijk in de UI:** bewerken of verwijderen is **geen wissen**.
- Een geheim dat niet in de vault staat, herkent de redactor niet. Het blijft in het event, in de blob en in de back-ups.
- Wat een employee al las, blijft staan: de kopie naar de hoofdtaak (`to_owner`, `discussion.rs:552`), de wake met context (`:490`) en de Claude-sessie zelf.
- De juiste actie na een uitgelekt geheim is roteren en in de vault zetten. Dan maskeert de redactor het bij het uitgeven. Zie B3.

**RULE 08:** de bewerking is een eigen command en roept `board_message` (`discussion.rs:861`) nooit aan. `addressees`, `wake` en `to_owner` zijn dus onbereikbaar. Er ontstaat geen `woke`-meta, dus `waking_posts` telt niet mee (`discussions.rs:270-279`) en er is geen hop. Employees krijgen geen tool: de bridge krijgt geen nieuwe op. Vastleggen met `an_edit_with_a_new_mention_wakes_nobody` en `a_deleted_message_starts_no_run`. Een bewerking met een nieuwe mention geeft ook geen Inbox-rij en geen automatisch volgen, want `mentioned_users` wordt alleen bij een nieuw bericht gezet (B6).

Een klein restje: `unread` leest de huidige tekst (`discussions.rs:341`), dus de mention-badge kan voor een nog ongelezen bericht wijzigen. Dat is onschuldig.

Blijft over: een employee die een board member noemt, kan veel meldingen geven. De wekgrenzen gelden alleen voor het wekken. Het samenvoegen tot één rij per ticket vangt dit op (B5).

### 5.4 QA: wie doet de slotronde?
**Voorstel: Orbit** doet de slot-E2E (3 d). Reden: Pulse is voor mij niet bereikbaar, en ik vond geen besluit over het vertrek in het geheugen. Trace leest code en test niet end-to-end. Flux en Bridge bouwen de clients en zouden hun eigen werk controleren. Orbit raakt in fase A alleen de installers en bouwt en installeert al de builds. Flux en Bridge controleren de registratie uit A5b, omdat Orbit die zelf bouwt. Wil Sebastien Pulse terug, dan vervangt Pulse Orbit zonder verdere impact (B4). De Testing-kolom werkt alleen met een actieve company tester (`board_testing`); of die nog op Pulse staat, heb ik niet in de live data gecontroleerd.

**Opzet:**
- Eén dev-server met Fake-runtime (`AURELIUS_ENV=dev`, `AURELIUS_FAKE_DELAY_MS`; QA-recept AUR-GC-01-T9), twee board members, desktop en mobiel tegelijk, met screenshots.
- Scenario's: toewijzen, volgen en ontvolgen, elke meldingssoort aan de juiste persoon en niet aan de actor, doorklikken, My issues gelijk op beide, link na inloggen en na serverwissel, bewerken zonder run, oude client (gebouwd van `main`).
- Eén keer een echte Claude-run voor het eindpad: fake bewijst geen `ticket.review` met een echte login. Twee board members met eigen login op hetzelfde account volstaan voor dit doel.

**Wat Orbit op Linux niet kan:**
- De scheme-registratie op Windows en macOS: daarvoor is één klik op zo'n toestel nodig.
- Echte iOS-push: dat vraagt een iPhone en de APNs-sleutel.

Beide vragen ongeveer een kwartier van Sebastien, of blijven staan als open acceptatiepunt.

### 5.5 Randgeval: geen meldingen meer op Done of in het archief
**Geen technisch probleem**, met drie punten om te weten:
1. **Heropenen bestaat vandaag niet.** `decide_move` weigert alles vanuit Done (`domain/src/ticket.rs:499`), er is geen unarchive (`mark_archived` is eenrichting, `storage/src/tickets.rs:241`) en de leestekst zegt "Follow-up work is a new ticket" (`discussion.rs:85`). De tak `(false, Some(_))` in `ticket_channel.rs:66-69` is in productie onbereikbaar. De uitzondering kost dus niets, omdat volgers bewaard blijven en de gate naar de huidige stand kijkt, maar ze kan niet voorkomen tot er een heropen-pad komt. V2 zegt dat de engine blijft; ik bouw dat pad dus niet.
2. **De gate moet op de server en in `notify`.** Een discussie op Done is alleen-lezen voor nieuwe berichten, maar `mirror_answer` controleert dat niet (`discussion.rs:611-630`). Een antwoord van een run die vóór Done begon, kan dus na Done binnenkomen met een mention. De gate in `notify` dekt dat, plus push. Let op de volgorde in de transactie: de gate leest de ticketrij, dus elk event moet na de kolomwijziging komen of de doelstand meegeven. Een test per event legt dat vast.
3. **Al ongelezen rijen.** Voorstel: `STILL_RELEVANT` verbergt ook ticketrijen van gesloten tickets, en een rij "in Review" zodra het ticket daar weg is. Dat volgt het bestaande principe "afgehandeld wordt niet meer getoond" (`timeline.rs:465-472`). Zie B7.

Rijen "voor iedereen" (`approval.requested`, `task.waiting`) horen niet bij volgers. Op Done komen ze in de praktijk niet voor, want Accept weigert als een gekoppelde taak nog werkt (`domain/src/ticket.rs:362`).

## 6. Schatting
Eenheid: werkdagen van één engineer, zoals in de eerdere fases. Nova's maten waren een eerste inschatting; hier staan ze ernaast.

| Blok | Onderdeel | Wie | Dagen | Blok totaal | Nova |
|---|---|---|---|---|---|
| 0 | Protocol-freeze, rechten, features, gen-dart | Nexus | 1,5 | | |
| 0 | Migratie 0029, `schema29_tests.rs`, storage-basis | Schema | 1,5 | **3** | |
| A1 | Command, event, systeemregel | Forge | 1 | | |
| A1 | Desktop / mobiel | Flux / Bridge | 1,5 + 1,5 | **4** | M |
| A2 | Volgen, automatisch volgen, beschrijving-diff | Forge | 1,5 | | |
| A2 | Desktop / mobiel | Flux / Bridge | 0,5 + 0,5 | **2,5** | M |
| A5a | `message.locate` | Nexus | 0,5 | | |
| A5a | Links in de app, anker | Flux / Bridge | 1,5 + 1,5 | **3,5** | S (A5) |
| A3 | Ontvangers, dedupe, samenvoegen, gate, mentions, push | Forge | 4 | | |
| A3 | Inbox en push-routing | Flux / Bridge | 2 + 2 | **8** | M |
| A4 | Query en read | Schema / Forge | 0,5 + 0,5 | | |
| A4 | Scherm | Flux / Bridge | 1,5 + 1,5 | **4** | S |
| A6 | Commands, audit, b10, history | Forge | 2 | | |
| A6 | Bewerken, verwijderen, historie | Flux / Bridge | 1,5 + 1,5 | **5** | S |
| A5b | Installers | Orbit | 1 | | |
| A5b | Scheme, doorgeven, na inloggen | Flux / Bridge | 2 + 1,5 | **4,5** | |
| Slot | Security (A5-invoer, A6-audit, pushinhoud) | Sentinel | 1 | | |
| Slot | Eindreview | Trace | 1,5 | | |
| Slot | E2E met twee board members | Orbit | 3 | | |
| Slot | Fixronde na review en QA | Forge, Flux, Bridge | 4 | **9,5** | |
| | **Totaal** | | | **≈ 44 d** | |

**Per persoon:** Flux 10,5 · Bridge 10 · Forge 9 · Orbit 4 · Nexus 2 · Schema 2 · Trace 1,5 · Sentinel 1, plus de fixronde 4.

**Doorlooptijd ≈ 3 weken**, want de client-sporen lopen parallel. Zonder A5b: ≈ 39,5 d werk en ongeveer een halve week minder doorlooptijd.

## 7. Bouwblokken en volgorde
Orions voorstel (één branch; freeze en migraties; A1 → A2 → A5 → A3 → A4 → A6) klopt op hoofdlijnen. Ik wijk op drie punten af:

1. **A5 splitsen; A5a vóór A3, A5b als laatste.** Orions reden blijft gelden: een melding moet doorklikken. Daarvoor is alleen de navigatie binnen de app nodig, niet het OS-niveau. A5b is het riskantste deel (drie besturingssystemen, IPC), raakt niets anders en kan afvallen zonder iets te breken.
2. **A4 en A6 mogen parallel met A3.** A4 hangt alleen van A1 en A2 af, A6 van niets behalve de freeze. Forge is één persoon, dus op de server blijft de volgorde A3 → A4 → A6, maar Flux en Bridge hoeven niet op A3 te wachten als de server van A4 of A6 eerder klaar is.
3. **Eén integratiebranch `feat/aur-fa`, met subbranches per engineer**, zoals AUR-MU-02. De guideline vraagt één worktree en branch per taak, en `app.rs` en de gegenereerde Dart-bestanden hebben één eigenaar nodig (RULE 09).

| Stap | Wat | Wie | Hangt af van |
|---|---|---|---|
| 0 | `fix/aur-fa-a0` in de basis | Orion | Akkoord op A0 |
| 1 | Protocol-freeze (eigen commit) en gen-dart (eigen commit) ‖ migratie 0029 | Nexus ‖ Schema | Stap 0 |
| 2 | A1 → A2 | Forge, daarna Flux en Bridge parallel | Stap 1 |
| 3 | A5a | Nexus, Flux, Bridge | Stap 1 |
| 4 | A3 | Forge, Flux, Bridge | A0, A1, A2, A5a |
| 5 | A4, A6 | Forge en Schema, Flux, Bridge | A1, A2 (A4); stap 1 (A6) |
| 6 | A5b | Orbit, Flux, Bridge | A5a |
| 7 | Sentinel, Trace, E2E op de hele integratie, één fixronde, één keer de volledige suite | Sentinel, Trace, Orbit | Stap 6 |

**Eigenaars (RULE 09):** Forge `crates/**` behalve wat Nexus en Schema in hun stap raken; Schema `migrations/**` en `crates/storage/**` tot de migratie gemerged is; Flux `clients/desktop/**`; Bridge `clients/mobile/**` behalve `lib/data/protocol/*`, dat alleen uit gen-dart komt; Orbit `scripts/**`. Eén slotronde aan het eind, zoals Sebastien vroeg. Sentinel kijkt alleen eerder als A6 of A5b onverwacht wijzigt. Merge en push alleen met akkoord van Sebastien.

## 8. Risico's
| # | Risico | Maatregel |
|---|---|---|
| R1 | Een enumwaarde sluipt in een snapshot en breekt oude clients | Test met de oude enum, zoals `channels_tests.rs:29`; review-punt bij de freeze |
| R2 | Conflict met A0 in `timeline.rs` | A0 eerst, feature-branch daarna |
| R3 | Dubbele meldingen naast de bestaande rijen voor iedereen | B2; test per event |
| R4 | Gate leest de oude kolom in dezelfde transactie | §5.5 punt 2 |
| R5 | "Verwijderd" wekt de verwachting dat het echt weg is | Eerlijke UI-tekst, B3 |
| R6 | Eigen IPC op drie besturingssystemen in de desktop | A5b als laatste, kan afvallen; alleen navigeren, geen acties |
| R7 | Veel events op één ticket | Samenvoegen; alleen de events uit US-A3 geven een rij |
| R8 | Pushverlies: `Lagged` slaat events over (`remote-gateway/src/lib.rs:556`) | Bestaand gedrag; de Inbox blijft kloppen, alleen de push ontbreekt. Niet oplossen in fase A |

## 9. B-punten met teamvoorstel
| # | Vraag | Waarom het uitmaakt | Teamvoorstel |
|---|---|---|---|
| **B1** | Hoort het eigen link-scheme op OS-niveau (A5b) in fase A? | 4,5 d, en zonder webclient beperkt bruikbaar buiten de app | **Ja, als laatste blok**, en het mag afvallen als de planning krap wordt. A5a blijft altijd |
| **B2** | Krijgen tickets nog de rijen "voor iedereen" `Task failed` en `Result ready`? | Anders krijgt iedereen elk ticket en verdubbelt de melding | **Nee voor taken van een ticket:** `task.failed` alleen aan volgers, `Result ready` vervalt omdat `ticket.review` dat zegt. Taken zonder ticket veranderen niet. Goedkeuringen blijven voor iedereen |
| **B3** | Moet een echt wispad bestaan voor een uitgelekt geheim? | Bewerken en verwijderen laten de tekst staan in audit, back-ups en employee-sessies | **Nee in fase A:** roteren en in de vault zetten. De dialoog zegt dat de tekst bewaard blijft. Een wispad vraagt een uitzondering op de append-only regel en is een los besluit |
| **B4** | Wie doet de slot-E2E nu Pulse niet in het organigram staat? | Bepaalt wie de 3 d krijgt | **Orbit.** Pulse terughalen kan ook en verandert verder niets; dat besluit ligt bij Sebastien |
| **B5** | Hoe worden meldingen samengevoegd? | Aanname in US-A3 | **Eén ongelezen rij per persoon per ticket**, met de laatste reden en een teller. Push alleen voor de eerste rij en altijd voor een toewijzing of mention |
| **B6** | Geeft een nieuwe mention door een bewerking een melding? | Tikfout herstellen versus spam | **Nee:** geen wake, geen Inbox, geen automatisch volgen. Eén regel, makkelijk uit te leggen |
| **B7** | Verdwijnen al ongelezen meldingen als een ticket op Done komt? | Orions besluit gaat over nieuwe meldingen | **Ja**, en een rij "in Review" verdwijnt zodra het ticket daar weg is |
| **B8** | Android heeft geen push (`push.rs:58-60`). In scope? | US-A3 zegt "push op mobiel" | **Nee:** iOS volgt de regels, Android krijgt de Inbox in de app. Android-push is een apart onderwerp |
| **B9** | Wat is "Wacht op mij" zonder verantwoordelijke? | US-A4 zegt "mijn tickets" | **Verantwoordelijke = ik, of geen verantwoordelijke en aanmaker = ik**, in Plan review of Review |
| **B10** | Wie mag de vorige tekst lezen? | "Audit" (US-A6) of "zichtbare historie" (roadmap) | **Elke board member** via "Historie" op het bericht, past bij "iedereen mag alles". Geen apps, geen employees |
