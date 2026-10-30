# AUR-MU-02: Fase 2, ticketdiscussie en kanalen in de Team chat (ter goedkeuring)

*Orion · 2026-10-31 · branch `docs/aur-mu-def` (lokaal) · basis: `docs/product/multi-user.md`, Fase 1 (B1–B10 goedgekeurd)*

| Document | Auteur | Inhoud |
|---|---|---|
| [user-stories.md](user-stories.md) | Nova | MU2-01 t/m MU2-12, Given/When/Then, OV-01..08 |
| [ui-ontwerp.md](ui-ontwerp.md) | Pixel | Discussie, kanalen en "ticket uit bericht", desktop en mobiel, P-21..27 |
| [impactanalyse.md](impactanalyse.md) | Titan | Migratie 0019, protocol, daemon, clients, schatting, volgorde, D2-1..6 |

**Scope:** een eigen discussie per ticket (`kind='ticket'`) voor board members en employees, met systeemregels voor delegaties, subtask-rapporten en kolomwijzigingen. Een @vermelding wekt een employee, binnen de bestaande lusgrenzen (MAX_HOPS 3, 2 wekkende posts per run, 4 per employee per uur). De plangoedkeuring en "terug naar Doing" (B2) verhuizen naar de discussie. De Team chat krijgt #general plus een kanaal per project. Je kan een ticket maken vanuit een chatbericht. De **samenvatting naar project-memory na Done hoort in deze fase** (MU2-12), want de discussie is de eerste bron die het hele ticket bevat.
**Buiten scope:** echte login en auteurschap met bewijs (Fase 3), meldingen per board member (Fase 3), zelf kanalen aanmaken of dempen, een tool waarmee employees kaarten verplaatsen, en "Give work" als ticket.
**Protocol:** **protocol 4 blijft, alleen additief**: features `ticket_discussion` en `team_channels`, de write `ticket.post`, `channel.list`, `ticket.create {from_message}` en een optioneel veld `system` op berichten (geen nieuwe `MessageRole`). Events van projectkanalen gaan alleen naar clients met `accepts: team_channels`. Zie impactanalyse §2.
**Schatting:** ≈ **25 dagen** werk, doorlooptijd 2 à 2,5 weken (Forge 10, Flux 5,5, Bridge 5,5, Schema 1, Sentinel 0,5, Pulse 2, Trace 1).
**Afhankelijkheden:** Fase 1 is af en heeft review en QA gehad (tickets, task-engine, `created_by_user`, B2-knop). De bouw takt af van `feat/aur-mu-01`.
**Bouwvolgorde:** Schema 0019 ‖ Forge discussiemotor (veralgemening van `conversation/group_chat.rs`) → protocol bevriezen + gen-dart → Flux en Bridge parallel, en Forge doet ondertussen systeemregels, kanalen en samenvatting → Sentinel, Pulse, Trace.
**Risico's:** een lus via meerdere discussies (het budget telt over alle discussies samen); ruis van systeemregels (inklapbaar); runkosten per @vermelding (bestaande limieten); auteurschap zonder bewijs tot Fase 3 (aanvaard, R2 uit Fase 1).

## Beslissingen (teamvoorstel geldt zonder tegenbericht)
| # | Vraag | Voorstel | Bron |
|---|---|---|---|
| B1 | Is de ticketdiscussie dezelfde conversatie als die van de hoofdtask? | Nee, een eigen conversatie. Een bericht zonder @naam gaat naar de eigenaar. Het hoofdtaskgesprek is nog te openen via ⋯ | D2-1, OV-03, P-23 |
| B2 | Lusgrenzen per discussie of samen? | Samen: één budget per employee over alle tickets en kanalen. Er komen geen nieuwe constanten | D2-3, OV-02 |
| B3 | Wie antwoordt in een projectkanaal zonder @naam? | De dispatcher van het project (anders de CEO). In een ticketdiscussie: de eigenaar zolang het ticket in Todo/Doing/Plan review staat, anders niemand (met hint) | D2-2, P-21 |
| B4 | Samenvatting na Done | In Fase 2, uit de discussie, als gegarandeerd project-memory-item. Tasks onder een ticket krijgen geen aparte extractie meer, tasks zonder ticket wel | D2-5, MU2-12, OV-06 |
| B5 | "Give work" als ticket (open B7)? | **Nee**, "Give work" blijft een taak zonder ticket. Een ticket uit een chatbericht kan wel. MU2-11 vervalt | D2-6 (gekozen boven Nova OV-04) |
| B6 | Mogen employees zelf tickets maken? | Nee. Ze stellen een ticket voor, en een board member maakt het in één klik | OV-05 |
| B7 | Schrijven in de discussie van een ticket op Done? | Nee, alleen lezen. Vervolgwerk wordt een nieuw ticket met een verwijzing | OV-01 |
| B8 | Badge van de Team chat | Alleen vermeldingen tellen, plus een stip bij ongelezen. Kanalen dempen komt later | P-24, P-25 |
| B9 | Een board member @vermelden | Wordt gemarkeerd in de discussie. Meldingen per persoon komen in Fase 3 | OV-07 |
