# Technische impactanalyse Fase 2: ticketdiscussie en kanalen in de Team chat

*AUR-MU-DEF-T · Titan (Tech Lead) · 2026-10-31 · basis: `main` 21015fd + `feat/aur-mu-01` @ fad4dd2 (Fase 1, protocol 4 bevroren) · bron: `docs/product/multi-user.md` §2 "Ticketdiscussie", "Team chat: kanalen", "Memory"*

Dit is alleen een analyse. Er is geen code gewijzigd. Bestandsverwijzingen gelden voor `feat/aur-mu-01`.

## 0. Samenvatting
- **Protocol: alles additief binnen protocol 4**, aangekondigd met twee features (`ticket_discussion`, `team_channels`). Protocol 5 is niet nodig (§2).
- **Eén discussiemotor voor de Team chat, de kanalen en de tickets**: de bestaande `conversation/group_chat.rs` wordt veralgemeend. De lusgrenzen (MAX_HOPS 3, WAKING_POSTS_PER_RUN 2, WAKING_POSTS_PER_WINDOW 4 per uur) blijven dezelfde constanten en tellen over **alle** discussies samen.
- **Samenvatting naar project-memory na Done: in Fase 2** (eens met Orions voorstel). De extractie draait sinds Fase 1 al bij `accept()` (`task_runs.rs:270-283`). Fase 2 voegt de bron toe die er in Fase 1 niet was (de discussie) en maakt er een gegarandeerd project-memory-item van.
- **Schatting: ≈ 25 d** (§6).

## 1. Datamodel en migraties (migratie 0019, `rebuild: true`)
1. **`conversations.kind`**: CHECK uitbreiden met `'ticket'` (rebuild, zoals 0017 deed). `ConversationKind` komt nooit op de wire (`domain/src/conversation.rs:17`), dus dit is alleen server.
2. **Kanalen = `kind='group'` met een optioneel `project_id`**. De unieke index `one_group_conversation` (0017:33) wordt vervangen door twee: één algemene groep (`project_id IS NULL`) en één per project. De bestaande groepsrij wordt zo vanzelf het **algemene kanaal**: geen datamigratie. Een projectkanaal wordt aangemaakt bij `project.create` en voor bestaande projecten in 0019 (geen lazy aanmaak, zodat `group_chat.get` read-only blijft, zoals in AUR-GC-01 beslist). Gearchiveerd project → kanaal alleen-lezen.
3. **`tickets.conversation_id`** (FK, NOT NULL voor nieuwe tickets; bestaande tickets uit Fase 1 krijgen er een in 0019). Eén discussie per ticket, aangemaakt in dezelfde transactie als `ticket.create`.
4. **`messages.author_user_id`** (nullable FK naar `users`): vanaf Fase 2 schrijven meerdere board members in dezelfde discussie, en de auteur moet in de rij staan (nu staat de user alleen in de event-metadata, S5). Dit is meteen de bron voor de chat-login in Fase 3.
5. **Systeemregels** hebben geen eigen tabel: het zijn berichten met `meta_json.system = { kind: delegated|reported|moved|plan|review, task_id?, from?, to?, by }`, met als auteur "Aurelius", zoals de bestaande melding van Aurelius in de groep.
6. Geen backfill van oude gesprekken (multi-user §3: schone werkruimte).

## 2. Protocol: binnen 4, additief
Oude Fase 1-clients slaan een onbekend event-type stil over (`clients/mobile/lib/core/net/connection.dart:385`, `FormatException` → frame genegeerd), negeren onbekende velden, en roepen nieuwe methodes nooit aan. Eén valkuil: `MessageRole` heeft alleen `User`/`Assistant` (`conversation.rs:54`). **Een nieuwe rol `system` zou een volledige berichtenlijst onleesbaar maken voor een oude client** (de Dart-enum gooit), dus systeemregels gaan via een optioneel veld, niet via een nieuwe rol.

| Wijziging | Soort | Waar | Oordeel |
|---|---|---|---|
| `TicketView.conversation_id` | veld (`serde(default)`) | `domain/src/ticket.rs:110` | additief in 4 |
| `MessageView.author_user: Option<UserView>`, `MessageView.system: Option<SystemLine>` | velden | `domain/src/views.rs` | additief in 4 |
| `ticket.post { ticket_id, text, images, files }` | nieuwe write (`Command::PostTicketMessage`) | `protocol/src/write.rs`, `remote.rs`, `app.rs` (apps: `NOT_FOR_APPS`, zoals `SendGroupMessage` in `app.rs:95`) | additief in 4 |
| Lezen van de discussie | bestaande `conversation.get` (`method.rs:25`) met het `conversation_id` van het ticket | — | geen wijziging |
| `channel.list` → `[ChannelView { conversation_id, project_id?, name, unread }]` | nieuwe read | `method.rs`, `rpc.rs` | additief in 4 |
| `group_chat.get { channel? }` | optionele parameter; zonder = algemeen kanaal | `method.rs:37` | additief in 4 |
| `SendGroupMessage.channel: Option<ConversationId>` | optioneel veld | `domain/src/group_chat.rs`, `write.rs` | additief in 4 |
| `ticket.create.from_message: Option<MessageId>` | "ticket maken vanuit een chat" | `domain/src/ticket.rs` | additief in 4 |
| Live berichten in projectkanalen | bestaande message-events | daemon `api/subscription` | **alleen naar verbindingen met `HelloParams.accepts` = `team_channels`** (`method.rs:392` `client_feature`), anders zou een oude client projectberichten in zijn ene Team chat tonen |
| Features `ticket_discussion`, `team_channels` | `FEATURES` | `protocol/src/rpc.rs:251` | additief |
| Agent-tools `read_discussion`, `post_to_discussion` (of `post_to_group_chat { channel }`) | bridge-protocol | `agent-bridge/src/mcp.rs:431`, `protocol/src/bridge.rs` | intern (zelfde build), raakt de clients niet |

`cargo xtask gen-dart` + de drift-test dekken de Dart-kant.

## 3. Daemon en application
- **Discussiemotor** (`application/src/conversation/group_chat.rs` → `discussion.rs`): `addressees`, `wake`, `mirror_answer` en de tellers krijgen de conversatie als parameter. Regels per soort:
  - **Kanaal**: zoals nu. Zonder @naam van een board member antwoordt de CEO in het algemene kanaal; in een projectkanaal de **dispatcher van het project** (`projects.dispatcher_employee_id` ?? company), voorstel D2-2.
  - **Ticket**: zonder @naam gaat een bericht van een board member naar de **eigenaar** van het ticket, als update in de conversatie van de hoofdtask (`link_run`, zodat de Claude-sessie van het werk doorloopt). Met @naam: de genoemde employees worden gewekt zoals in de groep (verborgen hoofdchat + ticketcontext), hun antwoord wordt teruggespiegeld. Voor een ticket in Review geldt het gedrag van S4: de run rapporteert opnieuw, het ticket blijft in Review.
- **Lusgrenzen (RULE 08)**: dezelfde constanten (`domain/src/group_chat.rs:22-32`). De hop zit in de job-payload (`group: {conversation_id, message_id, hop}`), nu met elke discussie als `conversation_id`. **Het venster per employee telt over alle discussies samen** (meta `woke/employee_id/run_id` over `kind IN ('group','ticket')`), anders verdrievoudigt een lus via drie kanalen. Een systeemregel wekt **nooit** iemand. Restrisico uit AUR-GC-01-T5 (lus via taken herstart na een uur) blijft.
- **Systeemregels** worden in dezelfde transactie geschreven als de oorzaak: delegatie (`insert_task` met `ticket_id`, S3), rapport van een subtask (`after_ended`/`post_result`), kolomwijziging (de `set_status`-hook → `ticket_line`, S4), plan ingediend/goedgekeurd, Review/Done/teruggestuurd. Eén regel per gebeurtenis met een link naar de task, **niet** het volledige rapport (dat staat in de task).
- **B2 uit Fase 1** (feedback via een tekstveld): de feedback van `send_back`/`revise_plan` wordt nu ook een bericht in de discussie van de board member, en gaat zoals nu naar de hoofdtask.
- **Samenvatting na Done**: `accept()` plant de extractie al. Fase 2: voor een ticket krijgt de extractie de discussie + de rapporten van de gekoppelde tasks als invoer en schrijft altijd één item in de **project-scope** (`"Ticket #N <titel>: …"`, locator = ticket), naast de gewone lessen. Dedupe via de bestaande job-id (`extract-{task}`).

## 4. Desktop (Flux)
Ticketweergave: discussie-paneel (hergebruik `group_chat.rs`-weergave + composer met @-aanvulling), systeemregels als compacte grijze regels, auteur per bericht. Team chat: kanalenlijst (algemeen + per project, ongelezen-teller), wissel van kanaal, "Maak ticket" in het berichtmenu. `app.rs` blijft het conflictrisico: één eigenaar tegelijk.

## 5. Mobiel (Bridge)
Ticketpagina krijgt een tab Discussie (`features/chat/chat_view.dart` hergebruiken), Team chat krijgt een kanaalkiezer bovenaan, systeemregels, "Maak ticket" via lang indrukken. `HelloParams.accepts` met `team_channels`.

## 6. Schatting
| Wie | Werk | Dagen |
|---|---|---|
| Schema | 0019 (kind `ticket`, kanaal-indexen, `tickets.conversation_id`, `messages.author_user_id`) + tests | 1 |
| Forge | discussiemotor veralgemenen + tellers over alle discussies | 4 |
| Forge | systeemregels op alle bronnen | 1,5 |
| Forge | kanalen (aanmaak, `channel.list`, gating op `accepts`) | 1,5 |
| Forge | samenvatting naar project-memory | 1 |
| Forge | protocol, rechten, gen-dart, `ticket.create.from_message`, agent-tools | 1 |
| Flux | discussie, kanalen, systeemregels, ticket uit chat | 5,5 |
| Bridge | idem mobiel | 5,5 |
| Sentinel | review: prompt-injectie via discussie (board member-tekst als JSON-context, zoals de groep nu) | 0,5 |
| Pulse | E2E (wekken, lusgrenzen over kanalen, systeemregels, oude client) | 2 |
| Trace | review | 1 |
| **Totaal** | | **≈ 25 d** (doorlooptijd 2–2,5 weken) |

## 7. Bouwvolgorde
1. Schema 0019 ‖ Forge discussiemotor (verschillende bestanden).
2. Forge: protocol + features bevriezen (additief op 4) → gen-dart. **Vanaf hier Flux en Bridge** parallel.
3. Forge: systeemregels, kanalen, samenvatting. Sentinel review direct na stap 2.
4. Pulse QA + Trace review per tak; elke tak brancht van de vorige, geen merge zonder akkoord.

## 8. Risico's
| # | Risico | Maatregel |
|---|---|---|
| R1 | Lus via meerdere discussies | venster telt over alle discussies; systeemregels wekken niet; test met 3 kanalen |
| R2 | Oude client toont projectberichten in de Team chat | events van projectkanalen alleen bij `accepts: team_channels` |
| R3 | Nieuwe `MessageRole` breekt oude clients | geen nieuwe rol, optioneel veld `system` |
| R4 | Ruis: veel systeemregels | één regel per gebeurtenis, inklapbaar in de UI |
| R5 | Kosten: elke @vermelding is een run | bestaande limieten; project-dispatcher antwoordt alleen bij een bericht van een board member zonder @naam |
| R6 | Auteurschap zonder sessies (R2 uit Fase 1) | aanvaard tot Fase 3 |

## 9. Beslispunten met voorstel
| # | Vraag | Voorstel |
|---|---|---|
| D2-1 | Is de ticketdiscussie dezelfde conversatie als die van de hoofdtask? | **Nee**, een eigen conversatie (`kind='ticket'`). De hoofdtask-conversatie blijft de werkruimte (Claude-sessie, tools); de discussie is voor mensen + systeemregels. Berichten zonder @naam gaan door naar de eigenaar |
| D2-2 | Wie antwoordt in een projectkanaal zonder @naam? | De dispatcher van het project (anders de CEO) |
| D2-3 | Tellen de lusgrenzen per discussie of samen? | Samen per employee (venster), per run (2 posts), hop per oorspronkelijk bericht |
| D2-4 | Mogen employees kaarten verplaatsen via een tool (Fase 1 §2.2 schoof `move_ticket` naar Fase 2)? | **Nog niet**: de engine verplaatst al tot Review. Pas toevoegen als Nova er een story voor heeft |
| D2-5 | Samenvatting na Done: in welke fase? | **Fase 2** (de discussie is de bron); de basis-extractie draait al sinds Fase 1 |
| D2-6 | "Give work" en "ticket uit chat" (B7) | Een ticket uit chat in Fase 2 (`from_message`); "Give work" blijft een taak zonder ticket |
