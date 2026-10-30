# Technische impactanalyse Fase 4: de vault

*AUR-MU-DEF-T · Titan (Tech Lead) · 2026-10-31 · basis: `main` 21015fd + `feat/aur-mu-01` @ fad4dd2 + Fase 2 en 3 (`../fase-2/`, `../fase-3/`) · bron: `docs/product/multi-user.md` §2 "Vault"*

Dit is alleen een analyse. Er is geen code gewijzigd.

## 0. Samenvatting
- **Protocol: additief binnen protocol 5** (de major van Fase 3), feature `vault`. Geen nieuwe major (§2). Fase 4 komt bewust **na** Fase 3: zonder sessies kan iedereen in de tailnet zich voor een board member uitgeven (R2 uit Fase 1), en dan is een vault niet te verdedigen.
- **Versleuteling**: AEAD **ChaCha20-Poly1305 uit `ring`** (zit al in de workspace via `crates/remote-gateway/Cargo.toml:30`, dus geen nieuwe crypto-dependency), met een willekeurige nonce per versie en `item_id‖version‖kind‖scope` als associated data. **Sleutel**: één master key van 32 bytes in de bestaande secret store `crates/host/src/secrets.rs` (Keychain/Credential Manager; op Linux een bestand 0600 in `<data>/secrets/`), nooit in de database of in backups.
- **Levering**: secrets **alleen als omgevingsvariabele** van de run. Tekst en url ook via een tool. Een tool die een secret teruggeeft, is uitgesloten: tool-resultaten staan in het transcript.
- **Redactie** van elke geleverde waarde in alles wat Aurelius bewaart of verstuurt. Restrisico: het eigen sessiebestand van Claude Code (§8 R1).
- **Schatting: ≈ 22 d** (§6).

## 1. Datamodel en migraties (migratie 0021)
1. **`vault_items`**: `id`, `company_id`, `project_id` (NULL = company), `kind` CHECK IN (`text`,`url`,`secret`), `name`, `env_name` (UPPER_SNAKE, uniek per scope; verplicht voor secrets), `description`, `value_text` (alleen voor text/url, leesbaar), `ciphertext` BLOB + `nonce` BLOB + `key_id` (alleen voor secrets), `version`, `created_by_user`, `updated_by_user`, `created_at_ms`, `updated_at_ms`, `archived_at_ms`. CHECK: een secret heeft `ciphertext` en geen `value_text`, omgekeerd voor text/url.
2. **`vault_audit`** (append-only, met triggers `no_update`/`no_delete` zoals `events` in 0014): `id`, `item_id`, `action` CHECK IN (`create`,`update`,`archive`,`reveal`,`deliver`,`tool_read`,`key_export`), `user_id`, `employee_id`, `run_id`, `task_id`, `at_ms`. **Nooit een waarde.** Een aparte tabel en niet `events`: leveringen per run zijn talrijk en hoeven niet naar elke client live.
3. Mutaties krijgen daarnaast een gewoon event (`vault.item_*`) zonder waarde.

## 2. Protocol: binnen 5, additief
| Wijziging | Waar | Oordeel |
|---|---|---|
| read `vault.list { project_id? }` → items **zonder** secretwaarde | `protocol/src/method.rs`, `rpc.rs` | additief in 5 |
| read `vault.reveal { item_id }` → de waarde, schrijft `vault_audit(reveal)` | `method.rs` | additief in 5 (een read: geen receipt, geen event, geen `request_hash`) |
| read `vault.audit { item_id?, before? }` | `method.rs` | additief in 5 |
| writes `vault.create`, `vault.update`, `vault.archive` | `write.rs` (`HostWrite::Vault*`), `remote.rs`; **apps: `NOT_FOR_APPS`** (`app.rs`) | additief in 5 |
| Events `vault.item_created/updated/archived { id, name, kind, scope, version, by }` | `protocol/src/events.rs` | additief in 5 (oude 5-clients slaan ze over, `connection.dart:385`) |
| Feature `vault` | `rpc.rs` `FEATURES` | additief |
| Agent-tools `vault_list` (namen, env-namen, beschrijving) en `vault_read` (alleen text/url) | `agent-bridge/src/mcp.rs`, `protocol/src/bridge.rs` | intern |

**De secretwaarde mag niet in de command-pipeline**: net als `user.create` in S3/S5 handelt de daemon `vault.create/update` af vóór de host-pipeline. Het domain-commando bevat alleen metadata; de waarde gaat versleuteld als apart argument naar `storage::vault::insert`. Zo staat ze niet in `command_receipts.request_hash` (SHA-256 zonder salt, `application/src/lib.rs:152`) en niet in een append-only event. De params krijgen een redacting `Debug`.

## 3. Daemon
- **Crypto** (nieuw `crates/storage/src/vault.rs` of `crates/host/src/vault_key.rs`): master key bij de eerste start aangemaakt (`OsRng`) onder secret-ref `vault-master`; `key_id` maakt rotatie mogelijk (opnieuw versleutelen in één transactie). Plaintext alleen in het geheugen, met `zeroize` waar het kan.
- **Levering aan runs**: een run krijgt de secrets van de **company + het project van de run** (een run zonder project: alleen company). Ze gaan als env in `ClaudeCodeRuntime::command_from` (spike 7e1388d), **na** `env_clear` + `ENV_ALLOWLIST`, net als de login, en op de runtime en niet op `RunRequest` (die is `Debug + Serialize` en komt in traces; spike §3.1). De snapshot bewaart alleen `(item_id, version)`. Eén `vault_audit(deliver)` per run met de lijst items.
- **Verboden env-namen** (validatie bij aanmaken én levering): `PATH`, `HOME`, `USER`, `SHELL`, `LD_*`, `DYLD_*`, alles uit `ENV_ALLOWLIST`, `ANTHROPIC_*`, `CLAUDE_*`, `CLAUDE_CODE_*`, `AWS_*`/`GOOGLE_*`/`AZURE_*`-providerschakelaars. Een vault-item `ANTHROPIC_API_KEY` zou anders de Claude-login van de board member overrulen (spike meting E) en het verbruik naar een andere rekening sturen.
- **Prompt**: alleen de **namen** van de beschikbare env-variabelen + de instructie "gebruik `$NAME`, druk de waarde nooit af". Nooit de waarde in prompts, memory, discussies of `request_hash`.
- **Redactie** (nieuw `application/src/run/redact.rs`): per run een Aho-Corasick-automaat over de geleverde waarden (≥ 8 tekens) plus hun base64- en URL-gecodeerde vorm; vervanging door `«secret:NAME»`. Toegepast **vóór** elke opslag of verzending: stream-deltas en activity-previews, berichten/blobs, `run_traces`, tool-input en -output, rapporten, memory (`remember` en de extractie), Team chat/discussie-spiegeling, webhooks-outbox en pushmeldingen. Grensgevallen tussen chunks: een staartbuffer van `max_len − 1` tekens.
- **Verdediging in de diepte**: `permissions.rs::ask` waarschuwt of weigert Bash-commando's die duidelijk de omgeving dumpen (`env`, `printenv`, `set`, `echo $NAME` van een vault-naam).
- **Backup**: de database bevat alleen ciphertext; de master key zit niet in de backup. Een restore op een andere machine heeft een **key export** nodig (D4-3).

## 4. Desktop (Flux)
Vault-scherm per company en per project: lijst, nieuw/bewerken (secretveld verborgen, nooit vooraf ingevuld), "Toon" met bevestiging (audit), kopiëren zonder de waarde in de UI-state te houden, auditlog per item. Instellingen: key export.

## 5. Mobiel (Bridge)
Lijst, aanmaken en bewerken, reveal met biometrische bevestiging (`local_auth`), auditlog. Geen caching van waarden op schijf; waarde na 30 s uit het scherm.

## 6. Schatting
| Wie | Werk | Dagen |
|---|---|---|
| Schema | 0021 (`vault_items`, `vault_audit` + triggers) + storage + tests | 1 |
| Forge | crypto, master key, rotatie, key export | 2 |
| Forge | vault-protocol, vóór de pipeline, redacting `Debug`, gen-dart | 2 |
| Forge | levering als env, naamvalidatie, snapshot, tools | 2 |
| Forge | redactie op alle bronnen + tests met een canary-secret | 3 |
| Forge | audit (opslag + `vault.audit`) | 1 |
| Sentinel | crypto- en sleutelreview, redactietest (canary zoeken in database-dump, blobs, logs, webhooks), threat model | 2 |
| Flux | desktop | 3 |
| Bridge | mobiel | 3 |
| Pulse | E2E: run gebruikt een secret zonder dat het ergens verschijnt | 2 |
| Trace | review | 1 |
| **Totaal** | | **≈ 22 d** (doorlooptijd 2–2,5 weken) |

## 7. Bouwvolgorde
1. Na Fase 3 (sessies). Schema 0021 ‖ Forge crypto + master key. Sentinel reviewt het ontwerp vóór de code.
2. Forge: protocol bevriezen (additief op 5) → gen-dart. **Vanaf hier Flux en Bridge.**
3. Forge: redactie **vóór** levering (zonder redactie geen secrets in runs), dan levering + tools + audit.
4. Sentinel canary-test, Pulse QA, Trace review per tak.

## 8. Risico's
| # | Risico | Maatregel |
|---|---|---|
| R1 | Claude Code schrijft zijn eigen transcript (`<loginmap>/projects/*.jsonl`); als het model een waarde afdrukt, staat die daar en kan Aurelius dat niet redigeren | prompt-instructie, de Bash-controle, 0700 op de loginmap; expliciet aanvaarden en documenteren |
| R2 | Plugins/MCP-servers van de employee erven de env van `claude` | aanvaard (zelfde vertrouwen als de employee); vermelden in de UI |
| R3 | Master key op dezelfde schijf als de database (Linux) | beschermt tegen gelekte database-kopieën/backups, niet tegen root op de server; zo documenteren |
| R4 | Sleutel kwijt (andere machine, herinstallatie) = secrets kwijt | key export (D4-3), met audit |
| R5 | Redactie mist een gecodeerde vorm | canary-tests op base64/URL/hex; minimumlengte; restrisico aanvaard |
| R6 | `request_hash`/events met een waarde (onomkeerbaar) | afhandeling vóór de pipeline + dump-test, zoals S5 |

## 9. Beslispunten met voorstel
| # | Vraag | Voorstel |
|---|---|---|
| D4-1 | Algoritme | ChaCha20-Poly1305 via `ring` (al aanwezig); alternatief `chacha20poly1305` (RustCrypto, XChaCha met 192-bit nonce) als Sentinel het wil |
| D4-2 | Sleutelopslag | master key in `host::secrets` (OS-keychain; Linux bestand 0600). Geen passphrase bij de start, want de daemon moet onbeheerd herstarten |
| D4-3 | Restore op een andere machine | expliciete "key export" door een board member (eenmalig tonen/downloaden, audit) |
| D4-4 | Levering: env of tool? | Secrets **alleen env**; text/url ook via `vault_read`. Geen tool die een secret teruggeeft |
| D4-5 | Welke secrets krijgt een run? | Alle van company + project van de run (multi-user §2: geen toegangslijsten). Later eventueel per employee beperken |
| D4-6 | Mogen app keys de vault lezen? | Nee (`NOT_FOR_APPS`) |
| D4-7 | Mogen board members een secret zien? | Ja (`vault.reveal`), altijd met audit |
| D4-8 | Type achteraf wijzigen tussen secret en tekst/url (Nova: met bevestiging; Pixel P-44: niet)? | **Pixel: niet.** Tekst en url mogen onderling wisselen. Secret naar tekst zou een geheim dat in runs geredigeerd werd, leesbaar maken en de audit omzeilen. Tekst naar secret maakt het niet meer geheim: de oude waarde stond al leesbaar in `value_text`, in prompts en mogelijk in transcripts. Wie dat wil, maakt een nieuw item en archiveert het oude. Technisch ook eenvoudiger: `kind` is onveranderlijk voor secrets, en de CHECK uit §1 blijft geldig zonder een versleutel/ontsleutel-migratie per item |
