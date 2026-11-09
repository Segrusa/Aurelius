# Workspaces

*2026-11-09 · branch `feat/astryx`*

Een **workspace is een project**. Je wisselt van project en de hele app laat alleen dat project zien. Employees werken voor het hele bedrijf; iedereen ziet alles. Er zijn nu geen leden of rechten per workspace.

Bovenaan de zijbalk staat de wisselaar: **All projects** en elk project dat niet gearchiveerd is. Hij zit ook in het commandopalet heeft een eigen sneltoets ("Switch workspace", `secondary-p` in `keys/keymap.ts`) en werkt ook met de ingeklapte zijbalk.

## Wat de huidige workspace is

- Eén stuk status per apparaat en per server. Hij staat niet op de server.
- Een link of melding naar een ticket, taak of gesprek van een ander project schakelt naar dat project. Staat de app op All projects, dan blijft hij daar: daar is alles te zien.
- Wordt het huidige project gearchiveerd of verwijderd, dan valt de app terug op All projects.
- Een server zonder feature `workspace_filters` geeft geen wisselaar: de app blijft op All projects. Liever geen filter dan verkeerde gegevens.

## Wat er gefilterd wordt

| Scherm | Hoe | In een workspace |
|---|---|---|
| Inbox | Server: `notification.list` en `approval.list` met `project_id` | Alleen meldingen en goedkeuringen van het project. Voorstellen voor nieuwe employees alleen onder All projects, want die horen bij het bedrijf |
| My tickets, Board | Client / bestaande `board.get {project_id}` | Alleen tickets van het project |
| Tasks | Client | Alleen taken van het project |
| Team chat | Client | Het kanaal van het project en zijn threads. Het algemene kanaal alleen onder All projects |
| Knowledge | Client | Richtlijnen die aan het project hangen of via een employee die er werkt gelden, plus richtlijnen op bedrijfsniveau (gemarkeerd als Company), en het geheugen van het project |
| Scheduled | Server: `scheduled_message.list` met `project_id` | Alleen wat in het project gepland is |
| Activity log | Server: `activity_log.list` met `project_id` | Alleen stappen in tickets, kanalen en gesprekken van het project |
| Palet | Client | Alleen tickets, kanalen en richtlijnen van het project. Het palet haalt nooit alle tickets op: het gebruikt de lijst die de app al heeft |

Taken en gesprekken zonder project staan alleen onder All projects.

Nieuw ticket, Give work en een terugkerende taak beginnen op het huidige project.

## Wat er niet gefilterd wordt

Employees, de groep Chats in de zijbalk (persoonlijk), Usage en Settings. Die horen bij het bedrijf of bij jou.

## De projectpagina

De tabs Board en Tasks zijn weg: dat zijn nu de gewone schermen in de workspace. Wat blijft is overzicht en instellingen (map, dispatcher, status, export, archiveren). Je komt er via **Workspace settings** in de wisselaar en via de projectlijst. De projectlijst staat als tab onder Knowledge en daar maak je een project aan.

Een projectvenster blijft werken zoals het was: vast op zijn eigen project, en het heeft nog wel Board en Tasks omdat het geen zijbalk heeft.

## Claude Design: styleguide en design

Een workspace kan een **styleguide** hebben en een ticket een **design**: een link naar een artifact in claude.ai (types Design System en Design). Alleen de link staat in Aurelius, nooit de inhoud; een medewerker leest het artifact zelf.

- **Styleguide** staat op de pagina van de workspace (Workspace settings), **Design** in de details van een ticket. Allebei met een Open-knop. Alleen `https://claude.ai/artifact/…` en `https://claude.ai/code/artifact/…`, maximaal 300 tekens; leeg maken verwijdert de link.
- Een run van een medewerker met zo'n link krijgt de tool `Artifact` van Claude Code en in de instructies: lees `project/README.md` van de styleguide voor je UI ontwerpt of bouwt en gebruik de tokens, en wat het design van dit ticket is. Runs zonder link krijgen niets extra.
- De link werkt alleen als de **board member wiens Claude-login de ticket draait** het artifact kan openen: delen in Claude gebeurt per persoon. Staat dat niet goed, dan meldt de medewerker dat de link niet opent.
- Een medewerker met **ReadOnly** kan het artifact niet openen (de tool hoort niet bij lezen alleen, want hij kan ook schrijven en publiceren). Hetzelfde geldt voor een connectie met een API-sleutel of een andere provider: de run krijgt dan de melding het artifact te vragen aan de board member.
- Een medewerker die zelf een Design publiceert, koppelt dat met `update_ticket` (`design_url`) aan zijn ticket, met dezelfde controle op de link.

## Leden later toevoegen

Niets in dit ontwerp blokkeert leden of rechten:

- De filters zitten in de reads op de server (`project_id`) en in één client-laag (`workspace-logic.ts`). Een server die rechten kent kan op dezelfde plek afdwingen dat een lezer alleen zijn eigen projecten ziet.
- Een tabel `project_members` (project, board member, rol) is genoeg om de wisselaar te beperken; `project.list` geeft dan alleen de projecten van de lezer.
- De filters zijn nu een weergave, geen beveiliging. Bij leden moeten alle reads zonder `project_id` (All projects) ook op lidmaatschap filteren, en moeten de lees-rechten op de server per project gaan.
