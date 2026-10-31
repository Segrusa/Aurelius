# UI-ontwerp Fase 4: vault (company en project) en auditlog

- **Task:** AUR-MU-DEF-P (Pixel, voor Orion) · **Status:** beknopt concept, ter goedkeuring
- **Basis:** `docs/product/multi-user.md` §2 (Vault: company en project, de types tekst, url en secret, versleuteld, nooit in prompts, memory of discussies, beschikbaar voor runs als env of via een tool, een auditlog, alle employees lezen alles), `fase-1/ui-ontwerp.md` (tokens, dialogen, Settings), Fase 3 (auteurschap, sessies).

## 1. Navigatie
- **Desktop:** een nieuw sidebar-item **Vault** onder Projects (Ctrl/⌘+9). Het projectvenster krijgt een tab **Vault** die hetzelfde scherm opent, gefilterd op dat project.
- **Mobiel:** **More → Vault**, en op de projectpagina de rij **Vault ›**.
- Bovenaan het scherm kies je de **scope**: *Company* of een project. Er is één scherm, zodat je niet tussen twee plaatsen hoeft te zoeken.

## 2. Vault-lijst, desktop
```
┌Sidebar┬ Vault ─────────────────────────────────────────────────────────────────┐
│       │ SCOPE           │ aurelius · 4 items      [🔍 Find… ] [+ New item]      │
│       │  ⌂ Company   6  │ [ Items ] [ Audit log ]                               │
│       │ PROJECTS        │ Name            Key             Type      Value                 Updated       │
│       │  aurelius    4  │ Staging URL     STAGING_URL     🔗 URL    https://stg.aur…     Marie · Oct 30 │
│       │  segrusa-web 2  │ GitHub token    GITHUB_TOKEN    🔒 Secret •••••••• [Show][Copy] You · Oct 31  │
│       │                 │ Deploy notes    DEPLOY_NOTES    ¶ Text    Run make deploy…      You · Oct 29  │
│       │                 │ Sentry DSN      SENTRY_DSN      🔒 Secret •••••••• [Show][Copy] Marie · Oct 28│
│       │                 │ Company items also available here: 6 ›                                       │
└───────┴─────────────────┴────────────────────────────────────────────────────────────────────────────────┘
```
- **Kolommen:** de naam, de **key** (zo heet het item voor een run, als env-variabele of in de tool), het type als icoon plus woord, de waarde (tekst: eerste regel, url: klikbare link, secret: altijd `••••••••`) en wie het wanneer laatst wijzigde.
- **Projectscope:** onderaan *"Company items also available here: 6 ›"*, omdat een run van dit project ook de company-items krijgt. Heeft een project-item dezelfde key als een company-item, dan krijgt de rij de pill **overrides Company** (P-42).
- **Rij openen** (klik of Enter) = de editor (§4). Het ⋯-menu bevat Edit, Copy key, Duplicate to… en Delete.
- **Leeg:** *"No items in aurelius yet. Employees of this project can read every item here."* + **New item**.
- **Mobiel:** een gegroepeerde lijst per scope (de scope-pill bovenaan opent een sheet). Een rij toont naam, key en type. Een tik opent de detailpagina met de waarde en daar **Show** of **Copy**. Op een secret toont de lijst nooit de waarde.

## 3. Een secret tonen (alleen na een expliciete klik)
```
│ GitHub token  GITHUB_TOKEN  🔒 Secret  ghp_4f9a…k2Qz  [Hide] [Copy]  Hides in 0:27 │
```
- **Show** toont de waarde inline gedurende **30 seconden** (met een zichtbare aftelling), of tot **Hide**, het verlaten van het scherm of het vergrendelen van het apparaat. Daarna is ze weer gemaskeerd.
- **Copy** kopieert zonder de waarde te tonen en wist het klembord na 60 s (op desktop waar dat kan, op mobiel als gevoelige inhoud gemarkeerd). Toast: *"Copied. Clears in 60 s."*
- Elke **Show** en **Copy** komt in de auditlog. De tooltip zegt het vooraf: *"Shows the value for 30 s. This is logged."*
- Een secret staat nooit in toasts, meldingen, schermvoorbeelden of logs. Op mobiel is het scherm bij het overschakelen naar een andere app geblurd (iOS) of `FLAG_SECURE` (Android), zolang er een secret zichtbaar is.

## 4. Item aanmaken of bewerken
```
┌ New item · aurelius ───────────────────────────────────────────┐
│ Type     ( Text | URL | ● Secret )                              │
│ Name *   ┌ GitHub token ────────────────────────────────────┐  │
│ Key *    ┌ GITHUB_TOKEN ────────────────────────────────────┐  │  ← afgeleid van de naam, aanpasbaar
│          A–Z, 0–9 and _ · unique in this scope                │
│ Value *  ┌ •••••••••••••••••••••••••••••••••••••••• ──── 👁 ┐  │  ← secret: gemaskeerde invoer
│ Note     ┌ Fine-grained token, repo aurelius, read/write ───┐  │  ← employees zien dit, geen secret
│ Scope    [ aurelius ⌄ ]                                        │
│ All employees can read this item. Secrets reach runs only as   │
│ an environment variable or via the vault tool, never in a      │
│ prompt, memory or discussion.                                  │
│                                         [Cancel]  [Save  ⌘↵]   │
└────────────────────────────────────────────────────────────────┘
```
- **Per type:** *Text* krijgt een tekstvak van meerdere regels. *URL* wordt gecontroleerd (`https://…`, met de fout *"Enter a full URL, starting with https://"*). *Secret* is een gemaskeerd veld van één regel. Het oog werkt alleen lokaal tijdens het typen en wordt niet gelogd, want de waarde is nog niet opgeslagen.
- **Een secret bewerken:** de bestaande waarde wordt **nooit** voorgevuld. Het veld zegt *"Leave empty to keep the current secret"*, met de knop **Replace secret**. Naam, key en note blijven vrij te bewerken.
- Het type wijzigen naar of van Secret kan niet na het aanmaken (P-44).
- **Key wijzigen** toont een waarschuwing: *"Runs that use GITHUB_TOKEN will no longer find it."*
- **Delete:** een bevestiging met de naam: *"Delete GitHub token? Runs and employees lose access. This is logged."*
- **Mobiel:** een volledige pagina met een segmented control voor het type, gegroepeerde velden en **Save** rechtsboven. Het secret-veld gebruikt `obscureText`, `enableSuggestions: false` en `autocorrect: false`.

## 5. Auditlog
**Vault → tab Audit log** (per scope, of *All scopes*), en per item een sectie **History** in de editor met dezelfde regels voor dat item.
```
┌ Audit log · aurelius ─────────────────────────────────────────────────────────┐
│ [Item: Any ⌄] [Who: Anyone ⌄] [Action: Any ⌄] [Last 30 days ⌄]               │
│ Time           Who                     Action        Item           Via        │
│ Oct 31 11:02   (S) You                 Revealed      GitHub token   iPhone     │
│ Oct 31 10:40   ▢ Forge (employee)      Read by run   GitHub token   #42 · run ›│
│ Oct 31 09:15   (M) Marie               Replaced      Sentry DSN     dev-laptop │
│ Oct 30 17:20   (S) You                 Copied        GitHub token   Desktop    │
│ Oct 30 16:03   (M) Marie               Created       Staging URL    Desktop    │
│                                    Load older                                  │
└───────────────────────────────────────────────────────────────────────────────┘
```
- **Acties:** Created · Edited (naam, key of note) · Replaced (de secret-waarde) · Revealed · Copied · Read by run · Deleted. De log toont **nooit een waarde**, ook niet de oude of nieuwe bij Edited: alleen welke velden wijzigden.
- **Who:** een board member (monogram als cirkel, Fase 3) of een employee (rechthoek). **Via** is het apparaat (de sessie uit Fase 3) of het ticket en de run, met een link naar de task.
- De auditlog is alleen-lezen voor iedereen. Er zijn geen knoppen om te wissen of te bewerken.
- **Lezen door een run:** één regel per run en per item, niet per gebruik. Zo blijft de log leesbaar.
- **Mobiel:** een lijst met per regel *"You revealed GitHub token"*, daaronder de tijd en het apparaat. De filters staan in een sheet (trechter-icoon).

## 6. Toegankelijkheid
- Het type staat er altijd als woord bij het icoon. Een gemaskeerde waarde heeft het label *"GitHub token, secret, hidden"*. Na Show wordt *"Shown for 30 seconds"* aangekondigd en na het verbergen *"Hidden"*.
- De aftelling is visueel. Bij Show wordt ze één keer aangekondigd, niet elke seconde. **Reduce motion:** geen animatie op de aftelbalk.
- De focus blijft na Show en Hide op dezelfde knop. Show en Copy hebben elk een label met de itemnaam.

## 7. Ontwerpvragen
| ID | Vraag | Voorstel |
|---|---|---|
| P-41 | Waar staat de vault: in de sidebar of onder Settings? | In de sidebar (**Vault**, Ctrl/⌘+9) en als tab in het project. Board members gebruiken hem tijdens het werk, niet alleen bij de inrichting |
| P-42 | Een project-item met dezelfde key als een company-item: toelaten? | Ja, het project-item gaat voor in runs van dat project, met de zichtbare pill *overrides Company* |
| P-43 | Vraagt Show een extra bevestiging (wachtwoord of biometrie)? | Nee, de klik plus de auditlog is genoeg voor één rol die alles mag. Op mobiel is optioneel Face ID of vingerafdruk mogelijk (een instelling, standaard uit). Sentinel kan dit strenger maken |
| P-44 | Het type achteraf wijzigen? | Niet van of naar Secret (anders zou een tekst die eerder zichtbaar was "geheim" lijken, of omgekeerd). Text en URL kunnen onderling |
| P-45 | Exporteren van de auditlog (CSV)? | Niet in Fase 4. De filters volstaan. Later kan het als idee terugkomen |
| P-46 | Een secret dat in een discussie of chat geplakt wordt: waarschuwen? | Ja, als een geplakte tekst op een bekende vault-waarde lijkt: *"This looks like a vault secret. Send anyway?"*. Dit vergt een controle aan de serverkant, dus Titan beslist of het haalbaar is in Fase 4 |
| P-47 | Hoe lang blijft een getoond secret zichtbaar? | 30 s. Het klembord wordt na 60 s gewist |
