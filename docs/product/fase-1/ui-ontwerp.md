# UI-ontwerp Fase 1: Board, Ticket, Onboarding en Instellingen

- **Task:** AUR-MU-00-P (Pixel, voor Orion)
- **Bronnen:** `docs/product/multi-user.md` (goedgekeurd 2026-10-30), `fase-1/user-stories.md` (Nova, MU1-01 … MU1-22), `fase-1/impactanalyse.md` (Titan, §3.4 kolom en status, §3.6 Plan review, §3.7 dispatcher, §4 clients).
- **Basis:** de huidige clients op `main` 4694dd7: desktop `clients/desktop` (gpui, tokens in `src/theme.rs`, helpers in `src/ui.rs`) en mobiel `clients/mobile` (Flutter, tokens in `lib/core/theme/tokens.dart`, thema in `theme.dart`).
- **Status:** concept, wacht op review door Orion en akkoord van Sébastien.
- **Regel:** waar dit ontwerp iets kiest dat de bronnen niet vastleggen, staat het als **ontwerpbeslissing** (D-xx) met de reden, of als **open punt** (P-xx) in §14.

---

## Inhoud
0. Termen en UI-taal
1. Aansluiting op de bestaande stijl
2. Navigatie
3. Board, desktop
4. Board, mobiel
5. De ticketkaart
6. Verplaatsen: regels per kolom (desktop en mobiel)
7. Ticketweergave, desktop
8. Ticketweergave, mobiel
9. Reviewpoort en Plan review (dialogen)
10. Ticket aanmaken (ook vanuit een chat)
11. Onboarding en actieve board member
12. Instellingen: kolommen, dispatcher, board members
13. Wat verdwijnt: afdelingen
14. Ontwerpbeslissingen en open punten
15. Toegankelijkheid
16. Verschillen desktop en mobiel
17. Traceerbaarheid (story → scherm)

---

## 0. Termen en UI-taal

De termen volgen Nova's tabel (user-stories §0): **Company, Board member, Employee, Board, Ticket, Task (hoofdtask, delegatie), Aanmaker, Eigenaar, Dispatcher, Start, Modus**. De kolomnamen staan altijd exact zo: **Backlog · Todo · Doing · Plan review · Review · Done**.

De clients zijn nu grotendeels Engelstalig ("Team chat", "Needs you", "New task"). Nieuwe labels zijn daarom ook Engels (D-01). De bestaande plan-review-component gebruikt al Nederlandse knoppen ("Goedkeuren + Auto", "Goedkeuren", "Pas aan", "Afwijzen"). Die hergebruik ik ongewijzigd, zodat er één plan-review-ervaring blijft (zie P-01).

| Term in de documenten | UI-label |
|---|---|
| Start | **Start** |
| Eigenaar | **Owner** |
| Aanmaker | **Created by** |
| Dispatcher | **Dispatcher** |
| Goedkeuren (plan) | **Goedkeuren + Auto** / **Goedkeuren** (bestaande component) |
| Plan terugsturen met feedback | **Pas aan** (bestaande component) |
| Op Done zetten | **Done** (knop met vinkje), dialoogtitel "Mark as Done?" |
| Terug naar Doing met feedback | **Send back to Doing** |
| Ticket annuleren (→ Backlog) | **Back to Backlog** |
| Verplaatsen zonder slepen | **Move to…** |

---

## 1. Aansluiting op de bestaande stijl

**Desktop (gpui, "Aurelius Core Screens"):** warme, bijna monochrome vlakken; groen alleen voor actieve staat en de primaire actie.

| Element | Token / helper |
|---|---|
| Achtergrond Board | `BG`; kolommen `PANEL` met `BORDER`, radius 8 |
| Kaart | `SURFACE`, rand `BORDER`, hover `SURFACE_2`, focus `FOCUS_RING` (2 px) |
| Status van de run op een kaart | bestaande `ui::task_mark` (Working, Queued, Waiting, Needs you, Failed, "Waiting for limit reset until …") |
| "Needs you"-markering | `ATTENTION_BG` + `ATTENTION_BORDER` + `diamond` (zoals in de Inbox) |
| Eigenaar | `ui::monogram(name, 18)` |
| Primaire knop | `ui::button(…, ButtonKind::Primary)` (ACCENT), secundair `ButtonKind::Outline` |
| Fout / Urgent / Overdue | `NEGATIVE` |
| High / waarschuwing | `0xD99A3A` (al gebruikt voor "Changes requested") |
| Typografie | UI 13 px, kaarttitel 13 px medium, meta 11–11,5 px `TEXT_2`; paneeltitels in de serif (zoals "Aurelius" in de sidebar). `MUTED` alleen voor uitgeschakelde elementen en plaatshouders (zie §15) |
| Dialogen | het bestaande overlay-patroon (`DialogKind`, `ConfirmKind` in `screens/overlays.rs`) |
| Rechterpaneel | het bestaande `Panel` (nu `Chat`/`Task`), uitgebreid met `Panel::Ticket` |

**Mobiel (Flutter, iOS 26-stijl):** gegroepeerde lijsten, cellen `Tokens.cell` met radius 26 (kaarten) of 12 (velden), pilknoppen, één groen accent, Liquid Glass alleen voor zwevende chrome (tabbalk, onderste actiebalk).

| Element | Token / widget |
|---|---|
| Kaart | `Card` (`Tokens.cell`, radius `cardRadius`) |
| Kolomkeuze | horizontale pill-chips (`chipTheme`, geselecteerd `accentTint`) met teller |
| Primaire actie | `FilledButton` (pill, 44 pt) in een glazen onderbalk |
| Keuzes (eigenaar, Move to…) | bottom sheet (`bottomSheetTheme`, radius 34, drag handle) |
| Feedback | `TextField` met `inputDecorationTheme`, focusrand `accent` 1,5 |
| Meldingen | `toast` (floating snackbar) |
| Fout / Urgent | `Tokens.red`; High `Tokens.orange` |

---

## 2. Navigatie

### Desktop: een nieuw sidebar-item "Board"
```
┌──────────────────────┐
│ ◆ Aurelius           │
│  ▦ Dashboard   Ctrl1 │
│  ▥ Board       Ctrl5 │  ← nieuw, direct onder Dashboard
│  ▢ Team chat   Ctrl8 │
│  ▭ Inbox  (3)  Ctrl2 │
│  ☐ Tasks       Ctrl7 │
│  ⚇ Employees   Ctrl3 │
│  ▸ Projects    Ctrl6 │
│                      │
│ ┌──────────────────┐ │
│ │● This computer   │ │
│ │  as Sébastien  ⌄ │ │  ← actieve board member (MU1-21 AC4)
│ └──────────────────┘ │
└──────────────────────┘
```
- **D-02:** Board staat direct onder Dashboard, omdat werk in Fase 1 via tickets start. Sneltoets **Ctrl/⌘+5**: die komt vrij doordat "Go to Departments" verdwijnt (§13). Het glyph: drie verticale balkjes (kolommen) in het 16×14-vak van `nav_icon`.
- **Tasks blijft bestaan:** tasks zonder ticket (Team chat, project-assistent) bestaan nog (Nova OV-16, Titan §7.4). Een rij in Tasks die bij een ticket hoort, toont `#42` en opent het ticket.
- **Ctrl/⌘+N** op het Board = **New ticket**. Het commandopalet (Ctrl+K) krijgt "New ticket", "Go to Board" en "Find ticket #…".

### Mobiel: Board vervangt de tab Tasks
```
┌──────────────────────────────────────────┐
│ ⌂ Home  ▥ Board  💬 Team  ▭ Inbox  ⋯ More │
└──────────────────────────────────────────┘
```
- **D-03:** de glazen tabbalk heeft maximaal 5 tabs. Board neemt de plaats van Tasks in; **Tasks** verhuist naar **More → Tasks** en is ook bereikbaar vanuit een ticket (gekoppelde tasks). Reden: het Board is het nieuwe startpunt van werk; Tasks wordt een detailweergave. Sneltoets ⌘/Ctrl+2 = Board (de volgorde van `shellTabs`).
- De "give work"-actie (⌘N, de knop op Home) maakt vanaf Fase 1 een **ticket** in plaats van een task (P-02).
- Nieuwe routes: `/board`, `/ticket/:tid`, `/onboarding`, `/settings/board`, `/settings/board-members`.

---

## 3. Board, desktop (MU1-05, MU1-09, MU1-13, MU1-16)

### 3.1 Wireframe (standaard, venster ±1440 px breed)
```
┌Sidebar┬──────────────────────────────────────────────────────────────────────────────────────────────┐
│       │ Board              [Project: All projects ⌄] [Owner: Anyone ⌄] [🔍 Find… ] [+ New ticket]    │
│       │ 14 tickets · 2 need you                                                                      │
│       ├──────────────────────────────────────────────────────────────────────────────────────────────┤
│       │┌Backlog 5───────┐┌Todo 2──────────┐┌Doing 3─────────┐┌P┐┌Review 2────────┐┌Done 2──────────┐ │
│       ││                ││                ││                ││l││ ◆ Needs you    ││                │ │
│       ││┌──────────────┐││┌──────────────┐││┌──────────────┐││a││┌──────────────┐││┌──────────────┐│ │
│       │││#51 Vault UI  ││││#48 Users tbl ││││#42 Login per ││││n│││#40 Argon2    ││││#35 Fix inbox ││ │
│       │││▲ High        ││││◌ Orion is    ││││  board member││││ ││││  hashing     ││││✓ Done Oct 29 ││ │
│       │││aurelius      ││││  choosing…   ││││▲▲ Urgent     ││││r││││◆ Needs you   ││││(F) Forge     ││ │
│       │││ui · vault    ││││aurelius      ││││● Working     ││││e││││  review      ││└──────────────┘│ │
│       │││Due Nov 7     ││││by Sébastien  ││││(F) Forge     ││││v││││(S) Sentinel  ││ Show older (12)│ │
│       │││by Sébastien  │││└──────────────┘│││Due Nov 3     ││││i││││  [Review ›]  │└────────────────┘ │
│       │││   [▸ Start ⌄]│││┌──────────────┐│││by Sébastien  ││││e│││└──────────────┘│                   │
│       ││└──────────────┘││││#47 Columns   ││││└──────────────┘│││w││                │                   │
│       ││┌──────────────┐││││◆ Choose an   ││││┌──────────────┐│││ ││                │                   │
│       │││#50 …         ││││  employee    ││││#44 … ✕Failed │││0││                │                   │
│       ││└──────────────┘│││└──────────────┘││││◆ Needs you   ││└─┘└────────────────┘                   │
│       ││ [+ Add ticket] │└────────────────┘│└──────────────┘│                                         │
│       │└────────────────┘                  └────────────────┘                                         │
└───────┴──────────────────────────────────────────────────────────────────────────────────────────────┘
```
De strook `P…0` is de ingeklapte kolom **Plan review** (leeg). `(F)` is het monogram van de eigenaar.

### 3.2 Opbouw
- **Kop:** titel "Board" (serif, zoals paneeltitels), eronder de teller "N tickets · M need you" (M = tickets met de markering Needs you).
- **Filterbalk (rechts in de kop):**
  - **Project** (Must, MU1-05 AC2): dropdown "All projects" + actieve projecten, één keuze, per client onthouden (`ui.json`). Wie het Board vanuit een project opent, krijgt dat project als filter (Could: een tab "Board" in het projectvenster `project_window.rs`).
  - **Owner** (Should): "Anyone", "No owner yet", of een employee.
  - **Find** (Should): zoekt in titel, `#nummer` en labels (`ticket.list { query }`).
  - Een actief filter krijgt de stijl "set" van de bestaande dropdowns en een × om het te wissen.
- **Kolommen:** naast elkaar, breedte flexibel 232–300 px, 12 px tussenruimte; de rij scrollt horizontaal als ze niet past, elke kolom scrollt verticaal op zich. De kolomkop is sticky: naam, teller en een ⋯-menu (Collapse / Expand, "Board settings…").
- **Plan review (MU1-13):** de kolom staat altijd op haar plaats, zodat het pad leesbaar blijft, maar is **ingeklapt tot een strook van 44 px** (verticale naam + teller) zolang ze leeg is. Tooltip en lege tekst: *"Only tickets in Plan mode stop here, with their plan."* Komt er een ticket in, dan klapt ze open; de gebruiker kan ze ook zelf openklappen.
- **Review (MU1-16):** de kop krijgt ◆ "Needs you" (ATTENTION-stijl) zolang er tickets in staan. Een kaart in Review toont **Review ›**, die de ticketweergave op de reviewpoort opent (§7.3).
- **Done:** de 20 recentste, nieuwste eerst, gedimd (opacity 0,75); onderaan "Show older (N)". Done kan ingeklapt worden (per client onthouden).
- **Backlog:** onderaan "+ Add ticket" (opent New ticket met het projectfilter ingevuld).
- **Extra kolommen** (alleen als P-05 die toelaat) staan op hun ingestelde plaats en gedragen zich als "Doing-varianten".

### 3.3 Staten
| Staat | Weergave |
|---|---|
| **Laden** (eerste `board.get`) | de kolomkoppen met de echte namen (uit de cache als die er is) en per kolom 2–3 skeletkaarten (`SURFACE_2`, zonder animatie bij "reduce motion"). Na 10 s zonder antwoord: de foutstaat. |
| **Fout bij laden** | midden op het Board, zoals de Team chat na AUR-BUG-03: *"Could not load the Board."* + de fouttekst (TEXT_2, selecteerbaar) + **Try again**. Bij `unknown_method` (oude server): *"This server does not have the Board yet. Update the server."* + Updates. |
| **Offline** | de laatst bekende stand blijft staan, gedimd tot 60 %; slepen, Start en de reviewknoppen zijn uitgeschakeld met de tooltip *"Offline · reconnecting"* (de bestaande tekst van de server picker). |
| **Leeg Board** (na de schone start, MU1-19 AC4) | alle kolommen leeg; in Backlog: titel *"No tickets yet"*, tekst *"New tickets land in Backlog. Nothing starts until you press Start."*, knop **New ticket** (primair). |
| **Geen projecten** | in plaats van de kolommen: *"Create a project first; work is done in a project."* (de bestaande zin van mobiel) + **New project**. |
| **Filter zonder resultaat** | boven de lege kolommen één regel *"No tickets in aurelius."* + **Clear filter**. |
| **Lege kolom** | één TEXT_2-regel: Backlog *"New tickets land here."* · Todo *"Started tickets wait here for an owner."* · Doing *"Nothing in progress."* · Plan review *"Only tickets in Plan mode stop here, with their plan."* · Review *"Nothing to review."* · Done *"Closed tickets appear here."* |
| **Actie mislukt** | de kaart springt terug (150 ms) en een toast geeft de reden van de server, bijv. *"Only a board member can mark a ticket as Done."* |
| **Live wijziging** (MU1-05 AC3) | een kaart die door een ander (board member, employee, engine) verplaatst wordt, schuift naar haar nieuwe plaats en licht 1 s op met `ACCENT_HALO`. Komt er tijdens het slepen een wijziging van die kaart binnen, dan wordt de drop geweigerd met *"#42 moved to Review on another device."* |

### 3.4 Interactie
- **Klik op een kaart:** opent de ticketweergave in het rechterpaneel (§7). Het Board blijft links staan en scrollt zo dat de kaart zichtbaar blijft; de geopende kaart krijgt de actieve stijl (`SURFACE_2` + groene balk links, zoals in de sidebar).
- **Start op een Backlog-kaart:** **▸ Start** staat altijd zichtbaar op de kaart (niet alleen bij hover), zodat duidelijk is dat Backlog niets vanzelf doet. Klik = Start met de ingestelde eigenaar of de dispatcher (§6). Het pijltje ⌄ (split button) opent **Start with…** met de actieve employees (MU1-12).
- **Slepen:** zie §6. De gesleepte kaart krijgt een schaduw (`shadow_lg`); toegestane kolommen krijgen een `ATTENTION_BORDER`-rand en `ACCENT_HALO`-achtergrond, verboden kolommen worden gedimd (50 %) met de reden in de kop (bijv. *"The engine moves tickets here"*). Een invoeglijn toont de plaats binnen de kolom.
- **Rechtsklik / ⋯ op een kaart:** **Open**, **Start**, **Start with…**, **Move to…** (submenu met alleen de toegestane kolommen), **Copy link**, **Back to Backlog** (danger), **Delete** (alleen in Backlog, danger). "Move to…" is ook de terugval als slepen tussen lijsten in gpui tegenvalt (Titan §4.1).
- **Toetsenbord:** zie §15.

---

## 4. Board, mobiel (MU1-05, MU1-09, MU1-13, MU1-16)

### 4.1 Telefoon: één kolom tegelijk
```
┌─────────────────────────────────┐
│ Board                     [+]   │  ← large title, + = New ticket
│ [aurelius ⌄] [Anyone ⌄]    🔍   │  ← filter-pills
│ (Backlog 5)(Todo 2)(Doing 3)(R… │  ← kolomchips, horizontaal scrollbaar
│ ┌─────────────────────────────┐ │
│ │ #51 Vault UI         ▲ High │ │
│ │ aurelius · ui · vault       │ │
│ │ Due Nov 7 · by Sébastien    │ │
│ │                 [ ▸ Start ] │ │  ← pill, 44 pt hoog
│ └─────────────────────────────┘ │
│ ┌─────────────────────────────┐ │
│ │ #50 Board settings          │ │
│ │ aurelius · by Sébastien     │ │
│ │                 [ ▸ Start ] │ │
│ └─────────────────────────────┘ │
│        ‹ swipe = kolom ›        │
│ ╭─────────────────────────────╮ │
│ │  ⌂    ▥    💬    ▭    ⋯     │ │  ← glazen tabbalk
│ ╰─────────────────────────────╯ │
└─────────────────────────────────┘
```
- **Kolomchips:** één chip per kolom met naam en teller; eronder een `PageView` waarin je tussen kolommen swipet (de chips volgen). De geselecteerde chip krijgt `accentTint`. Review krijgt een groene stip zolang er iets te reviewen valt. **Plan review** staat op haar plaats tussen Doing en Review; zolang ze leeg is, is de chip smal en gedimd ("Plan review 0") (D-04).
- **Bij openen** toont het Board de eerste kolom met iets dat jou nodig heeft (Review, Plan review, of een Needs you in Todo/Doing), anders Doing; daarna de laatst bekeken kolom.
- **Pull to refresh** herlaadt het Board (naast de live updates).

### 4.2 Verplaatsen zonder slepen (telefoon)
Slepen tussen kolommen werkt slecht als je maar één kolom ziet. De telefoon heeft daarom **geen sleepbewegingen tussen kolommen** (D-05), maar drie gelijkwaardige wegen naar dezelfde acties:

1. **De primaire knop op de kaart:** **Start** (Backlog), **Choose owner** (Todo met Needs you), **Review** (Review, Plan review).
2. **Swipe op een kaart** (zoals Mail op iOS):
   - **naar rechts** = de primaire actie van de kolom (groene achtergrond, icoon + label). Start voert meteen uit, met een toast *"Started. Orion chooses an employee."* en **Undo** (5 s, = Back to Backlog zolang er nog geen eigenaar gekozen is). In Review/Plan review opent de swipe de reviewpoort; daar wordt nooit zonder bevestiging iets beslist.
   - **naar links** = **Move to…** (grijze achtergrond), opent de sheet hieronder.
3. **Lang indrukken** opent het contextmenu (Cupertino context menu met preview): Open · Start · Start with… · Move to… · Copy link · Back to Backlog · Delete.

```
Sheet "Move #42 to…"  (bottom sheet, radius 34)
┌─────────────────────────────────┐
│              ───                │
│ Move #42 Login per board member │
│                                 │
│  Backlog      Stops the work  › │  ← toegestaan, met bevestiging
│  Todo         Not from Doing    │  ← gedimd, met reden
│  Doing        ✓ Current         │
│  Plan review  The engine moves… │  ← gedimd
│  Review       The owner hands…  │  ← gedimd
│  Done         Only from Review  │  ← gedimd
│                                 │
│            [ Cancel ]           │
└─────────────────────────────────┘
```
Verboden kolommen blijven in de lijst staan, gedimd, met de reden in één regel. Zo leert de board member de regels zonder fouten te maken.

### 4.3 Tablet en brede vensters (≥ 700 logische px)
Op een iPad of een breed Android-scherm staan de kolommen naast elkaar zoals op desktop, met **lang indrukken + slepen** (`LongPressDraggable`/`DragTarget`) volgens dezelfde regels (§6). Swipe, het contextmenu en Move to… blijven werken.

### 4.4 Staten (mobiel)
Dezelfde staten als §3.3, in mobiele vorm: skeletkaarten bij laden; de bestaande `ConnectionBanner` bij offline (acties uitgeschakeld); een volle lege staat met `FilledButton` "New ticket"; een foutstaat *"Could not load the Board."* + **Try again** (zoals de Team chat na AUR-BUG-03).

---

## 5. De ticketkaart (MU1-07)

### 5.1 Anatomie (desktop, breedte 232–300 px)
```
┌────────────────────────────────────────┐
│ #42  aurelius               ▲▲ Urgent  │  ← nummer (mono, TEXT_2) · project · prioriteit
│ Login per board member with argon2     │  ← titel, 13 px medium, max 3 regels
│ hashing                                │
│ [backend] [security] +1                │  ← labels, max 2 + teller
│ ● Working · 12 min        ⑂ 3 tasks    │  ← status van de lopende run · delegaties
│ (F) Forge        Due Nov 3 · by Séb.   │  ← eigenaar · deadline · aanmaker
└────────────────────────────────────────┘
```
| Veld | Op de kaart | Regel |
|---|---|---|
| Nummer | `#42`, mono 11 px, TEXT_2 | altijd |
| Project | projectnaam, 11 px, TEXT_2 | verborgen als het projectfilter al op dat project staat |
| Titel | 13 px medium, max 3 regels, dan "…" | altijd |
| Prioriteit | tekst + vorm, nooit alleen kleur: **▲▲ Urgent** (NEGATIVE), **▲ High** (0xD99A3A), **Normal** niet getoond, **▽ Low** (TEXT_2) | schaal volgens Nova OV-06 (P-03) |
| Labels | pill-chips (`SURFACE_2`, 10,5 px), max 2 + "+N" | als er labels zijn |
| Status van de run | `ui::task_mark` van de hoofdtask: Working (pulserende stip) · Queued · Waiting · Waiting for limit reset until … · Needs you · Failed; met de duur ("12 min") | in Todo/Doing/Plan review/Review als er een hoofdtask is |
| Todo-staat | ◌ *"Orion is choosing…"* (pulserende ring) · ◆ *"Choose an employee"* (Needs you) · *"Queued for Forge"* | zie §7.3 |
| Eigenaar | monogram 16 px + naam; leeg = *"No owner"* (TEXT_2) | altijd |
| Deadline | *"Due Nov 3"*; binnen 24 u oranje *"Due today 17:00"*; voorbij NEGATIVE *"Overdue · Oct 29"* | als er een deadline is; in Done niet rood |
| Aanmaker | *"by Sébastien"* (11 px, TEXT_2) | altijd |
| Delegaties | *"⑂ 3 tasks"* als er delegaties onder de hoofdtask zijn | Doing, Review |
| Actie | **▸ Start ⌄** (Backlog), **Review ›** (Review, Plan review) | zie §3.4 |

### 5.2 Kaartstaten
- **Needs you** (Review, Plan review, Todo zonder eigenaar na een mislukte dispatch, Failed, een approval van de hoofdtask): linkerbalk van 3 px `ACCENT` en een ◆-chip, dezelfde taal als de Inbox.
- **Failed** (Titan §3.4: blijft in Doing): ✕ Failed (NEGATIVE) + ◆ Needs you; in de ticketweergave **Start again** en **Back to Backlog**.
- **Done:** ✓ Done + datum, gedimd, geen actieknop.
- **Mobiel:** dezelfde inhoud in een `Card` op volle breedte, titel 15 pt; status en eigenaar op één regel; de actieknop als pill rechtsonder (44 pt).

---

## 6. Verplaatsen: regels per kolom

Bron: multi-user.md §2 "Board en workflow", MU1-09, MU1-13 t/m MU1-16, Titan §3.4. De tabel geldt **voor een board member** (de enige gebruiker van de clients). Slepen (desktop, tablet), **Move to…** (overal) en de knoppen (overal) volgen dezelfde regels.

| Van \ Naar | Backlog | Todo | Doing | Plan review | Review | Done |
|---|---|---|---|---|---|---|
| **Backlog** | volgorde | ✅ **= Start** (meteen) | ✗ "Start first" | ✗ | ✗ | ✗ |
| **Todo** | ✅ Back to Backlog (bevestiging als de dispatcher al bezig is) | volgorde | ✗ "Doing starts when an owner is chosen" | ✗ | ✗ | ✗ |
| **Doing** | ✅ Back to Backlog (**bevestiging**: stopt de run en de delegaties) | ✗ "Not from Doing" | volgorde | ✗ "The engine moves tickets here" | ✗ "The owner hands it in" | ✗ "Only from Review" |
| **Plan review** | ✅ = **Afwijzen** (bevestiging) | ✗ | ✅ **opent de plan-dialoog** (§9.1) | volgorde | ✗ | ✗ |
| **Review** | ✅ Back to Backlog (bevestiging) | ✗ | ✅ **opent "Send back to Doing"** met verplichte feedback (§9.2) | ✗ | volgorde | ✅ **opent "Mark as Done?"** (§9.3) |
| **Done** | ✗ "Done tickets are closed" (OV-08) | ✗ | ✗ | ✗ | ✗ | – |

Regels:
- **Een drop die een dialoog opent**, zet de kaart voorlopig (gedimd, met een ring) in de doelkolom. Annuleren laat haar terugspringen. Er gebeurt niets op de server tot de dialoog bevestigd is.
- **Start (naar Todo)** heeft geen dialoog: Start is de bewuste klik (MU1-09). De toast zegt wat er gebeurt: *"#51 started. Orion chooses an employee."* of *"#51 started with Forge."* Undo kan 5 s, zolang er geen eigenaar gekozen is.
- **Volgorde binnen een kolom** mag overal behalve in Done (gesorteerd op sluitdatum).
- **Employees** verplaatsen tickets niet via de client. Hun verplaatsingen (Doing → Review, naar Plan review) komen live binnen (§3.3) en staan met hun naam in de activiteit van het ticket (MU1-15 AC4).
- Een verboden doel krijgt altijd een **reden in woorden**, nooit alleen een stopcursor.

---

## 7. Ticketweergave, desktop (MU1-07, MU1-08, MU1-10 t/m MU1-18)

### 7.1 Plaats en opbouw
De ticketweergave opent in het **rechterpaneel** (`Panel::Ticket`, zelfde breedte en resize als `Panel::Task`, standaard 460 px). Esc of × sluit het. In het ⋯-menu: **Copy link** en **Open in window** (Could).

```
┌ Ticket ──────────────────────────────────────────── ⋯  × ┐
│ #42 · aurelius                                           │
│ Login per board member with argon2 hashing    ✎          │  ← serif 18 px, klik = bewerken
│ [Doing]  ● Working · 12 min   (F) Forge                  │  ← kolom-pill, run, eigenaar
├──────────────────────────────────────────────────────────┤
│ ┌ actiebalk (per kolom, §7.3) ─────────────────────────┐ │
│ │ [Open conversation]                [Back to Backlog] │ │
│ └──────────────────────────────────────────────────────┘ │
├ [ Ticket ] [ Conversation ] [ Activity ] ────────────────┤  ← tabs
│ Description                                         ✎    │
│ Board members log in with a username and password…       │  ← markdown (screens/markdown.rs)
│                                                          │
│ Details                                                  │
│  Project      aurelius                                   │
│  Priority     ▲▲ Urgent                            ⌄     │
│  Owner        (F) Forge                                  │
│               chosen by Orion: "backend + storage work"  │
│  Dispatcher   Orion · project default                    │
│  Mode         Auto                       🔒 run active   │
│  Model        Default (Opus 5.5)         🔒 run active   │
│  Labels       [backend] [security] [+]                   │
│  Deadline     Nov 3, 2026                          ⌄     │
│  Attachments  spec-users.md · 4 KB   [+ Add]             │
│  Created by   Sébastien · Oct 30, 14:05                  │
│                                                          │
│ Tasks (3)                                                │
│  ▾ ● Login per board member      (F) Forge    Working    │  ← hoofdtask
│    ├ ✓ Users table + migration   (S) Schema   Done       │  ← delegaties (boom)
│    └ ◌ Review argon2 params      (S) Sentinel Queued     │
│                                                          │
│ Discussion                                   Coming soon │  ← plaatshouder Fase 2 (§7.5)
│  The ticket discussion arrives in a later version.       │
│  Until then, use the Conversation tab.                   │
└──────────────────────────────────────────────────────────┘
```

### 7.2 Velden en bewerken
| Veld | Weergave | Bewerken |
|---|---|---|
| Titel | serif 18 px | inline, max 160 tekens, teller vanaf 140 |
| Beschrijving | markdown | inline editor (zoals bij notities); "Save" / Esc |
| Project | naam, klik = project openen | alleen in Backlog (P-04) |
| Kolom | pill met de kolomnaam | via de actiebalk, slepen of Move to… |
| Prioriteit | tekst + vorm (§5.1) | dropdown, altijd behalve in Done |
| Eigenaar | monogram + naam; eronder *"chosen by Orion: <reden>"* of *"chosen by Sébastien"* | Backlog: vrij kiezen of leeg (= dispatcher). Todo: **Choose employee** (overschrijft de dispatcher, MU1-12 AC2). Vanaf Doing alleen lezen (een hand-over door de eigenaar volgt automatisch, Titan §3.5) |
| Dispatcher | *"Orion · project default"* / *"Forge · set for this project"* / *"Orion · company default"* | alleen lezen; link "Change in project settings" (§12.2) |
| Modus | Ask · Accept edits · Auto · Plan (de bestaande `AgentMode`-labels); "Project default (Auto)" als niets gekozen is | dropdown; **vergrendeld** (🔒, tooltip *"Locked while the main task runs"*) zodra de hoofdtask loopt (MU1-07 AC5) |
| Model | het bestaande modelpicker-label ("Default (…)", Ctrl+M) | idem vergrendeld |
| Labels | pills + [+] | tekstveld met autocomplete uit bestaande labels; Enter voegt toe, × verwijdert |
| Deadline | datum (+ tijd) | datumkiezer; "Clear" |
| Bijlagen | bestandsnaam · grootte | de bestaande uploadflow van tasks (Ctrl+O) |
| Aanmaker | naam · datum | **nooit** te wijzigen (MU1-07), geen ✎ |
| Tasks | boom (hergebruik van `tasks::tree_order` en `task_mark`) | klik = de bestaande taskweergave (`Panel::Task`) met een terugknop "‹ #42" (MU1-08 AC4) |

Elke wijziging wordt meteen opgeslagen (geen globale Save) en verschijnt live op andere clients. Bij een conflict (verouderde `version`): *"Changed on another device. Showing the latest."* en de weergave laadt opnieuw.

### 7.3 Actiebalk per kolom
De actiebalk staat altijd op dezelfde plek, direct onder de kop, en bevat alleen wat in die kolom kan.

**Backlog**
```
│ Owner: [ Dispatcher (Orion) ⌄ ]                 [ ▸ Start ] │
```
- "Owner" toont eerst **Dispatcher (Orion)** (de dispatcher van het project) en dan de actieve employees, gegroepeerd volgens het organigram (de top eerst, dan per `reports_to`). Start = naar Todo (MU1-09, MU1-10, MU1-12).
- ⋯: Delete (met bevestiging).

**Todo, de dispatcher kiest** (MU1-10)
```
│ ◌ Orion is choosing an employee…  [ Choose yourself ⌄ ] [Back to Backlog] │
```
**Todo, de dispatcher kon niet kiezen of ontbreekt** (MU1-10 AC4, MU1-11 AC3)
```
│ ◆ Needs you · Orion could not choose: "no backend employee is free"   │
│   [ Choose employee ⌄ ]                              [Back to Backlog] │
```
- Zonder (actieve) dispatcher: *"No dispatcher is set for this project."* + link "Project settings".

**Todo, eigenaar gekozen, wacht op zijn beurt:** *"Queued for Forge."* (alleen Back to Backlog).

**Doing**
```
│ ● Working · 12 min        [ Open conversation ]   [Back to Backlog] │
```
- Bij een approval van de hoofdtask: ◆ *"Waiting for your approval"* + **Open in Inbox**.
- Bij quotum: *"Waiting for limit reset until 16:00"* (de bestaande tekst).
- **Failed:** ✕ *"The main task failed: <reden>"* + **Start again** (primair: een nieuwe hoofdtask, de oude blijft gekoppeld) + **Back to Backlog**.
- Na een terugsturing: *"Sent back by Sébastien: <feedback>"* (ingeklapt na 2 regels).

**Plan review** → de plan-reviewpoort (§9.1), inline in de tab Ticket boven de details.

**Review** → de reviewpoort, inline in de tab Ticket boven de details:
```
┌ ◆ Review · only a board member decides ───────────────────────┐
│ Report from Forge · Oct 31, 10:12                    [Expand] │
│ ┌───────────────────────────────────────────────────────────┐ │
│ │ (markdown van het rapport, max 12 regels, dan "Expand")   │ │
│ │ Branch: feat/users-argon2 · commits 3a1f0c2, 9be44d1      │ │ ← informatie, geen merge-knop (MU1-17)
│ └───────────────────────────────────────────────────────────┘ │
│ Feedback (required to send back)                              │
│ ┌───────────────────────────────────────────────────────────┐ │
│ │                                                           │ │
│ └───────────────────────────────────────────────────────────┘ │
│ [ Send back to Doing ]                         [ ✓ Done  ⌘↵ ] │
└───────────────────────────────────────────────────────────────┘
```
- **Send back to Doing** blijft uitgeschakeld tot de feedback niet leeg is (na trim); hover: *"Write what needs to change first."* (MU1-16 AC2/AC3).
- **✓ Done** opent de bevestiging (§9.3).

**Done**
```
│ ✓ Closed by Sébastien · Oct 31, 11:02                                  │
│   Nothing was merged, pushed or deployed.                              │
│   Summary saved to project memory.  /  ✕ Summary failed: … [Try again] │
```
Alle velden zijn alleen te lezen (MU1-16 AC1, MU1-18 AC4). Voor vervolgwerk: **New ticket from this one** (Could; neemt project en labels over en verwijst naar #42), omdat heropenen niet kan (OV-08).

### 7.4 Tabs
- **Ticket:** de (plan-)reviewpoort als de kolom dat vraagt, de beschrijving, de details, de tasks en de plaatshouder voor de discussie.
- **Conversation:** het gesprek van de **hoofdtask** (hergebruik van `screens/chat.rs`, schrijfbaar zoals nu). Voor de start: *"The conversation starts when the ticket starts."*
- **Activity:** de tijdlijn: aangemaakt, gestart (door wie), dispatcher-keuze met reden, kolomwijzigingen (wie: board member of employee, wanneer; MU1-15 AC4), goedkeuringen en terugsturingen met feedback (MU1-14 AC3), Done. Elke regel: tijd (tabulaire cijfers) · monogram · tekst.

### 7.5 Plaats voor de discussie (Fase 2)
In Fase 1 staat onderaan de tab Ticket een **plaatshouder "Discussion"** (één TEXT_2-regel, geen knoppen). In Fase 2 (D-06):
- wordt de tab **Conversation** vervangen door de tab **Discussion** (het gesprek van type `ticket`), op dezelfde plaats, met @vermeldingen;
- verhuizen de feedbackvelden van Plan review en Review naar een bericht in de discussie; de beslissing blijft een knop, de motivering wordt een bericht;
- verdwijnt de plaatshouder.

Zo hoeft de opbouw van de ticketweergave in Fase 2 niet te veranderen.

---

## 8. Ticketweergave, mobiel

Een volledig scherm op route `/ticket/:tid` (boven de tabbalk, zoals de taskpagina), met de iOS-terugveeg.

```
┌─────────────────────────────────┐
│ ‹ Board                     ⋯   │
│ #42 · aurelius                  │
│ Login per board member          │  ← Tokens.title(24)
│ with argon2 hashing             │
│ [Doing] ● Working · 12 min      │
│ ( Ticket | Conversation | Act. )│  ← segmented control
│ DESCRIPTION                     │  ← eyebrow
│ ┌─────────────────────────────┐ │
│ │ Board members log in with…  │ │
│ │                   Show more │ │
│ └─────────────────────────────┘ │
│ DETAILS                         │
│ ┌─────────────────────────────┐ │
│ │ Priority        ▲▲ Urgent › │ │
│ │ Owner           (F) Forge   │ │
│ │   chosen by Orion           │ │
│ │ Dispatcher      Orion       │ │
│ │ Mode            Auto     🔒 │ │
│ │ Model           Default  🔒 │ │
│ │ Labels   backend, security ›│ │
│ │ Deadline        Nov 3     › │ │
│ │ Attachments     1 file    › │ │
│ │ Created by      Sébastien   │ │
│ └─────────────────────────────┘ │
│ TASKS · 3                       │
│ ┌─────────────────────────────┐ │
│ │ ● Login per board… Working ›│ │
│ │   ✓ Users table…   Done    ›│ │
│ │   ◌ Review argon2… Queued  ›│ │
│ └─────────────────────────────┘ │
│ DISCUSSION                      │
│  Arrives in a later version.    │
│ ╭─────────────────────────────╮ │
│ │ [Back to Backlog] [Convers.]│ │  ← glazen actiebalk (per kolom)
│ ╰─────────────────────────────╯ │
└─────────────────────────────────┘
```
- **Actiebalk onderaan** (glas, boven de safe area), maximaal twee knoppen, de primaire rechts:
  - Backlog: **Owner: Dispatcher ⌄** (sheet met employees) + **Start**.
  - Todo: **Choose employee** + ⋯ (Back to Backlog).
  - Doing: **Conversation** + ⋯ (Back to Backlog); bij Failed: **Start again**.
  - Plan review: **Review plan** (opent de bestaande pagina `plan_review.dart`, §9.1).
  - Review: **Send back** + **Done** (§9.2–9.3); het rapport is de eerste sectie van de pagina.
  - Done: geen balk; bovenaan een kaart *"Closed by … · Nothing was merged, pushed or deployed."*
- Bewerkbare rijen hebben een › en openen een sheet of een eigen pagina (de beschrijving: een volledige editor). Vergrendelde rijen tonen 🔒; een tik geeft de toast *"Locked while the main task runs."*
- Een gekoppelde task opent de bestaande taskpagina.

---

## 9. Reviewpoort en Plan review (dialogen)

### 9.1 Plan review (MU1-13, MU1-14; Titan §3.6)
Hergebruik van de bestaande plan-review-component (`screens/approval_plan.rs`, `features/inbox/plan_review.dart`): het plan als markdown, opmerkingen bij een geselecteerde passage, een feedbackveld en de vier knoppen. Alleen de plaats (in het ticket i.p.v. de Inbox) en het effect van de knoppen veranderen:

| Knop (bestaand label) | Effect voor een ticket | Kolom erna |
|---|---|---|
| **Goedkeuren + Auto** (primair, Ctrl+Enter) | `ticket.approve_plan`, uitvoering in **Auto** | Doing |
| **Goedkeuren** | `ticket.approve_plan`, uitvoering in **Accept edits** | Doing |
| **Pas aan** (uitgeschakeld zonder feedback) | `ticket.revise_plan`, modus blijft Plan | Doing, daarna weer Plan review |
| **Afwijzen** (met bevestiging) | `ticket.cancel` | Backlog |

- **D-07 (voor Nova OV-07):** de uitvoeringsmodus kies je met de knop zelf, zoals nu in de Inbox; er komt geen extra moduskeuze. Het veld Modus toont daarna *"Plan → Auto"* (de historiek blijft zichtbaar).
- Slepen van Plan review naar Doing opent deze component als dialoog (desktop) of pagina (mobiel), met de focus op "Goedkeuren + Auto".
- Statusregels hergebruiken de bestaande teksten, bijv. *"Already handled on another device."* Voor tickets is er **geen** verval na 30 minuten (Titan §3.6); de tekst "Expired" komt bij tickets niet voor.

### 9.2 Send back to Doing (MU1-16 AC2/AC3)
```
┌ Send #40 back to Doing ─────────────────────────────┐
│ Sentinel gets your feedback and continues in the    │
│ same conversation. The ticket returns to Review     │
│ with a new report.                                  │
│                                                     │
│ Feedback *                                          │
│ ┌─────────────────────────────────────────────────┐ │
│ │ Use argon2id with m=64MiB; add a test for…      │ │
│ └─────────────────────────────────────────────────┘ │
│ Required.                                           │
│                                                     │
│                       [Cancel]  [Send back  ⌘↵]     │
└─────────────────────────────────────────────────────┘
```
- **Send back** blijft uitgeschakeld zolang het veld leeg is. ⌘↵ zonder tekst zet de focus in het veld met de fout *"Write what needs to change."* (NEGATIVE, als tekst onder het veld, niet alleen een rode rand).
- Op desktop is dit dezelfde inhoud als het inline blok van §7.3; de dialoog verschijnt alleen na slepen of via het contextmenu. Een concept in het inline veld blijft bewaard tot de ticketweergave sluit.
- **Mobiel:** een sheet op volle hoogte met het tekstveld in focus en "Send back" rechtsboven (iOS-patroon), uitgeschakeld tot er tekst is.

### 9.3 Mark as Done (MU1-16 AC1/AC4, MU1-17)
```
┌ Mark #40 as Done? ──────────────────────────────────┐
│ Done closes the ticket. It cannot be reopened.      │
│ Nothing is merged, pushed or deployed.              │
│                                                     │
│ ⚠ 1 delegation is still running:                    │  ← alleen als er nog tasks lopen
│   ◌ Review argon2 params — Sentinel — Working       │
│   It will be cancelled.                             │
│                                                     │
│                         [Cancel]  [✓ Done  ⌘↵]      │
└─────────────────────────────────────────────────────┘
```
- **D-08:** Done vraagt altijd een korte bevestiging, omdat het onomkeerbaar is (OV-08) en een sleepbeweging per ongeluk kan gebeuren. Zonder lopende delegaties zijn het twee regels; met lopende delegaties toont de dialoog welke geannuleerd worden (OV-09).
- Na Done: toast *"#40 is Done. A summary goes to project memory."*

---

## 10. Ticket aanmaken (MU1-07, MU1-12)

### 10.1 Desktop: dialoog "New ticket"
Het bestaande dialoogpatroon (`DialogKind`, zoals "New task"); te openen met Ctrl+N op het Board, "+ New ticket" in de kop en "+ Add ticket" onder Backlog.
```
┌ New ticket ────────────────────────────────────────────────────────────┐
│ Title *                                                                 │
│ ┌─────────────────────────────────────────────────────────────────────┐ │
│ │ Login per board member                                              │ │
│ └─────────────────────────────────────────────────────────────────────┘ │
│ Description                                                             │
│ ┌─────────────────────────────────────────────────────────────────────┐ │
│ │ (markdown, 6 regels hoog, groeit mee)                               │ │
│ └─────────────────────────────────────────────────────────────────────┘ │
│ [Project: aurelius ⌄] [Priority: Normal ⌄] [Owner: Dispatcher (Orion) ⌄]│
│ [Mode: Project default (Auto) ⌄] [Model: Default ⌄] [Deadline: None ⌄]  │
│ Labels  [backend ×] [+ label]                                           │
│ Attachments  [+ Add files  Ctrl+O]                                      │
│                                                                         │
│ Lands in Backlog. Nothing starts until you press Start.                 │  ← uitleg, TEXT_2
│                              [Cancel] [Create and start] [Create  ⌘↵]   │
└─────────────────────────────────────────────────────────────────────────┘
```
- Verplicht: titel en project (MU1-07 AC2); de fout staat onder het veld en de focus springt naar het eerste foute veld.
- Standaardwaarden: project = het projectfilter van het Board, anders het laatst gebruikte; prioriteit Normal; eigenaar *Dispatcher (<naam>)*; modus en model = project default. De aanmaker is de actieve board member; geen veld, wel in de uitleg: *"Created by Sébastien."*
- **Create** (primair, ⌘↵) zet het ticket in Backlog. **Create and start** (D-09) is Create plus Start in één bewuste klik; het label zegt het expliciet, dus dit respecteert "pas gestart na een klik op Start" (MU1-09).
- Bij Modus **Plan** verschijnt onder de keuze: *"Stops in Plan review with a plan before anything changes."*

### 10.2 Mobiel: pagina "New ticket"
Een variant van `new_task_page.dart`: gegroepeerde velden, chips voor project, eigenaar en prioriteit, "Advanced" ingeklapt voor modus, model en deadline, en "Photo or file" voor bijlagen. Rechtsboven **Create**, onderaan een `OutlinedButton` **Create and start**. Dezelfde regels en teksten als op desktop.

### 10.3 Vanuit een chat (multi-user.md §2 "Team chat"; Nova zet dit in Fase 2)
De productdefinitie zegt "Vanuit een chat kan je een ticket aanmaken"; Nova plaatst dat in Fase 2. Het ontwerp ligt klaar, zodat Orion het in Fase 1 of 2 kan plannen (P-06):
- **Desktop:** rechtsklik op een bericht (Team chat, gesprek met een employee, project-assistent, taskgesprek) → **Create ticket…**; en in het ⋯-menu van de chatkop → **New ticket from this chat**.
- **Mobiel:** lang indrukken op een bericht → actiesheet → **Create ticket**.
- "New ticket" opent **voorgevuld**: titel = de eerste regel van het bericht (max 160 tekens), beschrijving = het bericht als citaat + *"From: Team chat · Atlas · Oct 30, 14:05"* met een link terug, project = het project van het gesprek (project-assistent, taskgesprek, later het projectkanaal) of leeg. Na Create: toast *"#52 created in Backlog"* met **Open**. In de chat zelf verschijnt in Fase 1 niets (in Fase 2 een systeemregel in het kanaal).

---

## 11. Onboarding en actieve board member (MU1-20, MU1-21, MU1-22)

### 11.1 Desktop: "Create the first board member" (MU1-20)
Zolang de server geen board members heeft, toont de client **alleen** dit scherm: geen sidebar en geen sneltoetsen behalve Ctrl+Q en de serverkeuze (AC1). De opbouw volgt het bestaande servers-/koppelscherm: logo, serif-titel en één kaart van 420 px in het midden.
```
┌──────────────────────────────────────────────────────────────┐
│                         ◆ Aurelius                           │
│                                                              │
│             ┌───────────────────────────────────┐            │
│             │ Create the first board member     │            │
│             │ This server has no board members  │            │
│             │ yet. Board members create tickets │            │
│             │ and decide in review.             │            │
│             │                                   │            │
│             │ Username                          │            │
│             │ ┌───────────────────────────────┐ │            │
│             │ │ sebastien                     │ │            │
│             │ └───────────────────────────────┘ │            │
│             │ 3–32 characters: letters, digits, │            │
│             │ . _ -                          ✓  │            │
│             │ Password                          │            │
│             │ ┌───────────────────────────┐ 👁  │            │
│             │ │ ••••••••••••••            │     │            │
│             │ └───────────────────────────┘     │            │
│             │ At least 12 characters         ✓  │            │
│             │ Repeat password                   │            │
│             │ ┌───────────────────────────────┐ │            │
│             │ │ ••••••••••••••                │ │            │
│             │ └───────────────────────────────┘ │            │
│             │                                   │            │
│             │        [ Create board member ⌘↵ ] │            │
│             └───────────────────────────────────┘            │
│                                                              │
│      ● Connected to This computer · Choose another server    │
└──────────────────────────────────────────────────────────────┘
```
- De regels (Nova OV-12, nog te bevestigen door Sentinel) staan **vooraf** onder de velden en krijgen een ✓ zodra ze kloppen. Fouten verschijnen pas na het verlaten van een veld of bij bevestigen.
- Fouten van de server (AC3, AC4): *"This username is taken."* bij het veld. Maakt intussen een andere client een eerste board member aan, dan blijft dit scherm werken (beide lukken bij verschillende namen, AC4).
- Het wachtwoord komt nooit in logs of toasts; het oog-icoon toont het alleen lokaal (aria-label "Show password" / "Hide password").
- **Choose another server** onderaan, zodat wie met de verkeerde server verbonden is niet vastzit.
- Na succes: meteen het Board met de lege staat (§3.3) en de toast *"Welcome, sebastien."*

### 11.2 Desktop: "Who's working here?" (MU1-21)
Een client zonder actieve board member, op een server die er al heeft:
```
┌──────────────── Who's working here? ────────────────┐
│ Choose your board member on this server.            │
│  (S) sebastien                                 ›    │
│  (M) marie                                     ›    │
│ ─────────────────────────────────────────────────── │
│  + New board member                                 │
└─────────────────────────────────────────────────────┘
```
- Na de keuze: het wachtwoordveld van die board member (één keer, als OV-13 zo beslist wordt; anders meteen verder). Eronder: *"This device remembers you. Full sign-in comes later."* (eerlijk over Fase 1: identificatie, geen login).
- **Wisselen:** de server picker onderaan de sidebar toont *"as sebastien"*; een klik biedt **Switch board member…** naast de serverkeuze.

### 11.3 Mobiel
- Na het koppelen (`pair_page.dart`) of bij het openen: een volledige pagina zonder tabbalk, large title **"Create the first board member"**, één gegroepeerde sectie met de drie velden, de regels als footer van de sectie en een `FilledButton` op volle breedte boven het toetsenbord.
- Autofill-hints `username` en `newPassword` (password managers stellen een sterk wachtwoord voor). **Return** springt naar het volgende veld; op het laatste veld = Create.
- "Who's working here?" is een gegroepeerde lijst met de board members en "New board member", daarna een wachtwoord-sheet (als OV-13).
- De actieve board member staat bovenaan **More** (monogram + naam; › = Switch board member).

### 11.4 Board members beheren (MU1-22, Should)
**Settings → Board members** (desktop) en **More → Settings → Board members** (mobiel): de lijst met monogram, gebruikersnaam en "you"; **New board member** opent dezelfde velden als §11.1 (met Repeat password). Verwijderen of een wachtwoord resetten komt niet in Fase 1 (OV-14): geen knop, ook geen uitgeschakelde.

---

## 12. Instellingen (MU1-06, MU1-11)

### 12.1 Kolommen per company: Settings → Board
```
┌ Settings › Board ──────────────────────────────────────────────┐
│ Columns                                                        │
│ These columns apply to every project. Each has a fixed role    │
│ in the workflow; you can rename and reorder them.              │
│                                                                │
│ ⋮⋮  Backlog        [Start]        ✎                            │
│ ⋮⋮  Todo           [Dispatch]     ✎                            │
│ ⋮⋮  Doing          [Work]         ✎                            │
│ ⋮⋮  Plan review    [Plan gate]    ✎   Used only in Plan mode   │
│ ⋮⋮  Review         [Review gate]  ✎                            │
│ ⋮⋮  Done           [Closed]       ✎                            │
│                                                                │
│ [+ Add column]   ← alleen als P-05 extra kolommen toelaat      │
│ Restore defaults                                               │
│                                                                │
│ Dispatcher                                                     │
│ Default dispatcher   [ Orion ⌄ ]                               │
│ Chooses an owner for tickets that start without one.           │
│ Projects can set their own in Project settings.                │
└────────────────────────────────────────────────────────────────┘
```
- Elke rij: sleepgreep ⋮⋮ (met het toetsenbord **Alt+↑/↓**), de naam (✎ = inline hernoemen; Enter bewaart, Esc annuleert; niet leeg en uniek) en een **functielabel** (een kleine pill met de vaste rol). De functie verandert nooit mee met de naam (MU1-06 AC4), zodat een hernoemde kolom herkenbaar blijft.
- Een kolom met een functie heeft **geen** verwijderknop; de tooltip van de rij zegt *"This column has a role in the workflow and cannot be removed."* (MU1-06 AC2). Extra kolommen (als ze er komen) hebben een 🗑, uitgeschakeld met *"Move its 3 tickets first"* zolang ze niet leeg zijn (AC3).
- Elke wijziging wordt meteen bewaard (zoals de andere instellingen); alle clients krijgen `board.columns_updated` live (AC1). **Restore defaults** (Could, AC5) vraagt een bevestiging.
- **Mobiel:** More → Settings → Board: een `ReorderableList` met de rijen (sleepgreep rechts), tik = hernoemen in een dialoog, de functie als subtitel; daaronder de sectie Dispatcher met een picker.

### 12.2 Dispatcher per project: Project settings
In de bestaande projectinstellingen (`screens/project_settings.rs`; mobiel de instellingen op de projectpagina) komt een rij:
```
│ Dispatcher   [ Company default (Orion) ⌄ ]                     │
│ Chooses an owner when a ticket of this project starts without  │
│ one.                                                           │
```
- De lijst: **Company default (<naam>)** bovenaan, dan de actieve employees (MU1-11 AC1/AC2).
- Gearchiveerde dispatcher: ◆ *"Forge is archived. Tickets go to the company default (Orion)."* (MU1-11 AC3). Geen dispatcher: *"No dispatcher. Started tickets wait in Todo until you choose an owner."* (Titan §7.5).

---

## 13. Wat verdwijnt: afdelingen (MU1-02, MU1-04)

Na Fase 1 staat het woord "Department" nergens meer in de UI (MU1-04 AC2):

| Platform | Wat verdwijnt | Wat in de plaats komt |
|---|---|---|
| Desktop | Het tabblad **Departments** onder Employees en de sneltoets Ctrl+5 "Go to Departments" | Ctrl+5 = **Go to Board** |
| Desktop | `Page::Department` (head, leden en memory van een afdeling) | niets; die memory staat in Company memory (MU1-03) |
| Desktop | De dialogen **New department** / **Edit department** en de bevestiging **Archive department** | niets |
| Desktop | Het veld **Department** in New / Edit employee | alleen **Reports to** |
| Desktop | De filter **Department** op Tasks | Project, Employee en State blijven |
| Desktop | De groepering per afdeling in de werkkeuze op het Dashboard | groepering via het organigram: de top (zonder `reports_to`) eerst, dan per boss |
| Desktop | Afdelingsvakken in het organigram | een boom op basis van `reports_to` alleen |
| Mobiel | Het segment **Departments** in Company (Employees · Org chart · Departments) | Employees · Org chart |
| Mobiel | `department_page.dart`, `department_memory.dart`, route `/department/:did` | niets |
| Mobiel | De rij **Department** op de employeepagina en de afdeling in de rolregel | alleen de rol en "Reports to" |
| Mobiel | Filter **All departments** op Tasks; groepering per afdeling in de pickers (nieuwe task, give work) | groepering via het organigram, zoals op desktop |
| Mobiel | De afdelingsnaam op een voorstelkaart (`ProposalCard`) | "Reports to <naam>" |
| Beide | Memory-scope "Department" en het label "Global" | **Company memory**; verplaatste items tonen de oude afdeling als herkomstlabel (MU1-03 AC2), bijv. een pill "from Engineering" |

Lege staten en uitlegteksten die afdelingen noemen ("A department groups employees…") verdwijnen mee.

---

## 14. Ontwerpbeslissingen en open punten

### Ontwerpbeslissingen (binnen mijn rol genomen)
| ID | Beslissing | Reden |
|---|---|---|
| D-01 | Nieuwe labels in het Engels; de bestaande plan-reviewknoppen blijven zoals ze zijn | aansluiten op de huidige clients; één plan-review-ervaring |
| D-02 | Desktop: sidebar-item Board onder Dashboard, Ctrl+5 | werk start in Fase 1 via tickets; Ctrl+5 komt vrij |
| D-03 | Mobiel: Board vervangt de tab Tasks; Tasks onder More | max. 5 tabs; het Board is het nieuwe startpunt |
| D-04 | Plan review is altijd zichtbaar, maar ingeklapt als ze leeg is | het pad blijft leesbaar zonder ruimte te verspillen bij tickets zonder Plan-modus |
| D-05 | Telefoon: geen slepen tussen kolommen; knop + swipe + Move to… | slepen tussen pagina's is foutgevoelig en slecht toegankelijk |
| D-06 | De tab Conversation wordt in Fase 2 de tab Discussion | de discussie heeft nu al een vaste plaats |
| D-07 | De uitvoeringsmodus na een plan = de gekozen knop (Goedkeuren + Auto / Goedkeuren) | sluit aan op de bestaande component en Titan §3.6; beantwoordt OV-07 zonder extra veld |
| D-08 | Done vraagt altijd een korte bevestiging | onomkeerbaar (OV-08), en een sleepbeweging kan per ongeluk gebeuren |
| D-09 | "Create and start" naast "Create" | één bewuste klik op een knop met "start" in het label; respecteert MU1-09 |

### Open punten (escalatie naar Orion)
| ID | Vraag | Voorstel |
|---|---|---|
| P-01 | De UI is gemengd Engels/Nederlands (plan review). Eén taal kiezen? | Fase 1: zo laten; een aparte i18n-beslissing later |
| P-02 | Maakt "Give work" (Home en ⌘N op mobiel; de werkkeuze op het Dashboard op desktop) vanaf Fase 1 een ticket i.p.v. een task? | Ja, anders ontstaat werk buiten het Board. Raakt Nova OV-16 en Titan §7.4 |
| P-03 | Prioriteitsschaal | Nova OV-06 (Low · Normal · High · Urgent); het ontwerp gaat daarvan uit |
| P-04 | Mag het project van een ticket na de start nog wijzigen? | Nee, alleen in Backlog (de hoofdtask draait in de projectmap) |
| P-05 | Extra kolommen toevoegen: Nova A-02 zegt ja, Titan §7.1 stelt alleen hernoemen en volgorde voor | Fase 1: alleen hernoemen en volgorde; "+ Add column" pas als er een functie voor is. Het ontwerp toont de knop alleen voorwaardelijk |
| P-06 | Ticket vanuit een chat: Fase 1 of Fase 2? | Fase 2, zoals Nova; het ontwerp ligt klaar (§10.3) |
| P-07 | **Onboarding tegenover de impactanalyse:** Titan §2 maakt `created_by_user` in Fase 1 de principal van het apparaat (NULL toegelaten) en noemt geen `users`-tabel; Nova MU1-20/21 en multi-user.md §3 ("beslist") vragen in Fase 1 een board member met gebruikersnaam en wachtwoord | Het ontwerp volgt de productdefinitie (§11). Titan vult de impactanalyse aan met `users`, `user.create` en "wie ben ik" |
| P-08 | Nova OV-13: wachtwoord vragen bij "Who's working here?" | Ja, één keer per apparaat (het ontwerp heeft er de plaats voor) |

---

## 15. Toegankelijkheid

**Beide platformen**
- **Nooit alleen kleur:** prioriteit (▲▲/▲/▽ + woord), status (stip/ring/✕/✓ + woord), Needs you (◆ + woord), deadline (het woord "Overdue"), verplichte velden (* + de tekst "Required").
- **Contrast:** nagerekend op de tokens: `TEXT_2` op `SURFACE` haalt ±7:1 (dark), maar `MUTED` haalt maar ±3,6:1 (dark, 0x71716B op 0x181916) en ±3,7:1 (light), dus **onder AA** voor gewone tekst. Daarom gebruikt dit ontwerp voor alle leesbare tekst (nummers, "by …", lege staten, uitleg, Low-prioriteit) `TEXT_2`; `MUTED` alleen voor uitgeschakelde elementen, plaatshouders en decoratie. Op mobiel geldt hetzelfde voor `Tokens.muted`/`label3` tegenover `label2`.
- **Elke sleepactie heeft een alternatief zonder slepen** (knop, Move to…, contextmenu) met precies hetzelfde resultaat.
- **Verplaatsingen worden aangekondigd** (live region / `SemanticsService.announce`): *"#42 moved to Review by Forge."*, *"#51 started. Orion chooses an employee."* Wijzigingen van anderen worden gebundeld (max. één aankondiging per 5 s).
- **Reduce motion:** geen schuif-, kantel- of oplichtanimaties; de wijziging verschijnt meteen.
- **Foutmeldingen** staan als tekst bij het veld, krijgen de focus en worden voorgelezen.

**Desktop (gpui, `Role`/`aria_label` zoals in de sidebar)**
- Elke kolom is een lijst met `aria_label` "Backlog, 5 tickets"; elke kaart is één element met een samenvattend label: *"Ticket 42, Login per board member, priority Urgent, owner Forge, Working, due November 3, in Doing."*
- **Toetsenbord op het Board:**

| Toets | Actie |
|---|---|
| ← → | vorige/volgende kolom (focus op de kaart op dezelfde hoogte) |
| ↑ ↓ | vorige/volgende kaart in de kolom |
| Enter | ticket openen in het paneel |
| S | Start (alleen op een Backlog-kaart) |
| M | Move to… (menu met de toegestane kolommen) |
| Space | kaart oppakken; daarna ←/→ kolom, ↑/↓ positie, Enter neerzetten (opent de dialoog als die kolom dat vraagt), Esc annuleren |
| Ctrl/⌘+N | New ticket |
| Esc | paneel sluiten; de focus gaat terug naar de kaart |

- De focusring `FOCUS_RING` (2 px) is altijd zichtbaar op de gefocuste kaart, ook tijdens het oppakken.
- Dialogen houden de focus vast, starten in het eerste veld (feedback) of op de veilige knop (Cancel bij Back to Backlog en Done) en geven de focus bij sluiten terug aan de kaart.

**Mobiel (Flutter `Semantics`)**
- Elke kaart heeft een samenvattend `label` en **custom semantics actions** voor Start, Move to…, Review en Open, zodat VoiceOver (rotor "Actions") en TalkBack dezelfde acties bieden als de swipes.
- Tikdoelen ≥ 44×44 pt (pillknoppen, chips, rijen).
- **Dynamic Type / font scale:** kaarten groeien in hoogte; titels tonen max. 3 regels, vanaf 1,3× schaal de volledige titel. Chips scrollen horizontaal en worden niet afgekapt.
- De kolomchips gedragen zich als tabs: "Doing, tab 3 of 6, 3 tickets, selected".
- Wachtwoordvelden: `autofillHints`, `enableSuggestions: false`, het oog-icoon met een label.

---

## 16. Verschillen desktop en mobiel

| Onderwerp | Desktop | Telefoon | Tablet (≥ 700 px) |
|---|---|---|---|
| Navigatie | sidebar-item Board, Ctrl+5 | tab Board (vervangt Tasks) | tab Board |
| Kolommen | naast elkaar, horizontaal scrollen | één kolom tegelijk, chips + swipe tussen kolommen | naast elkaar |
| Plan review leeg | strook van 44 px | smalle, gedimde chip "Plan review 0" | strook |
| Verplaatsen | slepen, contextmenu, Move to…, toetsenbord | knop op de kaart, swipe (rechts = primair, links = Move to…), lang indrukken = menu | lang indrukken + slepen, plus alles van de telefoon |
| Ticketweergave | rechterpaneel naast het Board, tabs | volledige pagina, segmented control, glazen actiebalk | volledige pagina (split view: Could) |
| Reviewpoort | inline blok in het paneel; dialogen na slepen | sheet op volle hoogte | sheet |
| Plan review | `approval_plan.rs` inline | `plan_review.dart` als pagina | pagina |
| New ticket | dialoog | pagina | pagina |
| Ticket vanuit een chat | rechtsklik op een bericht | lang indrukken op een bericht | idem |
| Onboarding | gecentreerde kaart in het venster | volledige pagina na het koppelen | idem |
| Actieve board member | onderaan de sidebar (server picker) | bovenaan More | idem |
| Kolominstellingen | Settings → Board, sleepgreep + Alt+↑/↓ | More → Settings → Board, ReorderableList | idem |
| Filters | dropdowns in de kop | pills onder de titel | pills |

---

## 17. Traceerbaarheid (story → scherm)

| Story | Waar in dit ontwerp |
|---|---|
| MU1-01 | geen eigen scherm (de company is impliciet één per server) |
| MU1-02, MU1-04 | §13 |
| MU1-03 | §13 (Company memory, herkomstlabel) |
| MU1-05 | §3, §4 (kolommen, projectfilter, live) |
| MU1-06 | §12.1 |
| MU1-07 | §5, §7.2, §10 |
| MU1-08 | §7.1 (Tasks-boom), §7.2, §8 |
| MU1-09 | §3.4 Start, §4.2, §6 (Backlog → Todo), §10.1 Create and start |
| MU1-10 | §7.3 Todo (dispatcher bezig / kon niet kiezen) |
| MU1-11 | §12.2 |
| MU1-12 | §3.4 Start with…, §7.3 Owner / Choose yourself, §10.1 Owner |
| MU1-13, MU1-14 | §3.2 Plan review-kolom, §6, §9.1 |
| MU1-15 | §6 (employees verplaatsen niet via de client), §7.4 Activity |
| MU1-16 | §3.2 Review, §6, §7.3 Review, §9.2, §9.3 |
| MU1-17 | §7.3 Done, §9.3 ("Nothing is merged, pushed or deployed") |
| MU1-18 | §7.3 Done (samenvatting + Try again) |
| MU1-19 | §3.3 Leeg Board |
| MU1-20 | §11.1, §11.3 |
| MU1-21 | §11.2, §11.3, §2 (actieve board member) |
| MU1-22 | §11.4 |
