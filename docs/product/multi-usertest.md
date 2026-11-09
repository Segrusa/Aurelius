# Multi-usertest: draaiboek (ticket #44)

*Titan · 2026-11-09 · basis: main @ 1a19f67 (fase 1 t/m 4 + tickets 40-43 gemerged), `docs/tailscale.md`, `docs/mobile.md`, `target/aur-mu-03/eindrapport.md`, `target/aur-mu-04/eindrapport.md`, `spike-claude-logins.md`*

De gereedheid (§1) komt uit de code, de docs en de eerdere QA-rapporten. Op verzoek van de Owner zijn de scenario's daarna uitgevoerd op een aparte dev-instantie, zie §5. De test met echte mensen doet de Owner met de testers.

## Go / no-go

**GO, onder voorwaarden.** Een tweede board member kan vandaag worden toegevoegd. Er is geen blokkerende bug bekend, en de scenario's slagen op serverniveau (§5). Voor de test moeten wel vier dingen geregeld zijn (zie §2, de rijen "vóór de test"): Tailscale-ACL en uitnodiging, een eigen Claude-abonnement voor de tester, je akkoord op de licentiekwestie, en je akkoord op het restrisico "alle runs als dezelfde OS-user" (testers zijn vertrouwde mensen, geen vreemden).

## 1. Gereedheid: tweede board member toevoegen

Wat er al staat (op main, getest door QA in eerdere rondes met testdaemons, nooit met twee echte mensen):
- Gebruikers met naam + wachtwoord, sessies, rate limit, tijdelijk wachtwoord dat de tester bij de eerste login moet wijzigen, intrekken van een sessie of apparaat.
- Claude-login per board member (Connect via een terminal in de app; map `<data>/logins/<user>/`, 0700), ticketruns draaien op de login van de aanmaker, quota per login.
- Tailscale Serve: `https://omarchy.tailc72e64.ts.net:8443` staat nu aan (gecontroleerd: `tailscale serve status`, tailnet only, naar 127.0.0.1:47613).
- Team chat, ticketdiscussies, @mentions en @Everyone (T40), approvals, vault.

Stappen voor de Owner (jij doet het, niet een agent):

| # | Stap | Wie | Opmerking |
|---|---|---|---|
| 1 | Tester heeft Tailscale en een account | tester | Geen Tailscale-account = geen toegang. |
| 2 | Tester toegang geven tot het tailnet: uitnodigen of de machine delen, en de ACL zo zetten dat alleen `group:aurelius` poort 8443 (en 8444 voor dev) bereikt, **niet** ssh 22 (`docs/tailscale.md` §3) | Owner | Eerst akkoord van jou: dit nodigt een extern persoon uit. Een gedeelde machine (node sharing) is de minst brede optie als de tester een eigen tailnet heeft. |
| 3 | Desktop: Settings → Users → nieuwe board member, met tijdelijk wachtwoord | Owner | Geef het wachtwoord via een apart kanaal. Bij de eerste login moet de tester het wijzigen. |
| 4 | Desktop of mobiel: Servers → toevoegen met het adres en de code. Mobiel: op jouw desktop `N` (QR of 8 tekens, 5 minuten, één keer), de tester scant | Owner + tester | Een desktop-client van een tweede computer koppelt op dezelfde manier (Settings → Servers). |
| 5 | Tester logt in met naam en wachtwoord | tester | |
| 6 | Tester koppelt de eigen Claude-login: Settings → Claude login → Connect, browserlogin met het eigen account | tester | Zonder koppeling krijgen zijn tickets `needs: login` (Inbox "Run on my login" is bedoeld voor iets anders, zie §3 S6). |
| 7 | Sessiecontrole: `user.whoami` in de app toont de juiste naam, ticket aanmaken door de tester doet de run op zijn login | tester | |

Wat je nodig hebt: een eigen Claude-abonnement per tester (zie R2), Tailscale op het toestel van de tester, de desktop- of mobiele app (Linux/Windows/iPhone; Android alleen zolang de app open is, geen push).

## 2. Blokkades en risico's

"Vóór de test?" = moet opgelost of expliciet aanvaard zijn voordat de testers erop gaan.

| # | Punt | Effect | Vóór de test? | Voorstel |
|---|---|---|---|---|
| R1 | **Alle runs draaien als dezelfde OS-user.** Een run van Alex kan in theorie de login van Robin lezen. Mitigatie: deny-regels op `<data>/logins/**` behalve de eigen map (Read/Grep/Glob/Bash-cat). Handhaving is nooit bewezen met een echte login (Sentinel SEC2 M1/SEC3): Grep/Glob vanuit een map boven `logins/` is "best-effort". Sandbox-isolatie is een follow-up. | Tokens van de ander kunnen lekken, via een run met een slechte prompt. | **Aanvaarden (jij) of E2E doen.** Geen codefix nodig voor een test met vertrouwde testers. | Test alleen met mensen die je kent. Geef geen tester ssh (ACL, §1 stap 2). Optioneel (30 min, door jou): canary-bestand in een login-map en een run die ernaar zoekt, om de deny-regels echt te bewijzen. Sandbox-isolatie als aparte fase na de test. |
| R2 | **Licentie Claude-seats.** Pro/Max valt onder de Consumer Terms: geen account delen. Elke tester logt zelf in op de ongewijzigde `claude` met het eigen abonnement (zo is Aurelius gebouwd; er wordt geen token bewaard). Zware parallelle agent-runs gaan verder dan "ordinary, individual usage"; Anthropic kan limieten zonder aankondiging afdwingen. Voor een commerciële company zijn Team/Enterprise-seats of een API-key het juridisch zuivere pad. | Account kan worden beperkt of geblokkeerd. Nooit je eigen login aan een tester geven. | **Ja, beslissing van jou.** Dit is een aankoop/juridisch punt, niet technisch. | Voor een korte test: elke tester met het eigen persoonlijke abonnement en een beperkt aantal gelijktijdige runs. Voor structureel gebruik: Team-seats. De voorwaarden veranderen geregeld; lees ze vlak vóór de test na (links in `spike-claude-logins.md` §4). |
| R3 | **MU3-01 tweede echt account nooit getest.** De scheiding van quota en logins is getest met een tijdelijke map op één account en met gesimuleerde quotafouten. | Mogelijk onverwacht gedrag bij de eerste echte tweede login (bv. Connect, `/usage`). | Nee, dat is juist wat de test oplevert. | Scenario S1 is de eerste check; meld elke afwijking als bug. |
| R4 | Tailscale-ACL / uitnodiging van een extern persoon | Zonder ACL ziet de tester te veel van je tailnet. | **Ja** (akkoord Owner). | Zie §1 stap 2. Controleer na de koppeling: de tester bereikt alleen 8443. |
| R5 | Productiedata. Jouw echte tickets, chats en vault staan op dezelfde server; de tester ziet alle projecten, tickets en chats (één company, één rol). | Tester leest je werk; vault-geheimen zijn alleen via `$NAME` voor runs, maar board members kunnen items tonen (met audit). | **Ja, jouw keuze.** | Zet gevoelige vault-items niet in de company-vault tijdens de test, of maak een testproject. Maak voor de test een eigen project "Multi-test" en laat de scenario's daar draaien. |
| R6 | Open minors uit fase 3/4: `remote_clients.user_id` blijft na logout (push naar een uitgelogd apparaat), veld "Temporary password" is leeg maar verplicht na een tijdelijk-wachtwoord-login op desktop (QA4 n1), geen push voor Android, reset/disable van een ander zonder bevestiging (SEC1 m12). | Hinderlijk, geen dataverlies. | Nee. | Bekend; niet als nieuwe bug melden. |
| R7 | Eén gedeelde Claude-capaciteit per server: veel runs tegelijk door meerdere users, elk op de eigen login. | Quota-parkeren per login ("Waiting for X's limit"), geen fallback naar een andere login. | Nee. | Dit is gewenst gedrag; S6 test het. |
| R8 | De hele multi-userstack is nog nooit met twee mensen tegelijk gebruikt. | Onbekende races in lease/notificaties (zie §3 S5, S7). | Nee. | Daarom is dit de test. |

**Blokkerende fouten: geen bekend.** Er is geen open bug in de code of in de QA-rapporten die de test onmogelijk maakt. Zodra een tester iets vindt dat een scenario onmogelijk maakt, staat het in het blokkadelogboek van §4 met een voorstel.

## 3. Draaiboek: 2-3 gebruikers tegelijk

Voorbereiding: een testproject "Multi-test", drie accounts (jij = Alex, tester 1 = Robin, optioneel tester 2 = Sam), elke tester gekoppeld aan de eigen Claude-login (§1). Houd per scenario een korte notitie: wat verwacht, wat gebeurde, tijd, scherm (desktop/mobiel).

| # | Scenario | Stappen | Verwacht |
|---|---|---|---|
| S1 | Login en eigen Claude-login | Robin logt in, koppelt Claude (Connect), maakt een ticket, start het. | De run draait op Robins login (Settings toont zijn e-mail en plan), niet op die van Alex. Zonder koppeling: ticket krijgt `needs: login`. |
| S2 | Team chat (algemeen kanaal en projectkanaal) | Alex en Robin posten tegelijk, ook vanaf mobiel; Sam leest mee. | Berichten verschijnen live bij iedereen, juiste auteur, geen dubbele of verloren berichten, volgorde logisch. |
| S3 | Ticketdiscussie en @mentions | Alex post in een ticket, noemt `@Robin` en `@Forge`. Robin antwoordt zonder mention. | Mention geeft Robin een melding; `@Forge` wekt de employee; een bericht zonder mention in Review/Doing bereikt de eigenaar (ticket #14/#42). In Plan review wekt het niemand: het wacht in de taak van de eigenaar tot de volgende run. |
| S4 | @Everyone | Alex post `@Everyone` in een kanaal en een ticketdiscussie. | De publiekslijn ("audience") staat vóór het versturen. In een kanaal krijgen alle andere board members een melding; in een ticket alleen volgers, leden en de Responsible. Wie al een ongelezen melding op dat ticket heeft, houdt één rij (die wordt samengevoegd). De employees in de thread worden gewekt; een `@Everyone` van een employee wekt niemand. |
| S5 | Approvals: wie mag beslissen | Een run vraagt een approval (bv. een commando of plan). Alex en Robin zien het allebei. Beide proberen te beslissen, gelijktijdig. Aanmaker versus niet-aanmaker. | Eén beslissing telt; de tweede krijgt een duidelijke melding ("al beslist"), geen dubbele uitvoering. Noteer of een niet-aanmaker mag beslissen en of dat zo bedoeld is (alle board members hebben één rol). |
| S6 | Quota en login per user | Robin laat veel runs lopen tot zijn limiet (of simuleer via een kleine run). Alex werkt door. | Alleen Robins runs wachten ("Waiting for Robin's limit until …"); Alex loopt door; na reset hervat Robin. Geen fallback naar Alex' login. "Run on my login" is een bewuste keuze van de lezer (Inbox van de aanmaker) en mag alleen zijn eigen login gebruiken. |
| S7 | Leasen en notificaties | Twee tickets tegelijk gestart door Alex en Robin; kijk naar Inbox en meldingen op desktop en mobiel van beiden; sluit de app van Robin en kijk of de melding later komt (iPhone: push). | Elke board member krijgt alleen de meldingen voor zijn eigen tickets en mentions; jobs worden maar door één run geleased; geen dubbele runs op dezelfde job. |
| S8 | Gelijktijdig werk aan hetzelfde ticket | Alex en Robin bewerken tegelijk titel/beschrijving, slepen het ticket naar een andere kolom, voegen bestanden toe en posten in de discussie. Terwijl een run bezig is. | Tegelijk bewerken van hetzelfde veld: de laatste schrijver wint, zonder conflictmelding, en de ander ziet de update live. Slepen naar Backlog annuleert het lopende werk; een ticket blijft in Doing zolang een handoff/run open is (#42); Done alleen door een board member. |
| S9 | Intrekken en wachtwoord | Alex trekt Robins sessie of apparaat in; reset Robins wachtwoord. | De client van Robin valt uit binnen seconden, krijgt geen events meer; na reset moet het wachtwoord gewijzigd worden. |
| S10 | Vault (kort) | Alex zet een item in het testproject; Robin leest de naam, toont de waarde (audit). | Waarde komt nooit in chat, discussie of prompt; de audit toont wie wat toonde. |

## 4. Terugkoppeling

- **Waar:** elke bevinding wordt een **ticket in Backlog** in het project "Multi-test" (door de tester of door jou, titel `[MU-test] Sx: korte beschrijving`). Een chatbericht is geen bug-melding.
- **Inhoud:** scenario-nummer, wie (gebruiker), apparaat (desktop/mobiel, OS), tijdstip, stappen, verwacht versus gezien, screenshot, en of het reproduceerbaar is. Geen wachtwoorden, tokens of vault-waarden in een melding.
- **Ernst:**
  - **Blokkerend:** de test of een scenario kan niet door, dataverlies, een lek van data van een andere user, een ander dan de aanmaker die op diens login rekent. Dit komt ook direct bij jou, met een voorstel van mij.
  - **Bug:** fout gedrag met een reproduceerbaar effect.
  - **Idee/minor:** hinder of wens.
- **Fixrondes:** alleen echte bugs starten een fixronde (ik verdeel het werk), met alleen de tests van de geraakte crates. Ideeën en minors worden gebundeld na de test.
- **Blokkadelogboek:** (leeg bij de start) per blokkerende fout: ticket, scenario, voorstel.
- **Na de test:** één samenvatting door mij met aantal bevindingen per ernst en een advies voor Team-seats, sandbox-isolatie en vervolg.

## 5. Uitvoering op een dev-instantie (2026-11-09)

Testopzet: debug-build van main 1a19f67, `test-daemon.sh` met eigen datamap in `/tmp`, geen Tailscale, een nep-`claude`, drie gelijktijdige board members via het protocol over IPC. De live daemon en data zijn niet aangeraakt. Bewijs staat in `target/ticket-44/`.

**Niet gedekt:** de desktop- en mobiele schermen, de remote gateway met pairing, push-meldingen, een echt tweede Claude-account (R3), en de deny-regels met een echte `claude` (R1). De nep-`claude` voert geen tools uit. Die punten blijven voor de test met mensen.

| # | Uitkomst | Wat is waargenomen |
|---|---|---|
| S1 | PASS | Robins run draait op Robins loginmap, Sam zonder login krijgt `needs: login` en er start niets. Loginmappen 0700. |
| S2 | PASS | 2×30 berichten tegelijk: 60 van 60, geen dubbele, dezelfde volgorde bij alle drie, juiste auteur, live events bij iedereen. Herhaalde idempotency-key wordt `replayed`. |
| S3 | PASS | `@robin` geeft Robin een melding en Sam niet, `@Forge` wekt Forge. Zonder mention tijdens een run: vervolgrun van de eigenaar. |
| S4 | PASS | Publiek klopt per plek, verzender telt niet mee. Kanaal: robin en sam elk +1. |
| S5 | PASS | 6 races alex-goedkeuren tegen robin-weigeren: telkens precies één geslaagd, de run kreeg de beslissing van de winnaar, de ander "already handled". Een verkeerde digest geeft geen toestemming. Elke board member mag beslissen, ook zonder band met het ticket. |
| S6 | PASS | Robin op limiet: alleen Robin wacht, Alex loopt door op de eigen login, Robin hervat op de eigen login, geen fallback. Hervatten gebeurt op een afgeronde minuut. |
| S7 | PASS | 12 tickets tegelijk door 2 users: 12 unieke nummers, elk exact 1 run, geen dubbele jobs, meldingen alleen bij de aanmaker (6/6, Sam 0). Replay na herverbinden werkt. |
| S8 | PASS, met opmerking | 3 schrijvers tegelijk: geen fouten, verschillende velden blijven behouden, op hetzelfde veld wint de laatste zonder conflict (M1). Done zonder Accept en Accept buiten Review geweigerd. Plan goedkeuren door 3 tegelijk: één geslaagd, één run. |
| S9 | PASS | Intrekken verbreekt de verbinding meteen. Reset beëindigt de sessies, daarna is alles geblokkeerd tot het wachtwoord gewijzigd is. Oud wachtwoord werkt niet meer. |
| S10 | PASS | Waarde niet in lijst, wel via reveal met audit (wie, wat). Waarde in chat wordt eerst tegengehouden; na bevestigen opgeslagen als `«secret:QA_TOKEN»`. De waarde staat in geen enkel bestand in de datamap. |

Daemonlog: 0 errors of panics. 49 runs, geen vastgelopen job.

**Blokkerende fouten: geen gevonden.**

| # | Ernst | Bevinding | Voorstel |
|---|---|---|---|
| M1 | Minor | `ticket.update` kent geen versiecheck: bij gelijktijdig bewerken van hetzelfde veld gaat de eerste wijziging zonder melding verloren (de ander ziet wel de nieuwe waarde live). | Na de test: optionele `expected_version` en een conflictmelding. Geen fix vooraf. |
| M2 | Minor | `ticket.use_my_login` zonder eigen Claude-login antwoordt succes, maar het ticket blijft op `needs: login`. | Weigeren met een duidelijke fout. Geen fix vooraf; alleen zichtbaar als de knop aan iemand zonder login wordt getoond. |
| O1 | Let op | Elke board member kan het wachtwoord van een ander resetten, ook dat van de Owner, en zet die daarmee uit (bewezen: robin → alex; zie ook R6). Slepen naar Backlog annuleert andermans lopende werk. | Past bij één rol, maar spreek het af met de testers. |
| O2 | Let op | De server start niet alles tegelijk (`load:max_runs`, `load:cpu`). | Testers kunnen wachttijd zien; niet als bug melden. |

## Wat de Owner nu moet beslissen of doen

1. Akkoord voor het uitnodigen van de tester(s) in het tailnet en de ACL (R4).
2. Beslissing over de licentie van de Claude-seats (R2): eigen persoonlijke abonnementen voor een korte test, of eerst Team-seats.
3. Aanvaarden van het restrisico R1 (en optioneel de deny-E2E met een canary).
4. Bepalen of de test in een apart project "Multi-test" gebeurt (R5, aanbevolen).
