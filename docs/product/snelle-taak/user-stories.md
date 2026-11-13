# Snelle taak (ticket #53)

*2026-11-13 · Nova (BA) · keuze van Sebastien: optie A*

## Waarom

Elk werk is sinds ticket #16 een ticket. Elk ticket doorloopt triage, Plan en Plan review, de delegatieketen (Orion → Titan → developer → Trace), Testing, Review en een handmatige merge. Voor een kleine ad-hoc aanpassing is dat te traag en te veel werk. Een **snelle taak** blijft een ticket, zodat het op het bord staat, een historiek heeft en dezelfde login gebruikt. Alleen de zware stappen vallen weg.

## Beslissingen van Sebastien (2026-11-13)

| # | Beslissing |
|---|---|
| D1 | Optie A: een snelle taak is een soort ticket, geen losse taak zonder ticket. |
| D2 | Er wordt alleen getest als daarom gevraagd wordt (geen Testing-stap, geen volledige suite). |
| D3 | Done blijft een beslissing van een board member: een snelle taak eindigt altijd in Review. |
| D4 | De medewerker werkt altijd op een eigen branch. Na de goedkeuring (Done) wordt die automatisch in main gemerged. |

D4 wijkt af van de productdefinitie multi-user ("er wordt niets automatisch gemerged", `docs/product/multi-user.md`). Voor snelle taken gaat de expliciete keuze van Sebastien voor. Voor gewone tickets blijft de oude regel gelden.

## Flow

```
Nieuw (snelle taak) ──► Doing ──► Review ──► Done ──► merge naar main
                          ▲          │
                          └─ Send back┘            merge mislukt ──► Review + melding
```

Er is geen Backlog/Todo-wachtrij, geen triage, geen Plan review en geen Testing.

## User stories

### ST-01 Snelle taak aanmaken
Als board member wil ik een kleine aanpassing in één stap aan een medewerker geven, zodat het werk meteen begint.

- AC1: In "Nieuw ticket" en "Give work" (desktop en mobiel) staat een keuze **Snelle taak**. Ook het commandopalet heeft een eigen ingang.
- AC2: Verplichte velden zijn de beschrijving en de medewerker. De titel komt uit de eerste regel van de beschrijving als die leeg is. Het project is het huidige project (workspace). Standaard staat de laatst gekozen medewerker ingevuld.
- AC3: Na het aanmaken staat de kaart meteen in **Doing**, met de gekozen medewerker als eigenaar en de aanmaker als responsible. Er komt geen triage-job.
- AC4: De run draait in **Auto**-modus en krijgt nooit Plan mode of Plan review.
- AC5: Bijlagen en een plak-afbeelding werken zoals bij een gewoon ticket.

### ST-02 Werken zonder delegatieketen
Als board member wil ik dat de gekozen medewerker het werk zelf doet, zodat er geen overdrachten zijn.

- AC1: De instructies van de run zeggen dat het een snelle taak is: zelf uitvoeren, niet delegeren, alleen gerichte tests als daarom gevraagd wordt, en op de eigen branch werken (ST-04).
- AC2: `delegate_task` wordt geweigerd in de hoofdtaak van een snelle taak, met de melding "Snelle taak: doe dit zelf of vraag de board member om er een gewoon ticket van te maken". Escaleren (`hand_over_task` naar de baas) blijft mogelijk.
- AC3: Vindt de medewerker dat het werk te groot is, dan meldt die dat in de discussie met een voorstel. De board member beslist (zie ST-06).

### ST-03 Testen alleen op vraag
- AC1: Een snelle taak gaat bij het rapporteren van Doing naar **Review**, ook als er een tester is ingesteld (de Testing-kolom wordt overgeslagen).
- AC2: Staat er in de beschrijving of in een bericht van de board member een vraag om te testen, dan draait de medewerker zelf de gevraagde tests en zet het resultaat in het rapport. De volledige suite draait alleen als die uitdrukkelijk gevraagd wordt.
- AC3 (aanname A3): er komt geen aparte "Test"-knop.

### ST-04 Eigen branch
- AC1: Elke snelle taak krijgt een eigen branch `quick/ticket-<N>` met een eigen worktree, afgetakt van de actuele `main` van de projectmap op het moment van aanmaken.
- AC2: De medewerker commit daar zelf. Hij raakt `main` niet aan en pusht niet.
- AC3: Het rapport in Review noemt de branch, de commits (kort) en wat er gewijzigd is. Heeft de medewerker niets gecommit, dan zegt het rapport dat.

### ST-05 Goedkeuren en automatisch mergen
Als board member wil ik met één klik goedkeuren, zodat het werk zonder extra stap in main komt.

- AC1: In Review staan de gewone knoppen Done en Send back. Bij een snelle taak heet Done **Goedkeuren en mergen**.
- AC2: Na Done wordt `quick/ticket-<N>` in de lokale `main` van de projectmap gemerged (`--no-ff`, commitbericht met het ticketnummer). Daarna worden de worktree en de branch opgeruimd.
- AC3: Er wordt niet gepusht (aanname A1).
- AC4: Mislukt de merge door een conflict, een vuile `main` of een ontbrekende branch, dan wordt `main` niet gewijzigd en wordt de merge afgebroken. De kaart komt terug in **Review** met een systeemregel die de reden en de conflicterende bestanden noemt. De board member kiest dan Send back (de medewerker lost het op zijn branch op) of Done opnieuw.
- AC5: Zonder commits op de branch is Done een gewone Done, zonder merge.
- AC6: Het resultaat van de merge (commit-hash of fout) komt als regel in de ticketdiscussie en in de activiteit.

### ST-07 Herkenbaar op het bord
- AC1: Een snelle taak heeft een label/icoon **Snel** op de kaart en in de lijsten (desktop en mobiel).
- AC2: Het bord en My tickets hebben een filter **Snelle taken**.
- AC3: Een oudere client toont een snelle taak als een gewoon ticket. Een oudere server biedt de keuze niet aan (feature-vlag).

## Business rules

- BR1: Alleen een board member maakt een snelle taak aan, keurt die goed en zet die op Done. Apps mogen dat niet (NOT_FOR_APPS).
- BR2: De run gebruikt de Claude-login van de aanmaker, net als bij elk ticket.
- BR3: Een snelle taak heeft altijd een project met een projectmap die een git-repository is. Zonder repo kan een snelle taak niet aangemaakt worden ("Dit project heeft geen git-map").
- BR4: Cancel (terug naar Backlog) stopt de run en ruimt worktree en branch op na een bevestiging, want niet-gemergd werk gaat dan verloren.

## Edge cases

| Situatie | Gedrag |
|---|---|
| `main` heeft niet-gecommitte wijzigingen bij Done | Geen merge, Review + melding "main is niet schoon" (ST-05 AC4) |
| Twee snelle taken raken dezelfde bestanden | De eerste merge lukt, de tweede geeft een conflict → ST-05 AC4 |
| De gekozen medewerker staat op pauze of heeft geen quota | Zoals bij een gewoon ticket in Doing: wachten met de bestaande melding |
| De medewerker stelt een vraag in plaats van te rapporteren | Bestaand gedrag (#14/BUG-Q): Review met de vraag, antwoord in de discussie |
| Project wordt gearchiveerd met een open snelle taak | Bestaand gedrag voor tickets; de branch blijft staan |

## Aannames (te bevestigen of te corrigeren)

- A1: Auto-merge is alleen lokaal. Pushen blijft een aparte beslissing, zoals nu.
- ~~A2~~: vervallen. Sebastien (2026-11-13): omzetten naar een gewoon ticket is niet nodig.
- A3: Testen gebeurt alleen via tekst ("test X"), zonder aparte knop.
- A4: Een snelle taak hoort altijd bij een project (BR3). Werk zonder project blijft een chat.

## Buiten scope

Terugkerende taken (blijven zonder ticket, besluit #16), een snelle taak omzetten naar een gewoon ticket, push naar origin, PR's op GitHub en een Testing-stap op vraag.

## Open voor Titan (technisch)

- Wie voert de merge uit: de daemon zelf (git in een side effect na de commit van `ticket.accept`) of een korte run van de medewerker met een vaste merge-opdracht? De AC's gelden voor beide.
- Waar de soort opgeslagen wordt (bv. `tickets.kind` = `normal|quick`, migratie met CHECK) en de featurevlag in `FEATURES`.
