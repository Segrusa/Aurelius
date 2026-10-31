# UI-ontwerp Fase 2: ticketdiscussie en kanalen in de Team chat

- **Task:** AUR-MU-DEF-P (Pixel, voor Orion) · **Status:** beknopt concept, ter goedkeuring
- **Basis:** `docs/product/multi-user.md` §2 (Ticketdiscussie, Team chat), `fase-1/ui-ontwerp.md` (tokens §1, ticketweergave §7–8, reviewpoort §9, "ticket vanuit een chat" §10.3, D-06), `fase-1/README.md` B1–B10.
- **Taal:** UI-labels in het Engels (D-01). De plan-reviewknoppen blijven ongewijzigd.

## 1. Ticketdiscussie, desktop
Volgens D-06 vervangt de tab **Discussion** de tab Conversation, op dezelfde plaats in `Panel::Ticket`. Het gesprek is van het type `ticket`.
```
┌ Ticket ─────────────────────────────────────────────── ⋯  × ┐
│ #42 · aurelius  Login per board member         [Review] ◆   │
├ [ Ticket ] [ Discussion 3 ] [ Activity ] ───────────────────┤  ← 3 = ongelezen
│ (A) Alexandra · board member                        10:02   │
│     Please keep the argon2 params configurable.             │
│ ┄┄ Orion delegated "Users table" to (S) Schema · 10:05 ┄┄   │  ← systeemregel
│ ┄┄ Schema reported on "Users table" ✓ Done  [Open report]┄┄ │  ← subtask-rapport, ingeklapt
│ (F) Forge — Backend Developer                       10:40   │
│     Done. @Alexandra the hash cost is set via config.       │  ← @jij = gemarkeerd
│ ┄┄ Forge moved #42 Doing → Review · 10:41 ┄┄                │  ← kolomwijziging
│ ┌ ◆ Review · Report from Forge ──────────────── [Expand] ┐  │  ← reviewkaart in de stroom
│ │ Branch feat/users-argon2 · 2 commits                   │  │
│ └────────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│ ┌ Message the discussion · @ to mention ────────────────┐   │
│ │ Use m=64MiB and add a test for @Sen|                  │   │
│ │  ┌──────────────────────────────┐                     │   │
│ │  │ (S) Sentinel  Security Eng.  │ ← autocomplete      │   │
│ │  │ (A) Alexandra board member   │                     │   │
│ │  └──────────────────────────────┘                     │   │
│ └───────────────────────────────────────────────────────┘   │
│ [Send back to Doing]   [Send ⏎]                [✓ Done]     │  ← alleen in Review
└─────────────────────────────────────────────────────────────┘
```
- **Berichten:** monogram, naam en daarachter het soort auteur: *board member* voor een mens, de rol voor een employee. Een bericht van een employee rendert markdown zoals `screens/chat.rs`. Een lopend antwoord streamt live in de stroom.
- **Systeemregels** (gecentreerd, 11,5 px, TEXT_2, geen ballon): delegatie, rapport van een subtask (ingeklapt, **Open report** opent het rapport inline), kolomwijziging met de actor, Start, dispatcher-keuze met reden, Done. De Activity-tab blijft bestaan als compacte tijdlijn zonder berichten (P-22).
- **@vermelding:** `@` opent een autocomplete met de actieve employees (gegroepeerd volgens het organigram) en de board members. Navigeren met ↑↓, kiezen met Enter of Tab, sluiten met Esc. Een vermelding wordt een pill. Een vermelde employee krijgt in de stroom *"Forge is answering…"*. Als de lusgrens (MAX_HOPS of de limiet op wekkende posts) een vermelding tegenhoudt, verschijnt de systeemregel *"Limit reached: Forge was not woken."* Die wordt nooit stil weggelaten.
- **Zonder @:** de composer zegt *"Message Forge (owner) · @ to mention someone else"*. Een bericht zonder vermelding gaat naar de eigenaar als het ticket loopt (P-21).
- **Plan review in de discussie:** het plan verschijnt als kaart in de stroom met **Review plan**, die de bestaande plan-component opent (§9.1 van Fase 1). Onder de composer staan de knoppen **Goedkeuren + Auto · Goedkeuren · Pas aan · Afwijzen**. *Pas aan* gebruikt de tekst in de composer als feedback en is uitgeschakeld zolang die leeg is.
- **Review in de discussie:** **Send back to Doing** stuurt de tekst uit de composer als bericht en als feedback, en is uitgeschakeld zonder tekst (*"Write what needs to change first."*). **✓ Done** opent de bevestiging van §9.3 van Fase 1. Het inline feedbackveld uit Fase 1 (§7.3) verdwijnt: de motivering is nu een bericht.
- **Wie mag wat:** alleen board members zien de beslisknoppen. Employees gebruiken de discussie alleen voor berichten.
- **Staten:** leeg vóór de start (*"Discuss the ticket here. Employees join when you @mention them or when it starts."*), offline (composer uit, concept blijft bewaard), laadfout met **Try again** (zoals de Team chat na AUR-BUG-03). In Done is de discussie alleen-lezen.

## 2. Ticketdiscussie, mobiel
```
┌─────────────────────────────────┐
│ ‹ Board   #42 Login per…    ⋯   │
│ ( Ticket | Discussion ● | Act. )│  ← segmented, ● = ongelezen
│ (A) Alexandra · 10:02           │
│ Please keep the argon2 params…  │
│ ┄ Orion delegated to Schema ┄   │
│ ┄ Schema reported ✓  Report › ┄ │
│ (F) Forge · 10:40               │
│ Done. @Alexandra the hash cost… │
│ ┄ Forge moved to Review ┄       │
│ ┌ ◆ Review · Report  ›        ┐ │
│ └─────────────────────────────┘ │
│ ┌ @Sen ───────────────────────┐ │  ← suggestiestrook boven het toetsenbord
│ │ (S) Sentinel  (A) Alexandra │ │
│ └─────────────────────────────┘ │
│ ╭─────────────────────────────╮ │
│ │ Use m=64MiB…            [↑] │ │  ← composer (glas)
│ │ [Send back]        [✓ Done] │ │  ← alleen in Review / Plan review
│ ╰─────────────────────────────╯ │
└─────────────────────────────────┘
```
- De tab Discussion is een volledige chatweergave en hergebruikt de chatwidgets van de task- en Team chat-pagina. **Report ›** en **Review plan** openen een eigen pagina (`plan_review.dart`).
- De @-suggesties staan als horizontale chips boven het toetsenbord (tikdoel ≥ 44 pt), met daarachter de rol in een kleiner lettertype.
- Een systeemregel is één regel. Wie erop tikt, opent de task of het rapport.

## 3. Team chat: kanalen
### Desktop
```
┌Sidebar┬ Team chat ─────────────────────────────────────────────────────┐
│       │ CHANNELS        │ # aurelius                       ⋯         │
│       │  # general      │ Project channel · 4 employees active        │
│       │ PROJECTS        │ ─────────── New ─────────────               │  ← ongelezen-scheiding
│       │  # aurelius  3  │ (A) Atlas — CEO Agent                10:12  │
│       │  # segrusa-web @│     The vault design is ready for review.   │
│       │  # docs         │ (A) Alexandra · board member  ⋯ hover:      │
│       │                 │     Can we add an audit export?  [⌁ Ticket] │
│       │                 │ ┄┄ Alexandra created #52 from a message ┄┄  │
│       │                 │ ┌ Message #aurelius · @ to mention ────┐    │
│       │                 │ └──────────────────────────────────────┘    │
└───────┴─────────────────┴─────────────────────────────────────────────┘
```
- **Kanaallijst** (180 px, links in het Team chat-scherm): bovenaan **# general**, daaronder **Projects** met één kanaal per actief project in alfabetische volgorde. Gearchiveerde projecten staan onder een ingeklapte groep *Archived* en zijn alleen-lezen.
- **Ongelezen:** de naam wordt vet en krijgt een teller (TEXT_2-pill). Een vermelding van jou geeft een **@**-badge (ACCENT), die dus nooit alleen een kleur is. Het sidebar-item Team chat telt de vermeldingen, niet alle berichten (P-24). Een kanaal openen zet het gelezen tot de nieuwste regel, met de scheiding *"New"* op de oude plaats.
- **Sneltoetsen:** Alt+↑/↓ gaat naar het vorige of volgende kanaal, Ctrl+K "Go to #…".
- Het Projectvenster krijgt een link *"Open #aurelius"*.

### Mobiel
```
┌─────────────────────────────────┐
│ Team                            │
│ [# aurelius ⌄]   ● 2 unread     │  ← kanaalkiezer (pill)
│  …berichten…                    │
└─────────────────────────────────┘
Sheet "Channels"
│  # general                      │
│  PROJECTS                       │
│  # aurelius            3        │
│  # segrusa-web         @  1     │
```
- Een tik op de pill opent een sheet met dezelfde groepen, tellers en @-badges. Swipen tussen kanalen kan niet (te veel gebaren met de chat). De tab **Team** krijgt een badge met het aantal vermeldingen.
- Een melding van een vermelding opent het juiste kanaal op het juiste bericht.

## 4. "Create ticket" vanuit een chatbericht
- **Desktop:** bij hover verschijnt op een bericht de actie **⌁ Ticket**. Dezelfde actie staat in het rechtsklikmenu (*Create ticket…*) en geldt in kanalen, employee-gesprekken, de project-assistent en taskgesprekken. **Mobiel:** lang indrukken → actiesheet → **Create ticket**.
- Dit opent **New ticket** (Fase 1 §10.1/§10.2), al ingevuld volgens Fase 1 §10.3. Het project is dat van het kanaal, of leeg in #general. De beschrijving citeert het bericht en linkt terug.
- Na **Create** komt in het kanaal een systeemregel *"Alexandra created #52 from a message"*, en op het bronbericht een kleine link-pill **→ #52**, zodat het bericht niet twee keer omgezet wordt. Een tweede keer *Create ticket* op hetzelfde bericht vraagt: *"#52 was already created from this message. Create another?"*
- De eerste discussieregel van het nieuwe ticket is de systeemregel *"Created from #aurelius · Atlas · 10:12"*.

## 5. Toegankelijkheid (aanvulling op Fase 1 §15)
- De autocomplete is een `listbox` met een actieve optie die voorgelezen wordt (*"Sentinel, Security Engineer, 1 of 2"*). VoiceOver en TalkBack bieden de chips aan als knoppen.
- Systeemregels krijgen een volledige tekst als label (*"System: Forge moved ticket 42 from Doing to Review at 10:41"*). Nieuwe berichten worden gebundeld aangekondigd (hoogstens één keer per 5 s).
- Een ongelezen kanaal krijgt het label *"aurelius, 3 unread, 1 mention"*.

## 6. Ontwerpvragen
| ID | Vraag | Voorstel |
|---|---|---|
| P-21 | Wie krijgt een discussiebericht zonder @vermelding? | De eigenaar, zolang het ticket in Todo-met-eigenaar, Doing of Plan review staat. In Backlog, Review en Done niemand, met de hint *"No one is woken. @mention to ask someone."* |
| P-22 | De Activity-tab houden nu de systeemregels in de discussie staan? | Ja, als compacte tijdlijn zonder berichten, om snel na te gaan wie wat deed. In de discussie geen extra filter |
| P-23 | Waar blijft het gesprek van de hoofdtask (de tab Conversation uit Fase 1)? | Bereikbaar via Tasks → hoofdtask (*"Open main task conversation"* in ⋯). De discussie is de plaats voor mensen |
| P-24 | Wat telt de badge van Team chat: alle ongelezen berichten of alleen vermeldingen? | Alleen vermeldingen, plus een stip als er iets ongelezen is. Employees posten veel, dus een teller met alles wordt ruis |
| P-25 | Kanalen zelf aanmaken, dempen of verbergen? | Niet in Fase 2: #general plus één kanaal per project, automatisch. Dempen per kanaal kan later (idee) |
| P-26 | Mogen employees in een kanaal posten zonder vermelding (bijv. een rapport in #project)? | Ja, via `post_to_group_chat` naar het projectkanaal, met de bestaande lusgrenzen. Zo'n post wekt niemand |
| P-27 | Beslisknoppen ook in de tab Ticket (Fase 1 §7.3) houden? | Ja, de knoppen staan op beide plaatsen. De feedback komt altijd uit een bericht. De knop in de tab Ticket springt naar de composer van Discussion |
