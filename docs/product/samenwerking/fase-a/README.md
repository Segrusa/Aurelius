# AUR-FA: fase A, samenwerken per persoon (ter goedkeuring)

*Orion · 2026-11-02 · branch `docs/aur-fa` (lokaal) · basis: `main` 53cb385, `../analyse-linear.md` hoofdstuk 5 en 6, besluiten V1 en V2*

| Document | Auteur | Inhoud |
|---|---|---|
| [../analyse-linear.md](../analyse-linear.md) | Nova | Analyse, user stories US-A0 t/m US-A6 |
| [ui-ontwerp.md](ui-ontwerp.md) | Pixel | Desktop en mobiel, keuzes P-71..92 |
| [impactanalyse.md](impactanalyse.md) | Titan | Migratie 0029, protocol, schatting, volgorde |

**Scope:** een menselijke verantwoordelijke op een ticket (*Responsible*), volgen, een persoonlijke Inbox met push, *My tickets*, links naar ticket en bericht, en berichten bewerken en verwijderen.
**Buiten scope:** alles uit fase B-D, een handmatig werkpad (V2), rollen en uitnodigingen (V1), een webclient, en tickets heropenen (bestaat niet, zie impactanalyse §5.5).
**Protocol:** additief binnen 5, maar een feature alleen is niet genoeg: nieuwe events gaan achter `accepts`, en nieuwe meldingssoorten worden tekst in plaats van een enum (impactanalyse §5.1).

## Stand

**A0 is klaar:** de bug is gereproduceerd en opgelost op `fix/aur-fa-a0` (7db01b0 falende tests, efbe7ac fix met migratie 0028, alleen toevoegingen). Niet gemerged, niet gepusht.
- Zelf nagedraaid: `cargo test -p aurelius-storage notification` geeft 10 geslaagd, 0 mislukt.
- Rapport van Titan: storage 189 geslaagd. De daemon-suite draaide vóór de laatste testaanpassing in storage en gaf toen alleen de bekende fout in `connections.rs:154`.
- Wat al gelezen was vóór de migratie, blijft voor iedereen gelezen, want wie het las is nooit bewaard.
- Gedeelde meldingen worden niet meer verwijderd, dus de tabel groeit en een nieuwe board member ziet oude meldingen als ongelezen. Dit nemen we mee bij A3.

## Schatting en planning

**≈ 44 werkdagen, doorlooptijd ≈ 3 weken**, omdat de client-sporen parallel lopen. Zonder A5b ≈ 39,5 d. De maten van de analyse waren te laag: A3 is 8 d (M in de analyse), A5 is 8 d (S in de analyse) en A6 is 5 d (S in de analyse).

Sebastien noemde geen deadline. De data hieronder gaan uit van een start op 2026-11-03 en de schatting van Titan. Eerdere fases waren sneller klaar dan hun schatting, maar daar plannen we niet op.

| Mijlpaal | Inhoud | Klaar uiterlijk |
|---|---|---|
| M1 | Protocol-freeze, migratie 0029, A1 Responsible, A2 volgen, A5a links in de app | 2026-11-06 |
| M2 | A3 persoonlijke Inbox en push | 2026-11-13 |
| M3 | A4 My tickets, A6 berichten bewerken, A5b link-scheme op OS-niveau | 2026-11-17 |
| M4 | Sentinel, Trace, E2E met twee board members, één fixronde, één keer de volledige suite | 2026-11-24 |

**Werkwijze:** één integratiebranch `feat/aur-fa` met subbranches per engineer, afgetakt van `fix/aur-fa-a0`, omdat A3 dezelfde code in `timeline.rs` raakt. Titan leidt de bouw. Eén slotronde aan het eind. Merge naar `main` en push alleen met akkoord van Sebastien.

## Verschillen tussen Titan en Pixel die ik heb beslist

| Onderwerp | Besluit | Reden |
|---|---|---|
| Fragment van het bericht in een Inbox-rij (Pixel §4) | **Vervalt.** De rij toont *Forge mentioned you*, zonder tekst | Titan §2: dan komt er geen berichttekst in meldingen en hoeft er niets extra door de redactor |
| Linkvorm (P-88, B-P10) | Vorm van Titan: `aurelius://<host_id>/ticket/<id>` en `/message/<id>`, de plek via `message.locate` | Ids zijn stabiel en een bericht vindt zijn plek zelf. In de UI blijft het `#42` |
| Veldnamen | `responsible_user_id` op de wire; read en feature heten `my_tickets` | Eén woord in UI en code, zoals Pixel vraagt |
| Kop van een samengevoegde rij (Titan B5, P-83) | Belangrijkste reden (P-83), niet de laatste. Pushregel van Titan | Een mention mag *Ready for your review* niet verdringen |
| Afgehandelde ◆-rij | Verdwijnt (Titan B7), geen regel *Marie handled this* | Volgt het bestaande principe en is eenvoudiger |
| Verwijderde tekst in `message.history` (Titan B10, B-P14) | Elke board member ziet oude versies van een bewerkt bericht, maar niet de tekst van een verwijderd bericht | "Verwijderd" moet in de client ook echt weg zijn; de audit blijft |
| Mention in een ticketbeschrijving | Alleen board members, wekt geen employee | Een beschrijving wijzigen start nu ook geen run |
| Naam *Pulse* | Titan vond geen besluit over het vertrek | Zie B4 |

**Uit fase A gehaald, omdat het in geen schatting zit en in geen user story staat:**
- De Undo-toast die ook de melding intrekt (§2.1, §4.1): Responsible is altijd opnieuw te wijzigen.
- "Gelezen als de plek gelezen is" (P-84): in fase A maakt alleen het openen van de rij die gelezen.
- *Compare / Discard mine* bij een bericht dat op een ander apparaat is bewerkt (§7.5): berichten hebben geen versie, de laatste bewerking wint.

**Titan bevestigt bij de freeze:**
- Of *de feedback van een beslissing is niet bewerkbaar* (P-90) herkenbaar is zonder nieuwe kolom. Zo niet, dan vervalt die regel.
- Of het vervangen van *Plan to review* (B2) oudere clients een actie ontneemt.
- Wat B6 aan `ticket_needs` op beide clients kost; dit zit nog niet in de 44 d.

## Beslissingen (teamvoorstel)

Dit zijn de punten die afwijken van eerdere besluiten of die de gebruiker merkt. De overige punten uit beide documenten volgen het voorstel daar.

| # | Vraag | Voorstel | Bron |
|---|---|---|---|
| B1 | Hoort het link-scheme op OS-niveau (A5b, 4,5 d) in fase A? Zonder webclient werkt het alleen op een toestel met Aurelius | Ja, als laatste, en het mag afvallen als het krap wordt. Links binnen de app blijven altijd | Titan B1 |
| B2 | Vervangen ticketrijen de meldingen *Plan to review*, *Result ready* en *Task failed* voor iedereen? | Ja voor taken van een ticket. Taken zonder ticket en goedkeuringen veranderen niet | Titan B2, B-P9 |
| B3 | Moet bewerken of verwijderen een geheim echt wissen? | Nee: de tekst blijft in audit, back-ups en employee-sessies, en de dialoog zegt dat. Bij een lek: roteren en in de vault zetten. Een wispad is een los besluit | Titan B3 |
| B4 | Wie doet de slot-E2E? Pulse staat niet meer in het organigram | Orbit (3 d). Pulse terughalen kan ook en verandert verder niets | Titan B4 |
| B5 | Android heeft geen push. In scope? | Nee: iOS krijgt push, Android alleen de Inbox in de app | Titan B8 |
| B6 | Telt ◆ *Needs you* alleen nog voor de Responsible? **Wijkt af van `fase-3` §3** | Ja; zonder actieve Responsible voor iedereen, zoals nu. Iedereen kan nog steeds beslissen. *Waiting on me* is precies diezelfde ◆ | B-P4, B-P5; gekozen boven Titan B9 |
| B7 | Heet het *Responsible* en *My tickets* in plaats van *Assignee* en *My issues*? | Ja: *assign* gaat in de client over employees, en de client zegt overal *ticket* | B-P1, B-P2 |
| B8 | Mag *by …* (de aanmaker) van de kaart? **Wijkt af van `fase-3` §3** | Ja: vier namen passen niet. Tooltip, paneel en filter blijven | B-P3 |
| B9 | Staat een nieuw ticket standaard op *Responsible: You*? | Ja, *No one* blijft kiesbaar | B-P6 |
| B10 | Mag de Responsible ontvolgen? (nuance op US-A2) | Nee, eerst overdragen | B-P7 |
| B11 | Krijgen overige volgers ook een rij bij Review en Plan review? | Ja, zonder ◆ | B-P8 |
| B12 | Verdwijnen al ongelezen meldingen als een ticket op Done komt? | Ja | Titan B7 |
