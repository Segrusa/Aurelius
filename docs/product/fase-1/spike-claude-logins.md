# Spike AUR-MU-00-T1: twee Claude-logins op één server

**Opdrachtgever:** Orion · **Uitvoerder:** Titan · **Datum:** 2026-10-30
**Claude Code:** 2.1.312 (Linux) · **Spike-branch:** `spike/aur-mu-00-claude-logins`, commit `7e1388d` (worktree `target/wt-mu-spike`, niet gepusht, niet op main)

## Conclusie in het kort

- **Optie (a) werkt technisch.** Elke run krijgt via `CLAUDE_CONFIG_DIR` een eigen loginmap. Een proces ziet alleen de login in zijn eigen map. Twee processen met verschillende mappen lopen tegelijk zonder elkaar te storen. Een lege map heeft geen toegang: `Not logged in`, kost 0. Het daemon-account werkt ernaast gewoon verder.
- **Aanbevolen mechanisme: `CLAUDE_CONFIG_DIR` per board member.** De board member logt zelf in met de ongewijzigde `claude`, via het eigen loginscherm van Anthropic. Aurelius leest, kopieert of bewaart dan nooit een token. Dit past het best bij de voorwaarden van Anthropic (zie §4).
- **`CLAUDE_CODE_OAUTH_TOKEN` (setup-token) werkt ook**, maar dan bewaart Aurelius een token van de user. De voorwaarden verbieden "collect, store, or intermediate Claude.ai credentials or session tokens" voor developers die een product bouwen. Dit gebruiken we dus alleen als uitzondering of fallback, nooit als standaard.
- **Er is nog één bevestiging met een tweede echt account nodig.** Er was hier maar één account beschikbaar. De handeling voor Sébastien staat in §2.4.
- **Werk voor Fase 3:** quota-parkering en de `/usage`-probe zijn nu globaal per provider. Ze moeten per login worden (§3.3).

## 1. Wat elk mechanisme leest en schrijft

Bronnen: officiële docs (https://code.claude.com/docs/en/authentication) plus eigen metingen hieronder.

| | Standaard (geen variabele) | `CLAUDE_CONFIG_DIR=<dir>` | `CLAUDE_CODE_OAUTH_TOKEN=<token>` |
|---|---|---|---|
| Credentials | Linux/Windows: `~/.claude/.credentials.json` (0600). macOS: Keychain-item, met fallback naar dat bestand | `<dir>/.credentials.json`. Op macOS een Keychain-item **gekoppeld aan die map** (volgens de docs) | Alleen de variabele. Er wordt niets weggeschreven, want `setup-token` bewaart het token nergens. Het token is 1 jaar geldig en kan alleen model-requests doen |
| Globale config (`.claude.json`) | `~/.claude.json` | `<dir>/.claude.json` (gemeten: er komt niets in `$HOME`) | Volgt de config-map (standaard of `CLAUDE_CONFIG_DIR`) |
| Settings, sessies/transcripts | `~/.claude/settings.json`, `~/.claude/projects/...` | `<dir>/settings.json`, `<dir>/projects/...` (zie `projectsDirectory` in de output) | Volgt de config-map |
| Tijdelijke bestanden | `$TMPDIR/claude/<folder>/<session-id>` | Zelfde gedeelde map. De naam bevat het session-id, dus er is geen botsing | Idem |

**Voorrang** (docs, sectie "Authentication precedence", plus eigen meting): cloud-provider-variabelen > `ANTHROPIC_AUTH_TOKEN` > `ANTHROPIC_API_KEY` > `apiKeyHelper` > `CLAUDE_CODE_OAUTH_TOKEN` > Anthropic-profiel > `/login`-credentials in de config-map. Gevolgen:

- Zijn de token en de map **allebei** gezet, dan wint het token. De map bepaalt dan alleen nog waar settings en sessies staan (gemeten in C hieronder).
- Een `ANTHROPIC_API_KEY` in de omgeving zou alles overrulen (gemeten in E). Daarom mag die nooit van de daemon naar de child lekken. De bestaande allowlist houdt hem al tegen, en de spike-test bewijst dat.

**Storen twee processen elkaar?** Met verschillende mappen zijn er geen gedeelde bestanden behalve `$TMPDIR/claude`, en die is per session-id. Gemeten: A (ingelogd) en B (leeg) liepen tegelijk, A kreeg `OK` en B `Not logged in`. Met **dezelfde** map lopen nu al dagelijks parallelle runs op `~/.claude`. Eén risico is niet gemeten: als twee processen op dezelfde map tegelijk een token verversen, kan de refresh-token-rotatie misgaan. Per user één map, en geen kopieën van `.credentials.json` naar andere mappen. Een kopie zou bij een refresh de rotatie splitsen en de originele login ongeldig maken. Daarom is in deze spike bewust **geen** credentials-bestand gekopieerd.

## 2. Bewijs (commando's met uitvoer)

Alle runs gebruiken `/tmp/mu-spike/run.sh`. Dat script start `claude` zoals de daemon het doet: `env -i` met alleen `PATH HOME USER LOGNAME SHELL LANG` plus `CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1`, en extra `VAR=VAL` per run. E-mail en org-id zijn in dit rapport gemaskeerd. Er staan geen tokens in de commando's, behalve bewust ongeldige nep-tokens (`…INVALID…`).

### 2.1 Account per map (`auth status --json`)
```
$ run.sh -- auth status --json                      # standaard ~/.claude
{ "loggedIn": true, "authMethod": "claude.ai", "configDirectory": "/home/sdc/.claude",
  "projectsDirectory": "/home/sdc/.claude/projects", "email": "opt***@gmail.com",
  "orgId": "********-…", "subscriptionType": "max" }
exit=0

$ run.sh CLAUDE_CONFIG_DIR=/tmp/mu-spike/cfg-empty -- auth status --json
{ "loggedIn": false, "authMethod": "none", "configDirectory": "/tmp/mu-spike/cfg-empty",
  "projectsDirectory": "/tmp/mu-spike/cfg-empty/projects" }
exit=1
```

### 2.2 Twee runs tegelijk (`claude -p … --output-format json`, profiel zoals text-only)
```
P = -p "Reply with exactly: OK" --output-format json --model haiku --tools "" --strict-mcp-config
    --mcp-config '{"mcpServers":{}}' --safe-mode --no-session-persistence
$ ( run.sh -- $P ) & ( run.sh CLAUDE_CONFIG_DIR=/tmp/mu-spike/cfg-empty -- $P ) & wait
15:53:43.89 → 15:53:50.43
A (standaard): {'subtype': 'success', 'is_error': False, 'result': 'OK', 'total_cost_usd': 0.00045}   exit=0
B (lege map):  {'subtype': 'success', 'is_error': True,  'result': 'Not logged in · Please run /login', 'total_cost_usd': 0}   exit=1
Inhoud lege map na de run: .claude.json, backups/, plugins/store, sessions   (geen credentials)
```

### 2.3 Voorrang en wat er in `$HOME` terechtkomt
```
C  standaardlogin + CLAUDE_CODE_OAUTH_TOKEN=<ongeldig>
   auth status: "loggedIn": true, "authMethod": "oauth_token"
   -p: is_error=True, 'Failed to authenticate. API Error: 401 OAuth access token is invalid.'
   → het token wint van het credentials-bestand; geen stille fallback naar de map.
D  lege map + CLAUDE_CODE_OAUTH_TOKEN=<ongeldig>
   auth status: "loggedIn": true, "authMethod": "oauth_token"
   → `auth status` controleert een token NIET online (zie §3.2).
E  standaardlogin + ANTHROPIC_API_KEY=<ongeldig>
   auth status: "authMethod": "api_key";  -p: 'Invalid API key · Fix external API key' (401)
F  standaardlogin opnieuw: 'OK' (niets kapotgemaakt)

$ env -i … HOME=/tmp/mu-spike/home-fake CLAUDE_CONFIG_DIR=/tmp/mu-spike/cfg-x claude -p "Reply OK" …
geschreven onder fake HOME:          (niets)
geschreven onder CLAUDE_CONFIG_DIR:  .claude.json, backups/, plugins/store, sessions
```

### 2.4 `/usage`-probe per login (exact de argumenten van `subscription_usage`)
```
-- standaard:       is_error=False cost=0
   "You are currently using your subscription…  Current session: 13% used · resets Oct 30, 6pm …
    Current week (all models): 23% used …"
-- lege map:        is_error=False cost=0   "Total cost: $0.0000 … Usage: 0 input, 0 output…"
-- ongeldig token:  is_error=False cost=0   idem (kostenoverzicht, geen planlimieten)
```
Zonder geldige abonnementslogin geeft `/usage` geen fout terug, maar een kostenoverzicht zonder `% used`-regels. Daardoor geeft `parse_plan_limits` een lege lijst terug. Of `/usage` planlimieten toont met een **geldig** setup-token, kon ik niet meten, want dat vraagt een browserlogin. Dat is een open punt, en nog een reden om `CLAUDE_CONFIG_DIR` te verkiezen.

### 2.5 Hetzelfde via de Rust-runtime (spike-branch)
```
$ cargo test -p aurelius-runtime --lib login_tests -- --include-ignored --nocapture
own login signed in: Ok(true); empty folder signed in: Ok(false)     ← beide tegelijk (tokio::join!)
test claude_code::login_tests::live_an_empty_login_folder_has_no_access ... ok
test claude_code::login_tests::nothing_of_the_daemons_login_reaches_the_child ... ok
test claude_code::login_tests::each_run_gets_only_its_own_login ... ok
test claude_code::login_tests::the_usage_probe_and_sessions_follow_the_login ... ok
test claude_code::login_tests::a_token_never_shows_in_debug_output ... ok
$ cargo test -p aurelius-runtime --lib   → 56 passed; 0 failed; 1 ignored (de live-test)
$ cargo check -p aurelius-daemon --all-targets → ok
```

### 2.6 Bevestiging met een tweede account (handeling voor Sébastien)
Doe dit op de server, als de user die de daemon draait. Er komt niets in de repo, en de token verschijnt nergens:
```bash
mkdir -p ~/.local/share/aurelius/logins/test2 && chmod 700 ~/.local/share/aurelius/logins/test2
CLAUDE_CONFIG_DIR=~/.local/share/aurelius/logins/test2 claude auth login   # log in met het TWEEDE account in de browser
# daarna, tegelijk:
( env -i PATH="$PATH" HOME="$HOME" claude auth status --json | grep -E 'email|subscriptionType' ) &
( env -i PATH="$PATH" HOME="$HOME" CLAUDE_CONFIG_DIR=~/.local/share/aurelius/logins/test2 claude auth status --json | grep -E 'email|subscriptionType' ) & wait
# en een echte run per account (laat bij de tweede ook /usage zien dat het een ander quotum is):
env -i PATH="$PATH" HOME="$HOME" CLAUDE_CONFIG_DIR=~/.local/share/aurelius/logins/test2 claude -p /usage --output-format json </dev/null
```
Verwacht resultaat: twee verschillende e-mails, en `/usage` met andere percentages. Opruimen gaat met `CLAUDE_CONFIG_DIR=… claude auth logout` en het verwijderen van de map.

## 3. Voorstel voor `crates/runtime/src/claude_code/mod.rs`

### 3.1 Wat de spike-commit doet (`7e1388d`)
- `ClaudeCodeRuntime` krijgt `login: Option<ClaudeLogin>`, met `ClaudeLogin::ConfigDir(PathBuf)` of `ClaudeLogin::OauthToken { token, config_dir }`. Een eigen `Debug` maskeert het token.
- `command()` werkt nu via `command_from(args, parent_env)`: eerst `env_clear()` en de bestaande `ENV_ALLOWLIST`, **daarna** alleen de login van de run (`CLAUDE_CONFIG_DIR`, eventueel plus `CLAUDE_CODE_OAUTH_TOKEN`). De allowlist blijft ongewijzigd, dus van de daemon gaat er geen `ANTHROPIC_*`, `CLAUDE_CODE_OAUTH_TOKEN`, `CLAUDE_CONFIG_DIR` of `CLAUDE_CODE_USE_*` mee. De test geeft een vervalste parent-omgeving mee en controleert dat.
- `start`, `has_session` en `remove_session` gebruiken `self.claude_dir()`. Dat is de map van de login, en anders `~/.claude`. Zonder deze aanpassing zou de runtime transcripts in de verkeerde map zoeken, en zouden resume na een quota-stop en het voortzetten van gesprekken stuk gaan.
- `/usage` (`subscription_usage`), `auth status` (`signed_in`, `subscription_type`) en `--version` gebruiken dezelfde `command()`, dus ze vragen automatisch de login van die runtime.
- Alle bestaande aanroepers geven `login: None` mee. Het gedrag verandert dus niet tot Fase 3.

**Waarom op de runtime en niet als veld op `RunRequest`:** `RunRequest` is `Debug + Serialize` en komt in traces terecht, waardoor een token kan lekken. De daemon bouwt bovendien per run al een eigen `ClaudeCodeRuntime` in `runtimes::runtime_for_config(config)`. Een login in de run-config komt daardoor nog altijd per request binnen, en `/usage` en `auth status` profiteren er ook van.

### 3.2 Aansluiting in Fase 3 (nog niet gebouwd)
1. **Opslag:** een tabel `claude_logins(user_id, config_dir, status, email, plan)` met de map `<data>/logins/<user_id>/` (0700, eigenaar = daemon-user). Er komt geen token in de database. Koppelen gebeurt in een terminal of via een begeleide stap die `claude auth login` met die `CLAUDE_CONFIG_DIR` start, zodat de user zelf in de browser inlogt.
2. **Resolutie:** `ResolvedRunConfig` krijgt `login_user: Option<UserId>`. Die wordt vastgelegd in de snapshot van de run (net als `connection_id`), zodat retries en een hervatting na quota dezelfde login houden.
   - Ticketrun: `tickets.created_by_user`.
   - Delegatie of child-task: via `parent_task_id` naar de root-task en dan naar het ticket, en dus naar dezelfde aanmaker.
   - Team chat of project-assistent: de afzender van het bericht.
   - Geen login gevonden: de run wacht met `auth` ("koppel je Claude-login"). Er is **geen** fallback naar `~/.claude` of naar een andere user.
3. **`runtime_for_config`** vult `login` met `ClaudeLogin::ConfigDir(<data>/logins/<user>)`.
4. **Verificatie:** gebruik geen `auth status` om te bepalen of een user "ingelogd" is. Bij een token geeft dat `loggedIn:true` zonder online controle (meting D). Gebruik `auth status --json` voor e-mail en plan bij de ConfigDir-variant. Een 401 in een run wordt `auth`, net als nu.
5. **`sessions.rs::sweep`** loopt nu alleen over `session::claude_dir()`. Dat moet over alle loginmappen lopen.

### 3.3 Quota-detectie per user, met delegaties en de `/usage`-probe
- **Detectie blijft per run en klopt al per user.** `parse.rs` haalt `Quota` uit het resultaat of uit een rate-limit-event, en `mod.rs` doet dat bij een afgebroken stream uit stderr. Dat signaal komt van het abonnement van díe child, dus van de login van die run.
- **Parkeren is nu globaal en moet per login worden.** `dispatcher/chat.rs::park_for_quota` → `service.park_runs_for_quota(at)` → `queries::park_ready_runs_for_quota(tx, "claude_code", until)` zet **alle** klaarstaande `claude_code`-runs op `quota`. Met meerdere users zou het quotum van Alice ook de runs van Bob stilleggen. Voorstel:
  - Voeg `runs.login_user` toe (of lees die uit de config-snapshot) en filter in de query op `AND r.login_user = ?3`.
  - Dan wacht een run alleen op het quotum van zijn eigen aanmaker, met de bestaande reden `quota` en `until_ms`. De gekoppelde task gaat via `task_runs::on_run_quota_wait` mee naar `waiting/quota`, zoals nu.
  - Er is geen fallback: de login staat in de snapshot en mag na het parkeren niet wijzigen.
- **Delegaties:** child-tasks erven via de root-task de login van de aanmaker. Raakt het quotum op tijdens een delegatie, dan wachten de parent en alle children van dat ticket samen, en lopen tickets van andere board members door. Een delegatie van Titan aan Forge binnen het ticket van Alice verbruikt dus het abonnement van Alice. Dat is de bedoeling, en het moet zichtbaar zijn in de UI ("op quotum van Alice").
- **`/usage`-probe:** `runtimes::fetch_subscription` gebruikt nu één connectie en één cache (`shared.subscription`). Dat wordt `subscription_usage(user)`, met een `ClaudeCodeRuntime { login: Some(ConfigDir(user)) }` en een cache per user in een `HashMap<UserId, SubscriptionUsage>`. De UI toont dan het quotum van de ingelogde board member. Een lege lijst limieten met een kostenoverzicht betekent "geen abonnementslogin" (meting 2.4). Dat wordt een melding "koppel je Claude-login", geen 0%.

## 4. Voorwaarden van Anthropic (stand 2026-10-30)

Bronnen:
- Claude Code, Legal and compliance: https://code.claude.com/docs/en/legal-and-compliance
- Claude Code, Authentication: https://code.claude.com/docs/en/authentication
- Consumer Terms (effective Oct 8, 2025): https://www.anthropic.com/legal/consumer-terms
- Commercial Terms: https://www.anthropic.com/legal/commercial-terms
- Usage Policy: https://www.anthropic.com/legal/aup

**Wat er staat (letterlijk of nauw samengevat):**
- Pro/Max valt onder de Consumer Terms. Daar staat: "You may not share your Account login information … or Account credentials with anyone else." Automatische toegang ("bot, script") mag alleen met een API-key of "where we otherwise explicitly permit it". Claude Code met `-p` en `setup-token` ("for CI pipelines and scripts") is zo'n expliciet toegestaan gebruik in de eigen documentatie.
- Legal and compliance: OAuth is "designed to support ordinary use of Claude Code". Daarnaast: "Anthropic does not permit third-party developers to offer Claude.ai login into their own applications, or to route requests through Free, Pro, or Max plan credentials on behalf of their users. Moreover, developers may not collect, store, or intermediate Claude.ai credentials or session tokens — sign-in to a Claude account must complete through Anthropic's own flow." Maar ook: "Nor does it prevent an end user from signing in to the unmodified Claude Code binary with their own Claude subscription, including where a platform hosts Claude Code."
- Wie Claude Code in een product of dienst draait (hosted), valt onder de Commercial Terms: de binary blijft ongewijzigd, en "Each end user must authenticate with their own … Claude subscription plan credentials", zonder dat het gebruik betaald, doorverkocht of bemiddeld wordt.
- "Advertised usage limits for Pro and Max plans assume ordinary, individual usage of Claude Code and the Agent SDK."

**Advies:**
1. **Toegestaan, met de juiste vorm:** elke board member logt **zelf** in op de ongewijzigde `claude`, in een eigen `CLAUDE_CONFIG_DIR`, via het loginscherm van Anthropic. Elke run gebruikt alleen dat eigen abonnement. Aurelius ziet, bewaart en proxyt geen token: het zet alleen een map-pad in de omgeving. Dat valt onder "an end user signing in to the unmodified Claude Code binary with their own Claude subscription". Aurelius blijft een self-hosted tool van de eigen company, geen product dat Claude-gebruik doorverkoopt.
2. **`setup-token` niet als standaard gebruiken.** Technisch werkt het, maar Aurelius moet het token dan opslaan (vault of database). Dat komt dicht bij "collect, store … session tokens". Hooguit als expliciete uitzondering, waarbij de user het token zelf als env-bestand op de server zet, en nooit via een Aurelius-scherm.
3. **Risico's:**
   - Een board member die een ticket aanmaakt dat een ander laat werken, verbruikt nog steeds het eigen abonnement. Dat is geen account-sharing, zolang niemand een andere login kan kiezen. Leg dit vast in de UI, en geef geen keuze "gebruik login van X".
   - Zware parallelle agent-runs gaan verder dan "ordinary, individual usage". Anthropic kan limieten afdwingen "without prior notice". Het bestaande quota-wachten vangt limieten op, maar een account kan worden geblokkeerd.
   - De voorwaarden veranderen geregeld. Controleer ze opnieuw vóór Fase 3, en bij twijfel via https://www.anthropic.com/contact-sales.
   - De server bewaart `.credentials.json` van meerdere mensen in de home van de daemon-user. Gebruik mappen met 0700, geen back-up naar gedeelde plekken, en `claude auth logout` bij het verwijderen van een board member.
4. **Alternatief als (a) niet mag of niet werkt:** een **API-key** via de Claude Console onder de Commercial Terms. De provider `AnthropicApi` bestaat al in Aurelius, en `ANTHROPIC_API_KEY` werkt ook met `claude -p` (meting E).
   - **Per company** (één Console-organisatie, betaald door de company): dit is juridisch het eenvoudigst. Per member kan je een eigen key maken met de rol "Claude Code" voor kostenverdeling en limieten. De key gaat dan per run als `ANTHROPIC_API_KEY` in de child, via dezelfde `ClaudeLogin`-uitbreiding (een variant `ApiKey`).
   - **Per user** (ieder een eigen Console-account) als ieder zelf wil betalen.
   - Of **Claude for Teams/Enterprise**: aparte seats met eigen logins en een centrale facturatie, onder de Commercial Terms. Dat past het best bij "een company met board members" en werkt met precies hetzelfde `CLAUDE_CONFIG_DIR`-mechanisme.
