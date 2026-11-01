# UI-ontwerp Guidelines: bibliotheek, editor, koppelingen en voorstellen

- **Task:** AUR-GL-00-P (Pixel, voor Orion) · **Status:** goedgekeurd door Atlas (B1–B7, README)
- **Rechtzetting bij de start van AUR-GL-01 (README gaat voor):** (1) de grens per run is **16 000** tekens; de voorbeeldgetallen in de mockups zijn nog op 24 000 getekend, de werking (oranje vanaf 80 %, vakje vooraf uit boven de grens) blijft. (2) **Geen departments**: overal waar *department* of *Engineering / Quality Assurance / Product & Design* als groep staat, lees je een groep per leidinggevende (*Reports to ‹naam›*, `reports_to`), op desktop en mobiel; P-54 volgt die variant, niet de department-koppeling. (3) Een wijziging start bij de volgende run een **nieuwe sessie** voor de getroffen gesprekken (B6). (4) De startset komt via een **eenmalige seedstap** bij de eerste start, niet via de migratie (B7).
- **Basis:** `README.md` (B1–B7), `fase-1/ui-ontwerp.md` (tokens, dialogen), `fase-4/ui-ontwerp.md` (lijst en editor van de vault). Het sluit aan bij de bestaande schermen voor instructies: de voorstelkaart in de Inbox (desktop `screens/org.rs` `instructions_proposal_body`, mobiel `instruction_proposal_card.dart`) en de versielijst *Earlier instructions* met **Restore**.
- **User stories:** de koppen verwijzen naar Nova's GL-nummers in `user-stories.md` (GL-01..GL-18) en, waar nodig, naar haar open vragen (OV-xx).
- **Nummering:** de ontwerpkeuzes zijn **P-51 … P-66**, zodat ze niet botsen met Fase 1 (P-01..08) en Fase 2–4 (P-21..47).
- **UI-taal:** Engels, zoals de rest van de clients (D-01).

## 1. Navigatie
- **Desktop:** Employees krijgt een derde tab **Guidelines**, naast *List* en *Org chart* (P-51). Het commandopalet (Ctrl+K) krijgt *"Go to Guidelines"*, *"New guideline"* en *"Find guideline…"*. Er komt geen cijfertoets bij, want Ctrl/⌘+0..9 zijn allemaal bezet (let op: Ctrl+9 is vandaag ook Settings, terwijl Fase 4 P-41 hem voor de Vault wil gebruiken; dat los ik hier niet op).
- **Mobiel:** **More → Guidelines**. Op de pagina van een employee en van een project staat de rij **Guidelines ›** (§6).
- Je kunt elke guideline ook openen vanaf de pagina van een employee, de pagina van een project en een voorstel in de Inbox. Je komt dan steeds in dezelfde editor.

## 2. Bibliotheek, desktop (GL-01, GL-04, GL-16)
```
┌Sidebar┬ Employees ──────────────────────────────────────────────────────────────────────────────┐
│       │ [ List ] [ Org chart ] [●Guidelines ]                  [🔍 Find… ] [+ New guideline]       │
│       │ 6 guidelines · rules every linked run gets, on top of its instructions                     │
│       │ Title                 Applies to                          Version   Size      Updated      │
│       │ Flutter & Dart        ▢ Flux ▢ Bridge                     v2        3 410     You · Oct 30 │
│       │ Git & worktrees       ▸ aurelius  +3 ▢                    v4        2 050     Marie · Oct 31│
│       │ Reviews               ▢ Trace ▢ Pulse                     v1        1 880     You · Oct 29 │
│       │ Rust                  ▢ Forge ▢ Bridge ▢ Nexus ▢ Schema   v3 ◆1     6 120     You · Oct 31 │
│       │ Security checklist    ▢ Sentinel                          v1          940     You · Oct 29 │
│       │ Writing docs          —  not linked                       v1        1 200     You · Oct 28 │
│       │                                                                                          │
│       │ ◆1 = a proposal waits for you · Size in characters (max 8 000 each)                        │
└───────┴──────────────────────────────────────────────────────────────────────────────────────────┘
```
- **Kolommen:** de titel; **Applies to** met de gekoppelde employees (rechthoekige monogrammen, zoals in de auditlog van Fase 4) en projecten (`▸ naam`). Na drie items volgt `+n`, met de volledige lijst in de tooltip. Dan de huidige **versie**, de **grootte** in tekens en wie de guideline wanneer laatst wijzigde.
- **◆n** naast de versie: zoveel voorstellen van agents wachten op deze guideline. Een klik opent het eerste voorstel (§7).
- Een guideline die nergens aan gekoppeld is, toont *"— not linked"* in `TEXT_2`. Ze geldt dan in geen enkele run. Ze blijft wel staan, want misschien wil je haar later koppelen.
- **Sortering** gaat alfabetisch op titel, net als de volgorde in het blok (B3). Je kunt ook sorteren op Size of Updated.
- **Filter** (in het zoekveld): op titel, of met `@Forge` / `▸aurelius` op koppeling.
- **Rij openen** (klik of Enter) opent de editor (§4). Het ⋯-menu bevat *Edit*, *Links…* (§5), *Duplicate* en *Archive* (P-60).

## 3. Bibliotheek, mobiel (GL-17)
```
┌───────────────────────────────┐
│ ‹ More        Guidelines    ＋ │
│ ┌ 🔍 Search ────────────────┐ │
│                               │
│ ┌───────────────────────────┐ │
│ │ Flutter & Dart       v2  ›│ │
│ │ Flux, Bridge · 3 410 chars│ │
│ ├───────────────────────────┤ │
│ │ Git & worktrees      v4  ›│ │
│ │ aurelius +3 · 2 050 chars │ │
│ ├───────────────────────────┤ │
│ │ Rust          ◆ 1   v3  ›│ │
│ │ Forge, Bridge +2 · 6 120  │ │
│ ├───────────────────────────┤ │
│ │ Writing docs         v1  ›│ │
│ │ Not linked · 1 200 chars  │ │
│ └───────────────────────────┘ │
└───────────────────────────────┘
```
- Een gegroepeerde lijst (`Tokens.cell`). Per rij zie je de titel, de versie en daaronder de koppelingen en de grootte. **◆ 1** is de bestaande markering voor "Needs you".
- **＋** rechtsboven = *New guideline*. Een swipe naar links op een rij = *Archive*, met bevestiging.

## 4. Editor (B4, B6; GL-01, GL-02, GL-12)

### 4.1 Desktop
De editor is een volledige pagina in Employees › Guidelines (geen dialoog), want guidelines zijn lange teksten.
```
┌ ‹ Guidelines · Rust ─────────────────────────────────────────────────────────────────────────────┐
│ Title *  ┌ Rust ───────────────────────┐   Applies to  ▢ Forge ▢ Bridge ▢ Nexus ▢ Schema [Links…] │
│ [ Write ] [ Preview ] [●Side by side ]                                  [ History ▸ ]             │
│ ┌ Markdown ───────────────────────────────┐ ┌ Preview ──────────────────────────────────────┐   │
│ │ ## Errors                               │ │ Errors                                        │   │
│ │ - Use `thiserror` in libraries, `anyhow`│ │ • Use thiserror in libraries, anyhow only in  │   │
│ │   only in binaries.                     │ │   binaries.                                   │   │
│ │ - No `unwrap()` outside tests.          │ │ • No unwrap() outside tests.                  │   │
│ │ ## Tests                                │ │ Tests                                         │   │
│ │ - Run `cargo test -p <crate>` before …  │ │ • Run cargo test -p <crate> before …          │   │
│ └─────────────────────────────────────────┘ └───────────────────────────────────────────────┘   │
│ 6 120 / 8 000 characters                     Largest block with this: Bridge in aurelius        │
│ ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░                         18 940 / 16 000 · ≈ 4.7k tokens per run            │
│ Applies from the next run. Runs that already started keep version 3.     [Cancel]  [Save  ⌘↵]  │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```
- **Markdown met voorbeeld:** *Write*, *Preview* of *Side by side* (standaard onder 1100 px is dat *Write*). Het voorbeeld gebruikt dezelfde markdown-renderer als de chat, zodat de Owner ziet wat het team te lezen krijgt.
- **Tekenteller (B4):** `6 120 / 8 000 characters` met een balk. De balk is neutraal tot 80 %. Vanaf 80 % wordt hij oranje (`0xD99A3A`) met de tekst *"Close to the limit"*. Boven 8 000 wordt hij rood (`NEGATIVE`), dan is **Save** uitgeschakeld en staat er *"1 240 characters over the limit of 8 000"*. De teller kijkt naar de opgeslagen markdown, niet naar de weergave.
- **Blokgrootte rechts:** het grootste blok dat een run met deze tekst zou krijgen (employee × project van een run, zie P-56), met de naam van wie en waar. Komt dat boven 16 000, dan is Save uit en zie je *"Bridge would get 25 300 characters in aurelius (max 16 000). Shorten this text or unlink something."* met de link **Show block ›** (§6.1). Een run die bij zijn start toch boven de grens komt (bv. na een verlaging), start wel, met de melding welke guidelines wegvielen (GL-12 AC3). Die melding staat in de run-details en in de Inbox als notificatie.
- **Save** maakt een nieuwe versie (v3 → v4). De regel onderaan herhaalt B6: *"Applies from the next run. Runs that already started keep version 3."*
- **Een lege tekst** kan niet bewaard worden. Wil je een guideline niet meer gebruiken, dan archiveer je haar (P-60).
- **Twee board members tegelijk:** heeft iemand anders intussen bewaard, dan weigert Save met *"Marie saved version 4 while you were editing."* + **Compare** (toont de diff van je tekst tegen v4) + **Discard mine**. Er is geen stil overschrijven. Dit is dezelfde regel als *stale* bij de instructievoorstellen.

### 4.2 Versiegeschiedenis en terugzetten (GL-02, GL-03)
**History ▸** opent een rechterpaneel in de editor.
```
┌ History · Rust ─────────────────────────────┐
│ Version 3 · current · You · Oct 31          │
│   approved proposal from Forge              │
│ Version 2 · Marie · Oct 30      [Restore]   │
│   edited                                    │
│ Version 1 · Oct 29              [Restore]   │
│   starter set                               │
│ ─────────────────────────────────────────── │
│ Version 2 → current                         │
│ - No `unwrap()` in production code.         │
│ + No `unwrap()` outside tests.              │
│ + ## Tests                                  │
└─────────────────────────────────────────────┘
```
- Een rij kiezen toont de **diff van die versie tegen de huidige**, met dezelfde `line_diff`-weergave als het instructievoorstel (`+` ACCENT_TEXT, `-` NEGATIVE, mono 12 px).
- De **bron** gebruikt dezelfde woorden als bij instructies: *starter set* (seed, B7), *created*, *edited*, *approved proposal from X*, *restored from version n*.
- **Restore** maakt een **nieuwe** versie met de oude tekst (*restored from version 2*). Er gaat geen versie verloren. Eerst komt een bevestiging: *"Restore version 2 of Rust? This becomes version 4 and applies from the next run."* Ook hier gelden de controles op de limiet (B4).
- De koppelingen hebben geen versies. History gaat alleen over titel en tekst (P-57).

### 4.3 Mobiel
```
┌───────────────────────────────┐
│ ✕   Rust · v3          [Save] │
│ ( ●Write | Preview )          │
│ ┌───────────────────────────┐ │
│ │ ## Errors                 │ │
│ │ - Use `thiserror` in      │ │
│ │   libraries, `anyhow` …   │ │
│ │                           │ │
│ └───────────────────────────┘ │
│ 6 120 / 8 000 ▓▓▓▓▓▓▓░░░      │
│ Largest block: Bridge in      │
│ aurelius · 18 940 / 16 000    │
│ ┌───────────────────────────┐ │
│ │ Applies to: Forge, +3    ›│ │
│ │ History                  ›│ │
│ └───────────────────────────┘ │
└───────────────────────────────┘
```
- Een volledige pagina (`fullscreenDialog`) zoals `EditInstructionsPage`, met een segmented control *Write | Preview* (geen side by side op een telefoon; op een tablet ≥ 700 pt wel).
- De teller en de regel met het grootste blok staan **boven het toetsenbord** (vast onder het tekstveld), zodat je ze ziet terwijl je typt.
- **History** is een bottom sheet met dezelfde rijen. Een rij open je met een tik (`ExpansionTile` met de diff). **Restore** kan ook op mobiel (P-58).

## 5. Koppelen aan employees en projecten (B2, B4; GL-05, GL-06, GL-08)

### 5.1 Desktop: dialoog *Links*
Je opent de dialoog via **Links…** in de editor of in het ⋯-menu van de bibliotheek.
```
┌ Who gets “Rust” ─────────────────────────────────────────────────┐
│ EMPLOYEES                                    [🔍 Find… ]          │
│ ▾ ◩ Engineering                          4 of 7 · [Select all]   │
│     ☑ Forge         Backend Developer        14 200 → 20 320     │
│     ☑ Bridge        Fullstack Developer      12 820 → 18 940     │
│     ☑ Nexus         API & Integration         8 050 → 14 170     │
│     ☑ Schema        Database Engineer         6 400 → 12 520     │
│     ☐ Flux          Frontend Developer        9 900               │
│     ☐ Sentinel      Security Engineer         4 100               │
│     ☐ Titan         Tech Lead                 2 050               │
│ ▸ ☐ Quality Assurance                    0 of 2                  │
│ ▸ ☐ Product & Design                     0 of 3                  │
│ PROJECTS                                                          │
│     ☐ aurelius      everyone working in this project              │
│     ☐ segrusa-web                                                 │
│ ⓘ Selecting a department links its current members one by one.   │
│   New employees in Engineering are not linked automatically.      │
│                                              [Cancel]  [Save]     │
└──────────────────────────────────────────────────────────────────┘
```
- **Snelle keuze per leidinggevende (P-54, B2):** departments bestaan niet meer. De groepsrij is *"Reports to ‹naam›"* (`reports_to`), met een driestandenvakje (☐ geen, ◩ sommige, ☑ alle) en *"4 of 7"*. In de mockup hierboven lees je dus *"▾ ◩ Reports to Titan"* in plaats van *"Engineering"*, en zegt de ⓘ-regel *"Selecting a group links its current members one by one. New reports are not linked automatically."* **Select all** of een klik op het vakje zet alle huidige leden aan of uit. Opgeslagen worden **alleen koppelingen per employee**. De groep is alleen een indeling van de lijst en geen eigen koppeling.
- **Groottekolom:** per employee het grootste blok nu → met deze guideline (§6.1, P-56). Een waarde boven 16 000 wordt rood en het vakje kan dan niet aan, met de tooltip *"Forge would get 25 300 characters in aurelius (max 16 000)."* Dat is de weigering van B4, alleen zie je ze **vóór** het klikken in plaats van erna.
- **Projecten:** een projectkoppeling geldt voor elke run in dat project, van wie ook (B2). Vink je een project aan, dan toont de dialoog onder het project het grootste blok dat daardoor ontstaat (*"largest: Bridge · 22 400"*).
- Gearchiveerde of gepauzeerde employees staan in de lijst met *(paused)*. Je kunt ze koppelen, want hun runs starten later weer.
- **Save** bewaart alle wijzigingen in één keer. Faalt de controle op de server toch (een andere board member koppelde intussen iets), dan blijft de dialoog open met de melding van de server en blijft de selectie behouden.

### 5.2 Mobiel
```
┌───────────────────────────────┐
│ ✕   Who gets “Rust”    [Save] │
│ ( ●Employees | Projects )     │
│ ENGINEERING · 4 of 7  [All]   │
│ ┌───────────────────────────┐ │
│ │ Forge              ☑      │ │
│ │ Backend · 20 320 / 16 000 │ │
│ ├───────────────────────────┤ │
│ │ Flux               ☐      │ │
│ │ Frontend · 9 900          │ │
│ └───────────────────────────┘ │
│ QUALITY ASSURANCE · 0 of 2 [All]│
│ …                             │
│ Selecting All links today's   │
│ members, not future ones.     │
└───────────────────────────────┘
```
- Twee segmenten, *Employees* en *Projects*. Per department staat er een sectiekop met **All** / **None** (een tekstknop die wisselt). Een rij is een `CheckboxListTile` met de rol en de blokgrootte. Wat te groot is, is uitgeschakeld en heeft de reden in de subtitel.

## 6. Guidelines op de pagina van een employee en van een project (B2, B3, B4; GL-07, GL-10)

### 6.1 Employee, desktop
Op de pagina van de employee (Employees › Forge) komt er een sectie **Guidelines** direct onder *Instructions*.
```
│ INSTRUCTIONS                                                        [Edit] │
│ Je bent Forge, een senior Backend Developer. …                             │
│                                                                            │
│ GUIDELINES                                                [Manage links…]  │
│ Own (linked to Forge)                                                      │
│   1  Git & worktrees      v4    2 050    also via ▸ aurelius               │
│   2  Rust                 v3    6 120    ◆ 1 proposal                      │
│   3  Security checklist   v1      940                                      │
│ From a project (only in runs of that project)                              │
│   ▸ aurelius   +1  Release notes  v2  1 780         block 10 890 / 16 000  │
│   ▸ segrusa-web    —                                block  9 110 / 16 000  │
│ Without a project: 9 110 / 16 000 characters · ≈ 2.3k tokens   [Show block]│
│ Instructions, task, agent rules and permissions always go before these.    │
```
- **Welke er gelden:** eerst de eigen koppelingen, dan per project wat er in runs van dat project bijkomt. De volgorde is die van het blok (B3: employee, dan project, alfabetisch). Een guideline die zowel aan Forge als aan het project hangt, telt één keer en krijgt de noot *"also via ▸ aurelius"* (P-55).
- **Hoe groot het blok is:** per context een getal tegen 16 000: zonder project en per project waarin de employee werkt. Het grootste getal stuurt de waarschuwing (oranje vanaf 80 %, zoals GL-07 AC2).
- **Show block** opent een alleen-lezen voorbeeld van het blok zoals het in de context komt, met de labels en de volgorde, en met een projectkiezer bovenaan (*"Run in: no project ⌄"*). Zo kun je controleren wat een run echt krijgt.
- Een klik op een titel opent de editor. **Manage links…** opent een kleine variant van §5, maar dan per employee: de lijst van alle guidelines met een vinkje per guideline.
- **Leeg:** *"No guidelines yet. Forge works with its instructions only."* + **Add guideline…** (de lijst van §5 per employee) en **New guideline**.

### 6.2 Project, desktop
In het projectvenster komt een blok **Guidelines** in de tab *Overview* (geen nieuwe tab, P-53):
```
│ GUIDELINES FOR EVERYONE IN AURELIUS                       [Manage links…]  │
│   Git & worktrees   v4   2 050                                             │
│   Release notes     v2   1 780                                             │
│ Added to each run here: 3 830 characters · largest block: Bridge 18 940    │
```
- Het blok toont alleen de projectkoppelingen: wat elke run in dit project erbij krijgt, en het grootste totale blok dat dat oplevert, met de naam van de employee.

### 6.3 Mobiel
- **Employee-pagina:** de rij **Guidelines** met de subtitel *"3 · 9 110 / 16 000 characters"*. Een tik opent een pagina met de secties *Own* en *From a project* (zoals §6.1, één rij per guideline). Onderaan staat **Show block**, met een projectkiezer als segmented control of als sheet.
- **Projectpagina:** de rij **Guidelines** met *"2 · +3 830 characters per run"*, en daarachter dezelfde lijst.
- Koppelingen wijzigen op deze pagina's gebeurt via **Manage** rechtsboven, met de lijst van §5.2 in omgekeerde richting: de guidelines met een vinkje.

## 7. Voorstel van een agent goedkeuren (B5; GL-13, GL-14)

### 7.1 Desktop: Inbox
Het voorstel komt in de Inbox als een kaart naast de instructievoorstellen en gebruikt dezelfde opbouw (`instructions_proposal_body`):
```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ◆ GUIDELINE PROPOSED · WAITING FOR YOU                                       │
│ Forge proposes a change to “Rust”                                            │
│ Applies to Forge, Bridge, Nexus, Schema · version 3 → 4                       │
│ WHY                                                                          │
│ Twice this week a review found blocking I/O in async code. A rule prevents it.│
│ BASED ON                                                                     │
│ Task  AUR-MU-02-F3 — Trace R1 major M1                                       │
│ Lesson  tokio::fs in async handlers                                          │
│ CHANGE · NOW → PROPOSED                               6 120 → 6 410 / 8 000  │
│ ┌──────────────────────────────────────────────────────────────────────────┐ │
│ │   ## Async                                                               │ │
│ │ + - Never call std::fs or blocking I/O inside an async fn; use tokio::fs │ │
│ │ +   or spawn_blocking.                                                   │ │
│ └──────────────────────────────────────────────────────────────────────────┘ │
│ Largest block after this: Bridge in aurelius 19 230 / 16 000                 │
│                                 [Reject]  [Edit and approve…]  [Approve]     │
└──────────────────────────────────────────────────────────────────────────────┘
```
- **Kop:** wie stelt voor en wat. Er zijn drie soorten: *proposes a change to “Rust”*, *proposes a new guideline “Async Rust”* of *proposes to link “Rust” to Flux* (P-61).
- **Diff:** dezelfde regeldiff als bij instructies. Bij een **nieuwe** guideline is de diff helemaal `+`, en staan de voorgestelde koppelingen onder *APPLIES TO* als vinkjes die je vóór Approve nog kunt aanpassen (standaard: de voorsteller zelf).
- **Limiet:** de tekenteller en het grootste blok na goedkeuring. Is één van beide over de limiet, dan is **Approve** uit en zegt de kaart waarom. **Edit and approve…** blijft beschikbaar om in te korten.
- **Edit and approve…** opent de editor van §4 met de voorgestelde tekst. **Save** daar = goedkeuren met jouw tekst. De bron in de History wordt *approved proposal from Forge (edited)*.
- **Reject** vraagt een optionele noot voor de agent (*"Note for Forge"*), zoals op mobiel bij instructies. De agent krijgt het besluit en de noot terug (P-62).
- **Verouderd (stale):** is de guideline intussen gewijzigd (v3 is v4 geworden), dan toont de kaart in `NEGATIVE` *"“Rust” changed since this was proposed (version 3 → 4), so it can't be approved. Reject it; Forge can propose again on the new text."* en is er alleen **Reject**. Dat is dezelfde regel als bij instructies.
- Het voorstel staat ook in de editor van die guideline als balk bovenaan: *"Forge proposes a change · Review ›"* (de ◆ uit de bibliotheek).
- Alleen board members zien de knoppen. Een employee die de kaart via een taak ziet, leest alleen.

### 7.2 Mobiel: Inbox
```
┌───────────────────────────────┐
│ New guideline version         │
│ Rust                          │
│ Proposed by Forge · v3 → 4    │
│ Twice this week a review …    │
│ [Task AUR-MU-02-F3] [Lesson…] │
│ ┌───────────────────────────┐ │
│ │ + - Never call std::fs …  │ │
│ └───────────────────────────┘ │
│ 6 410 / 8 000 · block 19 230  │
│        [Reject] [Edit] [Approve]│
└───────────────────────────────┘
```
- Dezelfde `SurfaceCard` als `InstructionProposalCard`, met `DiffLines`, de chips met de bron, en **Reject** (noot), **Edit** (volledige pagina, *Approve* rechtsboven) en **Approve**. Bij een nieuwe guideline staat er onder de diff een rij **Applies to: Forge ›**, die de lijst van §5.2 opent.

## 8. Lege toestand en seed (B7; GL-18)

### 8.1 Met de starter set (bestaande company, na de migratie)
- De bibliotheek toont de guidelines van de starter set als gewone rijen. Hun versie 1 heeft de bron *starter set*. Er komt eenmalig een banner boven de lijst: *"Aurelius added a starter set with the team's current rules (Rust, Flutter, Git & worktrees, Reviews). Check them and their links; they apply from the next run."* met **Got it**. De banner verdwijnt per board member na Got it.
- Er komt geen apart label op de rij. Na de eerste wijziging is het gewoon een guideline (B7).

### 8.2 Leeg (nieuwe company, of alles gearchiveerd)
```
┌ Guidelines ─────────────────────────────────────────────────────┐
│                                                                  │
│         No guidelines yet                                        │
│   Guidelines are working rules (how to write Rust, how to use    │
│   worktrees) that you link to employees or projects. They come   │
│   on top of an employee's instructions in every run.             │
│                                                                  │
│          [+ New guideline]   [Add starter set…]                  │
│   Employees can also propose one; you'll find it in the Inbox.   │
└──────────────────────────────────────────────────────────────────┘
```
- **Add starter set…** toont de sjablonen met een vinkje per sjabloon. Ze worden gewone guidelines **zonder koppelingen**, en de toast zegt: *"Added 4 guidelines. Link them to employees to use them."* (P-63).
- **Mobiel:** dezelfde tekst gecentreerd, met **New guideline** als `FilledButton` en *Add starter set* als tekstknop.

## 9. Toegankelijkheid
- De tekenteller heeft een label (*"6 120 of 8 000 characters"*). Hij kondigt alleen aan bij het passeren van 80 % en van 100 %, niet bij elke toets. De kleur van de balk staat er nooit alleen: er hoort altijd een tekst bij (*"Close to the limit"*, *"over the limit"*).
- Een driestandenvakje van een department heeft het label *"Engineering, 4 of 7 selected"*. Een uitgeschakeld vakje leest de reden voor.
- De diff markeert met `+`/`-` als teken, niet alleen met kleur, net als nu.
- Preview en Write zijn tabs met pijltjestoetsen. **Ctrl/⌘+Shift+P** wisselt tussen beide in de editor. **Ctrl/⌘+↵** = Save.
- **Reduce motion:** het History-paneel en de banner schuiven niet in.

## 10. Ontwerpkeuzes en standpunten over B1–B7
| ID | Vraag | Voorstel |
|---|---|---|
| P-51 | Waar staat de bibliotheek? | Als tab **Guidelines** onder Employees, niet als eigen sidebar-item. Guidelines gaan over hoe employees werken, en de sidebar en de cijfertoetsen zijn vol. Mobiel: **More → Guidelines** |
| P-52 | Is de editor een dialoog of een pagina? | Een pagina, met side by side op een breed scherm. Guidelines zijn lange teksten met kopjes, en het voorbeeld is nodig om de opmaak te controleren |
| P-53 | Krijgt een project een tab Guidelines (zoals de Vault)? | Nee, het wordt een blok in *Overview*. Een project heeft meestal 0–3 guidelines, dus een tab is te zwaar |
| P-54 | Hoe werkt de keuze per department? | Een driestandenvakje dat de huidige leden één voor één koppelt. Departments worden geen eigen koppeling (dat ligt vast in de opdracht). De UI zegt dat nieuwe leden niet automatisch volgen. Nova stelt in OV-01 een department als echte koppeling voor. Kiest Orion daarvoor, dan wordt de department-rij zelf een koppeling (☑ = het department, ook voor nieuwe leden), met de vakjes per employee eronder uitgeschakeld en de noot *via Engineering*. De rest van dit ontwerp blijft gelijk |
| P-55 | Een guideline die aan de employee **én** aan het project hangt? | Ze komt één keer in het blok, op projectniveau (specifiek wint, B3), en telt één keer mee voor de grootte. De UI toont *"also via ▸ project"*. **Aanvulling op B3**, die dit geval niet noemt |
| P-56 | Wat telt B4 precies, en wanneer? | **Ik ben het deels oneens met B4.** Alleen bij het koppelen weigeren is niet genoeg: een **bewerking** van een tekst die al gekoppeld is, of een **projectkoppeling**, kan een blok ook boven 16 000 duwen. Voorstel: de controle draait bij elke handeling die een blok groter maakt (Save, Restore, koppelen, Approve), en ze kijkt naar het grootste blok over alle combinaties van employee × project waarin die employee werkt. De UI toont die grens vooraf (teller, groottekolom, uitgeschakelde vakjes) in plaats van alleen een foutmelding achteraf. Titan beslist of "werkt in een project" de lijst van projectleden is of elk project |
| P-57 | Krijgen koppelingen versies? | Nee, alleen titel en tekst hebben versies. Wie wat koppelde, staat als regel in de History van de guideline (*"Linked to Flux · You · Oct 31"*, GL-05 AC1), zonder Restore. B6 slaat bij de run de versies op die hij kreeg, en dat volstaat om een run te reviewen |
| P-58 | Kan je op mobiel een versie terugzetten? | Ja, met bevestiging. Bij instructies kan dat nu alleen op de pc, maar een guideline terugzetten is even veilig als ze bewerken, en een board member onderweg moet een foute regel snel kunnen terugdraaien. (Voor instructies is dat een idee voor later, niet voor deze scope) |
| P-59 | Waar zie je welke versies een run kreeg (B6)? | In de trace of de taakweergave van de run: de regel *"Guidelines: Rust v3, Git & worktrees v4"*, met een link naar die versie in de History. Klein, maar het maakt B6 zichtbaar |
| P-60 | Verwijderen of archiveren? | Archiveren: de koppelingen vervallen, de versies blijven, en de guideline is terug te halen via de filter *Archived*. Echt verwijderen is niet nodig en zou B6 breken (een oude run verwijst naar een versie). Dit is het alternatief van Nova in OV-07 |
| P-61 | Wat mag een agent voorstellen? | Een wijziging, een nieuwe guideline of een koppeling. Zoals Nova in OV-04 voorstelt, zijn de standaardkoppelingen de voorsteller zelf en het project van de run. Andere koppelingen mogen ook, want de board member ziet ze als vinkjes op de kaart en past ze aan vóór Approve. **Aanvulling op B5** |
| P-62 | Approve / Reject: welke woorden? | **Approve** en **Reject**, op beide clients. Op desktop zegt de instructiekaart nu *Decline* en op mobiel *Reject*. Ik stel voor dat de nieuwe kaart *Reject* gebruikt en dat de instructiekaart later meegaat (als idee, niet in deze scope) |
| P-63 | Seed: altijd via de migratie? | **Ik ben het deels oneens met B7.** Voor de bestaande company (dit team) is een eenmalige seed via de migratie goed, met de koppelingen uit Titans `seed.md`, want anders geldt er op dag één niets. Maar de afspraken over worktrees en de build gaan over de repo van Aurelius en horen bij het **project** aurelius, niet bij employees. Een **nieuwe** company zou via de migratie regels krijgen over een repo die ze niet heeft. Voorstel, in lijn met Nova's OV-05: geen inhoud in de schemamigratie, maar een eenmalige seedstap die alleen draait als de bibliotheek leeg is **en** het project aurelius bestaat, en die dan koppelt zoals in seed.md. Anders blijft de bibliotheek leeg, met *Add starter set…* (sjablonen zonder koppelingen) |
| P-64 | Hoe vermijden we twee board members die elkaar overschrijven? | Save met de basisversie. Is die intussen veranderd, dan volgt een weigering met **Compare** of **Discard mine**, dezelfde regel als *stale* bij voorstellen |
| P-65 | Toont de bibliotheek de grootte in tekens of in tokens? | In tekens, want de limiet B4 staat in tekens en dat is exact. Tokens alleen als benadering (*≈ 4.7k tokens*) bij de blokgrootte, waar het over kost gaat |
| P-66 | B1, B2, B6 | **Eens.** De bibliotheek op company-niveau, de optionele projectkoppeling zonder employee × project, en de vaste versies per run passen in dit ontwerp zonder extra schermen. Voor een uitzondering per employee in één project is een aparte guideline, gekoppeld aan dat project, voldoende |
