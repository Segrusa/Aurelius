# UI-ontwerp Fase A: samenwerken per persoon

- **Task:** AUR-FA-DEF-P (Pixel, voor Orion) · **Status:** concept, ter goedkeuring · 2026-11-02
- **Basis:** `samenwerking/analyse-linear.md` §6 (US-A0 t/m US-A6) en §8, besluit V2 (alleen AI voert uit), `fase-1/ui-ontwerp.md` (kaart §3–5, tokens), `fase-2/ui-ontwerp.md` (discussie, P-21..27), `fase-3/ui-ontwerp.md` §3, §5 (auteurschap, login), `guidelines/ui-ontwerp.md` P-51 (schaarse toetsen), `aur-chat-001/ui-ontwerp.md` §2, §5, §8. Ik heb de schermen en `keymap.rs` in de code nagelezen op `main` @ `53cb385`; de client heb ik niet gedraaid.
- **UI-taal:** Engels (D-01). De labels in dit document zijn de echte teksten.
- **Nummering:** ontwerpkeuzes zijn **P-71 … P-92**: een nieuw blok, zoals elke reeks (P-01, P-21, P-31, P-41, P-51..66). Open punten heten **B-P1 … B-P14**, zodat ze niet botsen met de roadmap-items B1–B8 van de analyse. Orion nummert ze om in de README.

## 0. Wat er is en wat dat betekent
| Onderdeel | Nu (code) | Gevolg |
|---|---|---|
| Kaart, desktop | `board.rs` `render_card`: onderste rij = monogram eigenaar, deadline, monogram + *"by Marie"* | Deze rij wordt het wie-blok (§1.3) |
| Kaart, mobiel | `ticket_card.dart`: dezelfde gegevens in een `Wrap` | Idem |
| Paneel | `ticket.rs` `render_ticket_details`: rijen *Owner*, *Created by*, *Runs on*, los van elkaar | Woorden blijven; ze komen samen in één blok (§1.4) |
| Filters | *Project, Owner, Created by, Find, Needs you* | Er komt één filter bij (§2.3) |
| Needs you | Voor iedereen gelijk, behalve de login (`ticket_needs`) | Wordt per persoon (§5.3, B-P4) |
| Inbox, desktop | Tabs *Needs you, Results, Failed, All* | Tab *Updates* erbij (§4) |
| Inbox, mobiel | Secties *Needs you, Running, Done*; openen markeert alles gelezen | Sectie *Updates* erbij, met een eigen leesregel (§4.5) |
| Meldingen | `notification_for` kent geen `ticket.*`; wel *Plan to review*, *Result ready*, *Task failed* | Risico op dubbele rijen (B-P9) |
| Copy link | Desktop-menu kopieert nu de tekst `#42 (ticket <id>)`; mobiel heeft alleen `Copy #42`; geen URL-schema | §6 vervangt dit |
| Toetsen | Ctrl+0..9 zijn allemaal bezet | §8 |

## 1. De vier wie-velden (grootste risico)

### 1.1 Benaming
Eén nieuw woord, de andere drie blijven zoals ze in de client staan.

| Veld | Label | Vaste zin (uitleg) | Soort | Monogram |
|---|---|---|---|---|
| Menselijke verantwoordelijke (`assignee` in de analyse) | **Responsible** | *"Follows up and reviews. Doesn't do the work."* | board member | cirkel |
| Employee-eigenaar | **Owner** | *"Does the work."* | employee (AI) | afgeronde rechthoek |
| Login-user | **Runs on** | *"Whose Claude subscription the runs use."* | board member | geen, alleen tekst |
| Aanmaker | **Created by** | *"Made the ticket. Never changes."* | board member | cirkel (alleen in het paneel) |

**Waarom Responsible (P-71, B-P1).** *Assignee* en *Assigned to* zeggen in elke tool: deze persoon doet het werk. Dat is precies de indruk die V2 uitsluit. Het botst ook met *assign* in de client, waar het altijd over employees gaat (`assign_ticket`, *"Give the selected task to an employee"*).

Afgewezen:
- *Reviewer* is te smal en botst met Trace, Pulse en de kolom Testing.
- *Lead* botst met de rol Tech Lead en de *lead* van outcome-projecten (C1).
- *Follow-up* botst met *Follow* uit US-A2.
- *Supervisor* suggereert een plek in het organigram.

**Woordenlijst:** *Responsible*, *No one responsible*, *Make me responsible*, *Set responsible…*, *Remove responsible*, *"Marie made you responsible for #42"*. Het woord **assign** komt in deze functie nergens in de UI voor. De veldnaam op de wire kiest Titan; ik stel `responsible_user` voor, zodat de code hetzelfde zegt.

### 1.2 Drie vaste regels voor één oogopslag (P-72)
1. **Vorm = soort.** Cirkel is mens, rechthoek is AI (P-31). Een kaart heeft hooguit twee monogrammen: de rechthoek voert uit, de cirkel volgt op. Een andere cirkel staat er niet.
2. **Uitvoering hoort bij elkaar.** Wie uitvoert en op wiens login staan in één regel, als een zin: *Forge · on Marie's login* (P-73).
3. **Opvolging staat apart, met het woord erbij.** *Alexandra · responsible*. Nooit een naam zonder rolwoord.

### 1.3 Kaart
```
Desktop (232–300 px)                   Mobiel
┌──────────────────────────────────┐  ┌───────────────────────────────┐
│ #42 aurelius                 ▲▲  │  │ #42  aurelius              ▲▲ │
│ Login per board member           │  │ Login per board member        │
│ ● Working           ⑂ 2 tasks    │  │ ● Working           ⑂ 2 tasks │
│ ▢F Forge · on Marie's login      │  │ ▢F Forge · on Marie's login   │ ← voert uit, op wiens login
│ ◯A Alexandra · responsible  Nov 3│  │ ◯A Alexandra · responsible    │ ← volgt op
└──────────────────────────────────┘  │ Due Nov 3                     │
                                      └───────────────────────────────┘
```
- **Desktop:** 11 px, TEXT_2. Monogrammen 16 px. De deadline staat rechts op de tweede regel. **Mobiel:** 13 px, monogrammen 18 pt, de deadline op een eigen regel. De bestaande knop (Start, Review…) blijft eronder.
- **Afkappen:** eerst de naam van de employee of de responsible. *"· responsible"* en *"on … login"* blijven altijd staan. Een tooltip toont de volledige tekst.
- **"You":** *You · responsible*, *on your login* (zoals `login_owner`).
- **De aanmaker staat niet meer op de kaart** (P-74, B-P3). Vier namen passen niet op 232 px en de aanmaker is het minst bruikbaar voor wie opvolgt. Toch blijft die bereikbaar:
  - **Desktop:** in de tooltip op het wie-blok.
  - **Mobiel:** als kopregel van het lang-indrukken-menu.
  - **Overal:** in het paneel, het filter *Created by* en het voorleeslabel.
- **Tooltip (desktop):** *"Forge does the work. Runs on Marie's Claude login. Alexandra is responsible: follows up and reviews. Created by Bram."*
- **Voorleeslabel (beide):** dezelfde zin, opgenomen in `card_label` en de `summary` van de kaart.

| Staat | Regel 1 | Regel 2 |
|---|---|---|
| Normaal | `▢F Forge · on Marie's login` | `◯A Alexandra · responsible` |
| Alles van mij | `▢F Forge · on your login` | `◯ You · responsible` |
| Nog geen eigenaar | `No owner · on Marie's login` (de statusregel zegt al *Orion is choosing…* of *Needs you*) | ongewijzigd |
| Geen responsible | ongewijzigd | `○ No one responsible` (TEXT_2, gestippelde ring, nooit een waarschuwing) |
| Responsible uitgeschakeld | ongewijzigd | `◯A Alexandra (disabled) · responsible`, monogram gedimd |
| Login-user uitgeschakeld | `▢F Forge · on Marie's login (disabled)` | ongewijzigd; de bestaande regel *Waiting: Marie is disabled* blijft |
| Wacht op login of quotum | ongewijzigd; de bestaande statusregel *Waiting for Marie's limit* blijft | ongewijzigd |
| Done | gedimd zoals de hele kaart, zelfde tekst | idem |

### 1.4 Paneel en ticketpagina
```
Desktop: Panel::Ticket                               Mobiel: ticketpagina
┌ #42 · aurelius ──────────────────────── ⋯  × ┐     ┌ ‹  #42 · aurelius       ⋯ ┐
│ Login per board member          [Review] ◆   │     │ Login per board member     │
│ ▢F Forge · on Marie's login                  │ ←   │ ▢F Forge · on Marie's login│ ← wie-strook, op elke tab
│ ◯A Alexandra · responsible      [✓ Following]│ ←   │ ◯A Alexandra · responsible │
├ [ Ticket ] [ Discussion 3 ] [ Activity ] ────┤     │ ( Ticket | Discussion | A.)│
│ WHO                                          │     │ WHO                        │
│ Responsible ◯A Alexandra              ⌄      │     │ Responsible   Alexandra  › │
│             Follows up and reviews.          │     │ Follows up and reviews.    │
│ Owner       ▢F Forge · Backend Dev.   ⌄      │     │ Owner             Forge  › │
│             Does the work.                   │     │ Does the work.             │
│ Runs on     Marie's Claude login             │     │ Runs on  Marie's Claude l. │
│ Created by  ◯B Bram · Oct 30                 │     │ Created by    Bram · Oct 30│
│ Followers   ◯A ◯M ◯B +2                      │     │ Followers              5 › │
│ ──────────────────────────────────────────── │     │ Following              [●] │
│ Project · Priority · Dispatcher · Mode …     │     │ DETAILS                    │
└──────────────────────────────────────────────┘     └────────────────────────────┘
```
- **Wie-strook** onder de titel op alle tabs: dezelfde twee regels als de kaart. Zo ziet een reviewer ook in *Discussion* wie wat doet (P-75).
- **Blok WHO** op de tab Ticket: de vier velden in vaste volgorde, met de uitlegzin eronder (11,5 px TEXT_2). *Project, Priority, Dispatcher, Mode, Model, Labels, Deadline, Attachments* volgen daarna ongewijzigd. *Run on my login* blijft in de actiebalk (`render_ticket_actions`).
- **In de reviewbalk** ziet een ander dan de responsible: *"Alexandra is responsible. You can decide too."* De knoppen blijven: één rol, iedereen mag alles (V1, V6).
- **Elders:** de header van een ticketkanaal (`aur-chat-001` §3) toont na de employees *◯A Alexandra · responsible*. De bestaande regel met *by …* in `fase-3` §3 vervalt alleen op de kaart.
- **Oudere server** (feature ontbreekt): geen wie-strook en geen Responsible. De kaart toont de huidige rij met *by …* (P-91).

## 2. US-A1: Responsible instellen, wijzigen, wissen

### 2.1 Desktop
- **Paneel:** de rij *Responsible* is een `board_picker`, als *Owner*.
- **Kaart:** ⋯ of rechtsklik voegt toe: *Make me responsible* (of *Remove me as responsible*) en *Set responsible…*
- **Toets:** `a` op het Board opent de picker voor de geselecteerde kaart. Ook in het palet (§8).
```
┌ Responsible for #42 ──────────────────┐
│ Follows up and reviews. An employee   │
│ does the work.                        │
│ ◯ You                              ✓  │
│ ◯ Alexandra                           │
│ ◯ Bram                                │
│ ────────────────────────────────────  │
│ ○ No one                              │
└───────────────────────────────────────┘
```
- **Volgorde:** eerst jij, dan actieve board members A–Z, dan *No one* (alleen als er iemand staat). Uitgeschakelde leden staan er niet tussen.
- **De keuze geldt direct**, zonder bevestiging. De toast zegt: *"Alexandra is now responsible for #42."* met **Undo** (5 s). Een Undo trekt ook de melding aan Alexandra weer in. Een toast met Undo is nieuw op desktop: ik vond die daar niet in de code, op mobiel bestaat hij al bij Start.
- **Activity en discussie:** *"Marie made Alexandra responsible"*, *"Marie changed responsible: Alexandra → Bram"*, *"Marie removed Alexandra as responsible"*. Tijd staat er zoals elders. De regel wekt niemand.
- **New ticket** krijgt het veld *Responsible*, standaard **You** (P-76, B-P6). *Create ticket* vanuit een bericht gebruikt hetzelfde formulier.

### 2.2 Mobiel
De rij *Responsible* in WHO opent een bottom sheet met dezelfde lijst (rijen ≥ 44 pt) en dezelfde uitlegzin. Lang indrukken op een kaart geeft dezelfde twee menu-items. De toast met Undo bestaat al voor Start. Op een tablet is dit gelijk.

### 2.3 Filter op het Board (P-77)
- **Desktop:** het dropdown **Responsible: Anyone**, als eerste persoonsfilter, vóór *Owner*. Opties: *Anyone, Me, No one*, actieve leden A–Z, en onderaan de groep *Disabled*, alleen met leden die nog tickets hebben. Onthouden per client (`ui.json`). De filterbalk wrapt al (`flex_wrap`), dus er is geen nieuw gedrag bij smalle vensters.
- **Mobiel:** de pill **Responsible: Anyone** in de horizontale strook, vóór de pill van de eigenaar. Die pill toont nu alleen *Anyone* en krijgt het voorvoegsel **Owner:**, omdat er anders twee keer *Anyone* staat.
- **Geen resultaat:** de bestaande regel: *"No tickets for Alexandra."* + **Clear filter**.

### 2.4 Uitgeschakelde board member (P-78, B-P13)
- Blijft zichtbaar als `Alexandra (disabled)`, op kaart en paneel, met gedimd monogram. Dat is tekst én vorm, nooit alleen kleur.
- Niet meer te kiezen. In het paneel staat onder de rij: *"Alexandra is disabled and gets no updates. Choose someone else."*
- Het veld wordt niet stil gewist: wie opvolgde, blijft historie. De ticket telt voor *Needs you* wel als *no one responsible*, zodat een review niet verdwijnt (§5.3).
- De bevestiging bij het uitschakelen krijgt de zin: *"Alexandra is responsible for 3 open tickets. They keep that name until someone changes it."* met de link **Show tickets**, die het Board opent op dat filter.

### 2.5 Fouten en lege staten
| Geval | Weergave |
|---|---|
| Geen responsible | `○ No one responsible` op de kaart; in het paneel **Make me responsible** als tekstlink |
| Done | Alleen-lezen, geen pijltje. Tooltip of subtitel: *"Done tickets are closed."* Menu-items ontbreken |
| Offline | Picker uit, tooltip *"Offline · reconnecting"* (bestaand) |
| Versieconflict | Toast: *"Marie changed this first. Bram is responsible now."* De rij toont de nieuwe stand; er is niets overschreven |
| Gekozen lid net uitgeschakeld | Toast: *"Alexandra is disabled and can't be chosen."* De picker ververst en blijft open |
| Andere serverfout | Toast met de reden; de rij springt terug |

## 3. US-A2: Volgen en ontvolgen

### 3.1 Desktop
De knop staat rechts in de wie-strook, dus op elke tab:
```
[ Follow ]                        niet volgen   (Outline)
[ ✓ Following ]                   wel volgen    (Outline, tekst ACCENT_TEXT); bij hover: Unfollow
[ ✓ Following ] tooltip: "You follow because you were @mentioned."
```
- **Reden** in de tooltip: *you created it, you're responsible, you were @mentioned, you chose to follow*. Automatisch volgen is daardoor uitlegbaar in plaats van verrassend (P-79).
- **Toets:** `f` op het Board; kaartmenu *Follow* of *Unfollow*; palet.
- **Rij Followers:** monogrammen, na drie `+n`. Een klik opent een lijst met naam en reden. Employees staan hier nooit: alleen board members volgen. *"No one follows this ticket."* als die leeg is.

### 3.2 Mobiel
De schakelaar *Following* staat in WHO, met de reden als subtitel. Daarnaast: kaartmenu *Follow* of *Unfollow*, en rijen in de Inbox bieden *Unfollow* bij lang indrukken. De rij *Followers* opent een sheet.

### 3.3 Toasts en regels
- Follow: *"You'll get updates on #42."* Unfollow: *"Unfollowed #42. A new @mention or becoming responsible makes you follow again."*
- **Responsible blijft volger** (P-80, B-P7): de knop is uit, met *"You're responsible for #42. Hand it to someone else to stop following."* Na het overdragen volg je nog; ontvolgen kan dan wel.
- Automatisch volgen geeft geen eigen melding en geen toast; wel de reden hierboven.
- Alleen het kanaal lezen maakt niemand volger (US-A2). In de composer verandert niets.
- **Done of gearchiveerd:** de knop blijft werken, met de subtitel *"Done tickets send no updates."* (voorstel in de analyse, randgevallen §6).

### 3.4 Fouten
Offline: knop uit met *"Offline · reconnecting"*. Mislukt schrijven: de knop springt terug, toast met de reden. Uitgeschakelde volger: staat in de lijst als *(disabled)* en krijgt niets.

## 4. US-A0 en US-A3: Persoonlijke Inbox

### 4.1 Wat een melding geeft
Dit is de hele lijst (P-81).

| Gebeurtenis | Rijtekst | Voor wie | Opent op | ◆ Needs you |
|---|---|---|---|---|
| Responsible gemaakt | *Marie made you responsible* | de nieuwe responsible | tab Ticket, blok WHO | nee |
| @vermelding | *Forge mentioned you* + fragment | de vermelde | het bericht (geflitst) in Discussion of kanaal; bij een beschrijving: tab Ticket | nee |
| Plan review | *Plan ready for your review* | responsible; zonder responsible: alle volgers | de planweergave | ja |
| | *Plan ready for review* | overige volgers | idem | nee |
| Review | *Ready for your review* | responsible; zonder responsible: alle volgers | de reviewpoort | ja |
| | *Ready for review* | overige volgers | idem | nee |
| Teruggestuurd | *Marie sent it back to Doing* | volgers | de feedback in Discussion | nee |
| Mislukt | *The main task failed* | volgers (◆ zoals bij Review) | tab Ticket, melding met **Start again** | ja voor responsible |
| Wacht op mijn login | *Waiting for your Claude login* | alleen de login-user, ook als die niet volgt | tab Ticket, knop Connect/Reconnect | ja |

Een eigen actie geeft nooit een melding aan jezelf. Een Undo binnen 5 s trekt de melding in.

**Nooit een melding** (P-82), ook niet voor volgers: run gestart, gestopt of hervat, delegaties en rapporten van subtasks, kolomwijziging naar Doing of Testing, testresultaat, wijziging van titel, prioriteit, labels of eigenaar, een bericht zonder vermelding, reacties, en een bewerkt bericht (§7). Die staan in Activity en de discussie, met de bestaande ongelezen-stip.

### 4.2 Samenvoegen (P-83)
- **Eén rij per ticket.** Een vermelding buiten een ticket: één rij per kanaal.
- Elke nieuwe gebeurtenis zet de rij bovenaan en op ongelezen; `+2` toont het aantal extra gebeurtenissen sinds je laatst las.
- **De kop toont de belangrijkste reden**, niet de laatste: login > mislukt > review/plan > vermelding > teruggestuurd > responsible. Zo verdrijft een vermelding nooit *Ready for your review*.
- **◆ verdwijnt vanzelf** als het werk gedaan is, ook door een ander. De rij zakt dan naar gelezen, met *"Marie handled this"* in het detail.

### 4.3 Desktop
Nieuwe tab **Updates**, tweede in de rij: *Needs you · Updates · Results · Failed · All*. `h`/`l` en Ctrl+Tab doorlopen de tabs al; de cijfers 1–4 blijven voor vragen in de Inbox.
```
┌ Inbox ─────────────────────────────────┬──────────────────────────────────────┐
│ [Needs you 2] [●Updates 3] [Results] … │ TICKET · 3 UPDATES                   │
│ ◆ #42 Login per board member    10:41  │ #42 Login per board member  [Review] │
│   Ready for your review  +2            │ ▢F Forge · on Marie's login          │
│ ● #47 Vault rotation            09:12  │ ◯ You · responsible                  │
│   Forge mentioned you: "Can you…"      │ ───────────────────────────────────  │
│ ● # general                     08:50  │ ◆ Ready for your review  10:41     › │
│   Atlas mentioned you: "The vault…"    │   Marie mentioned you    10:30     › │
│   #35 Fix inbox              yesterday │   Forge mentioned you    10:02     › │
│   Marie sent it back to Doing          │ [Review ›]         [Unfollow] [Read] │
└────────────────────────────────────────┴──────────────────────────────────────┘
```
- De rij gebruikt de bestaande vorm: ◆ of een stip plus tekst, nooit alleen kleur. Vet is ongelezen.
- **Enter** of `o` opent de hoofdreden op de juiste plek, net als de hoofdknop. Elke gebeurtenis in het detail is ook te openen.
- ◆-rijen staan ook in *Needs you*, onder de approvals en voorstellen; alle rijen staan in *All*. Eén rij dus op meerdere tabs, nooit dubbel in dezelfde lijst.
- **Badge op het sidebar-item:** wat nu telt (*Needs you* + ongelezen *Failed*) plus ongelezen *Updates*.
- De badge van Team chat blijft vermeldingen tellen (P-24). **Lezen is één stand:** het bericht lezen zet de Inbox-rij op gelezen, en andersom.

### 4.4 Lezen en wissen (US-A0, P-84)
- Een rij is gelezen als je hem opent of als de plek waarnaar hij wijst gelezen is. De tab openen is niet genoeg.
- *Mark all read* en *Clear all* gelden alleen voor jou. De bevestiging zegt erbij: *"Others keep theirs."* Tellers op desktop en mobiel volgen dezelfde persoonlijke stand. Verder ziet deze bugfix er niet anders uit.

### 4.5 Mobiel
```
┌ Inbox ──────────────────────────┐
│ 2 need you · 3 unread           │
│ NEEDS YOU  2                    │
│ ◆ #42 Login per board member    │
│   Ready for your review  +2 · 2m│
│ UPDATES  3                      │
│ ● #47 Vault rotation       1h   │
│   Forge mentioned you: "Can y…" │
│ ● # general                2h   │
│   Atlas mentioned you: "The v…" │
│ RUNNING 1                       │
│ DONE 4                          │
└─────────────────────────────────┘
```
- De sectie **Updates** staat onder *Needs you*. Tik = direct naar de plek; geen tussenscherm. *+2* opent bij lang indrukken de gebeurtenissen.
- **Veeg naar links** = wegklikken (zoals de resultaten nu). **Lang indrukken:** *Open · Mark read · Unfollow · Copy link*.
- De regel *"Seeing the list counts as reading it"* in `inbox_page.dart` blijft alleen voor *Done* gelden. Anders zou een blik op de Inbox alles wissen.
- **Push volgt dezelfde tabel** (P-85): titel *#42 Login per board member*, tekst = de reden. Geen berichtfragment in een push; dat staat pas in de Inbox, na de redactor. Eén ticket vervangt zijn vorige push (samenvoegen). Tik: `/board` → `/ticket/:id` op de juiste plek, via `openNotification`. De acties op een plan-push blijven.

### 4.6 Lege en foutstaten
| Geval | Desktop | Mobiel |
|---|---|---|
| Geen updates | *"No updates."* + infotekst: *"You get one when you're made responsible or @mentioned, and when a ticket you follow needs review, is sent back or fails."* | *"No updates."* als `_EmptyLine`, de zin in het lege scherm |
| Laden mislukt | *"Could not load the Inbox."* + **Try again** | idem |
| Offline | De laatste stand gedimd; openen kan, schrijven niet | `ConnectionBanner` |
| Doel weg | Ticket verwijderd: de rij verdwijnt. Bericht verwijderd: de rij blijft zonder fragment (*"Marie mentioned you (message deleted)"*) en opent de achterblijvende regel | idem |
| Geen feature | Tab of sectie verborgen | idem |

Elk fragment gaat door de vault-redactor vóór het wordt opgeslagen of verstuurd; de UI toont alleen wat de server geeft.

## 5. US-A4: My tickets

### 5.1 Plek (P-86, B-P2)
Toetsen en navigatie zijn schaars (P-51, D-03), dus geen nieuw sidebar-item en geen nieuwe mobiele tab.

- **Desktop:** Board krijgt bovenaan de schakelaar **[ Board ] [ My tickets ]**, dezelfde tabvorm als *List / Org chart / Guidelines*. Sneltoets **Ctrl/⌘+Shift+I**, naast Ctrl+I voor de Inbox; dezelfde vorm als Ctrl+Shift+V voor de Vault. In het palet: *"Go to My tickets"*. Ctrl+5 opent altijd het bord.
- **Mobiel:** in de tab Board de segmented control **( Board | My tickets )** onder de grote titel. Niet onder More, want dan is het drie tikken voor het overzicht dat je het vaakst nodig hebt.
- **Projectvenster:** geen schakelaar. Daar staat de tab Board al op één project.
- **Naam:** *My tickets*, niet *My issues*: de client zegt overal *ticket*.

### 5.2 Desktop
```
┌Sidebar┬ [ Board ] [●My tickets ]                     [Project: All ⌄]  [ ] Show Done ┐
│       │ [Responsible 7] [Created by me 12] [Following 9] [●Waiting on me 2]         │
│       │ ◆ ▲▲ #42 Login per board member [Review]  ▢F Forge · on Marie's login       │
│       │                                           ◯ You · responsible   2h  [Review›]│
│       │ ◆ ▲  #47 Vault rotation    [Plan review]  ▢A Atlas · on your login          │
│       │                                           ◯ You · responsible   5h  [Review›]│
└───────┴─────────────────────────────────────────────────────────────────────────────┘
```
- **Tabs en toetsen:** vier tabs op de bestaande filtertoetsen **1–4** (`Filter1..4`). `j`/`k` en ↑↓ bewegen, Enter of `o` opent. `a`, `f`, `c` werken zoals op het Board. Hier geen Board-pijltjes, want dit is een lijst.
- **Openen:** rechts `Panel::Ticket`; de lijst blijft staan. Rechtsklik of ⋯ = het kaartmenu.
- **Eerste tab bij openen:** *Waiting on me* als er iets wacht, anders *Responsible*. Dit doet de Inbox ook.
- **Sortering:** prioriteit, dan laatste update. **Done** staat uit, met *Show Done*. Filter *Project* (`ui.json`). De rij gebruikt het wie-blok van de kaart: dezelfde tekst overal.

### 5.3 De vier tabs
| Label | Toont | Lege staat |
|---|---|---|
| **Responsible** | Tickets waar ik responsible ben | *"You're not responsible for any open ticket. You'd follow up and review; employees do the work."* |
| **Created by me** | Tickets die ik maakte | *"You haven't created a ticket yet."* + **New ticket** |
| **Following** | Tickets die ik volg | *"You don't follow any open ticket. You follow one by creating it, becoming responsible, being @mentioned or pressing Follow."* |
| **Waiting on me** | Tickets met ◆ voor mij | *"Nothing waits for you."* |

**Waiting on me = de ◆ op de kaart** (P-87, B-P4, B-P5). Eén definitie, zodat de teller en de lijst nooit verschillen:
- Plan review, Review, mislukt, een open approval en *Choose owner*: voor de responsible. Zonder responsible, of met een uitgeschakelde: voor elke board member, zoals nu.
- Wacht op login: alleen voor de login-user, zoals nu (`ticket_needs`).

Hetzelfde geldt voor de filter *Needs you* en de badge van Board. Een ander kan nog steeds beslissen (§1.4); het telt alleen niet meer als werk van die persoon. Dit wijkt af van `fase-3` §3 (*"een review die iedereen mag doen telt voor iedereen"*).

### 5.4 Mobiel
```
┌ Board                      [+]  ┐
│ ( Board | ●My tickets )         │
│ [ Responsible 7 ][ Created 12 ] │  ← horizontaal scrollende chips, zoals de kolomchips
│ [ Following 9 ][●Waiting 2 ]    │
│ [All projects ⌄]  Show Done ○   │
│ ┌ TicketCard ─────────────────┐ │
│ │ #42 Login per board member  │ │
│ │ ◆ Needs you · review        │ │
│ │ ▢F Forge · on Marie's login │ │
│ │ ◯ You · responsible         │ │
│ │                 [ Review ]  │ │
│ └─────────────────────────────┘ │
└─────────────────────────────────┘
```
Gelijke tabs, sortering en lege teksten. Het gebruikt de bestaande `TicketCard`, met dezelfde knop, swipes en het menu; op een tablet ook. Geen nieuwe rijvorm. Chiplabels: *Responsible, Created by me, Following, Waiting on me*; *Created by me* mag afkappen als *Created* bij grote tekst, met het volledige label voor de screenreader.

### 5.5 Laden en fouten
Laden: 3–4 skeletrijen, zonder animatie bij *reduce motion*. Fout: *"Could not load your tickets."* + fouttekst + **Try again**. Filter zonder resultaat: *"No tickets in aurelius."* + **Clear filter**. Offline: laatste stand gedimd, acties uit. Oudere server: geen schakelaar.

## 6. US-A5: Copy link

### 6.1 Waar
| Op | Desktop | Mobiel |
|---|---|---|
| **Ticket** | Kaartmenu en paneelmenu ⋯ (*Copy link* bestaat, krijgt de echte link); toets `c` op het Board en in My tickets | Kaartmenu en ⋯ op de ticketpagina: **Copy link** naast het bestaande *Copy #42* |
| **Bericht** | De balk en het menu per bericht (🔗 *Copy link*, zoals ontworpen in `aur-chat-001` §5), via Tab + Enter ook met het toetsenbord | Lang indrukken: **Copy link** na *Copy* |
| **Inbox-rij** | Detail, ⋯ | Lang indrukken |

Geldt voor berichten in Discussion, kanalen en threads. Niet op systeemregels, een bericht in de wachtrij of tijdens het streamen, of de regel van een verwijderd bericht. Toast: *"Link to #42 copied."*, *"Link to message copied."*

### 6.2 De link (P-88, B-P10)
```
aurelius://<server-id>/ticket/42
aurelius://<server-id>/ticket/42/message/<message-id>
aurelius://<server-id>/channel/<conversation-id>/message/<message-id>
```
Geen token, geen serveradres. De server-id is nodig omdat een client meerdere servers kent. In een bericht wordt zo'n link een pill (`#42 Login per board member`); is die niet te lezen, dan *"Unavailable link"*. Buiten de apps blijft het een kale URL, want er is geen webclient (V4).

### 6.3 Openen
- **Ticket:** desktop = Board met `Panel::Ticket`; mobiel = `/ticket/:id`.
- **Bericht:** de juiste tab, gescrold naar het bericht, 1,5 s `ACCENT_HALO` (`aur-chat-001` §5). Een threadantwoord opent ook de thread.
- **Niet ingelogd:** eerst inloggen met *"Sign in to open the link."*, daarna door naar de plek. De link blijft alleen in het geheugen, nooit op schijf, en is weg als de app sluit.
- **Done of gearchiveerd:** opent gewoon, alleen-lezen.

### 6.4 Meldingen (P-89)
Op desktop in het paneel (de lege staat van `render_ticket_panel`), op mobiel op de pagina `/ticket/:id`. Nooit een kale fout.

| Geval | Tekst | Actie |
|---|---|---|
| **Onbekend ticket** | **Ticket not found** · *"#42 doesn't exist on Segrusa. It may have been deleted from the Backlog."* | **Go to Board** |
| Bericht weg (ticket bestaat) | Ticket opent, toast: *"That message no longer exists."* | geen |
| Bericht verwijderd | Springt naar de regel *Message deleted* | geen |
| Geen rechten (kan bij één rol nu niet, wel voorzien) | **No access** · *"You don't have access to this ticket."* | **Go to Board** |
| Andere, bekende server | *"This link is for Thuis. You're on Segrusa."* | **Switch to Thuis** · Cancel |
| Onbekende server | *"This link is for a server this device isn't paired with."* | **Servers** |
| Server onbereikbaar | *"Can't reach Segrusa. Trying again…"* (`ConnectionBanner`) | Opent vanzelf zodra hij terug is; **Cancel** |
| Ongeldige link | Toast: *"This isn't a valid Aurelius link."* | geen |
| Oudere server | *"This server can't open links yet. Update the server."* | **Updates** |

## 7. US-A6: Bericht bewerken en verwijderen

### 7.1 Wie en waar (P-90, B-P11)
- **Alleen de auteur** ziet *Edit* en *Delete*. Anderen zien de items niet, ook niet uitgeschakeld.
- **Wel:** eigen berichten in Discussion, kanalen en threads. **Niet:**
  - berichten van employees en systeemregels (US-A6);
  - berichten in employee- en taakgesprekken, want die zijn de opdracht aan een run;
  - een bericht dat de feedback van een beslissing is (*Send back*, *Pas aan*). Tooltip of subtitel: *"This message is the feedback of a decision."*
  - alles in Done of een gearchiveerd kanaal (read-only, `fase-2` §1, `aur-chat-001` §8).
- Een bericht in de wachtrij houdt zijn bestaande *Delete*.

### 7.2 Bewerken, desktop
In de balk en het menu per bericht. De rij wordt op zijn plaats een invoerveld:
```
◯A Alexandra · board member                          10:02
┌───────────────────────────────────────────────────────┐
│ Please keep the argon2 params configurable.|          │
└───────────────────────────────────────────────────────┘
Editing wakes no one. An employee that already read this
keeps the original.                   [Cancel] [Save ⏎]
```
- **Toetsen:** dezelfde als de composer op die plek (in de Team chat Enter = opslaan, Shift+Enter = nieuwe regel, O-2); Esc = annuleren (bestaande `Composer > Input`). Geen nieuwe toetsen.
- **Nieuwe vermelding:** onder het veld, in `unknown_warning`-stijl: *"@Bram won't be notified by an edit. Send a new message to reach them."* (P-92, B-P12).
- Niets gewijzigd: Save sluit zonder te schrijven. Leeg: Save uit, *"To remove it, delete the message."*

### 7.3 Bewerken, mobiel
Lang indrukken → **Edit**. De composer onderaan vult zich, met daarboven *"Editing message · ✕"* en dezelfde hint. De verzendknop wordt ✓. De lijst scrolt naar het bericht en dimt de rest niet. Een concept in de composer blijft bewaard en komt terug na het annuleren.

### 7.4 Het resultaat
```
◯A Alexandra · board member           10:02 · edited     ← 11 px TEXT_2, tooltip "Edited Nov 2, 10:14"
    Please keep the argon2 params configurable.

◯A Alexandra · board member           10:05
    ⊘ Message deleted                                    ← TEXT_2, cursief, geen ballon
```
- **edited** is klikbaar: de lijst met eerdere versies (desktop popover, mobiel sheet; alleen-lezen, voor elke board member; B-P14). Hetzelfde patroon als de versielijst bij guidelines, zonder Restore. Ook oude versies gaan door de vault-redactor.
- **Geen ongelezen** en geen *New messages*-regel door een bewerking; live bijwerken wel. In de stroom ziet iedereen de nieuwe tekst.
- **Verwijderd:** naam en tijd blijven, de tekst weg, reacties en acties weg. De tekst blijft in de audit, niet in de client.
  - Thread: `↳ 3 replies` blijft staan en de antwoorden blijven open.
  - Een pill `→ #52` blijft, want dat ticket bestaat.
- **Bevestiging verwijderen:** *"Delete this message? A line stays that says it was deleted. The text stays in the audit log."* **Cancel · Delete** (danger). Met een pill erbij: *"#52 keeps its copy."* Geen Undo.
- **Screenreader:** *"Message by Alexandra at 10:02, edited."* en *"Deleted message by Alexandra at 10:05."*

### 7.5 Fouten
| Geval | Weergave |
|---|---|
| Offline | Save uit; de tekst blijft staan. *"Offline · reconnecting"* |
| Op een ander apparaat gewijzigd | Save weigert: *"You edited this on another device."* **Compare** · **Discard mine** (P-64) |
| Intussen verwijderd | *"This message was deleted."* + **Copy my text**, sluit het veld |
| Kanaal intussen dicht (`channel_archived`) | Nooit een kale fout: de read-only-balk, en het veld toont **Copy my text** |
| Te lang | Teller als in de composer; Save uit |
| Mislukt | Veld blijft open, toast met reden |
| Geen feature | Items verborgen; *edited* niet getoond |

## 8. Toetsen en navigatie: botsingscontrole
Getoetst aan `SHORTCUTS` en `overlay_bindings` in `keymap.rs`.

| Nieuw | Context | Label | Vrij? |
|---|---|---|---|
| `a` | Board en My tickets (eigen context, zoals `BOARD_KEYS`), niet in een tekstveld | Set responsible… | Ja. `a` wordt alleen in Settings gebruikt (`SettingsPushKey`); `alt-a` blijft *Give task* |
| `f` | idem | Follow / Unfollow | Ja |
| `c` | idem | Copy link | Ja. `c` wordt alleen in Settings gebruikt |
| **Ctrl/⌘+Shift+I** | overal | Go to My tickets | Ja. Bezet met Shift: A, B, D, E, G, H, K, M, N, O, P, R, U, V, X, Y |
| `1`–`4` | My tickets | Tab kiezen | Bestaande `Filter1..4` (NAV) |
| Tabs | Board-pagina | Board ↔ My tickets | Bestaande acties `NextTab`/`PrevTab`, die de Board-pagina nog moet afhandelen. `h`/`l` claimt het Board niet; `←`/`→` blijven van de kaarten |

Alles ook in het palet (Ctrl+K): *Go to My tickets, Set responsible…, Make me responsible, Follow ticket, Copy link*. Geen nieuwe cijfertoets en geen nieuw sidebar-item. Geen nieuwe mobiele tab (D-03: vol op 5). Bewerken en verwijderen krijgen geen eigen toets: Tab + Enter opent het bestaande berichtmenu (`MESSAGE_ROW`).

**Bestaande keuzes waar dit document van afwijkt of op aansluit** (de eerste twee zijn B-punten):
- `fase-3` §3: *by …* op de kaart (B-P3).
- `fase-3` §3: *Needs you* voor iedereen (B-P4).
- `aur-chat-001` §3: het woord *assignees* in de header van een ticketkanaal is daar de employees. Dat blijft intern; in de UI staat nu *Responsible* apart.

Niet aangeraakt: de P-21-regel (bericht zonder @ gaat naar de eigenaar), de beslisknoppen, *Run on my login*, Start en de kolomregels (V2).

## 9. Toegankelijkheid (aanvulling op Fase 1 §15)
- Rol staat altijd in woorden. Cirkel of rechthoek is extra, net als kleur.
- Wie-blok voorgelezen als één zin (§1.3), niet als losse namen.
- *Following* is een `Switch` met de reden als beschrijving. Op mobiel horen *Follow* en *Set responsible* ook in `customSemanticsActions`.
- Inbox-rij: *"Ticket 42, Login per board member, ready for your review, 2 more updates, unread, 2 minutes ago."*
- De flits bij een geopende link duurt niet bij *reduce motion*: het bericht krijgt dan 1,5 s een rand.
- Tikdoelen ≥ 44 pt; chips van My tickets zijn knoppen met de teller in het label.

## 10. Ontwerpkeuzes
| ID | Vraag | Keuze |
|---|---|---|
| P-71 | Welke naam voor de menselijke verantwoordelijke? | **Responsible**. *Owner*, *Runs on* en *Created by* blijven (§1.1) |
| P-72 | Hoe zie je mens en AI uit elkaar? | Twee monogrammen op een kaart: rechthoek voert uit, cirkel volgt op, altijd met rolwoord |
| P-73 | Waar staat de login op de kaart? | In de regel van de uitvoerder: *Forge · on Marie's login*, altijd, ook als het je eigen login is |
| P-74 | Blijft de aanmaker op de kaart? | Nee; in tooltip, menukop, paneel en filter (B-P3) |
| P-75 | Zie je de wie-velden ook in Discussion? | Ja, wie-strook onder de titel op alle tabs |
| P-76 | Standaard in New ticket? | *You*, aan te passen (B-P6) |
| P-77 | Waar staat het filter? | *Responsible* als eerste persoonsfilter; mobiele pill *Owner:* krijgt voorvoegsel |
| P-78 | Uitgeschakelde responsible? | Blijft met *(disabled)*, niet kiesbaar, niet stil gewist; telt voor *Needs you* als niemand |
| P-79 | Waar zie je waarom je volgt? | Tooltip op de knop, subtitel op mobiel |
| P-80 | Mag de responsible ontvolgen? | Nee, eerst overdragen (B-P7) |
| P-81 | Wat geeft een melding? | Alleen de tabel in §4.1 |
| P-82 | Wat nooit? | Run-updates en de lijst in §4.1; er is geen instellingenmatrix per melding |
| P-83 | Hoe samenvoegen? | Eén rij per ticket, kop = belangrijkste reden, `+n`, ◆ lost vanzelf op |
| P-84 | Wanneer gelezen? | Bij openen van de rij of het doel; de lijst zien telt niet (behalve *Results/Done*, zoals nu) |
| P-85 | Wat staat er in een push? | Nummer, titel, reden; geen berichtfragment; één push per ticket |
| P-86 | Plek van My tickets? | Schakelaar in Board op beide clients, Ctrl+Shift+I op desktop |
| P-87 | Wat is *Waiting on me*? | Precies de ◆ op de kaart |
| P-88 | Vorm van de link? | `aurelius://<server-id>/…`, zonder token (B-P10) |
| P-89 | Onbekend ticket? | Eigen staat op de plek van het ticket, met uitweg; nooit een kale fout |
| P-90 | Wat is bewerkbaar? | Eigen berichten in Discussion, kanalen, threads; niet in gesprekken, niet in Done, niet de feedback van een beslissing |
| P-91 | Oudere server? | Functies verborgen (`FEATURES`), kaart als nu met *by …* |
| P-92 | Nieuwe vermelding in een bewerking? | Geen melding, wel een zichtbare hint (B-P12) |

## 11. B-punten
Mijn voorstel staat erbij; Orion of Atlas beslist.

| ID | Vraag | Voorstel | Waarom het uitmaakt |
|---|---|---|---|
| **B-P1** | Heet het in de UI *Responsible*? | Ja | Bepaalt alle teksten. *Assignee* wekt de indruk die V2 uitsluit |
| **B-P2** | *My tickets* in plaats van *My issues* (opdracht en US-A4)? | Ja | De client kent geen *issue*; twee woorden voor één ding verwarren |
| **B-P3** | Mag *by …* van de kaart, tegen `fase-3` §3 in? | Ja | Vier namen passen niet. Wie de aanmaker zoekt, heeft tooltip, paneel en filter |
| **B-P4** | Telt ◆ voortaan alleen voor de responsible, tegen `fase-3` §3 in? | Ja; zonder (actieve) responsible voor iedereen | Anders heeft de responsible geen effect en blijft de badge voor iedereen tellen. Beslissen blijft voor iedereen mogelijk |
| **B-P5** | *Waiting on me* breder dan US-A4 (ook mislukt, approval, Choose owner, login)? | Ja, gelijk aan ◆ | Twee definities geven verschillende tellers |
| **B-P6** | New ticket standaard op *You*? | Ja; *No one* blijft kiesbaar | Met een lege standaard blijft *Responsible* in My tickets leeg. Nadeel: je wordt soms ongevraagd verantwoordelijk |
| **B-P7** | Responsible kan niet ontvolgen (nuance op US-A2)? | Ja | Anders mist de verantwoordelijke de eigen reviewmelding |
| **B-P8** | Krijgen overige volgers ook een rij bij Review en Plan review? | Ja, zonder ◆ | US-A3 zegt *volger*; zonder ◆ blijft het rustig. Alternatief: alleen de responsible, stiller maar minder zichtbaar |
| **B-P9** | Vervangen ticketrijen de huidige meldingen *Plan to review*, *Result ready* en *Task failed* van de hoofdtask van een ticket? | Ja, vervangen; taken zonder ticket blijven gelijk | Anders twee rijen voor één gebeurtenis, en *Result ready* is een run-update. **Titan:** kan dit binnen `notification_for`? |
| **B-P10** | Linkvorm met server-id en ticketnummer? Registratie van `aurelius://` op elk platform? | Ja; **Titan** bevestigt of een nummer stabiel genoeg is en wat de server-id is | Nummers zijn leesbaar; de OS-registratie en de koude start zijn de echte kosten |
| **B-P11** | Geen bewerken in employee- en taakgesprekken? | Ja | Dat bericht is de opdracht aan een run; bewerken zou de historie vervalsen |
| **B-P12** | Nieuwe vermelding in een bewerking: geen melding en geen automatisch volgen (uitbreiding van de aanname in US-A6)? | Ja, met hint | Voorkomt meldingen en lussen via bewerkingen; de hint vermijdt een stille mislukking |
| **B-P13** | Uitgeschakeld lid: niet automatisch wissen? | Ja, wel de zin bij het uitschakelen (§2.4) | Historie blijft; de review verdwijnt niet door B-P4 |
| **B-P14** | Oude versies van een bewerkt bericht zichtbaar voor elke board member? Tekst van een verwijderd bericht niet? | Ja op beide (de analyse zegt *zichtbare historie*, US-A6 alleen audit) | Bewerken is dan controleerbaar; verwijderen blijft echt weg uit de client. Kost een extra read: **Titan** |

Te bevestigen door Titan, zonder B-punt:
- Leest een latere run de bewerkte tekst of de oorspronkelijke? De hint in §7.2 gaat uit van: wat al gelezen is, blijft.
- `ticket.list` of een eigen read voor de tabs in My tickets? Het Board laat Done maar beperkt zien.
- Mag een vermelding in de beschrijving? Voorstel: alleen board members in de autocomplete, en het wekt geen employee (US-A2 noemt alleen *genoemd*; de randgevallen in §6 van de analyse zeggen dat een mention van een employee wekt).

## 12. Dekking
| Story | Desktop | Mobiel | Leeg en fout |
|---|---|---|---|
| US-A0 Eigen leesstand | §4.3–4.4 | §4.4–4.5 | §4.6 |
| US-A1 Responsible | §1.3–1.4, §2.1, §2.3–2.4 | §1.3–1.4, §2.2–2.4 | §1.3 (tabel), §2.3, §2.5 |
| US-A2 Volgen | §3.1, §3.3 | §3.2–3.3 | §3.1, §3.4 |
| US-A3 Inbox | §4.1–4.3 | §4.5 | §4.6 |
| US-A4 My tickets | §5.1–5.3 | §5.1, §5.3–5.4 | §5.3, §5.5 |
| US-A5 Copy link | §6.1, §6.3 | §6.1, §6.3 | §6.4 |
| US-A6 Bewerken | §7.1–7.2, §7.4 | §7.1, §7.3–7.4 | §7.2, §7.5 |
