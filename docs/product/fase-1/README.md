# AUR-MU-00: resultaten van Fase 0 en de voorbereiding van Fase 1

*Orion · 2026-10-30 · branch `docs/aur-mu-00` (lokaal, niet gepusht) · basis: `docs/product/multi-user.md`*

| Document | Auteur | Inhoud |
|---|---|---|
| [spike-claude-logins.md](spike-claude-logins.md) | Titan | De spike voor een Claude-login per board member, met bewijs, een codevoorstel, quota per user en de voorwaarden van Anthropic |
| [user-stories.md](user-stories.md) | Nova | 22 user stories (MU1-01 t/m MU1-22) met Given/When/Then, traceerbaarheid en open vragen |
| [ui-ontwerp.md](ui-ontwerp.md) | Pixel | Het Board, de ticketkaart, de ticketweergave, de reviewpoort, de onboarding en de instellingen, voor desktop en mobiel |
| [impactanalyse.md](impactanalyse.md) | Titan | Migraties (fresh start + 0018), protocol 4, de task-engine, de clients, een schatting per onderdeel en de bouwvolgorde |

De spike-code staat op `spike/aur-mu-00-claude-logins` (commit 7e1388d, worktree `target/wt-mu-spike`). Die branch is niet gepusht en zit niet in main.

## Conclusie van de spike
Optie (a) werkt. Met een eigen `CLAUDE_CONFIG_DIR` per board member ziet elk `claude`-proces alleen die ene login. Dat is aangetoond met twee mappen die tegelijk liepen: één ingelogd, één leeg met "Not logged in". De voorwaarden van Anthropic laten dit toe, op voorwaarde dat elke board member zelf inlogt op de ongewijzigde `claude` en Aurelius geen token bewaart. Gebruik `setup-token` dus niet als standaard. Het quotum moet per login geparkeerd worden: nu legt één leeg quotum alle runs stil (werk voor Fase 3).
**Nog nodig:** Sébastien bevestigt de spike met een tweede echt account (spike §2.6, één `claude auth login` in een nieuwe map).

## Beslissingen die bij Sébastien liggen (vóór de bouw van Fase 1)
Bij elke vraag staat het voorstel van het team. Zonder tegenbericht gaat het team ervan uit dat die voorstellen gelden.

| # | Vraag | Voorstel | Bron |
|---|---|---|---|
| B1 | Schone werkruimte: een nieuwe database met import, of een migratie die tabellen leegmaakt? | Een nieuwe database met import. De oude `data/` blijft volledig bewaard als `pre-v0.11/`, met eerst een dry-run op een kopie | Impact A1/A3, Nova OV-11 |
| B2 | Hoe werken de plangoedkeuring en "terug naar Doing" in Fase 1, nu de ticketdiscussie pas in Fase 2 komt? | Een knop met een verplicht tekstveld op het ticket. De tekst gaat naar het gesprek van de hoofdtask | Nova OV-01 |
| B3 | Kolommen per company: wat mag een board member instellen? | De zes kolommen met een functie zijn vast. Je kan ze hernoemen en hun volgorde wijzigen. Eigen kolommen komen later | Nova OV-02, Titan §7.1, Pixel P-05 |
| B4 | Eén Board per company of per project? | Eén Board per company, met een filter per project | Nova OV-03 |
| B5 | Dispatcher bestaat niet of is gearchiveerd | Het ticket blijft in Todo met de melding "kies een eigenaar" | Nova OV-04, Titan §7.5 |
| B6 | Done met taken die nog lopen, of een failed hoofdtask | Done kan pas als er niets meer loopt. Een failed hoofdtask blijft in Doing met een fout en de knop "Opnieuw" | Nova OV-09, Titan §7.3 |
| B7 | Taken zonder ticket (Team chat, project-assistent, schedules, "Give work") | Ze blijven bestaan, zonder reviewpoort en niet op het Board. Schedules worden gepauzeerd geïmporteerd. Of "Give work" een ticket maakt, beslissen we samen met Fase 2 | Nova OV-16, Titan §7.2/§7.4, Pixel P-02 |
| B8 | Onboarding in Fase 1: wordt het wachtwoord gecontroleerd bij het kiezen van een board member? | Ja, één keer per apparaat. Sessies en de koppeling van apparaten komen in Fase 3. Restrisico: tot Fase 3 vertrouwt de server op de tailnet | Nova OV-13, Pixel P-07/P-08 |
| B9 | Prioriteitsschaal | Low · Normal · High · Urgent (standaard Normal) | Nova OV-06 |
| B10 | Fase 1 op desktop én mobiel | Ja, beide, omdat de onboarding in elke client nodig is | Nova OV-10 |

## Planning
De schatting staat in de impactanalyse §5/§6: ongeveer 42 dagen werk, waarvan 6 voor S5 (board members en onboarding: tabel `users`, `user.*`, de gate in de clients). Met vier ontwikkelaars parallel duurt het 3,5 à 4,5 weken, inclusief review door Trace en QA door Pulse. Er wordt pas gebouwd na het akkoord van Sébastien op Fase 1.
