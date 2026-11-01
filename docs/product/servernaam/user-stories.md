# User stories Servernaam

- **Task:** ticket #1 "Ik zou graag de servernaam kunnen wijzigen" (Nova, gevraagd door de Owner)
- **Bron:** de vraag van de board member en de huidige code (zie "Huidige situatie").
- **Status:** klaar voor de bouw. OV-01 is beslist door de Owner (2026-11-01): één naam op de server (SN-01/02 Must). SN-03 blijft Could en valt buiten deze bouw.
- **Regel:** wat de bron niet vastlegt, staat als **aanname** (A-xx) of **open vraag** (OV-xx).

## Huidige situatie

- De servernaam is de naam van de computer, in kleine letters: `aurelius_host::host_name()` (`crates/host/src/lib.rs:277`) leest `COMPUTERNAME`/`HOSTNAME` (op macOS `scutil --get ComputerName`). Valt dat weg, dan is de naam "this computer". In Aurelius kun je de naam nergens instellen.
- De daemon geeft de naam door in `HelloResult.host_name` (`crates/daemon/src/api/mod.rs:559`), in de koppeltekst en in de poll van de gateway (`crates/remote-gateway/src/lib.rs:366`, `http.rs:103`).
- Clients bewaren de naam **bij het koppelen** en werken hem daarna niet meer bij:
  - Desktop: `SavedServer.host_name` in `<data>/client/servers.json` (`crates/client/src/servers.rs`). Die naam verschijnt in Settings → Servers, in "Connecting to …", in "Choose a folder on …" en in de koppelmelding. Alleen `ServerInfo.host_name` volgt de hello.
  - Mobiel: `Host.name` (`clients/mobile/lib/core/hosts/host.dart`). Die naam staat in de lijst van servers, de titel in Settings en de Power-sectie.
- De servercomputer zelf staat op de desktop als het vaste label "This computer".

## Aannames

- **A-01: één naam per server.** De naam hoort bij de server (de daemon), niet bij een company of project. Een server kan meerdere companies hebben, maar heeft één naam.
- **A-02: weergavenaam, geen netwerknaam.** De wijziging verandert alleen de getoonde naam. De OS-hostname, de Tailscale/MagicDNS-naam, de URL en `host_id` blijven ongewijzigd. Een koppeling blijft dus werken en niemand moet opnieuw koppelen.
- **A-03: leeg = standaard.** Zonder ingestelde naam blijft het huidige gedrag: de computernaam. Een lege naam opslaan zet de standaard terug.
- **A-04: wie mag het.** Alleen een board member (`require_board_member`), zoals andere instellingen van de server. Apps (`NOT_FOR_APPS`) en employees in een run kunnen het niet. Over een remote verbinding is de Owner-capability nodig, zoals bij de andere host-instellingen (Titan bevestigt dat in `remote.rs`).
- **A-05: grenzen.** 1–40 tekens na trimmen, geen stuurtekens of regeleinden, emoji en accenten mogen. Twee servers met dezelfde naam zijn toegelaten, want de server is gekend via `host_id`.

## User stories

Prioriteit: **M** = Must, **S** = Should, **C** = Could.

#### SN-01: Servernaam instellen (M)
**Als** board member **wil ik** de naam van mijn server wijzigen, **zodat** ik mijn servers herken aan een naam die ik zelf kies (bv. "Kantoor" of "Thuis-PC") in plaats van de computernaam.

- **AC1.** *Given* de instellingen van de server (desktop: Settings, mobiel: Settings van de server), *When* ik ze open, *Then* zie ik de huidige servernaam en, als ik er een gekozen heb, ook de computernaam als standaard ("standaard: ‹hostname›").
- **AC2.** *Given* een geldige nieuwe naam (A-05), *When* ik opsla, *Then* geeft elke nieuwe hello en elke nieuwe koppeltekst die naam.
- **AC3.** *Given* een lege naam, *When* ik opsla, *Then* komt de computernaam terug (A-03).
- **AC4.** *Given* een naam van meer dan 40 tekens of met een stuurteken of regeleinde, *When* ik opsla, *Then* weigert de server de naam met de reden. De oude naam blijft.
- **AC5.** *Given* een app-sessie, een niet-board-member of een remote apparaat zonder Owner, *When* het de naam probeert te wijzigen, *Then* krijgt het `board_member_required` of `Forbidden`. De client toont de bewerkknop dan niet.
- **AC6.** *Given* dezelfde naam als nu, *When* ik opsla, *Then* gebeurt er niets: er komt geen event en geen nieuwe versie.
- **AC7.** *Given* een wijziging, *When* ze opgeslagen is, *Then* staat ze in de audit/timeline met wie, wanneer, de oude en de nieuwe naam.

#### SN-02: Alle clients tonen de nieuwe naam (M)
**Als** board member **wil ik** dat de nieuwe naam overal verschijnt, **zodat** iedereen dezelfde naam ziet.

- **AC1.** *Given* een client die verbonden is, *When* de naam wijzigt, *Then* ziet die client de nieuwe naam binnen een paar seconden, zonder opnieuw te verbinden (via een event).
- **AC2.** *Given* een gekoppelde client die op dat moment niet verbonden was, *When* die opnieuw verbindt, *Then* overschrijft de naam uit de hello de bewaarde naam (desktop `servers.json`, mobiel `Host.name`), zodat de lijst van servers de nieuwe naam toont, ook als je hem daarna niet gebruikt.
- **AC3.** *Given* een oudere client, *When* de naam wijzigt, *Then* negeert die het onbekende event en blijft hij werken. Hij toont de oude bewaarde naam tot hij zelf bijwerkt (aanvaard).
- **AC4.** *Given* een oudere server zonder deze feature, *When* een nieuwe client verbindt, *Then* verbergt de client de bewerkknop (feature in `FEATURES`).

#### SN-03: Eigen label op dit apparaat (C, zie OV-01)
**Als** gebruiker met meerdere servers **wil ik** een server op mijn eigen apparaat een andere naam geven, **zodat** ik ze uit elkaar houd zonder de naam voor anderen te veranderen.

- **AC1.** *Given* een gekoppelde server in de lijst, *When* ik "Rename" kies, *Then* bewaart alleen dit apparaat het label. Ik heb daar geen board member voor nodig.
- **AC2.** *Given* een lokaal label, *When* de servernaam wijzigt, *Then* blijft mijn label staan. Het menu toont ook de servernaam ("‹label› · ‹servernaam›").
- **AC3.** *Given* een lokaal label, *When* ik het leegmaak, *Then* toont het apparaat weer de servernaam.

## Edge cases

- **E1.** Twee board members slaan tegelijk een naam op: de laatste wint, en beide wijzigingen staan in de audit. Een versieconflict is hier niet nodig (OV-03).
- **E2.** Een koppeling die bezig is: de wachtmelding "Waiting for "Allow as owner" on X" toont de naam van de koppeltekst. Komt de naam uit de poll anders terug, dan geldt die (`remote.rs:225` doet dat al).
- **E3.** "This computer" op de desktop (de lokale service) blijft "This computer" in de lijst van servers, met de servernaam ernaast als detail (OV-02).
- **E4.** Een naam met alleen spaties = leeg = standaard (A-03).
- **E5.** Een backup terugzetten zet ook de naam terug naar wat in de backup staat, als de naam in de database staat (Titan beslist waar de naam staat).

## Open vragen

- **OV-01 — beslist: (a), één naam op de server.** Oorspronkelijke vraag: (a) **één naam op de server**, die alle apparaten en board members zien (SN-01/02), of (b) **een eigen label per apparaat**, alleen voor jezelf (SN-03). Voorstel: (a) als Must, (b) als Could. Alleen (b) is veel kleiner: puur client, zonder protocol.
- **OV-02:** Moet "This computer" op de desktop de servernaam tonen in plaats van het vaste label? Voorstel: label houden en de naam als detail tonen (E3).
- **OV-03:** Is "de laatste wint" goed genoeg, of wil je een versie zoals bij guidelines? Voorstel: de laatste wint, want het is één kort veld.
- **OV-04 (Titan):** Waar staat de naam: in de database (gaat mee in een backup en de audit is gratis) of in een configbestand van de host? Voorstel: de database, via een command met event en receipt.
