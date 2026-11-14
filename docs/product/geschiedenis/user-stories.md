# History: wat er over de tijd gedaan is (ticket #57)

Analyse: Nova (Business Analyst). Keuze van de Owner: optie A, uitgebreid met een samenvatting die bij het afsluiten van een ticket gemaakt wordt.

## Doel

De Owner wil kunnen teruglezen wat er over de tijd heen gedaan is. Daarvoor komt er een rustig logboek van het afgeronde werk. Elke regel toont een samenvatting van wat het ticket echt opleverde. Die samenvatting komt niet uit de titel of de beschrijving van het ticket.

## Wat er nu al is

- Bij Done (accept door een board member) maakt de server al een samenvatting van twee tot vier zinnen over wat het ticket opleverde en besliste. Hij doet dat met dezelfde extractiejob als het geheugen (`crates/daemon/src/memory.rs`, `ticket_summary`). De bron is het werk zelf: het gesprek van de hoofdtask en de ticketdiscussie. De samenvatting wordt alleen bewaard als projectkennis ("Ticket #n “titel” is done: …") en staat niet op het ticket. Lukt het niet, dan zet de server `summary_failed` in de discussie en kan een board member het opnieuw proberen met `ticket.retry_summary`.
- De Done-kolom toont maar de 20 nieuwste tickets (`DONE_SHOWN`).
- Elk ticket heeft een eigen Activity-tab. De verborgen Activity log (`/activity`) is technisch: hij toont runs, de wachtrij en afleveringen.

Conclusie: de samenvatting bestaat al. Ze moet nog op het ticket zelf terechtkomen en zichtbaar worden in een overzicht.

## User stories

### H-01 Samenvatting bij het afsluiten
Als Owner wil ik dat elk ticket bij het afsluiten een samenvatting krijgt van wat er echt gedaan is, zodat ik later weet wat het opleverde zonder het hele gesprek terug te lezen.

Acceptatiecriteria
1. Wanneer een ticket naar Done gaat, krijgt het een samenvatting van twee tot vier zinnen. Dat geldt ook voor een snelle taak (quick ticket).
2. De samenvatting beschrijft wat er opgeleverd en beslist is. De bron is het werk zelf: het gesprek van de hoofdtask (met het rapport) en de ticketdiscussie. De titel en de beschrijving zijn geen bron. Een samenvatting die alleen de titel of de beschrijving herhaalt, voldoet niet.
3. De samenvatting wordt bewaard op het ticket (een historiekveld), naast de bestaande projectkennis. Het is dezelfde tekst, dus er komt geen tweede AI-aanroep.
4. De tekst gaat door de vault-redactor voordat hij wordt opgeslagen.
5. Mislukt de samenvatting, dan blijft het ticket op Done en toont History "Summary not available" met een Try again voor board members (de bestaande `ticket.retry_summary`). Een mislukte samenvatting staat nooit eindeloos op "Loading…".
6. Een geannuleerd ticket krijgt geen samenvatting en staat niet in History (zie aanname A2).

### H-02 History-pagina
Als Owner wil ik op één pagina zien wat er over de tijd gedaan is, zodat ik de voortgang van het bedrijf kan volgen.

Acceptatiecriteria
1. In de zijbalk staat onder Workspace een nieuwe rij "History", die niet verborgen is achter een feature-vlag. Verwacht de server de nieuwe feature niet, dan toont de app de rij niet.
2. De pagina toont tickets op Done, nieuwste eerst, gegroepeerd per dag ("Today", "Yesterday", daarna de datum).
3. Elke regel toont het ticketnummer, de titel, de eigenaar (employee), het project, het tijdstip waarop het ticket Done werd en de samenvatting uit H-01. Een lange samenvatting wordt na drie regels afgekapt en klapt open met een klik.
4. Een klik op het ticketnummer of de titel opent het ticket.
5. Tickets van vóór deze functie hebben geen opgeslagen samenvatting. Ze staan er wel in, met "No summary (finished before History)" (aanname A3).
6. De pagina laadt per pagina (bijvoorbeeld 50 regels), met "Load more" of oneindig scrollen tot het oudste ticket. Een leesfout toont wat er misging, met Try again.
7. Gaat er een ticket naar Done terwijl de pagina open is, of komt er een samenvatting binnen, dan verschijnt dat zonder herladen.

### H-03 Filteren
Als Owner wil ik History kunnen beperken, zodat ik snel vind wat er in een project of door een employee gedaan is.

Acceptatiecriteria
1. Je kunt filteren op project, employee (eigenaar) en periode (Last 7 days, Last 30 days, All time, of zelf gekozen data).
2. De filters staan in de URL, net als bij de Activity log, zodat een link de gefilterde weergave bewaart.
3. Zoeken op tekst doorzoekt de titel en de samenvatting.
4. Zonder resultaten toont de pagina "Nothing finished in this period" en een knop om de filters te wissen.

## Business rules

- B1. "Gedaan" betekent: het ticket staat op Done. Werk dat nog loopt, in Review of in Testing, telt niet mee.
- B2. Gaat een ticket terug van Done (heropenen), dan verdwijnt het uit History. Gaat het opnieuw naar Done, dan wordt er een nieuwe samenvatting gemaakt en telt het nieuwe tijdstip.
- B3. History volgt dezelfde zichtbaarheid als het bord: wie een ticket niet op het bord ziet, ziet het ook niet in History.
- B4. UI-tekst is Engels en gebruikt dezelfde woorden als de rest van de app.

## Edge cases

- Een ticket gaat naar Done terwijl de samenvatting nog loopt: History toont "Summarizing…" en vult de tekst in zodra die binnen is. Mislukt het, dan geldt H-01 AC5.
- Een quick ticket gaat pas naar Done na de merge-job. Het tijdstip in History is dat van Done, niet dat van de accept.
- Een ticket zonder eigenaar of met een gearchiveerde employee toont de naam zoals die op het ticket stond. Er is geen eigenaar → "—".
- Een gearchiveerd project: de tickets blijven in History staan en je kunt erop filteren.

## Aannames (graag bevestigen of corrigeren)

- A1. Eerst alleen de desktop-app (clients/ts). Mobiel komt later als een eigen ticket. De Owner heeft hier nog niet op geantwoord.
- A2. Geannuleerde tickets horen niet in History, omdat ze niets opgeleverd hebben.
- A3. Voor oude Done-tickets maken we achteraf geen samenvatting. Dat zou per ticket een AI-aanroep kosten. Wel kan de bestaande projectkennis "Ticket #n … is done: …" als samenvatting dienen wanneer die er al is. Titan beslist of dat betrouwbaar te koppelen is (via `locator.ticket_id`).

## Voor Titan (technisch te bepalen, niet door de BA)

- Waar de samenvatting op het ticket komt (een kolom of een historiektabel), de migratie, en hoe `record_extraction` die invult in dezelfde transactie.
- De leesmethode voor History (filters, keyset-paging, een nieuwe feature in `FEATURES`) en de rechten in `app.rs`/`remote.rs`.
- Of de bestaande accept van een quick ticket al dezelfde extractie met `ticket_summary` start. Zo niet, dan moet dat erbij (H-01 AC1).
