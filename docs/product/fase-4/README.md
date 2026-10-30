# AUR-MU-04: Fase 4, vault (ter goedkeuring)

*Orion · 2026-10-31 · branch `docs/aur-mu-def` (lokaal) · basis: `docs/product/multi-user.md` §2 "Vault"*

| Document | Auteur | Inhoud |
|---|---|---|
| [user-stories.md](user-stories.md) | Nova | MU4-01 t/m MU4-11, Given/When/Then, OV-01..07 |
| [ui-ontwerp.md](ui-ontwerp.md) | Pixel | Vault-lijst, tonen, aanmaken/bewerken, auditlog, P-41..47 |
| [impactanalyse.md](impactanalyse.md) | Titan | Migratie 0021, protocol, crypto, levering, redactie, D4-1..8 |

**Scope:** een vault op company- en projectniveau met items van het type tekst, url en secret. Alle waarden worden versleuteld opgeslagen (ChaCha20-Poly1305 via `ring`, dat al in de workspace zit; de master key staat in `host::secrets`, nooit in de DB of in backups). Secrets komen nooit in prompts, memory, discussies, events of `request_hash`. Een run krijgt ze alleen als omgevingsvariabele. Tekst en url zijn ook via de tool `vault_read` te lezen. Aurelius redigeert elke geleverde waarde in alles wat het bewaart of verstuurt. Er is een auditlog van wie (board member of employee/run) wat las of wijzigde. Alle employees mogen lezen, en een run krijgt de items van de company plus die van zijn eigen project. Een board member kan een secret tonen (met audit) en de sleutel exporteren.
**Buiten scope:** toegangslijsten per item of employee, auditexport (CSV), schrijfrechten voor employees, een externe KMS.
**Protocol:** **protocol 4 wijzigt niet meer, Fase 4 bouwt additief op protocol 5** uit Fase 3 (feature `vault`, `vault.*`-methodes vóór de write-pipeline met een redigerende `Debug`, events zonder waarde). Er komt geen nieuwe major (impactanalyse §2).
**Schatting:** ≈ **22 dagen** werk, doorlooptijd 2 à 2,5 weken (Forge 10, Flux 3, Bridge 3, Schema 1, Sentinel 2, Pulse 2, Trace 1).
**Afhankelijkheden:** **Fase 3 is nodig.** Zonder sessies kan iedereen in de tailnet zich voor een board member uitgeven, en dan valt een vault niet te verdedigen. Het auteurschap uit Fase 3 voedt de auditlog.
**Bouwvolgorde:** Schema 0021 ‖ Forge crypto en master key (Sentinel reviewt het ontwerp vóór de code) → protocol bevriezen + gen-dart → Flux en Bridge → Forge eerst redactie, daarna levering, tools en audit (zonder redactie gaan er geen secrets naar runs) → Sentinel canary-test, Pulse, Trace.
**Risico's:** Claude Code bewaart zijn eigen transcript in de loginmap, en als een model een secret afdrukt, kan Aurelius dat daar niet redigeren (aanvaard en gedocumenteerd: instructie, Bash-controle, 0700). Plugins en MCP-servers erven de env. De master key staat op dezelfde schijf (beschermt tegen gelekte DB-kopieën, niet tegen root). Een verloren sleutel betekent verloren secrets (key export). Redactie kan een gecodeerde vorm missen (canary-tests op base64/hex/URL).

## Beslissingen (teamvoorstel geldt zonder tegenbericht)
| # | Vraag | Voorstel | Bron |
|---|---|---|---|
| B1 | Wat wordt versleuteld? | Alle waarden. Alleen secrets worden gemaskeerd en geredigeerd | OV-01 |
| B2 | Algoritme en sleutel | ChaCha20-Poly1305 (`ring`) met een master key in de OS-keychain (Linux: bestand 0600). Geen passphrase bij de start, want de daemon moet onbeheerd herstarten | D4-1, D4-2, OV-04 |
| B3 | Levering van secrets | **Alleen als omgevingsvariabele.** Geen tool die een secret teruggeeft of naar een bestand schrijft, want tool-resultaten komen in het transcript | D4-4 (boven Nova OV-05) |
| B4 | Welke items krijgt een run? | Alles van de company plus het eigen project. Een project-item wint van een company-item met dezelfde naam (pill "overrides Company") | D4-5, OV-02, P-42 |
| B5 | Rechten | Employees en board members lezen alles, alleen board members wijzigen. App keys hebben geen toegang | OV-03, D4-6 |
| B6 | Een secret tonen | Een klik plus een auditregel, zonder extra wachtwoord. Na 30 s wordt het verborgen en na 60 s wordt het klembord gewist | D4-7, P-43, P-47 |
| B7 | Type wijzigen | Tussen tekst en url mag. **Van of naar secret mag niet**: maak een nieuw item | D4-8, P-44 (boven Nova OV-06) |
| B8 | Restore op een andere machine | Een expliciete key export door een board member, met audit | D4-3 |
| B9 | Bewaartermijn van de auditlog | Onbeperkt in Fase 4 | OV-07 |
| B10 | Waarschuwen bij een geplakt secret in een chat of discussie | Ja, als Titan het in Fase 4 haalbaar vindt. Anders wordt het een idee voor later | P-46 |
