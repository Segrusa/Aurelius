# Technische impactanalyse Fase 3: multi-user (login, sessies, gate, Claude-login per board member)

*AUR-MU-DEF-T · Titan (Tech Lead) · 2026-10-31 · basis: `main` 21015fd + `feat/aur-mu-01` @ fad4dd2 + Fase 2 (deze map, `../fase-2/impactanalyse.md`) · bronnen: `docs/product/multi-user.md` §2 "Toegang" en "Claude-abonnementen", `../fase-1/impactanalyse.md` §4b.5, `../fase-1/spike-claude-logins.md` (code 7e1388d op `spike/aur-mu-00-claude-logins`)*

Dit is alleen een analyse. Er is geen code gewijzigd.

## 0. Samenvatting
- **Protocol 5 is nodig** (§2): `hello` met een onthouden `user_id` zonder bewijs wordt geweigerd, `user.use` verdwijnt en `user.create` vraagt na de onboarding een sessie. Een Fase 1/2-client zou daardoor zonder uitleg vastlopen; met major 5 krijgt hij de bestaande melding "Update needed".
- **Voorwaarde vóór de bouw van het Claude-deel**: de voorwaarden van Anthropic zijn opnieuw gecontroleerd. De tweede-account-test (spike §2.6) **blokkeert niet meer** (Atlas/Sébastien, 2026-10-31: er is voorlopig geen tweede account). Dev en test gebruiken twee board members op **hetzelfde** account, elk met een eigen `CLAUDE_CONFIG_DIR` en een eigen login (§7a). De quotascheiding tussen echte accounts blijft een open acceptatiepunt (MU3-01).
- **Geen fallback**: een run zonder login van de juiste board member wacht met een nieuwe reden `login`. Er bestaat nog geen reden `auth` (0018:91 kent `approval,input,dependency,quota,offline,paused,reconcile,plan_review`).
- **Schatting: ≈ 29 d** (§6).

## 1. Datamodel en migraties (migratie 0020)
1. **`user_sessions`** (naar het voorbeeld van `remote_sessions`, 0001:392): `id`, `user_id` FK, `client_id` (nullable FK naar `remote_clients`; NULL = lokale IPC), `device_name`, `token_verifier` UNIQUE (SHA-256 van een willekeurig token van 32 bytes; het token zelf alleen bij de client), `created_at_ms`, `last_seen_ms`, `expires_at_ms`, `revoked_at_ms`.
2. **`remote_clients.user_id`** (nullable FK, `ADD COLUMN`): de board member die het apparaat het laatst gebruikte, voor de lijst "mijn apparaten". Een apparaat intrekken (`revoked_at_ms`) trekt al zijn user-sessies in.
3. **`users`**: `disabled_at_ms`, `password_changed_at_ms` (`ADD COLUMN`, zoals Fase 1 §4b.1 voorzag). Users worden nooit verwijderd.
4. **`claude_logins`**: `user_id` PK/FK, `status` (`linked|missing|expired`), `email`, `plan`, `linked_at_ms`, `checked_at_ms`. **Geen token en geen pad in de database**: de map is altijd `<data>/logins/<user_id>/` (0700, eigenaar = daemon-user).
5. **`runs.login_user`** (nullable FK) + in de run-snapshot (`ResolvedRunConfig.login_user`, spike §3.2): nodig voor de quota-query per login.
6. **Wachtreden `login`**: CHECK op `tasks.waiting_reason` (en de runs-tegenhanger) uitbreiden → rebuild van `tasks`.
7. **`app_keys.login_user`** en **`recurring_tasks.login_user`** (nullable): op wiens login taken van een app key of schedule draaien (D3-4).
8. **`tickets.login_user`** (NOT NULL, standaard = `created_by_user`): alleen te wijzigen naar de board member die het zelf vraagt (D3-3).

## 2. Protocol: major 5
| Wijziging | Waar | Oordeel |
|---|---|---|
| `PROTOCOL_MAJOR` 4 → 5 | `protocol/src/rpc.rs:21` | **5** |
| `HelloParams.user_id` → `session_token: Option<String>` (redacting `Debug`); `user_id` zonder bewijs wordt niet meer aanvaard | `protocol/src/method.rs:351`, `daemon/src/api/mod.rs` (hello) | **5** (betekeniswijziging) |
| `user.use` verdwijnt → `user.login { username, password, device_name }` → `{ user, session_token, expires_at_ms }` | `method.rs`, `daemon/src/api/users.rs` | **5** (verwijderd) |
| `user.create` vraagt een sessie, behalve zolang er nog geen users zijn | `write.rs` (`HostWrite::UserCreate`), `api/users.rs` | **5** (Fase 1-clients maken nu users vanuit het keuzescherm) |
| De gate: elke methode zonder sessie → `unauthenticated` (nieuwe `ErrorCode`), allowlist `hello`, `user.whoami`, `user.login`, `user.create` (bootstrap), `user.list` | `daemon/src/api/mod.rs` dispatch, `domain` `ErrorCode` | **5** |
| `user.logout`, `session.list`, `session.revoke`, `user.set_password`, `user.disable` | `method.rs`, `write.rs` | nieuw binnen 5 |
| `claude_login.status`, `claude_login.link` → `{ link_id }` (start `claude auth login` in een PTY, §3), `claude_login.cancel`, `claude_login.unlink` | `method.rs`, `write.rs` | nieuw binnen 5 |
| `claude_login.input { link_id, text }`: een **sessiemethode, geen write** (zoals `user.use`): geen receipt, geen `request_hash`, geen event, redacting `Debug` | `method.rs`, `daemon/src/api/` | nieuw binnen 5 |
| Event `claude_login.prompt { link_id, url?, asks_code }` alleen naar de verbinding die koppelt (geen broadcast) | `events.rs` | nieuw binnen 5 |
| `HelloResult.claude_login: Option<ClaudeLoginView>`; `SubscriptionUsage` per user | `rpc.rs:135`, `runtimes.rs:155` | nieuw binnen 5 |
| `TicketView.login_user` + `needs: login`; write `ticket.use_my_login { ticket_id }` (D3-3) | `domain/src/ticket.rs`, `write.rs` | nieuw binnen 5 |
| Events `user.disabled`, `claude_login.updated` (geen sessie-events naar alle clients) | `protocol/src/events.rs` | nieuw binnen 5 |

*Alternatief binnen 4* (afgeraden): `user.use` laten bestaan en alleen het wachtwoordloze `hello` weigeren. Oude clients vragen dan bij elke verbinding het wachtwoord, maar `user.use` blijft een sessieloze wachtwoordcontrole en een oude client krijgt bij "New board member" een onverklaarde fout.

## 3. Daemon
- **Login** (`daemon/src/api/users.rs`): argon2id via de bestaande `HASHERS`-semaphore (`users.rs:35`), dezelfde fout voor een onbekende user en een fout wachtwoord, opnieuw hashen bij sterkere parameters. **Limieten**: `user.login` 5/min per (verbinding, username) met oplopende vertraging; **`user.create` (R3)**: 5/min per verbinding + 20/uur globaal, en na de bootstrap alleen met een sessie, waarmee R3 grotendeels vervalt. In-memory tellers volstaan (één daemon-instantie).
- **Sessies**: token alleen in `HelloParams`; de server bewaart de verifier. Verloop: 30 dagen zonder gebruik, 90 dagen absoluut. Wachtwoord wijzigen of user uitschakelen trekt alle sessies in en sluit open verbindingen van die user. `Peer.user` wordt alleen nog gezet door `hello{session_token}`, `user.login` en de bootstrap-`user.create`.
- **Relatie met pairing**: de koppeling (`remote_clients` + `remote_sessions` van 12 uur via de gateway) blijft de authenticatie van het **apparaat**; de user-sessie is een tweede laag erbovenop. Een remote verbinding zonder geldig apparaat komt er nooit bij. De lokale IPC (desktop op de server) krijgt een user-sessie met `client_id = NULL` (D3-1).
- **De volledige gate** vervangt `user.use` en de Fase 1-controle (`require_board_member` blijft als tweede slot in de ticket-handler). App keys blijven een aparte principal (eigen scopes, geen user); de bridge van de employees gaat niet langs de gate. Raakt veel daemon-tests: een helper `connect_as(user)` in de testsupport.
- **Tailscale**: geen applicatiecode. Orbit controleert dat de remote-gateway alleen op het tailnet luistert (bind-adres of firewall) en documenteert de installatie (ACL, MagicDNS). Een `tailscale whois`-koppeling is buiten scope.
- **Claude-login per board member** (spike §3.1/§3.2; `ClaudeLogin::ConfigDir` uit 7e1388d in `runtime/src/claude_code/mod.rs` overnemen):
  - **Koppelen** (D3-8): `claude_login.link` start `claude auth login` met `CLAUDE_CONFIG_DIR=<data>/logins/<user>` in een **PTY** (de pty-laag van de bestaande terminal-feature, zonder shell). De daemon herkent de regel `visit: https://claude.com/cai/oauth/authorize?…` en de prompt `Paste code here if prompted >`, en stuurt die als `claude_login.prompt` naar de client. De board member opent de link zelf, logt in bij Anthropic en plakt de code. `claude_login.input` schrijft die code in de PTY. Daarna `auth status --json` voor e-mail en plan. Time-out 10 min, één link per user tegelijk. Geen `setup-token` (voorwaarden, spike §4).
  - **Gemeten (2026-10-31, CLI 2.1.312, stdin `/dev/null`, geen TTY)**: het commando start zonder TTY, drukt de URL af (`redirect_uri=platform.claude.com/oauth/code/callback`, PKCE S256, dus geen localhost-callback nodig) en wacht op de code. Dat het de code via een pipe aanvaardt, is niet gemeten (daarvoor is een echte login nodig). Daarom een PTY: daar gedraagt `claude` zich zoals in een terminal. De echte koppeling testen we in de eerste dev-test (§7a).
  - **Resolutie** (`runtimes::runtime_for_config`, vastgelegd in de snapshot): ticketrun, delegatie en triage → `tickets.login_user` (= de aanmaker, tenzij D3-3) via `tasks.ticket_id` (erft al via SQL in `insert_task`, S3); chatrun (employee-chat, project-assistent) → `messages.author_user_id` van het bericht (Fase 2); gewekte run in Team chat of ticketdiscussie → de board member van het **oorspronkelijke** bericht, meegegeven in de groepsmarkering (`group: {…, login_user}`), ook op hop 2–3; extractie/samenvatting → de login van de bron; taak van een app key → `app_keys.login_user`.
  - **Geen login** (de aanmaker heeft er geen, of die is verlopen of uitgeschakeld): `ticket.start` laat het ticket in Todo met `needs: login` ("<naam> moet zijn Claude-login koppelen"), zoals B5. Een lopende run die een 401 krijgt, gaat naar `waiting/login`. Er is **nooit** een fallback naar `~/.claude` of naar een andere user.
  - **Quota per login**: `queries::park_ready_runs_for_quota(tx, provider, until)` (`storage/src/queries.rs:246`) krijgt `login_user` als filter; `wake_parked_runs` idem. Alleen het ticket (met al zijn delegaties) van die aanmaker wacht. UI: "wacht op quotum van Alice".
  - **`/usage`-probe per login**: `fetch_subscription` (`runtimes.rs:155`) en `Shared.subscription` (`lib.rs:86`) worden een `HashMap<UserId, SubscriptionUsage>`. Een lijst zonder `% used` = "geen abonnementslogin" (spike §2.4), geen 0%.
  - **Claude-sessies per login**: transcripts staan in de loginmap, dus een conversatie die eerst op Alices login liep, kan niet hervat worden op Bobs login (een employee-chat of de verborgen hoofdchat die door de groep gewekt wordt). `conversation_sessions` krijgt de sleutel (conversatie, login); bij een andere login start een nieuwe sessie met de recente berichten als context. `sessions.rs::sweep` loopt over alle loginmappen.
  - **Opruimen**: user uitschakelen → `claude auth logout` + de map verwijderen. De map komt nooit in backups of de fresh start (`fresh_start::CONFIG_FILES` blijft alleen `remote.json`).

## 4. Desktop (Flux)
Login-scherm vervangt de wachtwoordprompt van `user.use` (zelfde schermen uit Pixel §11), sessietoken in `host::secrets` per server, uitloggen, "Mijn apparaten/sessies", Board members uitschakelen/wachtwoord resetten, Claude-login koppelen (begeleide Connect-flow van Pixel P-32, met "Show command" als terminalvariant), quotum van de eigen login, badges `needs: login` en "op quotum van X".

## 5. Mobiel (Bridge)
Idem: login, token in secure storage per gekoppelde server (naast de apparaat-credential), sessies, Claude-login-status en dezelfde Connect-flow (de link opent in de browser van de telefoon; de code wordt teruggeplakt), badges.

## 6. Schatting
| Wie | Werk | Dagen |
|---|---|---|
| Schema | 0020 (sessies, `remote_clients.user_id`, users-kolommen, `claude_logins`, `runs.login_user`, wachtreden `login`, `app_keys.login_user`) | 1 |
| Forge | login, sessies, volledige gate + test-helper, limieten (R3) | 4,5 |
| Forge | protocol 5 + gen-dart + client-crate | 1 |
| Forge | Claude-login: runtime uit de spike, resolutie + snapshot, koppelen via PTY (Connect-flow) | 4 |
| Forge | quota per login + `/usage` per user | 1,5 |
| Forge | Claude-sessies per (conversatie, login) + sweep | 1,5 |
| Orbit | Tailscale-only bind/firewall + installatiedoc | 1 |
| Sentinel | threat model, review login/sessies/tokens, voorwaarden opnieuw controleren | 1,5 |
| Flux | desktop | 4 |
| Bridge | mobiel | 4 |
| Pulse | E2E volgens §7a (twee mappen op één account, gesimuleerde quota-fout per login), oude client → "Update needed" | 2,5 |
| Trace | review | 1,5 |
| **Totaal** | | **≈ 29 d** (doorlooptijd 3–3,5 weken) |

## 7. Bouwvolgorde
1. Geen externe voorwaarde meer: de tweede-account-test is een open acceptatiepunt, geen blokkade.
2. Schema 0020 ‖ Forge login + sessies + gate. Sentinel reviewt meteen.
3. Forge: protocol 5 bevriezen → gen-dart. **Vanaf hier Flux en Bridge.** Orbit parallel.
4. Forge: Claude-login (runtime, resolutie, koppelen) → quota en `/usage` per login → sessies per login.
5. Pulse QA volgens §7a, Trace per tak. Uitrol: server + desktop samen, mobiel daarna (tussendoor "Update needed").

## 7a. Teststrategie zonder tweede account (MU3-01)
- **Opstelling**: board members A en B, elk met een eigen `<data>/logins/<user>/`, en allebei via de Connect-flow ingelogd op **hetzelfde** Claude-account (twee aparte logins, dus twee aparte refresh-tokens).
- **Bewezen met echte Claude**: (1) een ticketrun van A draait met `CLAUDE_CONFIG_DIR` = map van A, die van B met die van B (in de proces-env en `runs.login_user`); (2) een delegatie van het ticket van A, ook op hop 2–3, draait op de map van A; (3) gescheiden token en sessie: B uitloggen (`claude auth logout` in de map van B) laat de runs van A doorlopen, B krijgt `needs: login`; een transcript van A staat alleen in de map van A en wordt nooit hervat in die van B (R4); (4) een lege map faalt met "Not logged in" en valt **niet** terug op `~/.claude`.
- **Gesimuleerd**: de quota-fout per login. Een testseam in de runtime laat de run van één `login_user` een rate-limit-resultaat teruggeven. Verwacht: alleen het ticket van die login parkeert (`waiting/quota`, "wacht op quotum van A"), het ticket van B loopt door, `wake_parked_runs` wekt alleen A. `/usage` per login met een vaste fixture per user.
- **Open acceptatiepunt**: echte quotascheiding en `/usage` met twee echte accounts. Te doen zodra er een tweede account is; tot dan staat het als bekend restrisico (R1) in de oplevering.

## 8. Risico's
| # | Risico | Maatregel |
|---|---|---|
| R1 | Geen tweede account: echte quotascheiding en `/usage` per account zijn niet bewezen; een tweede account kan zich anders gedragen (refresh, plan) | twee logins op één account + gesimuleerde quota-fout (§7a); open acceptatiepunt in MU3-01, herhalen zodra er een tweede account is |
| R1b | Twee logins op hetzelfde account delen in dev één quotum; zware tests leggen beide tegelijk stil | testtickets klein houden; de quotatest is gesimuleerd |
| R1c | De Connect-flow hangt af van de tekst die `claude auth login` afdrukt (URL-regel, prompt); een CLI-update kan die wijzigen | tolerant parsen (eerste `https://claude.com/…oauth/authorize`-URL); lukt het niet, dan valt de client terug op "Show command" |
| R2 | Voorwaarden van Anthropic wijzigen; zware parallelle runs op één abonnement | opnieuw controleren vóór de start; geen keuze "login van X gebruiken"; alternatief API-key/Teams (spike §4.4) |
| R3 | Refresh-token-rotatie bij parallelle runs op dezelfde map | één map per user, nooit kopiëren (spike §1) |
| R4 | Resume breekt bij een loginwissel in een gedeelde conversatie | sessie per (conversatie, login) |
| R5 | Werk staat stil omdat de aanmaker afwezig is (quotum, geen login) | zichtbaar `needs: login`/quotum; overnemen met D3-3 |
| R6 | Credentials van meerdere mensen op de server | 0700, nooit in backups, logout bij uitschakelen |
| R7 | De gate raakt ~40 daemon-testbestanden | één helper, één commit (RULE 09) |

## 9. Beslispunten met voorstel
| # | Vraag | Voorstel |
|---|---|---|
| D3-1 | Moet de desktop op de server zelf (lokale IPC) ook inloggen? | **Ja**, voor correct auteurschap; de sessie wordt onthouden, dus één keer. Wie shell-toegang heeft, leest de database toch al: dit is auteurschap, geen extra beveiliging |
| D3-2 | Protocol 5 of 4 met verminderde oude clients? | **5** |
| D3-3 | Wat als de aanmaker geen login heeft of weg is? | Ticket blijft in Todo/wacht met `needs: login`. Een andere board member kan expliciet **"Run on my login"** kiezen: `tickets.login_user` wordt dan die persoon zelf (nooit iemand anders kiezen). `created_by_user` blijft ongewijzigd (MU1-07) |
| D3-4 | Op wiens login draaien taken van een app key, heartbeats en terugkerende taken? | De board member die de app key of schedule aanmaakte (`login_user`); zonder → wacht met `login` |
| D3-5 | Achtergrondjobs zonder bron (memory-consolidatie) | Een board member kiest zelf zijn eigen login als "company-login" voor deze jobs; zonder keuze draaien ze niet |
| D3-6 | Wie mag users aanmaken na de bootstrap? | Elke board member met een sessie (één rol) |
| D3-7 | Sessieduur | 30 d idle / 90 d absoluut, intrekken per apparaat |
| D3-8 | Claude-login koppelen: terminal op de server (Nova OV-01) of begeleide Connect-flow in de app (Pixel P-32)? | **Connect-flow als standaard, terminal als alternatief.** Het kan zonder TTY (gemeten, §3): Aurelius geeft alleen de link door en schrijft de geplakte code in een PTY. Aurelius blijft buiten de credentials: het token schrijft `claude` zelf in de map, en Aurelius leest `.credentials.json` nooit. De code is eenmalig en zonder de PKCE-verifier, die alleen het `claude`-proces kent, niets waard. Hij gaat niet langs de write-pipeline en komt niet in een event, log of de database. Het is hetzelfde vertrouwen als de bestaande terminal-feature, die ook toetsaanslagen doorgeeft. Sentinel bevestigt dit in zijn review vóór de bouw. Herkent de daemon de prompt niet, dan valt de flow terug op "Show command" |
