# Move the configuration of one Aurelius server to another: employees (with
# their instructions, org chart, model choices and plugins), guidelines (with
# their links to employees), the company (name, dispatcher, tester), the board
# column names and the server settings people change in the app.
#
# Not moved: projects (they belong to folders on that machine), and so neither
# the links of guidelines to projects, nor tickets, chats, tasks, memory,
# users and their passwords, connections and API keys, vault items, the
# employees' own work folders and the server name.
#
#   scripts/transfer-config.ps1 export [-OutFile <zip>] [-DataDir <dir>] [-Dev]
#       On the source server. May run while Aurelius runs (it reads a consistent
#       copy of the database). Writes aurelius-config-<date>.zip.
#   scripts/transfer-config.ps1 import <zip> [-Replace] [-DryRun] [-DataDir <dir>] [-Dev]
#       On the target server, with Aurelius stopped (close the app, or
#       `aurelius-ctl shutdown`). The target must have done its onboarding.
#       -Replace  archives the employees and guidelines the target has now
#                 (a fresh install has a starter team); without it the import
#                 refuses when names or titles clash.
#       -DryRun   does the whole import on a copy and reports, changes nothing.
#       The import works on a copy of the database and checks it (foreign keys,
#       integrity) before it takes its place; the old database stays in
#       <data>/config-import-backup-<time>/.
#
# Needs the sqlite3 command line tool on both machines
# (Windows: `winget install SQLite.SQLite`; Linux: the sqlite package; macOS has it).
# Both servers must run the same Aurelius version (the same database schema)
# and the same environment (production or dev).
#Requires -Version 7
param(
    [Parameter(Mandatory, Position = 0)][ValidateSet("export", "import")][string]$Command,
    [Parameter(Position = 1)][string]$Path,
    [string]$OutFile,
    [string]$DataDir,
    [switch]$Dev,
    [switch]$Replace,
    [switch]$DryRun
)
$ErrorActionPreference = "Stop"
$utf8 = [Text.UTF8Encoding]::new($false)
$OutputEncoding = $utf8
[Console]::OutputEncoding = $utf8

$Format = 1
# Server settings that are configuration; the rest of `settings` belongs to this machine.
$SettingKeys = @(
    "load_limits", "browser", "browser_backend", "auto_allow_safe_tools", "management_chooses_mode",
    "follow_org_chart", "mcp_always_allow", "run_idle_timeout_s", "session_compact_threshold"
)

# --- helpers -----------------------------------------------------------------

function Get-DataDir {
    if ($DataDir) { return [IO.Path]::GetFullPath($DataDir) }
    if ($env:AURELIUS_DATA_DIR) { return [IO.Path]::GetFullPath($env:AURELIUS_DATA_DIR) }
    $name = if ($Dev) { "Aurelius-Dev" } else { "Aurelius" }
    if ($IsWindows -or $env:OS -eq "Windows_NT") { $base = $env:LOCALAPPDATA }
    elseif ($IsMacOS) { $base = Join-Path $HOME "Library/Application Support" }
    elseif ($env:XDG_DATA_HOME) { $base = $env:XDG_DATA_HOME }
    else { $base = Join-Path $HOME ".local/share" }
    Join-Path $base $name
}

function Get-Sqlite {
    $cmd = Get-Command sqlite3 -ErrorAction SilentlyContinue
    if (-not $cmd) {
        throw "sqlite3 was not found. Install it (Windows: winget install SQLite.SQLite) and open a new terminal."
    }
    $cmd.Source
}

# A path inside an SQL string literal.
function Q([string]$p) { "'" + ($p -replace '\\', '/' -replace "'", "''") + "'" }

# Run SQL (from stdin) against $db; rows come back as tab-separated lines.
function Invoke-Sql([string]$db, [string]$sql) {
    $out = $sql | & $script:sqlite -bail -batch -noheader -separator "`t" $db 2>&1
    if ($LASTEXITCODE -ne 0) { throw "sqlite3 failed on $($db):`n$($out -join "`n")" }
    $out | ForEach-Object { "$_" }
}

function Get-Value([string]$db, [string]$sql) {
    $rows = @(Invoke-Sql $db $sql)
    if ($rows.Count -eq 0) { return $null }
    $rows[0]
}

function Test-DaemonRunning([string]$root) {
    $lock = Join-Path $root "daemon.lock"
    if (-not (Test-Path $lock)) { return $false }
    try {
        # The daemon holds an exclusive lock on this file while it runs.
        $fs = [IO.File]::Open($lock, "Open", "ReadWrite", "None")
        $fs.Close()
        $false
    } catch { $true }
}

function New-TempDir {
    $dir = Join-Path ([IO.Path]::GetTempPath()) ("aurelius-config-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $dir | Out-Null
    $dir
}

function Get-Environment([string]$root) {
    $file = Join-Path $root "data/environment"
    if (Test-Path $file) { (Get-Content -Raw $file).Trim() } else { "" }
}

$sqlite = Get-Sqlite
$root = Get-DataDir
$db = Join-Path $root "data/aurelius.sqlite"
if (-not (Test-Path $db)) { throw "No Aurelius database at $db (use -DataDir, or -Dev for a dev server)." }
$now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()

# --- export ------------------------------------------------------------------

function Export-Config {
    $zip = if ($OutFile) { $OutFile } elseif ($Path) { $Path } else { "aurelius-config-$(Get-Date -Format yyyy-MM-dd-HHmm).zip" }
    $zip = [IO.Path]::GetFullPath($zip)
    $tmp = New-TempDir
    try {
        $copy = Join-Path $tmp "source.sqlite"
        # SQLite's online backup: consistent, also while the server writes.
        Invoke-Sql $db ".backup $(Q $copy)" | Out-Null

        $stage = Join-Path $tmp "stage"
        New-Item -ItemType Directory -Path $stage | Out-Null
        $out = Join-Path $stage "config.sqlite"
        $keys = ($SettingKeys | ForEach-Object { "'$_'" }) -join ","
        $envName = Get-Environment $root
        Invoke-Sql $copy @"
ATTACH $(Q $out) AS x;
CREATE TABLE x.meta(key TEXT PRIMARY KEY, value TEXT);
INSERT INTO x.meta VALUES
  ('format', '$Format'),
  ('schema_version', (SELECT MAX(version) FROM schema_migrations)),
  ('environment', '$envName'),
  ('exported_at_ms', '$now');
CREATE TABLE x.employees AS SELECT * FROM employees WHERE status <> 'archived';
CREATE TABLE x.main_chats AS
  SELECT e.id AS employee_id, c.agent_mode, c.browser_allowed, c.screen_allowed
  FROM x.employees e JOIN conversations c ON c.id =
    (SELECT id FROM conversations WHERE employee_id = e.id AND kind = 'employee' AND thread_key IS NULL ORDER BY created_at_ms, id LIMIT 1);
CREATE TABLE x.employee_overrides AS
  SELECT e.id AS employee_id, o.purpose, o.config_json FROM config_overrides o JOIN x.employees e ON e.scope_id = o.scope_id;
CREATE TABLE x.global_overrides AS
  SELECT o.purpose, o.config_json FROM config_overrides o JOIN scopes s ON s.id = o.scope_id AND s.kind = 'global';
CREATE TABLE x.employee_plugins AS
  SELECT p.employee_id, p.plugin_id, p.enabled_at_ms FROM employee_plugins p JOIN x.employees e ON e.id = p.employee_id;
CREATE TABLE x.guidelines AS
  SELECT g.id, g.created_at_ms, g.archived_at_ms, v.title, v.text, v.source
  FROM guidelines g JOIN guideline_versions v ON v.guideline_id = g.id AND v.version = g.current_version;
CREATE TABLE x.guideline_links AS
  SELECT l.guideline_id, l.employee_id, l.created_at_ms FROM guideline_links l JOIN x.employees e ON e.id = l.employee_id;
CREATE TABLE x.company AS SELECT name, dispatcher_employee_id, tester_employee_id FROM company;
CREATE TABLE x.board_columns AS SELECT kind, name, position FROM board_columns;
CREATE TABLE x.settings AS SELECT key, value_json FROM settings WHERE key IN ($keys);
CREATE TABLE x.blobs AS SELECT * FROM blobs WHERE id IN
  (SELECT identity_blob_id FROM x.employees UNION SELECT instructions_blob_id FROM x.employees);
"@ | Out-Null

        $encrypted = Get-Value $out "SELECT COUNT(*) FROM blobs WHERE encryption_key_ref IS NOT NULL;"
        if ([int]$encrypted -gt 0) { throw "Some instructions are stored encrypted; this script cannot move those." }
        $blobDir = Join-Path $stage "blobs"
        foreach ($key in Invoke-Sql $out "SELECT storage_key FROM blobs;") {
            $from = Join-Path $root "blobs/$key"
            if (-not (Test-Path $from)) { throw "Missing file for an employee's instructions: $from" }
            $to = Join-Path $blobDir $key
            New-Item -ItemType Directory -Path (Split-Path -Parent $to) -Force | Out-Null
            Copy-Item $from $to
        }

        if (Test-Path $zip) { Remove-Item $zip }
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
        $counts = @(Invoke-Sql $out "SELECT (SELECT COUNT(*) FROM employees), (SELECT COUNT(*) FROM guidelines WHERE archived_at_ms IS NULL), (SELECT COUNT(*) FROM guideline_links);")
        $c = $counts[0] -split "`t"
        Write-Host "Exported $($c[0]) employees, $($c[1]) guidelines ($($c[2]) links to employees) to:"
        Write-Host "  $zip"
        Write-Host "Copy it to the other server and run there: scripts/transfer-config.ps1 import <zip> [-Replace]"
    } finally {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    }
}

# --- import ------------------------------------------------------------------

function Import-Config {
    if (-not $Path) { throw "Give the zip made by 'export': scripts/transfer-config.ps1 import <zip>" }
    $zip = [IO.Path]::GetFullPath($Path)
    if (-not (Test-Path $zip)) { throw "Not found: $zip" }
    if (-not $DryRun -and (Test-DaemonRunning $root)) {
        throw "Aurelius is running on this machine. Close the app (or run 'aurelius-ctl shutdown') and try again."
    }

    $tmp = New-TempDir
    try {
        $pkg = Join-Path $tmp "package"
        Expand-Archive -Path $zip -DestinationPath $pkg
        $cfg = Join-Path $pkg "config.sqlite"
        if (-not (Test-Path $cfg)) { throw "This is not an export of transfer-config.ps1 (no config.sqlite)." }

        # The work copy: everything happens here first.
        $work = Join-Path $tmp "work.sqlite"
        Invoke-Sql $db ".backup $(Q $work)" | Out-Null

        # Checks before anything is written.
        $meta = @{}
        foreach ($row in Invoke-Sql $cfg "SELECT key, value FROM meta;") { $k, $v = $row -split "`t", 2; $meta[$k] = $v }
        if ([int]$meta.format -ne $Format) { throw "This export has format $($meta.format); this script reads format $Format." }
        $schema = Get-Value $work "SELECT MAX(version) FROM schema_migrations;"
        if ($schema -ne $meta.schema_version) {
            throw "The export comes from database schema $($meta.schema_version), this server has $schema. Update both servers to the same Aurelius version first."
        }
        $envName = Get-Environment $root
        if ($meta.environment -and $envName -and $meta.environment -ne $envName) {
            throw "The export comes from a $($meta.environment) server; this is a $envName server."
        }
        $company = Get-Value $work "SELECT id FROM company LIMIT 1;"
        $user = Get-Value $work "SELECT id FROM users WHERE disabled_at_ms IS NULL ORDER BY created_at_ms LIMIT 1;"
        $globalScope = Get-Value $work "SELECT id FROM scopes WHERE kind = 'global';"
        if (-not $company -or -not $user -or -not $globalScope) {
            throw "This server has not finished its onboarding yet (no company or board member). Start Aurelius once, do the onboarding, stop it, and import again."
        }

        $a = "ATTACH $(Q $cfg) AS x;"
        $already = Get-Value $work "$a SELECT COUNT(*) FROM employees WHERE id IN (SELECT id FROM x.employees);"
        if ([int]$already -gt 0) { throw "These employees are on this server already: this export was imported before." }
        if (-not $Replace) {
            $names = @(Invoke-Sql $work "$a SELECT name FROM employees WHERE status <> 'archived' AND name IN (SELECT name FROM x.employees) ORDER BY name;")
            $titles = @(Invoke-Sql $work "$a SELECT v.title FROM guidelines g JOIN guideline_versions v ON v.guideline_id = g.id AND v.version = g.current_version WHERE g.archived_at_ms IS NULL AND v.title IN (SELECT title FROM x.guidelines WHERE archived_at_ms IS NULL) ORDER BY 1;")
            if ($names.Count -or $titles.Count) {
                $msg = "This server already has"
                if ($names.Count) { $msg += "`n  employees: $($names -join ', ')" }
                if ($titles.Count) { $msg += "`n  guidelines: $($titles -join ', ')" }
                throw "$msg`nRun again with -Replace to archive what this server has now and take the imported ones."
            }
        }

        $uuid = "lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' || substr(hex(randomblob(2)), 2) || '-' || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(6)))"
        $replaceSql = if ($Replace) {
            @"
UPDATE employees SET status = 'archived', version = version + 1, updated_at_ms = $now WHERE status <> 'archived';
UPDATE guidelines SET archived_at_ms = $now WHERE archived_at_ms IS NULL;
"@
        } else { "" }

        Invoke-Sql $work @"
PRAGMA foreign_keys = ON;
$a
BEGIN;
$replaceSql
INSERT INTO blobs SELECT * FROM x.blobs WHERE id NOT IN (SELECT id FROM blobs);

INSERT INTO scopes(id, kind, parent_id, owner_id) SELECT scope_id, 'employee', '$globalScope', id FROM x.employees;
INSERT INTO employees(id, scope_id, name, role, identity_blob_id, instructions_blob_id, config_json, status, version,
                      created_at_ms, updated_at_ms, folder, title, reports_to, description, access_level)
  SELECT id, scope_id, name, role, identity_blob_id, instructions_blob_id, config_json, status, version,
         created_at_ms, $now, NULL, title, NULL, description, access_level FROM x.employees;
UPDATE employees SET reports_to = (SELECT xe.reports_to FROM x.employees xe WHERE xe.id = employees.id)
  WHERE id IN (SELECT id FROM x.employees)
    AND (SELECT xe.reports_to FROM x.employees xe WHERE xe.id = employees.id) IN (SELECT id FROM x.employees);
INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms, agent_mode, browser_allowed, screen_allowed)
  SELECT $uuid, 'employee', NULL, e.id, e.name, 1, $now,
         COALESCE(m.agent_mode, 'ask'), COALESCE(m.browser_allowed, 0), COALESCE(m.screen_allowed, 0)
  FROM x.employees e LEFT JOIN x.main_chats m ON m.employee_id = e.id;
INSERT INTO employee_plugins(employee_id, plugin_id, enabled_at_ms) SELECT employee_id, plugin_id, enabled_at_ms FROM x.employee_plugins;

-- Model choices. Connections differ per server: an employee's inherits the
-- target's, the company-wide ones keep the target's own connection.
CREATE TEMP TABLE ov AS
  SELECT e.scope_id AS scope_id, o.purpose AS purpose,
         json_set(o.config_json, '$.connection', json('{"mode":"inherit"}')) AS config_json,
         1 AS version, $uuid AS event_id
  FROM x.employee_overrides o JOIN x.employees e ON e.id = o.employee_id;
INSERT INTO ov
  SELECT '$globalScope', g.purpose,
         json_set(g.config_json, '$.connection',
                  json(COALESCE((SELECT json_extract(t.config_json, '$.connection') FROM config_overrides t
                                 WHERE t.scope_id = '$globalScope' AND t.purpose = g.purpose), '{"mode":"inherit"}'))),
         1 + MAX(COALESCE((SELECT version FROM config_overrides t WHERE t.scope_id = '$globalScope' AND t.purpose = g.purpose), 0),
                 COALESCE((SELECT MAX(aggregate_version) FROM events WHERE aggregate_type = 'config' AND aggregate_id = '${globalScope}:' || g.purpose), 0)),
         $uuid
  FROM x.global_overrides g;
INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind,
                   correlation_id, metadata_json, occurred_at_ms, recorded_at_ms, dedupe_key)
  SELECT event_id, 'config', scope_id || ':' || purpose, version, 'config.changed', 1, 'system',
         event_id, json_object('settings', json(config_json)), $now, $now, 'config-import:' || event_id FROM ov;
INSERT INTO config_overrides(scope_id, purpose, version, config_json, source_event_id)
  SELECT scope_id, purpose, version, config_json, event_id FROM ov WHERE true
  ON CONFLICT(scope_id, purpose) DO UPDATE SET version = excluded.version, config_json = excluded.config_json,
                                                source_event_id = excluded.source_event_id;

INSERT INTO guidelines(id, company_id, current_version, created_by_user, updated_by_user, created_at_ms, updated_at_ms, archived_at_ms)
  SELECT id, '$company', 1,
         CASE WHEN source = 'seed' THEN NULL ELSE '$user' END, CASE WHEN source = 'seed' THEN NULL ELSE '$user' END,
         created_at_ms, $now, archived_at_ms FROM x.guidelines;
INSERT INTO guideline_versions(guideline_id, version, title, text, source, by_user, created_at_ms)
  SELECT id, 1, title, text, CASE WHEN source = 'seed' THEN 'seed' ELSE 'created' END,
         CASE WHEN source = 'seed' THEN NULL ELSE '$user' END, $now FROM x.guidelines;
INSERT INTO guideline_links(guideline_id, employee_id, project_id, created_by_user, created_at_ms)
  SELECT guideline_id, employee_id, NULL, '$user', created_at_ms FROM x.guideline_links;

UPDATE company SET
  name = COALESCE((SELECT name FROM x.company), name),
  dispatcher_employee_id = (SELECT dispatcher_employee_id FROM x.company WHERE dispatcher_employee_id IN (SELECT id FROM x.employees)),
  tester_employee_id = (SELECT tester_employee_id FROM x.company WHERE tester_employee_id IN (SELECT id FROM x.employees)),
  version = version + 1, updated_at_ms = $now;
UPDATE board_columns SET
  name = (SELECT name FROM x.board_columns b WHERE b.kind = board_columns.kind),
  position = (SELECT position FROM x.board_columns b WHERE b.kind = board_columns.kind),
  version = version + 1
  WHERE kind IN (SELECT kind FROM x.board_columns);
INSERT INTO settings(key, value_json, updated_at_ms) SELECT key, value_json, $now FROM x.settings WHERE true
  ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at_ms = excluded.updated_at_ms;
COMMIT;
"@ | Out-Null

        $fk = @(Invoke-Sql $work "PRAGMA foreign_key_check;")
        if ($fk.Count) { throw "The imported database does not hold together (nothing was changed):`n$($fk -join "`n")" }
        $integrity = Get-Value $work "PRAGMA integrity_check;"
        if ($integrity -ne "ok") { throw "Integrity check failed (nothing was changed): $integrity" }

        $c = @(Invoke-Sql $cfg "SELECT (SELECT COUNT(*) FROM employees), (SELECT COUNT(*) FROM guidelines WHERE archived_at_ms IS NULL), (SELECT COUNT(*) FROM guideline_links), (SELECT COUNT(*) FROM settings);")[0] -split "`t"
        $summary = "$($c[0]) employees, $($c[1]) guidelines ($($c[2]) links to employees), company, board columns, $($c[3]) settings"
        if ($DryRun) {
            Write-Host "Dry run: the import of $summary works on a copy; nothing was changed."
            if (-not $Replace) { Write-Host "(No clashes found, so -Replace is not needed.)" }
            return
        }
        if (Test-DaemonRunning $root) { throw "Aurelius was started during the import; nothing was changed. Stop it and try again." }

        # The blob files first (only new files), then the database swap.
        $blobDir = Join-Path $pkg "blobs"
        if (Test-Path $blobDir) {
            foreach ($f in Get-ChildItem -Recurse -File $blobDir) {
                $rel = [IO.Path]::GetRelativePath($blobDir, $f.FullName)
                $to = Join-Path $root "blobs/$rel"
                if (-not (Test-Path $to)) {
                    New-Item -ItemType Directory -Path (Split-Path -Parent $to) -Force | Out-Null
                    Copy-Item $f.FullName $to
                }
            }
        }
        $backup = Join-Path $root "config-import-backup-$(Get-Date -Format yyyyMMdd-HHmmss)"
        New-Item -ItemType Directory -Path $backup | Out-Null
        foreach ($suffix in "", "-wal", "-shm") {
            if (Test-Path "$db$suffix") { Move-Item "$db$suffix" (Join-Path $backup "aurelius.sqlite$suffix") }
        }
        Copy-Item $work $db
        Write-Host "Imported $summary."
        if ($Replace) { Write-Host "The employees and guidelines this server had are archived (not deleted)." }
        Write-Host "The old database is in $backup"
        Write-Host "Start Aurelius again. Check each employee's model and give the guidelines that were linked to projects their projects again."
    } finally {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    }
}

if ($Command -eq "export") { Export-Config } else { Import-Config }
