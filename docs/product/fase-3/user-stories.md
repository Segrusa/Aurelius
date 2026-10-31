# User stories Fase 3: Multi-user (login, sessies, auteurschap, Claude-login per board member)

- **Task:** AUR-MU-DEF-N (Nova, voor Orion)
- **Bron:** `docs/product/multi-user.md` §2 "Toegang" en "Claude-abonnementen"; `docs/product/fase-1/spike-claude-logins.md` (§2.6, §3.2, §3.3, §4); `docs/product/fase-1/README.md` (B8: tot Fase 3 vertrouwt de server op de tailnet); `docs/product/fase-1/user-stories.md` (tabel na MU1-22, OV-14).
- **Status:** concept, wacht op review door Orion en akkoord van Sébastien.
- **Regel:** wat de bron niet vastlegt, staat als **aanname** (A-xx) of **open vraag** (OV-xx).

## Aannames

- **A-01: mechanisme.** Een Claude-login per board member is een eigen `CLAUDE_CONFIG_DIR` (`<data>/logins/<user_id>/`, 0700). De board member logt zelf in met de ongewijzigde `claude`. Aurelius bewaart in de database alleen de map, de status, het e-mailadres en het plan, **nooit een token**. `setup-token` (`CLAUDE_CODE_OAUTH_TOKEN`) is geen standaard en zit niet in de UI (spike §4 advies 2).
- **A-02: login vastgelegd per run.** De gekozen login staat in de snapshot van de run. Retries en een hervatting na quota houden dezelfde login (spike §3.2 punt 2).
- **A-03: één rol.** Elke board member mag alles, ook sessies van anderen intrekken en board members deactiveren. Niemand kan wel een andere Claude-login kiezen dan de regels hieronder (spike §4 risico 1).

## User stories

Prioriteit: **M** = Must, **S** = Should, **C** = Could.

### Epic A: Voorwaarden

#### MU3-01: Bewijs van het mechanisme met één account, plus controle van de voorwaarden (M, voorwaarde; blokkeert niet)
**Als** het team **wil ik** het mechanisme per user bewijzen zonder een tweede Claude-account, **zodat** Fase 3 niet wacht op een account dat er voorlopig niet is (Sébastien, via Atlas, 2026-10-31).

- **AC1 (dev/test).** *Given* twee board members die aan **hetzelfde** Claude-account gekoppeld zijn, elk met een eigen `CLAUDE_CONFIG_DIR` en een eigen `claude auth login`, *When* elk een ticket start, *Then* gebruikt elke ticketrun de config-map van zijn aanmaker. Delegaties erven die map, en elke map heeft een eigen `.credentials.json` en eigen sessies (projects/). Dit is aangetoond met een test plus een echte run.
- **AC2 (open acceptatiepunt).** De scheiding van de quota per login wordt hiermee **niet** bewezen, omdat beide logins dezelfde limieten delen. De parkeerlogica per login (MU3-11) wordt getest met een gesimuleerde quota-fout per login. De test met een echt tweede account (spike §2.6) blijft open tot er een is en blokkeert de oplevering van Fase 3 niet.
- **AC3.** *Given* de voorwaarden van Anthropic, *When* de bouw van Epic D start, *Then* zijn ze opnieuw gecontroleerd (spike §4 risico 3). Het delen van één abonnement door meerdere personen is apart beslist (README B-punt). Verbieden de voorwaarden optie (a), dan escaleert Orion vóór de bouw.
- **AC4.** *Given* verboden voorwaarden, *When* dat blijkt, *Then* start Epic D niet. Epic A–C (login, sessies, gate, auteurschap) kunnen wel door.

### Epic B: Login en sessies

#### MU3-02: Inloggen met gebruikersnaam en wachtwoord (M)
**Als** board member **wil ik** inloggen met mijn gebruikersnaam en wachtwoord, **zodat** niemand anders zich voor mij kan uitgeven.

- **AC1.** *Given* een client zonder sessie op een server met board members, *When* ik de client open, *Then* zie ik alleen het loginscherm. Dit vervangt het kiezen van een board member uit Fase 1 (MU1-21, B8).
- **AC2.** *Given* een juiste gebruikersnaam en een juist wachtwoord, *When* ik inlog, *Then* controleert de server de argon2-hash, krijg ik een sessie en kom ik in de app.
- **AC3.** *Given* een foute gebruikersnaam of een fout wachtwoord, *When* ik inlog, *Then* zie ik één algemene melding ("Gebruikersnaam of wachtwoord klopt niet"), zonder te verraden welk van de twee fout is.
- **AC4.** *Given* herhaalde mislukte pogingen, *When* de limiet bereikt is, *Then* wacht de server steeds langer voor een volgende poging (OV-03). Het wachtwoord komt nooit in logs, commando's of events.
- **AC5.** *Given* een server zonder board members, *When* een client verbindt, *Then* geldt de onboarding van MU1-20 ongewijzigd en is de eerste board member meteen ingelogd.

#### MU3-03: Sessie per apparaat (M)
**Als** board member **wil ik** op elk apparaat een eigen sessie, **zodat** ik niet bij elke verbinding opnieuw moet inloggen en ik elk apparaat apart kan afsluiten.

- **AC1.** *Given* een login op een apparaat, *When* die lukt, *Then* koppelt de server de sessie aan dat apparaat (`remote_clients` of de lokale desktop) en aan mij. De server bewaart alleen een hash van het sessiegeheim.
- **AC2.** *Given* een geldige sessie, *When* de client opnieuw verbindt of de server herstart, *Then* blijf ik ingelogd.
- **AC3.** *Given* een sessie die verlopen is (OV-02), *When* de client verbindt, *Then* komt het loginscherm, zonder dat er werk verloren gaat.

#### MU3-04: Sessies bekijken, intrekken en uitloggen (M)
**Als** board member **wil ik** zien waar ik ingelogd ben en sessies kunnen intrekken, **zodat** ik een verloren apparaat meteen kan afsluiten.

- **AC1.** *Given* de instellingen, *When* ik "Sessies" open, *Then* zie ik per sessie het apparaat, wanneer ze begon en wanneer ze laatst actief was.
- **AC2.** *Given* een sessie, *When* ik ze intrek, *Then* wordt de verbinding van dat apparaat meteen verbroken en moet dat apparaat opnieuw inloggen.
- **AC3.** *Given* een client, *When* ik uitlog, *Then* is de sessie van dat apparaat ingetrokken en zie ik het loginscherm.
- **AC4.** *Given* een andere board member, *When* ik diens sessies open, *Then* kan ik ze ook intrekken (A-03). Het intrekken staat met mijn naam in de timeline.

#### MU3-05: Wachtwoord wijzigen en board member deactiveren (S)
**Als** board member **wil ik** mijn wachtwoord wijzigen en een vertrokken collega deactiveren, **zodat** de toegang klopt met wie er echt werkt (Fase 1 OV-14).

- **AC1.** *Given* mijn account, *When* ik mijn wachtwoord wijzig (met het oude wachtwoord), *Then* blijft mijn huidige sessie geldig en worden mijn andere sessies ingetrokken.
- **AC2.** *Given* een andere board member, *When* ik diens wachtwoord reset, *Then* worden al diens sessies ingetrokken.
- **AC3.** *Given* een board member, *When* ik die deactiveer, *Then* worden de sessies ingetrokken, wordt diens Claude-login ontkoppeld (MU3-09 AC4) en kan die niet meer inloggen. Tickets houden hun aanmaker.
- **AC4.** *Given* de laatste actieve board member, *When* iemand die probeert te deactiveren, *Then* wordt dat geweigerd.

### Epic C: Gate, Tailscale en auteurschap

#### MU3-06: Volledige gate: niets zonder sessie (M)
**Als** board member **wil ik** dat de server niets doet voor een verbinding zonder sessie, **zodat** de tailnet niet meer de enige bescherming is (B8 restrisico).

- **AC1.** *Given* een verbinding zonder geldige sessie, *When* die een methode aanroept, *Then* weigert de server met `unauthenticated`. Alleen de handshake, de login en de onboarding van de eerste board member (MU1-20) zijn toegestaan.
- **AC2.** *Given* een verbinding zonder sessie, *When* die zich abonneert op events, *Then* krijgt ze niets, ook geen events uit het verleden.
- **AC3.** *Given* een ingetrokken of verlopen sessie, *When* er nog een verbinding open is, *Then* sluit de server die en lukt elke volgende aanroep niet meer.
- **AC4.** *Given* de tests, *When* ze lopen, *Then* is er een test die elke methode van het protocol zonder sessie aanroept en controleert dat ze geweigerd wordt (behalve die van AC1).

#### MU3-07: Toegang via Tailscale (M)
**Als** board member **wil ik** de centrale server via de tailnet bereiken, **zodat** we op meerdere apparaten werken zonder de server publiek te zetten.

- **AC1.** *Given* een server met Tailscale, *When* een client in dezelfde tailnet het adres van de server krijgt, *Then* kan die verbinden en inloggen (MU3-02).
- **AC2.** *Given* de standaardinstelling, *When* de server start, *Then* luistert die alleen op localhost en het tailnet-adres, niet op andere interfaces (OV-04).
- **AC3.** *Given* een apparaat buiten de tailnet, *When* het probeert te verbinden, *Then* bereikt het de server niet.

#### MU3-08: Auteur op elke actie en elk bericht (M)
**Als** board member **wil ik** bij elke actie en elk bericht zien wie het deed, **zodat** we elkaars werk kunnen volgen.

- **AC1.** *Given* een ingelogde board member, *When* die een bericht stuurt (ticketdiscussie, kanaal, project-assistent, gesprek met een employee), *Then* is de auteur die board member, uit de sessie en niet uit een veld van de client.
- **AC2.** *Given* een write (ticket, Board, company, project, employee, goedkeuring, Done, intrekken), *When* die gebeurt, *Then* draagt het event de user-id van de sessie, en toont de UI de naam.
- **AC3.** *Given* een bericht of actie van een employee, *When* die getoond wordt, *Then* is de auteur die employee (en de run).
- **AC4.** *Given* een @vermelding van een board member (Fase 2 OV-07), *When* die verstuurd wordt, *Then* krijgt alleen die board member een melding in de eigen Inbox.

### Epic D: Claude-login per board member (na MU3-01)

#### MU3-09: Eigen Claude-login koppelen (M)
**Als** board member **wil ik** mijn eigen Claude-abonnement koppelen, **zodat** het werk van mijn tickets op mijn eigen quotum loopt.

- **AC1.** *Given* mijn instellingen, *When* ik "Claude-login koppelen" kies, *Then* krijg ik de stap om `claude auth login` te doen met mijn eigen `CLAUDE_CONFIG_DIR` (`<data>/logins/<user_id>/`, 0700), en log ik zelf in via het loginscherm van Anthropic (OV-01).
- **AC2.** *Given* een gekoppelde login, *When* ik de instellingen open, *Then* zie ik de status, het e-mailadres en het plan (uit `auth status --json`), en mijn quotum (uit `/usage` van mijn login).
- **AC3.** *Given* de database, de events, de logs en de back-ups, *When* ze worden doorzocht, *Then* staat er geen Claude-token in. Er is geen scherm om een token te plakken.
- **AC4.** *Given* een gekoppelde login, *When* ik ze ontkoppel (of word gedeactiveerd), *Then* voert de server `claude auth logout` uit voor die map en verwijdert die de map.
- **AC5.** *Given* een `/usage` zonder `% used`-regels, *When* de status bepaald wordt, *Then* toont de UI "Koppel je Claude-login" en niet 0% (spike §3.3).

#### MU3-10: Welke login een run gebruikt (M)
**Als** board member **wil ik** dat elke run een vaste, voorspelbare login gebruikt, **zodat** duidelijk is wiens abonnement er verbruikt wordt.

- **AC1.** *Given* een ticket, *When* de hoofdtask, de triage van de dispatcher, een delegatie (op elke diepte) of de samenvatting na Done draait, *Then* gebruikt de run de login van de **aanmaker van het ticket**.
- **AC2.** *Given* een chatrun (kanaal, project-assistent, gesprek met een employee), *When* ze start, *Then* gebruikt ze de login van de board member die het bericht stuurde. Een employee die verder gewekt wordt (hop > 0), gebruikt de login van de board member van hop 0.
- **AC3.** *Given* een run in de wachtrij of na quota, *When* ze hervat of opnieuw probeert, *Then* gebruikt ze dezelfde login als vastgelegd bij de start (A-02).
- **AC4.** *Given* de UI van een run of ticket, *When* ik ze bekijk, *Then* zie ik op wiens abonnement ze loopt ("op Claude van Alice"). Er is nergens een keuze "gebruik login van X".

#### MU3-11: Quota-parkering per login (M)
**Als** board member **wil ik** dat een leeg quotum alleen de runs op die login stillegt, **zodat** het werk van collega's doorloopt.

- **AC1.** *Given* runs op de logins van Alice en Bob, *When* het quotum van Alice op is, *Then* wachten alleen de runs op de login van Alice met de reden `quota` en een tijd tot reset. De runs van Bob lopen door.
- **AC2.** *Given* een ticket van Alice met delegaties, *When* het quotum van Alice op is, *Then* wachten de hoofdtask en alle delegaties van dat ticket samen, en toont de kaart "wacht op quotum van Alice tot ‹tijd›".
- **AC3.** *Given* het einde van de wachttijd, *When* de reset voorbij is, *Then* hervatten de geparkeerde runs op dezelfde login.

#### MU3-12: Geen fallback, duidelijke status bij ontbrekende of verlopen login (M)
**Als** board member **wil ik** zien dat een ticket wacht omdat de Claude-login van de aanmaker ontbreekt of verlopen is, **zodat** de juiste persoon het kan oplossen en niemand ongemerkt een ander abonnement gebruikt.

- **AC1.** *Given* een ticket waarvan de aanmaker geen gekoppelde login heeft, of een run die een 401 krijgt, *When* de run zou starten, *Then* start ze niet op een andere login (niet op `~/.claude` van de daemon, niet op die van iemand anders) en wacht ze met de reden `auth`.
- **AC2.** *Given* zo'n wachtend ticket, *When* iemand het Board of het ticket opent, *Then* zie ik "Wacht: Claude-login van ‹aanmaker› ontbreekt" of "… is verlopen", en krijgt de aanmaker een melding in de eigen Inbox.
- **AC3.** *Given* dat de aanmaker de login (opnieuw) koppelt, *When* dat lukt, *Then* hervatten de wachtende runs van die login vanzelf (OV-05).
- **AC4.** *Given* een gedeactiveerde aanmaker, *When* diens ticket nog werk heeft, *Then* blijft het wachten met `auth` en toont de UI dat de aanmaker gedeactiveerd is (OV-06).
- **AC5.** *Given* de runtime-tests, *When* ze lopen, *Then* bewijzen ze dat er geen `ANTHROPIC_*`, `CLAUDE_CODE_OAUTH_TOKEN` of `CLAUDE_CONFIG_DIR` van de daemon naar een run lekt, en dat een run zonder login nooit de standaardmap gebruikt (spike §2.5).

## Open vragen (escalatie naar Orion)

| ID | Vraag | Voorstel |
|---|---|---|
| **OV-01** | Hoe doet een board member `claude auth login` op een centrale server zonder browser? Een begeleide stap in de app die de login-URL en de code doorgeeft, komt dicht bij "intermediate credentials" (spike §4). | Basis: de board member doet het zelf in een terminal op de server (ssh via de tailnet), met het commando dat de app toont. Een begeleide stap in de app alleen als Titan en Sentinel bevestigen dat Aurelius dan niets ziet of bewaart |
| **OV-02** | Hoe lang blijft een sessie geldig? | Geen vaste vervaldatum, wel na 30 dagen zonder activiteit. Intrekken en uitloggen werken altijd meteen |
| **OV-03** | Welke limiet geldt voor mislukte logins? | Na 5 mislukte pogingen per gebruikersnaam een oplopende wachttijd (tot 15 minuten). Geen definitieve blokkering. Sentinel valideert |
| **OV-04** | Moet de lokale desktop op de server zelf ook inloggen, en mag de server op andere interfaces luisteren? | Ja, ook lokaal inloggen (één gate voor alles). Standaard alleen localhost en tailnet; een ander adres alleen via een expliciete instelling met een waarschuwing |
| **OV-05** | Hervatten runs vanzelf na het (opnieuw) koppelen van een login, of pas na een klik op "Opnieuw"? | Vanzelf, net als na een quotumreset |
| **OV-06** | Wat gebeurt er met lopende of wachtende tickets van een board member die gedeactiveerd wordt? Mag een andere board member het ticket "overnemen", zodat het op diens login verder loopt? | Geen stille overname. Een board member kan het ticket expliciet overnemen (wordt de nieuwe aanmaker, staat in de discussie), en vanaf dan loopt het op diens login. Dat is een bewuste keuze, geen account-sharing |
| **OV-07** | Welke login gebruiken schedules (B7: gepauzeerd geïmporteerd)? | De login van de board member die de schedule (weer) activeert. Die wordt op de schedule vastgelegd en is in de UI te zien |
| **OV-08** | Moet de server met een Claude-login van de daemon kunnen draaien voor werk zonder board member (bijvoorbeeld extracties of probes)? | Nee. Alle Claude-runs hebben een board member als bron (MU3-10). De daemon-login wordt niet meer gebruikt voor runs |
