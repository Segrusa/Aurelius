# User stories: employees als bots

- **Bron:** [README.md](README.md) B1-B6 en [../bots-feature-set.md](../bots-feature-set.md).
- **Regel:** bij een verschil gaat de README voor. Wat de bron niet vastlegt staat als aanname (A-xx).
- **Prioriteit:** M = Must, S = Should, C = Could.

## Aannames

- **A-01:** een lead kan een mens zijn. Op een ticket met Responsible is geen employee lead; de owner is daar dus een niet-lead en leest ook wat een board member zonder naam schrijft, omdat dat aan hem gericht is.
- **A-02:** "alleen in de thread" betekent: geen andere employee schreef er of werd er genoemd, in de laatste 60 berichten.
- **A-03:** een antwoord dat Aurelius kopieert naar de discussie wekt niemand (RULE 08). Sinds stap 6 (B7) is een handoff de tool `hand_off`; een post wekt niemand meer.

## Stap 1: promptregels

**BOT-01 Handoff (M, P1).** *Als* board member *wil ik* dat een employee afsluit met wie er nu aan zet is, *zodat* werk niet blijft liggen.
- *Given* een run van een employee, *Then* begint de systeemtekst met het platformblok, met de handoff-regel.

**BOT-02 Lead (M, P2).** *Als* employee *wil ik* weten wie de thread leidt, *zodat* ik bij twijfel daar terechtkan.
- *Given* een wekbericht uit een ticket met Responsible *Then* noemt het die als lead. *Given* een kanaal *Then* de dispatcher of CEO. *Given* een ticket zonder actieve Responsible *Then* de owner.

**BOT-03 Scoped view (M, P3, B2).** *Als* board member *wil ik* dat een employee die niet leidt alleen krijgt wat hem aangaat, *zodat* delegatie goedkoop blijft en handoffs op zichzelf staan.
- *Given* een niet-lead die niet alleen is, *Then* bevat zijn prompt alleen berichten die hem noemen en zijn eigen, plus hoeveel andere er zijn en `read_discussion` of `read_group_chat`.
- *Given* de lead of de enige employee, *Then* de hele thread zonder notitie.
- *Given* de owner van een ticket, *Then* ook de berichten van een board member zonder naam.
- *Given* welke employee dan ook, *Then* blijft de hele thread leesbaar in de UI en met de tool.

**BOT-04 Zelfstandige handoffs (M, P4).** Het platformblok en `delegate_task` zeggen: doel, paden of PR en branch, criteria, wat te melden; nooit "see above".

**BOT-05 Triggers (M, P5).** *Given* een bericht dat deze run wekte, *Then* eindigt zijn label op `(new)`. *Given* dat het binnenkwam voor het vorige antwoord af was, *Then* `(new, arrived before your last reply; it may already be handled)`, en het platformblok zegt wat dan te doen. Een gewoon bericht van de gebruiker krijgt alleen die tweede markering; een slash command nooit.

**BOT-06 Beschrijving (M, P7).** *Als* board member *wil ik* per employee zeggen waarvoor die er is.
- *Given* een beschrijving (max. 160 tekens, één regel), *Then* staat ze in de lijst van collega's als `- @Naam (rol): beschrijving`. *Given* geen beschrijving, *Then* de eerste instructieregel.
- *Given* een lege tekst bij `employee.update`, *Then* is de beschrijving weg; *Given* geen veld, *Then* blijft ze.

**BOT-07 Signatuur (S, P8).** Het platformblok vraagt op externe systemen een laatste regel `Aurelius - @Naam`.

**BOT-08 Versie (M, B5).** *Given* een nieuwe versie van het platformblok, *Then* start de volgende run van elk gesprek een nieuwe sessie.

## Stap 3: geheugen

**BOT-09 Scope-regel (M, C5, B6).** *Given* de extractie na een taak of een `remember`, *Then* staat de regel erbij dat feiten van één ticket of taak in de discussie horen.

## Stap 4: run card, interrupt card en odometer

**BOT-12 Run card (M, U1).** *Als* board member *wil ik* onder een post of een `Reported`-regel zien wat de run deed, *zodat* ik niet naar de taak hoef.
- *Given* een run die meer dan eens postte, *Then* staat de kaart alleen op zijn laatste post in de lijst.
- *Given* een lopende run, *Then* pulseert de status en kan ik *Cancel* drukken; *Given* een geopende kaart, *Then* zie ik elke tool call met zijn resultaat.

**BOT-13 Interrupt card (M, U2).** *Given* een open vraag of approval van een run op dit ticket of uit dit kanaal, *Then* staat dezelfde kaart als in de Inbox in de discussie. *Given* een post waarmee de hoofdtaak op een vraag eindigde, *Then* krijgt die een antwoordveld, tenzij er al een vraagkaart van die run open staat.

**BOT-14 Odometer (S, U4, C11).** *Given* een ticket of kanaal met verbruik, *Then* staan calls, tokens in en uit en kosten in de kop; *Given* usage van voor deze stap, *Then* blijft Calls weg in plaats van 0 te tonen. *Given* het Usage-scherm, *Then* een tabel per board member (per Claude-login, 30 dagen).

## Latere stappen (samengevat; AC's bij de start van de stap)

| Id | Prio | Stap | Wat |
|---|---|---|---|
| BOT-10 | M | 2 | Provider, model en effort per employee, met Inherit (C1) |
| BOT-11 | M | 2 | Toegangsniveau per employee met expliciete `--tools`, in de sessiesleutel (C3) |
| BOT-15 | M | 5 | Elk wekpad schrijft een inbox-item; de worker leest alleen daaruit (R1) |
| BOT-16 | M | 5 | Eén run per employee; wachtende items van een thread worden samengevoegd (R2) |
| BOT-17 | M | 5 | Een thread die op een antwoord wacht is geparkeerd (R3) |
| BOT-18 | M | 5 | `--resume` binnen een thread, herbouw als terugval (R6) |
| BOT-19 | M | 6 | Hop op elk bericht, ook in delegaties; een mens zet hem op nul (R4). `TICKET_MAX_HOPS` 8 op een ticket, 3 in een kanaal |
| BOT-20 | M | 6 | Handoff vastgehouden zolang de zender ongelezen items heeft, met een regel "handoff held"; daarna ongewijzigd bezorgd (R5) |
| BOT-31 | M | 6 | `hand_off(to, message)` is de enige manier om een collega te wekken; een `@Naam` van een employee is alleen een verwijzing (B7) |
| BOT-21 | S | 7 | `schedule_message` (R7), scratchpad (C7), waiting indicator (U3), Scheduled-pagina (U7) |
| BOT-22 | S | 8 | Toollijst, approval-vinkjes en MCP per employee (C2) |
| BOT-23 | S | 9 | Startprofielen bij een nieuwe employee (C6, P6) |
| BOT-24 | S | 10 | App key als actor met `@handle` (C9) |
| BOT-25 | C | 10 | Telegram als extern kanaal (C10) |
| BOT-26 | S | 11 | Activity log met pagina (R8) |
| BOT-27 | S | 11 | Employee-pagina met Inbox, Memory, status-dot (U6) |
| BOT-28 | C | 12 | Living Office (U8) |
| BOT-29 | M | 5-11 | Notificaties alleen volgens P-82: niet voor run gestart of gestopt, delegaties of berichten zonder mention |
| BOT-30 | M | 5-6 | Bewerken of verwijderen wekt niemand en telt geen hop |
