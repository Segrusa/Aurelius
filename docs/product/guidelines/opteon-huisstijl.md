# Opteon-huisstijl als guidelines per employee (voorstel)

*2026-11-13 · analyse van de klantprojecten onder `~/Windows/Work` en `~/Work/Opteon` (zonder Planora)*

## Wat is geanalyseerd

| Project | Frontend | Backend | Database | Infra/CI |
|---|---|---|---|---|
| Creos.PortfolioApp (+ FrontEnd) | React 18, Vite, axios, CSS-bestanden | Web API 2, .NET 4.8, ADFS | SQL 1.0 + **2.0 (beste SQL-referentie)** | SWA-pipeline |
| Eurocontrol mmit (+ DevOps-kopie) | **React 19, Vite 8, TS 6, Fluent v9, RTK (beste referentie)** | **Functions isolated, net8 (beste referentie)** | SQL 1.0 | ADO, deployment-jobs |
| Eurocontrol mmit-iac, applications-setup-conf | | | | **Terraform met ECTL-modules (beste referentie)** |
| Manuchar TeamsApp | React 19, voorganger van mmit | | | SWA-pipeline |
| WaterLink KOMPAS | legacy: geen lint, inline styles, NL-namen | Functions net48, SAP, CSOM | SQL 1.0/2.0, WL_-views | Teams Toolkit + bicep |
| Spuerkeess PROMM | PCF (React 16, Fluent 8), canvas app | 3× Functions net48 | SQL 2.0–2.8 (grootste set) | handmatig, niet kopiëren |

Alle projecten komen uit één sjabloon (de "FA/PA"-portfolio-app): dezelfde prefixen, stored procedures die JSON teruggeven, `FAInitializeDb` als deployer, en een Teams-tab als frontend.

## Beslissingen die jij moet nemen

1. **Legacy of modern?** De huisstijl heeft zichtbare conventies (prefixen `PA_`/`FA_`, `m`/`l`/`i`/`o`-namen in C#, 3 spaties, SP's met `FOR JSON`) én zwakke plekken (synchrone SQL in async functies, `new HttpClient()` per call, string-SQL, `e.Message` naar de client, status 300, geen tests, secrets in de repo). **Voorstel:** conventies behouden, zwakke plekken in nieuwe code niet herhalen. Zo staan de teksten hieronder.
2. **De Aurelius-guidelines hangen nu aan employees** (Rust style, Flutter and Dart, Desktop app (gpui-kit), Worktrees). Ze gelden dus ook voor een run op een klantproject, en met de nieuwe teksten erbij gaat Bridge over de 16.000 tekens. **Besloten:** Rust style, Flutter and Dart en Worktrees gaan naar project Aurelius in plaats van naar employees. "Desktop app (gpui-kit)" is achterhaald (de desktop is nu Electron + React) en wordt gearchiveerd.
3. **Klantspecifiek of algemeen?** De Terraform-regels gelden alleen voor Eurocontrol (ECTL-modules, landing zones). **Voorstel:** "Opteon: Eurocontrol infra" aan project Eurocontrol koppelen, de algemene pipeline-regels aan Orbit. Voor PROMM (Power Platform) is er geen project in Aurelius; de PCF-regels zitten kort in de frontend-guideline.

## Koppeling per employee (na beslissing 2)

| Employee | Guidelines | ≈ tekens |
|---|---|---|
| Flux (frontend) | Frontend · Security · Tests and evidence | 4 566 |
| Forge (backend) | Backend · Database · Security · Tests and evidence | 6 664 |
| Bridge (fullstack) | Frontend · Backend · Security · Tests and evidence | 6 880 |
| Nexus (API & integratie) | Backend · Security · Tests and evidence | 4 306 |
| Schema (database) | Database · Backend · Security · Tests and evidence | 6 664 |
| Orbit (DevOps) | Pipelines · Security | 2 296 |
| Sentinel (security) | Security · Backend · Frontend · Code review | 7 008 |
| Trace (code review) | Security · Frontend · Backend · Code review | 7 008 |
| Titan (tech lead) | Frontend · Backend · Security · Pipelines · Code review | 8 161 |
| Project Eurocontrol | Eurocontrol infra | +1 648 in dat project |

"Tests and evidence" (849) en "Code review" (977) bestaan al. In project Aurelius komen de Aurelius-guidelines (6 234 tekens na het archiveren van gpui-kit) erbovenop; daarom krijgen Bridge, Trace en Titan geen Database-guideline (Forge en Schema hebben die), anders gaan ze daar over 16 000. De aantallen zijn de lengtes van de teksten hieronder; de server telt bij het koppelen opnieuw en weigert een koppeling die over de limiet van 16 000 gaat.

---

## G-FE · Opteon: frontend (React, Fluent UI)

```markdown
House style for client frontends (Teams tabs). Reference: Eurocontrol mmit/frontend.

Stack
- Vite + React 19 + TypeScript strict. Environments in env/.env.<mode>; build per client with `vite build --mode <client>`. No URLs or keys in source.
- Fluent UI v9 only (@fluentui/react-components, @fluentui/react-icons). Style with makeStyles + tokens in a sibling <feature>.styles.ts exporting use<Feature>Styles. No hard-coded colours; inline style only for one-off layout.
- State: Redux Toolkit, one folder per entity: store/<entity>/<entity>.slice.ts, .state.ts, .thunks.ts. Typed hooks useAppDispatch/useAppSelector. Type thunks fully: createAsyncThunk<Result, Arg, { rejectValue: string }> with rejectWithValue; no `as unknown as`.
- Status per slice: 'idle' | 'loading' | 'succeeded' | 'failed', plus error: string | null. Fetch when status is 'idle'.
- Data access only in services/<entity>/<entity>.service.ts + <entity>.model.ts, through one services/api.client.ts (apiGet/apiPost/apiPut/apiDelete<T>) that adds the bearer token. Components never call fetch.
- Auth: Teams SSO (teams-js getAuthToken) first, MSAL fallback outside Teams. Never put a Function key in the bundle.
- Errors: throw ApiError from the client, show toasts centrally (listener middleware), MessageBar for inline errors.
- Routing: react-router v7; each feature exports xxxRoutes: RouteObject[] from pages/<feature>/<Feature>Routes.tsx.

Code
- Function components with named exports (default only for route pages). No React.FC.
- interface for models, props and state; type for unions. Model fields PascalCase exactly as the API sends them; optional as `?: T | null`. No any, no enum (string unions or `as const`), avoid `!`.
- `import type` for types; relative imports; order: React, router, Fluent, icons, shared, store, services, local.
- Prettier: single quotes, semicolons, 2 spaces, printWidth 100, trailingComma es5. ESLint flat config (typescript-eslint, react-hooks, unused-imports).
- Files: PascalCase .tsx for components; kebab/dot-case .ts for logic (project-page.hooks.ts). Folders kebab-case; page subfolders tabs/, drawers/, dialogs/.
- Reusable UI in src/shared behind shared/index.ts, client-prefixed (EuroControlButton).
- UI text in the client's language as agreed; aria-label on icon-only buttons. Comments explain why.

Power Apps PCF (PROMM): virtual React controls, React 16.14 + Fluent 8 matching the manifest platform-library, JSON in/out through bound text properties and an eventName output, no WebAPI or HTTP in a control, bump the manifest version on every change.
```

## G-BE · Opteon: backend (C#, Azure Functions)

```markdown
House style for client backends. Reference: Eurocontrol mmit/backend (MUAC.PortfolioApp.FunctionApp).

Shape
- Azure Functions v4, isolated worker, .NET 8, ASP.NET Core integration (IActionResult). OpenTelemetry to Azure Monitor.
- One HTTP function class per resource: <Prefix><Entity>API with one Run that dispatches on req.Method and a {type} route segment. Logic in Helpers/<Prefix><Entity>Helper.cs, POCOs in Models/<Prefix><Entity>.cs, SQL in SQL/<version>/.
- Namespaces <Client>.<Product>.FunctionApp[.Helpers|.Models]; helpers and models internal.
- Prefixes: FA plumbing (FASettings, FAUtils, FALogging, FAGraphHelper), PA portfolio domain, GP groups, MSP MS Project, PO Project Online, SS SharePoint, SAP.

Naming and layout
- Parameters mName, locals lName (declared at the top, explicit types), loop variables iName, using resources oName, the logger _logger. Constants UPPER_SNAKE in PAConst.
- 3-space indent, Allman braces, `return (value);`, one-line `if (...) return/throw` allowed.
- English identifiers and comments; client domain terms may keep their language.

Behaviour
- Settings through FASettings.Init(out lError) from environment variables (UPPER_SNAKE); secrets are Key Vault references in app settings, never in code or repo.
- Data access: Microsoft.Data.SqlClient against stored procedures <Prefix>_<Entity>_SP_<Verb>, parameters through FAUtils.AddSqlParameter, CommandTimeout 600. Reads pass the procedure's JSON through. Managed identity when no SQL user is configured.
- New code: async ADO.NET (OpenAsync, ExecuteReaderAsync); parameters only, never string-built SQL; explicit SqlTransaction for multi-statement writes.
- Helpers return bool with out string mError (and out mJson); callers `if (!X(out lError)) throw new Exception(lError);`.
- Log every request through FALogging.AddEventLog and errors through AddErrorLog, source "<Class> - <METHOD>".
- Errors: catch SqlException first (2627/2601 duplicate, 547 in use: 409 Conflict), then Exception. Return a clear message, not the raw exception or stack, and 409 instead of 300 for duplicates.
- Graph through FAGraphHelper (GraphServiceClient with the .default scope). Reuse HttpClient (static or IHttpClientFactory); no new HttpClient per call, no .Result.
- No commented-out code. A short /// <summary> on public helpers.
```

## G-DB · Opteon: database (SQL Server)

```markdown
House style for client databases (Azure SQL). Reference: Creos.PortfolioApp SQL/2.0; naming as in PROMM.

Deployment
- Scripts under <App>/SQL/<version>/{Tables,Stored Procedures,Functions,Views,Data}, listed as Content in the csproj and run by FAInitializeDb. New release, new version folder.
- One batch per file, never GO. One procedure or function per file, file name = object name (the deployer drops and recreates it).
- Tables re-runnable: IF OBJECT_ID(N'dbo.X', N'U') IS NULL BEGIN CREATE TABLE ... END; PK, FK and indexes in their own guarded blocks; new columns in PATableUpdates.sql guarded by sys.columns.

Naming
- Schema dbo. Prefix per module: PA_, FA_, GP_, MSP_, PO_, MIG_, client prefix for reporting (WL_).
- Tables PascalCase, singular (PA_ProjectRisk). Columns carry the entity name: ProjectName, ProjectStatusId.
- PK <Entity>Id uniqueidentifier, clustered. A readable number as <Entity>Identity int IDENTITY next to it.
- PK_<Table>, FK_<Table>_<Column> (WITH CHECK), UK_<Table>, IX_/UX_<Table>_<Column>. Index foreign keys you filter on.
- Audit: <Entity>DateCreated, <Entity>CreatedBy nvarchar(255), <Entity>DateModified, <Entity>ModifiedBy.
- Types: uniqueidentifier ids, nvarchar(255) names, nvarchar(max) free text, decimal(25,6) amounts, datetime (date for pure dates), bit flags. Times in UTC (GETUTCDATE()).
- No soft delete columns: <Entity>Archived bit or a status lookup; deletes remove children first.
- Dropdown values in PA_LookupTable/PA_LookupTableEntry; seeds with readable fixed GUIDs, guarded IF NOT EXISTS, with a column list.

Procedures
- <Module>_<Entity>_SP_<Verb> with Get, GetAll, GetList, GetDetails, Set (upsert), Add, Upd, Delete; functions <Module>_<Entity>_FN_<Verb>.
- Parameters named after columns; optional ones `= null`.
- Start with SET NOCOUNT ON (and SET XACT_ABORT ON with a transaction). Business errors with THROW 5000x, 'message', 1.
- Reads return JSON: FOR JSON PATH (WITHOUT_ARRAY_WRAPPER for one row), child lists as nested (SELECT ... FOR JSON PATH) as X.
- Set = IF EXISTS UPDATE ELSE INSERT; use UPDLOCK, HOLDLOCK where two callers can race.
- Prefer set-based SQL over cursors; temp tables only when needed.
- Format: UPPERCASE keywords, lowercase as, 3-space indent, one column per line, WHERE ProjectId=@ProjectId, short aliases. Comments with -- explain why; no commented-out code.
```

## G-SEC · Opteon: security

```markdown
Security rules for client work, from what went wrong in earlier projects.

- No secrets in git: no Function keys, client secrets, passwords, .pfx files or connection strings with credentials in source, env files, canvas formulas or scripts. Use Key Vault references, ADO variable groups or secret environment variables. If you find one, report it; do not copy it.
- Nothing secret in a browser bundle: VITE_* values are public. Call APIs with the user's bearer token (Teams SSO/MSAL), not a Function key.
- The API decides who the user is from the token, not from a UserId or email in the query string.
- SQL only with parameters; dynamic SQL only through sp_executesql with parameters and whitelisted identifiers.
- Return clear messages to clients, never exception text, stack traces or SQL errors; log the details server-side.
- Managed identity for Azure resources and SQL where the platform allows it; no username/password (ROPC) sign-in in new code.
- Private by default: no public network access where a private endpoint exists; least-privilege role assignments.
- Pin versions of packages, Terraform providers and modules; no `latest`.
```

## G-OPS · Opteon: pipelines (Azure DevOps)

```markdown
CI/CD house style for client work. Reference: Eurocontrol mmit/frontend and mmit/backend pipelines.

- Azure DevOps YAML pipelines only. name: $(Date:yyyyMMdd)$(Rev:.r).
- Stages: Build, then Deploy_<ENV> as a deployment job bound to an ADO environment (approvals live there, not in YAML). Pass a pipeline artifact between stages.
- trigger/pr none for manual pipelines; otherwise trigger on the main branch only.
- Pool and service connection from variable-group variables ($(agent_pool), $(service_connection_writer)); never a GUID or subscription id in YAML.
- Frontend: Cache@2 for npm, npm ci, lint, `npm run build:<client>` with VITE_* from the variable group, deploy to Static Web Apps with the token mapped through env:.
- Backend: UseDotNet 8.x, restore, build, test, publish, ArchiveFiles, PublishPipelineArtifact; deploy with AzureFunctionApp to the environment's app.
- SQL is deployed by the app's FAInitializeDb after the code; mention it in the release notes when a version folder is added.
- Bash steps start with set -euo pipefail and report with ##vso[task.logissue].
- Pin tool versions (Node, .NET, Terraform). Single quotes in YAML.
```

## G-ECTL · Opteon: Eurocontrol infra (project Eurocontrol)

```markdown
Terraform rules for EUROCONTROL. Reference: mmit-iac.

- Use the ECTL shared modules, pinned to a tag: git::ssh://git@ssh.dev.azure.com/v3/ECTL-AZURE/ECTL-Terraform-Modules/terraform-azurerm-<svc>//module?ref=<semver>. Raw azurerm resources only where no module exists.
- Terraform >= 1.6, azurerm >= 4.0.0, < 5.0.0, azapi 2.x, azuread ~> 3.0.
- One root per landing zone: landing-zones/<lz>/ with backend.tf, versions.tf, providers.tf, data.tf, locals.tf, main.tf, variables.tf, outputs.tf; pipeline in pipelines/<lz>/azure-pipeline.yaml with parameters/vars.yaml.
- backend "azurerm" {} empty; the pipeline sets the key $(System.TeamProject)-$(Build.Repository.Name)-$(landingZoneSlug).
- Names from landing_zone_slug, location_short (regions module), stack and the env: rg-<lz>-<loc>-<stack>, kv-<lz>-<loc>-<env>. Pass stack, landing_zone_slug, location, location_short and default_tags to every module.
- Tags only through basetagging (application, environment, cost_center, change, owner, spoc, technical_owner, tlp_colour, cia_rating, ado_repo).
- Private endpoints with the central privatelink DNS zones (connectivity provider alias), public access off, Entra-only SQL with managed identity, diagnostics to Log Analytics.
- Variables typed with a description naming their variable group; secrets as -var from variable groups; Key Vault secrets with content_type and expiration_date.
- Pipelines extend greenfield-ado-templates: pre-validation, validation, plan, security, apply; reader connection for plan, writer for apply.
- pre-commit: terraform fmt, terraform-docs, tflint, validate, gitleaks; checkov skips with a reason in the comment.
```

---

# Uit online skills (2026-11-13)

Bekeken: [obra/superpowers](https://github.com/obra/superpowers) (MIT: test-driven-development, systematic-debugging, verification-before-completion, writing-plans, receiving-code-review), [vercel-labs/agent-skills](https://github.com/vercel-labs/agent-skills) react-best-practices (MIT), [anthropics/skills](https://github.com/anthropics/skills) webapp-testing (Apache 2.0), [microsoft/skills](https://github.com/microsoft/skills) en [MicrosoftDocs/agent-skills](https://github.com/MicrosoftDocs/agent-skills) (vooral SDK-gebruik en het ophalen van Learn-documentatie, geen huisregels; niets overgenomen).

De teksten hieronder zijn in eigen woorden, beperkt tot wat past bij onze stack. Een skill laadt Claude alleen als hij nodig is; een guideline gaat in elke run mee. Daarom kort: in project Aurelius blijft Titan zo onder de 16 000 tekens.

| Guideline | Gekoppeld aan |
|---|---|
| Opteon: engineering practices | Flux, Forge, Bridge, Nexus, Schema, Orbit, Sentinel, Trace, Titan |
| Opteon: React performance and UI testing | Flux, Bridge |

## G-PRAC · Opteon: engineering practices

```markdown
How every developer works on client projects. After obra/superpowers and Anthropic's skills.

Before code
- Read the code you change and similar working code first. Restate the task in one line; ask when it is ambiguous instead of guessing.
- Split the work into small steps, each with the files it touches and how it is checked. One step at a time.
- Change only what the task needs: no drive-by refactors, no speculative options.

Tests first where there is logic
- Bug or business rule: write the failing test, see it fail for the right reason, make the smallest fix, clean up while green.
- Real code over mocks; mock only an external boundary (Graph, SAP, HTTP).
- No test setup yet: add the smallest that fits (vitest, xUnit) when you add logic, and say so.

Debugging
- Root cause before a fix: read the whole error, reproduce it, check recent changes, follow the data across each boundary (UI, API, procedure).
- One hypothesis and one minimal change at a time. After three failed fixes stop and report: the design is probably the problem.
- Say you do not know yet instead of guessing.

Review
- Check feedback against the code before acting. Push back with reasons when it is wrong. Blockers first, one at a time, each with its check.

Done means checked: build, lint and tests that prove it, with their output (Tests and evidence).
```

## G-WEB · Opteon: React performance and UI testing

```markdown
Performance and UI-test rules for the React SPAs. After vercel-labs react-best-practices and Anthropic's webapp-testing; only what fits a Vite SPA.

Performance
- No waterfalls: start independent requests together (Promise.all, parallel thunks); await where the value is needed.
- Bundle: import from the package path, not big barrels; lazy-load heavy pages, editors and charts with React.lazy; load third-party scripts late.
- Re-renders: derive values while rendering instead of syncing them in useEffect; select narrow store slices; functional setState from the previous value; lazy useState for expensive defaults; memo only where profiling shows a cost.
- Lists: stable keys, virtualise long grids, Map or Set for lookups inside loops.
- Effects: clean up listeners and timers; useEffectEvent for handlers read inside an effect.
- Keep the previous data visible while refetching; Spinner or Skeleton only on the first load.

UI tests (Playwright)
- Start the dev server, wait for the network to settle, look at what rendered (screenshot or DOM), then act.
- Select by role, label and visible text (getByRole, getByLabel), not CSS classes or generated ids.
- Wait for a state (a row, a toast), never a fixed sleep. Headless, and close the browser.
```
