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

## Latere stappen

| Stap | Scherm | Astryx | Opmerking |
|---|---|---|---|
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
