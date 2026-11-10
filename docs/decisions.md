# Beslissingen

Eén lijst van wat beslist is. Een fasedocument onder `docs/product/` beschrijft hoe iets
bedacht en gebouwd is; bij verschil geldt deze lijst. Een nieuwe beslissing krijgt een
nieuwe regel en noemt welke ze vervangt. Een beslissing wordt nooit stil aangepast.

**Status:** *gebouwd* (zit in de code), *deels* (een deel zit in de code, de rest staat
erbij), *open* (beslist, nog niet gebouwd).

## 2026-11-10: onderzoek naar tegenstrijdigheden (Sébastien akkoord met alle voorstellen)

| # | Beslissing | Vervangt | Status |
|---|---|---|---|
| D1 | Een chat van een board member met een employee is privé: alleen die board member en de employee lezen hem. Dat geldt voor lezen, de live-stroom, goedkeuringen en elke opdracht die de chat, een run of een bericht ervan bij id noemt. | `multi-user.md` §2 "iedereen mag alles zien" (voor deze chats) | gebouwd |
| D2 | Er is een **administrator** naast de ene rol. Alleen een administrator zet een tijdelijk wachtwoord voor een ander, beëindigt andermans sessies, ziet andermans apparaten, schakelt een board member uit en benoemt administrators. De eerste board member van een server is administrator; de laatste blijft het. | `multi-user.md` §2 "één rol"; `fase-3` "one role" | deels: accounts gebouwd. Open: apparaten koppelen, back-ups, vault-sleutel en serverbrede schakelaars hangen nog aan het apparaatrecht `owner` |
| D3 | Een ticket heeft **één betaler**: de board member op wiens Claude-login alle runs van dat ticket draaien, ook runs die een ander met een @vermelding wekt. De betaler is zichtbaar als "Runs on …" en verhuist mee als de Responsible wijzigt en die dat bevestigt. | `multi-user.md` §2 "de aanmaker"; het gedrag "wie vermeldt, betaalt" | open |
| D4 | Employees mogen zelf werk starten (een manager zet een ticket op Todo, een employee maakt een ticket vanuit een chat), binnen een **budget per ticket**: runs, handoffs en testrondes sinds het laatste bericht van een mens. Is het budget op, dan vraagt het ticket om een mens. | `multi-user.md` §2 "geen automatische pickup"; `fase-2` B6 | open (starten kan al, het budget niet) |
| D5 | Employees verplaatsen een ticket alleen van Backlog naar Todo, en alleen managers. Al het andere verplaatst het werk zelf of beslist een board member. | `multi-user.md` §2 "tot en met Review" | gebouwd (de code deed dit al; het document was fout) |
| D6 | **Plan-modus is alleen-lezen voor het hele bedrijf**: geen bestanden, geen delegaties, geen handoffs, geen geplande berichten en geen geheugen. Een delegatie erft de modus van wie delegeert. | — | open |
| D7 | "Niet pushen of mergen zonder akkoord" is een **slot**, geen afspraak: `git push`, een merge en een deploy vragen in elke modus eerst een board member. | RULE 07 als tekst in de prompt | open |
| D8 | De kolom **Testing** bestaat, met een uitweg als de test wacht (vraag, quotum, login): het ticket vraagt dan om een mens. | `multi-user.md` §2 (zes kolommen) | deels: de kolom bestaat, de uitweg niet |
| D9 | Een ticket in Done kan **heropend** worden; discussie, rapport en branch blijven. | `fase-1` "It cannot be reopened"; `fase-a` "bestaat niet" | open |
| D10 | **Al het werk loopt via het bord.** Een terugkerende taak en werk uit een chat maken een ticket; taken zonder ticket verdwijnen voor mensen. | `fase-2` B5 | open |
| D11 | Iedereen ziet ieders **verbruik** (tokens en kosten per login). Het percentage van het abonnement blijft alleen voor de eigenaar ervan. | `fase-3` B12 | gebouwd (de code deed dit al) |
| D12 | In de app heet het **workspace**, in code en protocol **project**. Een workspace is een project. | `fase-1` verbod op het woord "workspace" | gebouwd |

Kleinere punten, in dezelfde ronde beslist:

| # | Beslissing | Status |
|---|---|---|
| D13 | Een wijziging op een verouderde versie wordt geweigerd, op elke client: `expected_version` bij elke ticketschrijfactie. | open (mobiel doet het, desktop alleen bij bewerken) |
| D14 | De server berekent wat een ticket nodig heeft en van wie (*Needs you*); clients rekenen het niet na. | open |
| D15 | De server ontleedt mentions; clients gebruiken dezelfde regels. | open |
| D16 | Een venster waar niemand naar kijkt leest niet voor zijn board member (`peek`, feature `read_peek`). | deels: desktop gebouwd. Open: een eigen leesopdracht met een event, zodat alle apparaten van één persoon gelijk lopen |
| D17 | De regels RULE 01–10 staan in de repo, met een versie. | open (ze staan nu alleen in de database van de server) |
| D18 | Een antwoord dat een client niet kan lezen is een fout, geen time-out; een enum met de waarde `unknown` leest een onbekende waarde als `unknown`. | gebouwd |

## Ontwerpwijzigingen waartoe besloten is

Nog niet gebouwd, in de volgorde waarin ze het meeste opleveren.

1. **Eén tickettoestand op de server.** Eén functie bepaalt uit de gebeurtenissen de kolom, wat het ticket nodig heeft, van wie, en of er nog iets loopt. Ze vervangt de drie plekken die nu de kolom zetten en de vier definities van "nog bezig", en ze is de plek voor D8, D13 en D14.
2. **Expliciet "klaar" en "vraag".** De tools `report_done` en `ask_board` in plaats van raden uit de laatste regel. Een run die zonder een van beide eindigt, vraagt aandacht en gaat niet naar Review.
3. **Herstelbaar in plaats van definitief.** Een run die mislukt of onderbroken wordt, zet de taak op wachten met Hervatten op dezelfde sessie. Definitief mislukt is er alleen voor fouten die niet overgaan.
4. **Een worktree en branch per ticket**, gemaakt door de server bij Start. Accepteren biedt mergen, een PR openen of bewaren (D7).
5. **Eén manier van werk doorgeven** in plaats van `delegate_task` en `hand_off` naast elkaar, met het budget per ticket (D4).
6. **Aanwezigheid.** Wie bekijkt dit ticket, wie beantwoordt deze goedkeuring, wie typt.
7. **"Sinds je weg was".** Een overzicht per ticket uit de genummerde gebeurtenissenlog.
8. **Uitnodigingslink.** Eén link of QR die het apparaat koppelt, het wachtwoord laat kiezen en naar het verbinden van Claude leidt.

## Woordenlijst

| Begrip | Het woord | Niet |
|---|---|---|
| Mens | board member | user, Owner (in tekst voor mensen en in prompts) |
| Board member met beheerrechten | administrator | owner (dat is het recht van een gekoppeld apparaat) |
| AI-collega | employee | agent, bot, manager |
| Kaart op het bord | ticket | issue, task (een task is de interne werkeenheid) |
| Wie het ticket maakte | creator | |
| De employee die het werk doet | owner | assignee |
| De board member die erop let | Responsible | assignee |
| Op wiens Claude-login het draait | "Runs on" | |
| Gesprek met meerdere | channel (general, project, ticket) | Team chat als naam voor één gesprek |
| Gesprek met één employee | chat | |
| Project in de app | workspace | |
| De machine | server | PC |
| De organisatie | company | |
