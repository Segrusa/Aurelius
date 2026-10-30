# AUR-MU-03: Fase 3, echt multi-user (ter goedkeuring)

*Orion · 2026-10-31 · branch `docs/aur-mu-def` (lokaal) · basis: `docs/product/multi-user.md`, spike `../fase-1/spike-claude-logins.md`*

| Document | Auteur | Inhoud |
|---|---|---|
| [user-stories.md](user-stories.md) | Nova | MU3-01 t/m MU3-12, Given/When/Then, OV-01..08 |
| [ui-ontwerp.md](ui-ontwerp.md) | Pixel | Login, sessies, auteurschap, Claude Connect, ticket wacht op login of quotum, P-31..37 |
| [impactanalyse.md](impactanalyse.md) | Titan | Migratie 0020, **protocol 5**, daemon, clients, teststrategie §7a, D3-1..8 |

**Scope:** login met gebruikersnaam en argon2-wachtwoord (vervangt `user.use`), een sessie per apparaat (lijst, intrekken, uitloggen), de volledige gate (niets zonder sessie, behalve hello/login/bootstrap), alleen bereikbaar via localhost en tailnet (Orbit), auteur op elke actie, wachtwoord wijzigen en board member uitschakelen. Elke board member koppelt een eigen Claude-login (`<data>/logins/<user_id>/`, de board member logt zelf in, Aurelius bewaart nooit een token). Ticketruns en de hele delegatieketen gebruiken de login van de aanmaker, chatruns die van de afzender. Quota-parkering en `/usage` per login. Geen fallback: zonder login wacht een run met de nieuwe reden `login`.
**Buiten scope:** meerdere rollen, `setup-token` als standaard, toegang buiten de tailnet, meerdere companies.
**Protocol:** **protocol 5 is nodig** (`PROTOCOL_MAJOR` in `protocol/src/rpc.rs:21`). `hello` krijgt `session_token` in plaats van `user_id`, `user.use` verdwijnt, `user.create` vraagt na de bootstrap een sessie, en er komt een nieuwe ErrorCode `unauthenticated`. Oude clients krijgen de bestaande melding "Update needed". Alle andere methodes zijn nieuw binnen 5 (impactanalyse §2).
**Schatting:** ≈ **29 dagen** werk, doorlooptijd 3 à 3,5 weken (Forge 12,5, Flux 4, Bridge 4, Schema 1, Orbit 1, Sentinel 1,5, Pulse 2,5, Trace 1,5).
**Afhankelijkheden:** Fase 1 (users, `created_by_user`) en Fase 2 (auteur op berichten). De spike-code `7e1388d` dient als basis voor de runtime.
**Bouwvolgorde:** Schema 0020 ‖ Forge login, sessies en gate (Sentinel reviewt meteen) → protocol 5 bevriezen + gen-dart → Flux en Bridge, en Orbit parallel → Forge Claude-login, dan quota en `/usage` per login, dan sessies per (conversatie, login) → Pulse volgens §7a, Trace. Uitrol: eerst server en desktop, dan mobiel.
**Tweede account (spike §2.6) blokkeert niet meer:** Sébastien heeft voorlopig geen tweede account. Voor dev en test koppelen we twee board members aan hetzelfde account, elk met een eigen map en een eigen login. Zo bewijzen we de juiste map per ticketrun, de erfenis via delegaties en een gescheiden token en sessie (MU3-01, impact §7a). De quotascheiding testen we met een gesimuleerde quota-fout per login. **De echte quotascheiding blijft een open acceptatiepunt** tot er een tweede account is.
**Risico's:** quota tussen echte accounts niet bewezen (R1); voorwaarden van Anthropic (B1); de Connect-flow hangt af van de uitvoer van `claude auth login` (fallback naar "Show command"); credentials van meerdere mensen op de server (0700, nooit in backups); de gate raakt ~40 testbestanden (één helper, één commit).

## Beslissingen (teamvoorstel geldt zonder tegenbericht)
| # | Vraag | Voorstel | Bron |
|---|---|---|---|
| **B1** | **Voorwaarden van Anthropic: mag één abonnement door meerdere personen gebruikt worden?** **Legt Atlas voor aan Sébastien** (aankoop) | Nee, delen is niet de bedoeling. Elke echte board member heeft een **eigen abonnement** (Pro/Max) of een **seat in een Team-plan**. Twee logins op Sébastiens eigen account zijn alleen voor dev en test (één persoon). Sentinel controleert de voorwaarden opnieuw vóór de bouw van het Claude-deel | Atlas, spike §4, R2 |
| B2 | Protocol 5 of 4 met beperkte oude clients? | Protocol 5 | D3-2 |
| B3 | Claude-login koppelen: terminal of begeleide flow in de app? | De Connect-flow in de app als standaard (link + code via een PTY, Aurelius leest `.credentials.json` nooit en de code komt in geen enkel event of log), met de terminal als alternatief. Sentinel bevestigt dit vóór de bouw | D3-8, P-32 (boven Nova OV-01) |
| B4 | De aanmaker heeft geen login of is afwezig | Het ticket wacht met `needs: login`. Een andere board member kan expliciet "Run on my login" kiezen en neemt het zo over op de eigen login. Nooit stil, en nooit de login van een ander kiezen | D3-3, OV-06 |
| B5 | Login voor schedules, app keys en heartbeats | De board member die ze aanmaakte of (her)activeerde. Zonder die persoon wacht het met `login` | D3-4, OV-07 |
| B6 | Achtergrondjobs (memory-consolidatie) | Een board member kiest de eigen login als "company-login" voor die jobs. Zonder keuze draaien ze niet. De daemon-login wordt niet meer gebruikt | D3-5, OV-08 |
| B7 | Sessieduur | 30 dagen zonder gebruik, 90 dagen absoluut. Intrekken werkt meteen. Een wachtwoordwijziging of uitschakeling trekt alle sessies in | D3-7, OV-02, P-35 |
| B8 | De desktop op de server zelf ook laten inloggen? | Ja, één gate voor alles, één keer per apparaat | D3-1, OV-04 |
| B9 | Mislukte logins | Na 5 pogingen per gebruikersnaam een oplopende wachttijd (tot 15 min), geen definitieve blokkering. Ook een limiet op `user.create` (R3 uit Fase 1) | OV-03 |
| B10 | Wachtwoord vergeten | Een andere board member zet een tijdelijk wachtwoord, dat bij de volgende login gewijzigd moet worden. Alle sessies worden ingetrokken | P-34 |
| B11 | Hervatten na het koppelen van een login | Automatisch, net als na een quotumreset | OV-05 |
| B12 | Ziet een board member het gebruik van een ander? | Nee, alleen "Waiting for X's limit until …" | P-37 |
