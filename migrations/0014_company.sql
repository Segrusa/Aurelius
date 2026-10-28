-- v0.9 / protocol 3.0: one company. Managers of a project become company-wide
-- employees (ids kept), with real departments that have their own memory scope,
-- one company-wide org chart, employee chats outside projects and
-- `follow_org_chart` as a server setting.
--
-- Runs with foreign keys off (storage::migrate does that for this migration),
-- the SQLite way to rebuild tables: make the new table, copy, drop the old one,
-- rename. `PRAGMA foreign_key_check` must be clean before it commits.
-- Then `storage::company_convert` runs in the same transaction: departments
-- from the old projects, same-name employees merged, the conversion report.

-- 1. Departments, and scopes that know them.
CREATE TABLE departments (
  id TEXT PRIMARY KEY,
  scope_id TEXT NOT NULL UNIQUE REFERENCES scopes(id),
  name TEXT NOT NULL,
  description TEXT,
  -- The employee who leads it; cleared when that employee is archived.
  head_id TEXT REFERENCES employees(id),
  status TEXT NOT NULL CHECK(status IN ('active','archived')),
  version INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE scopes_new (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('global','project','employee','department')),
  parent_id TEXT REFERENCES scopes(id), owner_id TEXT,
  CHECK((kind='global' AND parent_id IS NULL AND owner_id IS NULL)
     OR (kind IN ('project','employee','department') AND parent_id IS NOT NULL AND owner_id IS NOT NULL)),
  UNIQUE(kind, owner_id)
) STRICT;
-- An employee's scope hangs below the global one now (no project).
INSERT INTO scopes_new(id, kind, parent_id, owner_id)
  SELECT id,
         CASE kind WHEN 'manager' THEN 'employee' ELSE kind END,
         CASE kind WHEN 'manager' THEN (SELECT g.id FROM scopes g WHERE g.kind = 'global') ELSE parent_id END,
         owner_id
  FROM scopes;
DROP TABLE scopes;
ALTER TABLE scopes_new RENAME TO scopes;
CREATE UNIQUE INDEX one_global_scope ON scopes(kind) WHERE kind='global';

-- 2. Managers become employees. The rename carries over to every REFERENCES.
ALTER TABLE managers RENAME TO employees;
ALTER TABLE manager_proposals RENAME TO proposals;
ALTER TABLE manager_plugins RENAME TO employee_plugins;
ALTER TABLE manager_instruction_versions RENAME TO employee_instruction_versions;

-- Which project each employee came from, for the convert step (dropped there).
CREATE TABLE _company_convert (employee_id TEXT PRIMARY KEY, project_id TEXT NOT NULL) STRICT;
INSERT INTO _company_convert(employee_id, project_id) SELECT id, project_id FROM employees;
CREATE TABLE _company_convert_proposals (proposal_id TEXT PRIMARY KEY, project_id TEXT NOT NULL) STRICT;
INSERT INTO _company_convert_proposals(proposal_id, project_id) SELECT id, project_id FROM proposals;

DROP INDEX managers_project;
DROP INDEX managers_reports_to;
CREATE TABLE employees_new (
  id TEXT PRIMARY KEY,
  scope_id TEXT NOT NULL UNIQUE REFERENCES scopes(id), name TEXT NOT NULL, role TEXT NOT NULL,
  identity_blob_id TEXT REFERENCES blobs(id), instructions_blob_id TEXT REFERENCES blobs(id),
  config_json TEXT NOT NULL CHECK(json_valid(config_json)),
  status TEXT NOT NULL CHECK(status IN ('active','paused','archived')),
  version INTEGER NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL,
  folder TEXT,
  -- The org chart: a position title (at most 80 characters) and who this employee reports to.
  title TEXT,
  reports_to TEXT REFERENCES employees(id),
  -- The department it belongs to (none: no department).
  department_id TEXT REFERENCES departments(id),
  CHECK(reports_to IS NULL OR reports_to <> id)
) STRICT;
INSERT INTO employees_new(id, scope_id, name, role, identity_blob_id, instructions_blob_id, config_json, status, version,
                          created_at_ms, updated_at_ms, folder, title, reports_to, department_id)
  SELECT id, scope_id, name, role, identity_blob_id, instructions_blob_id, config_json, status, version,
         created_at_ms, updated_at_ms, folder, title, reports_to, NULL
  FROM employees;
DROP TABLE employees;
ALTER TABLE employees_new RENAME TO employees;
CREATE INDEX employees_reports_to ON employees(reports_to);
CREATE INDEX employees_department ON employees(department_id);

-- A colleague an employee proposed (with the department it would join), or new instructions.
DROP INDEX manager_proposals_project;
DROP INDEX manager_proposals_proposer;
DROP INDEX manager_proposals_target;
CREATE TABLE proposals_new (
  id TEXT PRIMARY KEY,
  department_id TEXT REFERENCES departments(id),
  proposed_by TEXT NOT NULL REFERENCES employees(id),
  status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected','withdrawn')),
  name TEXT NOT NULL, role TEXT NOT NULL, title TEXT,
  instructions_blob_id TEXT NOT NULL REFERENCES blobs(id),
  reports_to TEXT REFERENCES employees(id),
  reason TEXT NOT NULL,
  decision_note TEXT,
  created_employee_id TEXT REFERENCES employees(id),
  origin_conversation_id TEXT REFERENCES conversations(id),
  created_at_ms INTEGER NOT NULL, decided_at_ms INTEGER,
  kind TEXT NOT NULL DEFAULT 'colleague' CHECK(kind IN ('colleague','instructions')),
  target_employee_id TEXT REFERENCES employees(id),
  base_version INTEGER,
  evidence_json TEXT CHECK(evidence_json IS NULL OR json_valid(evidence_json))
) STRICT;
INSERT INTO proposals_new(id, department_id, proposed_by, status, name, role, title, instructions_blob_id, reports_to, reason,
                          decision_note, created_employee_id, origin_conversation_id, created_at_ms, decided_at_ms,
                          kind, target_employee_id, base_version, evidence_json)
  SELECT id, NULL, proposed_by, status, name, role, title, instructions_blob_id, reports_to, reason,
         decision_note, created_manager_id, origin_conversation_id, created_at_ms, decided_at_ms,
         kind, target_manager_id, base_version, evidence_json
  FROM proposals;
DROP TABLE proposals;
ALTER TABLE proposals_new RENAME TO proposals;
CREATE INDEX proposals_status ON proposals(status, created_at_ms);
CREATE INDEX proposals_proposer ON proposals(proposed_by, status);
CREATE INDEX proposals_target ON proposals(target_employee_id, status);

ALTER TABLE employee_plugins RENAME COLUMN manager_id TO employee_id;
ALTER TABLE employee_instruction_versions RENAME COLUMN manager_id TO employee_id;

-- 3. manager_id → employee_id everywhere.
ALTER TABLE tasks RENAME COLUMN manager_id TO employee_id;
ALTER TABLE runs RENAME COLUMN manager_id TO employee_id;
ALTER TABLE heartbeats RENAME COLUMN manager_id TO employee_id;
ALTER TABLE recurring_tasks RENAME COLUMN manager_id TO employee_id;
ALTER TABLE ideas RENAME COLUMN manager_id TO employee_id;
ALTER TABLE agent_functions RENAME COLUMN manager_id TO employee_id;
CREATE INDEX tasks_employee ON tasks(employee_id, status);

-- Conversations: a manager's conversations (kind 'project' with a manager) become
-- employee chats without a project. task → project + employee; employee →
-- employee, no project; project → project, no employee; chat → neither.
CREATE TABLE conversations_new (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('chat','project','task','employee')),
  project_id TEXT REFERENCES projects(id), employee_id TEXT REFERENCES employees(id),
  title TEXT NOT NULL, version INTEGER NOT NULL, created_at_ms INTEGER NOT NULL,
  archived_at_ms INTEGER,
  web_research INTEGER NOT NULL DEFAULT 0,
  agent_mode TEXT NOT NULL DEFAULT 'ask' CHECK(agent_mode IN ('plan','ask','accept_edits','auto')),
  browser_allowed INTEGER NOT NULL DEFAULT 0 CHECK(browser_allowed IN (0,1)),
  screen_allowed INTEGER NOT NULL DEFAULT 0 CHECK(screen_allowed IN (0,1)),
  run_json TEXT,
  CHECK((kind='chat' AND project_id IS NULL AND employee_id IS NULL) OR
        (kind='project' AND project_id IS NOT NULL AND employee_id IS NULL) OR
        (kind='task' AND project_id IS NOT NULL AND employee_id IS NOT NULL) OR
        (kind='employee' AND project_id IS NULL AND employee_id IS NOT NULL))
) STRICT;
INSERT INTO conversations_new(id, kind, project_id, employee_id, title, version, created_at_ms, archived_at_ms, web_research,
                              agent_mode, browser_allowed, screen_allowed, run_json)
  SELECT id,
         CASE WHEN kind = 'project' AND manager_id IS NOT NULL THEN 'employee' ELSE kind END,
         CASE WHEN kind = 'project' AND manager_id IS NOT NULL THEN NULL ELSE project_id END,
         manager_id, title, version, created_at_ms, archived_at_ms, web_research,
         agent_mode, browser_allowed, screen_allowed, run_json
  FROM conversations;
DROP TABLE conversations;
ALTER TABLE conversations_new RENAME TO conversations;
CREATE INDEX conversations_employee ON conversations(employee_id, kind, created_at_ms);
CREATE INDEX conversations_project ON conversations(project_id, kind);

-- The audit log: same rows and seq, the new names in type, aggregate, actor and metadata.
CREATE TABLE events_new (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  id TEXT NOT NULL UNIQUE,
  aggregate_type TEXT NOT NULL, aggregate_id TEXT NOT NULL,
  aggregate_version INTEGER NOT NULL CHECK(aggregate_version > 0),
  type TEXT NOT NULL, schema_version INTEGER NOT NULL CHECK(schema_version > 0),
  project_id TEXT, employee_id TEXT, task_id TEXT, run_id TEXT,
  actor_kind TEXT NOT NULL CHECK(actor_kind IN ('user','employee','system','runtime')),
  actor_id TEXT, correlation_id TEXT NOT NULL,
  causation_id TEXT REFERENCES events(id),
  payload_blob_id TEXT REFERENCES blobs(id),
  metadata_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(metadata_json)),
  occurred_at_ms INTEGER NOT NULL, recorded_at_ms INTEGER NOT NULL,
  dedupe_key TEXT NOT NULL UNIQUE,
  UNIQUE(aggregate_type, aggregate_id, aggregate_version)
) STRICT;
INSERT INTO events_new(seq, id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, project_id, employee_id,
                       task_id, run_id, actor_kind, actor_id, correlation_id, causation_id, payload_blob_id, metadata_json,
                       occurred_at_ms, recorded_at_ms, dedupe_key)
  SELECT seq, id,
         CASE WHEN aggregate_type LIKE 'manager%' THEN 'employee' || substr(aggregate_type, 8) ELSE aggregate_type END,
         aggregate_id, aggregate_version,
         CASE WHEN type LIKE 'manager.%' THEN 'employee.' || substr(type, 9) ELSE type END,
         schema_version, project_id, manager_id, task_id, run_id,
         CASE actor_kind WHEN 'manager' THEN 'employee' ELSE actor_kind END,
         actor_id, correlation_id, causation_id, payload_blob_id,
         replace(replace(replace(replace(metadata_json,
           '"manager_id":', '"employee_id":'),
           '"target_manager_id":', '"target_employee_id":'),
           '"created_manager_id":', '"created_employee_id":'),
           '"from_manager":', '"from_employee":'),
         occurred_at_ms, recorded_at_ms, dedupe_key
  FROM events;
DROP TABLE events;
ALTER TABLE events_new RENAME TO events;
CREATE INDEX events_project_seq ON events(project_id, seq);
CREATE INDEX events_correlation ON events(correlation_id, seq);
CREATE INDEX events_causation ON events(causation_id);
CREATE INDEX events_task_seq ON events(task_id, seq);
CREATE TRIGGER events_no_update BEFORE UPDATE ON events BEGIN
  SELECT RAISE(ABORT, 'events are append-only');
END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events BEGIN
  SELECT RAISE(ABORT, 'events are append-only');
END;

-- Other stored names.
UPDATE messages SET meta_json = replace(meta_json, '"from_manager":', '"from_employee":') WHERE meta_json LIKE '%"from_manager":%';
UPDATE memory_sources SET locator_json = replace(locator_json, '"authored_by_manager":', '"authored_by_employee":') WHERE locator_json LIKE '%authored_by_manager%';
UPDATE memory_items SET decided_by = 'employee' WHERE decided_by = 'manager';
UPDATE memory_items SET extraction_version = 'employee' WHERE extraction_version = 'manager';
UPDATE config_overrides SET purpose = 'employee' WHERE purpose = 'manager';
UPDATE jobs SET payload_json = replace(payload_json, '"manager_id":', '"employee_id":') WHERE payload_json LIKE '%"manager_id":%';
UPDATE notifications SET target_json = replace(target_json, '"manager_id":', '"employee_id":') WHERE target_json LIKE '%"manager_id":%';

-- 4. "Follow the org chart" is one server setting now: on when any project had it
-- on. A server without projects keeps no row (the default is on).
INSERT INTO settings(key, value_json, updated_at_ms)
  SELECT 'follow_org_chart',
         CASE WHEN EXISTS(SELECT 1 FROM projects WHERE follow_org_chart = 1) THEN 'true' ELSE 'false' END,
         CAST(strftime('%s', 'now') AS INTEGER) * 1000
  WHERE EXISTS(SELECT 1 FROM projects)
  ON CONFLICT(key) DO NOTHING;
-- Rebuilt rather than DROP COLUMN: SQLite's rewrite of the old CREATE text
-- trips over the comment above the dropped (last) column.
CREATE TABLE projects_new (
  id TEXT PRIMARY KEY, scope_id TEXT NOT NULL UNIQUE REFERENCES scopes(id),
  name TEXT NOT NULL, description_blob_id TEXT REFERENCES blobs(id),
  status TEXT NOT NULL CHECK(status IN ('active','paused','archived')),
  version INTEGER NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL,
  folder TEXT
) STRICT;
INSERT INTO projects_new(id, scope_id, name, description_blob_id, status, version, created_at_ms, updated_at_ms, folder)
  SELECT id, scope_id, name, description_blob_id, status, version, created_at_ms, updated_at_ms, folder FROM projects;
DROP TABLE projects;
ALTER TABLE projects_new RENAME TO projects;
