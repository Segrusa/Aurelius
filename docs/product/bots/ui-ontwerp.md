# UI-ontwerp: employees als bots

- **Basis:** [README.md](README.md), [user-stories.md](user-stories.md), `clients/ts/app/CLAUDE.md`.
- **Regels:** alleen Astryx-componenten en StyleX, kleuren via tokens, logica in `*-logic.ts`, `HIDDEN_FEATURES` blijft leidend, tickets als `#21`.
- **Nummering:** ontwerpkeuzes zijn P-200 en verder, zodat ze niet botsen met eerdere fases.
- **UI-taal:** Engels, zoals de rest van de clients.

## Stap 1: beschrijving (gebouwd)

- **Employee-dialoog:** `TextInput` *Description* boven *Instructions*, met de uitleg *"Shown to other employees so they know when to hand work to this one. Empty: they see the first line of the instructions."*
- **Employee-pagina:** sectie *DESCRIPTION* boven *INSTRUCTIONS*, alleen als er een is.

## Stap 2: provider, model, effort en toegang (gebouwd)

- **Employee-dialoog (bewerken):** `Selector` *Provider* en *Model*, `SegmentedControl` *Effort* en *Access*. Inherit is de standaard en toont wat dat oplevert (*Inherit · Sonnet 4.5*). Een andere provider beperkt de modellenlijst; een model dat die provider niet heeft gaat terug naar Inherit.
- **Employee-dialoog (nieuw):** alleen *Access*. Provider, model en effort kies je daarna via Edit.
- **Employee-pagina:** tweede `MetadataList` met Provider, Model, Effort en Access; wijzigen gaat via Edit.
- **Toegangsniveaus:** *Inherit* (alle werktools), *Read only* (`Read`, `Glob`, `Grep`), *Read and edit* (daarbij `Edit`, `Write`). Onder Inherit hebben runs geen MCP-servers van de gebruiker, plugins of artifacts; browser en scherm houden hun eigen schakelaar, de bridge blijft zodat ze kunnen overdragen.

## Stap 4: run card, interrupt card en odometer (gebouwd)

- **Run card (U1):** onder een post of `Reported`-regel, alleen de laatste van die run in de lijst. `Collapsible` met `StatusDot` (pulseert tijdens de run), toestand, aantal tool calls, mislukt en tokens; *Cancel* zolang hij loopt. Open: `ChatToolCalls` uit `run.get_activity`.
- **Interrupt card (U2):** één component (`ApprovalCard` plus `ReplyCard`) in de discussie, de Team chat en de Inbox. Een vraag of approval staat in de discussie via `ApprovalView.ticket_id` / `discussion_id`. Een post met `awaiting_user` zonder open approval krijgt een antwoordveld; in een kanaal gaat dat via `group_chat.send`, in een ticket via `ticket.post`.
- **Odometer (U4):** `MetadataList` in de ticketkop (Calls, Tokens in, Tokens out, Cost), één regel in de kop van een kanaal, en een tabel per board member op het Usage-scherm. Geen rij of odometer zolang er niets verbruikt is.

## Stap 13: gesprekken met employees (gebouwd)

- **Chat:** een knop op elke actieve rij van de Employees-lijst en in de kop van de pagina van de employee. Hij opent het gesprek van dit board member, dat de eerste keer wordt gemaakt (`/talk/<conversation>`).
- **Zijbalk:** groep "Chats" boven Live, in de gewone zijbalkstijl, de laatste vier met de naam en het merkteken van de employee en een stip bij ongelezen. Een gesprek zonder berichten staat er niet in. In de ingeklapte zijbalk niet.
- **Scherm:** dezelfde berichtrijen, approvalkaarten en het invoerveld als de Team chat, maar zonder threads, reacties en `@`-suggesties. Een gepauzeerde of gearchiveerde employee: geen invoerveld, wel een regel die het zegt.
- **Berichten uit een ticket:** een regel met het label FROM A TICKET en de link. De aanleiding van een update uit Review is onzichtbaar; wat de board member leest is het bericht van de employee zelf.

## Latere stappen

| Stap | Scherm | Astryx | Opmerking |
|---|---|---|---|
| 6 | Handoff | geen nieuw scherm: een gewoon bericht van de employee ("Flux handed over to Bo" met de boodschap eronder) | Een vastgehouden handoff is een grijze `limit`-regel met `action` `handoff_held` en de tekst "Handoff held: …"; de client toont de tekst. Besluit B7 |
| 7 | Waiting indicator | `Text` onder de thread-titel | Gevoed uit inbox-items |
| 7 | Scheduled | `Table` | `Heartbeats` blijft in `HIDDEN_FEATURES`; `areasOf` voor `recurring_task` en `heartbeat` |
| 8 | Tools per employee | `Table` met `Switch` en approval-`Checkbox` | |
| 9 | Startprofielen | `Selector` bovenaan de dialoog | Kiezen vult de velden in; bewerken blijft kunnen |
| 11 | Employee-pagina | `TabList` Inbox, Memory, Settings; `StatusDot` in de lijst | Status uit `activity_state` |
| 12 | Living Office | lazy route, three.js | Eigen StyleX; geen externe modellen |

## Keuzes

| # | Vraag | Keuze |
|---|---|---|
| P-200 | Waar staat de beschrijving? | In de dialoog en op de pagina, niet in de lijst: de lijst toont rol en titel al |
| P-201 | Laat de UI de scoped view zien? | Nee. De UI toont altijd de hele thread (B2); de scoped view bestaat alleen in de prompt |
| P-202 | Een lege beschrijving | Geen plaatshouder op de pagina. De terugval op de instructies geldt alleen voor de prompt, zodat de gebruiker ziet wat hij zelf schreef |
| P-203 | Eén run, meerdere posts | Eén kaart, op de laatste post van de run in de lijst, zodat tokens niet dubbel lijken |
| P-204 | Wanneer wacht een post op iemand | Open approval of vraag van zijn run, of een post waarmee de hoofdtaak van een ticket op een vraag eindigde (`group_chat::asks_question`, dezelfde regel als `ends_on_question`) |
| P-205 | Oude runs zonder model calls | `model_calls` 0 betekent onbekend: de odometer laat Calls dan weg, de tabel toont een streepje |
| P-206 | Hoe heet de actie | "Chat", en de bestaande link "Talk in Team chat" blijft ernaast: dat is een andere plek |
| P-207 | Hoeveel gesprekken in de zijbalk | Vier, op volgorde van de laatste activiteit; favorieten of een volledige lijst bouwen we niet |
| P-208 | Melding bij een bericht uit een ticket | Alleen ongelezen (stip in de zijbalk), geen notificatie: P-82 verandert niet |
| P-209 | Interrupt-kaarten in een gesprek | Alleen voor een echte beslissing: de employee vraagt gewone dingen in zijn tekst. Dat staat in de richtlijn van de run; de server dwingt het niet af |
| P-210 | Wie kan een gepland bericht terugnemen | Alleen een board member, op de Scheduled-pagina met een bevestiging. Een employee heeft geen tool om het terug te nemen: een lopende planning op de pagina is genoeg om in te grijpen |
| P-211 | Mag de gebruiker de scratchpad bewerken | Nee, alleen lezen. Het is de eigen notitie van de employee; instructies horen in de instructies en het geheugen is een andere plek |
| P-212 | Waar staat de wachtrij | Onder de titel van een ticket of gesprek, alleen als er iemand wacht of iets gepland staat. Geen lege regel, en geen aparte pagina |
| P-213 | Een plek in de navigatie | Scheduled is één pagina voor alles wat later vanzelf komt; de instellingen van recurring tasks blijven waar ze waren |
| P-214 | Hoe ziet de toollijst eruit | Geen `Table` met `Switch`, maar rijen met twee `CheckboxInput`s in een uitklapbaar blok in de dialoog: aan of uit, en "Asks first". Dicht op deze breedte, en het zijn lijsten die je zelden opent. De dialoog toont de samenvatting op de knop |
| P-215 | Wat kan niet uit | Vragen aan de gebruiker, een plan inleveren, en van de bridge wat een run nodig heeft om te lezen en te rapporteren: eigen geheugen en scratchpad, lezen van tickets en discussies, `post_to_discussion`, `hand_off`, `tell_responsible`. Anders klopt het platformblok niet meer |
| P-216 | Lijst of uitzonderingen | Een lijst van wat aan staat (geen lijst: alles van het niveau). Een later toegevoegde tool staat dan niet vanzelf aan bij iemand met een eigen lijst, net als bij een toegangsniveau |
| P-217 | Wanneer geldt "Asks first" | Meteen, ook voor een lopende run: de bridge leest de actuele instelling. In elke modus, ook Auto; geen modusschakelaar of "Always allow" haalt het weg. De toollijst zelf geldt vanaf de volgende run, en start een nieuwe sessie |
| P-218 | Wat doet een startprofiel | Het vult de velden van de dialoog in, ook bij een tweede keer; alleen een getypte naam blijft staan. Er wordt niets vanzelf aangenomen. Een profiel dat al gekozen is toont wie ermee begon en stelt zijn naam niet opnieuw voor. Geen icoon per profiel: een employee heeft overal een letter als merkteken |
