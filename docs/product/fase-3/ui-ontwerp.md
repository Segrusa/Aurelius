# UI-ontwerp Fase 3: login, sessies, auteurschap en de eigen Claude-login

- **Task:** AUR-MU-DEF-P (Pixel, voor Orion) · **Status:** beknopt concept, ter goedkeuring
- **Basis:** `docs/product/multi-user.md` §2 (Toegang, Claude-abonnementen), `fase-1/spike-claude-logins.md` §3.2–3.3 (`CLAUDE_CONFIG_DIR` per user, de ongewijzigde `claude auth login`, geen token bewaren, quotum per login), `fase-1/ui-ontwerp.md` §11 (onboarding, "Who's working here?"), README B8.
- **Wat verandert ten opzichte van Fase 1:** "Who's working here?" en de eenmalige wachtwoordcontrole per apparaat (B8) worden een echte login met een sessie per apparaat.

## 1. Loginscherm
### Desktop
```
┌──────────────────────────────────────────────────────────────┐
│                         ◆ Aurelius                           │
│             ┌───────────────────────────────────┐            │
│             │ Sign in                           │            │
│             │ to aurelius-server (tailnet)      │            │
│             │ Username                          │            │
│             │ ┌───────────────────────────────┐ │            │
│             │ │ sebastien                     │ │            │
│             │ └───────────────────────────────┘ │            │
│             │ Password                          │            │
│             │ ┌───────────────────────────┐ 👁  │            │
│             │ │ ••••••••••••              │     │            │
│             │ └───────────────────────────┘     │            │
│             │ ✕ Username or password is wrong.  │            │  ← één generieke fout
│             │             [ Sign in  ⌘↵ ]       │            │
│             │ Forgot it? Ask another board      │            │
│             │ member to reset your password.    │            │
│             └───────────────────────────────────┘            │
│      ● Connected to This computer · Choose another server    │
└──────────────────────────────────────────────────────────────┘
```
- Dezelfde kaart van 420 px als de onboarding (Fase 1 §11.1). Zonder geldige sessie toont de client alleen dit scherm.
- **Fouten:** altijd *"Username or password is wrong."*, zonder te verraden welke van de twee. Na te veel pogingen: *"Too many attempts. Try again in 0:30."* met een aftellende knop.
- **Sessie verlopen of ingetrokken** tijdens het werk: een modale melding *"You were signed out on this device."* en daarna het loginscherm. Een concept in een composer blijft lokaal bewaard.
- **Mobiel:** een volledige pagina zonder tabbalk, large title **Sign in**, één gegroepeerde sectie, autofill-hints `username` en `password`, en een `FilledButton` boven het toetsenbord.
- **Uitloggen:** desktop via de server picker (*"as sebastien"* → **Sign out**), mobiel via More → (S) Sébastien → **Sign out**.

## 2. Eigen sessies en apparaten
**Settings → Account → Sessions** (mobiel: More → Account → Sessions)
```
┌ Settings › Account › Sessions ─────────────────────────────────┐
│ Devices signed in as sebastien                                 │
│ ▣ Desktop · sdc-laptop (Linux)        This device              │
│   Signed in Oct 31 · active now                                │
│ ▯ iPhone · Sébastien's iPhone         Active 2 h ago  [Revoke] │
│ ▯ Android · Pixel 9                   Active Oct 12   [Revoke] │
│                                                                │
│ [Sign out all other devices]                                   │
└────────────────────────────────────────────────────────────────┘
```
- Per rij: het soort client, de apparaatnaam (die het apparaat zelf doorgeeft, of de tailnet-hostnaam), de eerste login en de laatste activiteit. Het eigen apparaat is gemarkeerd en kan hier niet ingetrokken worden (gebruik **Sign out**).
- **Revoke** vraagt een bevestiging (*"Sign out iPhone? It needs your password to come back."*). Dat apparaat valt meteen terug op het loginscherm.
- De gekoppelde apparaten (`remote_clients`) gaan op in deze lijst. Er komt geen tweede lijst met *Paired devices* (P-33).
- **Mobiel:** een gegroepeerde lijst. Swipen naar links = **Revoke**, met dezelfde bevestiging.

## 3. Auteurschap in de UI
| Waar | Weergave |
|---|---|
| Berichten (discussie, kanalen, chats) | monogram + naam + *board member* of de rol |
| Ticketkaart en ticketweergave | *"by Sébastien"* (aanmaker); de Activity-regels krijgen een monogram en naam per actor |
| Tasks, approvals, Inbox | *"Approved by Marie"*, *"Started by Sébastien"* |
| Board-filter | nieuw filter **Created by: Anyone / Me / <board member>** |
- **Avatar:** het bestaande monogram. **Board members** zijn een cirkel, **employees** een afgeronde rechthoek, zodat je mens en agent ook zonder kleur uit elkaar houdt. Een board member krijgt een vaste kleur uit een palet van 8 kleuren, met een contrast van minstens 4,5:1 (P-31).
- **"You":** je eigen berichten en acties tonen *"You"* in de activiteit, en je naam in berichten.
- **Needs you is per board member:** een review die iedereen mag doen telt voor iedereen. Een wachtende Claude-login of een quotum telt alleen voor de aanmaker (§5).

## 4. De eigen Claude-login koppelen
**Settings → Account → Claude login** (mobiel: More → Account → Claude login)
```
┌ Claude login ──────────────────────────────────────────────────┐
│ Tickets you create run on your own Claude subscription.        │
│                                                                │
│ ● Connected  sebastien@example.com · Max                       │  ← status
│   Session limit   ███████░░░  68 % · resets 16:00              │  ← /usage per user
│   Weekly limit    ██░░░░░░░░  21 % · resets Mon                │
│   [Check now]  [Reconnect]  [Disconnect]                       │
└────────────────────────────────────────────────────────────────┘
```
**Statussen** (altijd met tekst en vorm, nooit alleen met kleur):
| Status | Weergave | Actie |
|---|---|---|
| Not connected | ○ *"Not connected. Your tickets wait until you connect."* | **Connect** (primair) |
| Connected | ● groen + e-mail + plan + gebruik | Check now · Reconnect · Disconnect |
| Expired | ✕ *"Expired. Claude asked to sign in again."* (een 401 in een run) | **Reconnect** (primair) |
| Limit reached | ⏸ *"Limit reached until 16:00."* + de balk op 100 % | geen; je tickets hervatten vanzelf |
| No subscription | ⚠ *"This login has no Claude subscription (API usage only)."* | Reconnect |

**Connect: de user logt zelf in via de ongewijzigde claude-flow.** De server start `claude auth login` in de map van deze board member. De client toont alleen wat Claude zelf vraagt:
```
┌ Connect your Claude login ──────────────────────────────┐
│ 1  Open the Claude sign-in page and sign in yourself.   │
│    [ Open sign-in page ↗ ]  [Copy link]                 │
│ 2  Paste the code Claude shows you.                     │
│    ┌──────────────────────────────────────────────┐     │
│    │                                              │     │
│    └──────────────────────────────────────────────┘     │
│ Aurelius never sees your Claude password.               │
│ Prefer a terminal? [Show command]                       │
│                                     [Cancel] [Connect]  │
└─────────────────────────────────────────────────────────┘
```
- De link en de vraag om de code komen letterlijk uit het `claude`-proces. Aurelius vult niets in en bewaart geen token (spike §3.2). **Show command** toont de terminalvariant voor wie op de server werkt: `CLAUDE_CONFIG_DIR=<map> claude auth login` (P-32).
- Gaat het mis, dan toont het dialoogvenster de tekst van Claude zelf, met **Try again**. Na succes toont de status meteen het e-mailadres en het plan (uit `auth status --json`).
- **Herinnering zonder login:** de kop van het Board toont een banner (ATTENTION) *"Connect your Claude login to run your tickets. [Connect]"*. Op mobiel is dat een kaart bovenaan Home. **Start** blijft mogelijk, met de toast *"#51 started. It waits until you connect your Claude login."*
- **Mobiel:** **Open sign-in page** opent de systeembrowser. De gebruiker komt terug naar de app en plakt de code (de knop *Paste* staat naast het veld).

## 5. Een ticket wacht op het quotum of de login van de aanmaker
```
Kaart                                   Actiebalk in de ticketweergave
┌──────────────────────────────────┐   │ ⏸ Waiting for Marie's Claude limit · until 16:00     │
│ #42 Login per board member       │   │   Runs only on the creator's subscription.           │
│ ⏸ Waiting for Marie's limit      │   │                                     [Back to Backlog]│
│   until 16:00                    │
│ (F) Forge          by Marie      │   │ ◆ Waiting for your Claude login (expired)            │
└──────────────────────────────────┘   │   [Reconnect Claude login]          [Back to Backlog]│
```
- De bestaande tekst *"Waiting for limit reset until …"* krijgt de naam van de aanmaker erbij. Voor de aanmaker zelf staat er *"your"* en een **◆ Needs you** met de directe actie (**Connect** of **Reconnect**). Bij een quotum is er geen actie, alleen het tijdstip.
- Andere board members zien het wachten, zonder Needs you. Er is geen knop om over te nemen of via iemand anders te laten lopen, want er is geen stille fallback (multi-user.md §2).
- In de tab Discussion verschijnt één systeemregel per wachtperiode: *"Paused: Marie's Claude limit is reached until 16:00."* en daarna *"Resumed."*
- **Runs zonder ticket** (Team chat, project-assistent) gebruiken de login van wie het bericht stuurde. Zonder login verschijnt in de chat de systeemregel *"Connect your Claude login to get an answer. [Connect]"*.

## 6. Ontwerpvragen
| ID | Vraag | Voorstel |
|---|---|---|
| P-31 | Avatars: foto's uploaden of alleen monogrammen? | Alleen monogrammen met een vaste kleur. Het verschil in vorm (cirkel voor een mens, rechthoek voor een employee) is genoeg. Foto's later |
| P-32 | De begeleide Connect-flow in de client, of alleen een terminalcommando? | De begeleide flow als standaard (die werkt ook op mobiel en voor wie niet op de server zit), met de terminal als alternatief. Titan bevestigt dat de flow met link en code van `claude auth login` zonder TTY door te geven is. Lukt dat niet, dan valt de flow terug op alleen het commando |
| P-33 | De bestaande koppeling van apparaten (`remote_clients`) en de sessies samenvoegen? | Ja, één lijst Sessions per user. Koppelen blijft de eerste stap, en daarna volgt de login |
| P-34 | Wachtwoord vergeten: wat kan een board member doen? | In Settings → Board members kan elke board member (één rol) een tijdelijk wachtwoord zetten voor een ander. Bij de volgende login moet dat gewijzigd worden en worden alle sessies van die user ingetrokken. Het eigen wachtwoord wijzigen gaat via Settings → Account |
| P-35 | Hoe lang blijft een sessie geldig? | 30 dagen inactiviteit op desktop en mobiel, en verlengd bij gebruik. Er komt geen "remember me"-vinkje |
| P-36 | Mag Start zonder Claude-login? | Ja, met een duidelijke toast en Needs you voor de aanmaker. Zo blijft het Board bruikbaar en is de oorzaak zichtbaar |
| P-37 | Ziet een board member het gebruik (`/usage`) van een ander? | Nee, alleen *"Waiting for Marie's limit until 16:00"*. Het eigen gebruik staat in Account |
