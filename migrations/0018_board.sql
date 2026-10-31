-- v0.11 (protocol 4): the company as a table, its board members (users), the
-- board's columns and its tickets. A ticket's main task carries `ticket_id`
-- (inherited by every delegation below it); a planned ticket waits with
-- waiting_reason 'plan_review'. A project may name its own dispatcher.
--
-- A protocol 3 workspace never migrates into this schema: it gets a fresh
-- database with a one-time import (storage::fresh_start), so in practice this
-- runs on an empty database. It is still a correct rebuild (foreign keys off,
-- checked before the commit), so test fixtures and any other order stay safe.

-- The company: exactly one row (the unique index on a constant), its memory is
-- the global scope's. `dispatcher_employee_id` NULL: no default dispatcher yet.
CREATE TABLE company (
  id TEXT PRIMARY KEY,
  scope_id TEXT NOT NULL UNIQUE REFERENCES scopes(id),
  name TEXT NOT NULL,
  dispatcher_employee_id TEXT REFERENCES employees(id),
  version INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX one_company ON company((0));

-- The board members. The password is never stored, only its argon2id PHC string;
-- a user is never deleted, so every `created_by_user` stays valid.
CREATE TABLE users (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES company(id),
  username TEXT NOT NULL CHECK(length(username) BETWEEN 3 AND 32),
  password_hash TEXT NOT NULL,
  -- NULL: made in the onboarding (no active board member).
  created_by_user TEXT REFERENCES users(id),
  version INTEGER NOT NULL DEFAULT 1,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX users_username ON users(username COLLATE NOCASE);

-- The six columns. Behaviour hangs on `kind`; name and order can be changed.
CREATE TABLE board_columns (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES company(id),
  kind TEXT NOT NULL CHECK(kind IN ('backlog','todo','doing','plan_review','review','done')),
  name TEXT NOT NULL,
  position INTEGER NOT NULL,
  version INTEGER NOT NULL,
  UNIQUE(company_id, kind)
) STRICT;

CREATE TABLE tickets (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES company(id),
  project_id TEXT NOT NULL REFERENCES projects(id),
  -- Numbered per company, for display.
  number INTEGER NOT NULL CHECK(number > 0),
  title TEXT NOT NULL,
  description_blob_id TEXT REFERENCES blobs(id),
  column_id TEXT NOT NULL REFERENCES board_columns(id),
  -- The order within its column.
  position REAL NOT NULL,
  priority TEXT NOT NULL DEFAULT 'normal' CHECK(priority IN ('low','normal','high','urgent')),
  -- The active board member of the connection that made it; never changes.
  created_by_user TEXT NOT NULL REFERENCES users(id),
  owner_employee_id TEXT REFERENCES employees(id),
  run_json TEXT CHECK(run_json IS NULL OR json_valid(run_json)),
  labels_json TEXT CHECK(labels_json IS NULL OR json_valid(labels_json)),
  attachments_json TEXT CHECK(attachments_json IS NULL OR json_valid(attachments_json)),
  deadline_ms INTEGER,
  -- The current main task (an earlier one stays linked through tasks.ticket_id).
  root_task_id TEXT REFERENCES tasks(id),
  review_feedback TEXT,
  version INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  started_at_ms INTEGER,
  done_at_ms INTEGER,
  archived_at_ms INTEGER,
  UNIQUE(company_id, number)
) STRICT;
CREATE INDEX tickets_column ON tickets(column_id, position);
CREATE INDEX tickets_project ON tickets(project_id, archived_at_ms);
CREATE INDEX tickets_root_task ON tickets(root_task_id) WHERE root_task_id IS NOT NULL;

-- Tasks: `ticket_id` and the waiting reason 'plan_review'. Every row is copied.
CREATE TABLE tasks_new (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id),
  employee_id TEXT REFERENCES "employees"(id), parent_task_id TEXT REFERENCES tasks(id),
  title TEXT NOT NULL, brief_blob_id TEXT NOT NULL REFERENCES blobs(id),
  acceptance_blob_id TEXT REFERENCES blobs(id),
  status TEXT NOT NULL CHECK(status IN ('draft','queued','running','waiting','review','succeeded','failed','cancelled')),
  waiting_reason TEXT CHECK(waiting_reason IN ('approval','input','dependency','quota','offline','paused','reconcile','plan_review')),
  priority INTEGER NOT NULL DEFAULT 0, version INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL,
  result_blob_id TEXT REFERENCES blobs(id),
  conversation_id TEXT REFERENCES conversations(id),
  run_json TEXT,
  origin_conversation_id TEXT REFERENCES conversations(id),
  requested_by TEXT REFERENCES "employees"(id),
  attachments_json TEXT,
  -- The recurring task this one was made from.
  schedule_id TEXT, created_by_app_key_id TEXT REFERENCES app_keys(id),
  -- The ticket of the main task, inherited by its delegations.
  ticket_id TEXT REFERENCES tickets(id),
  CHECK((status='waiting' AND waiting_reason IS NOT NULL) OR (status<>'waiting' AND waiting_reason IS NULL))
) STRICT;
INSERT INTO tasks_new(id, project_id, employee_id, parent_task_id, title, brief_blob_id, acceptance_blob_id, status, waiting_reason,
                      priority, version, created_at_ms, updated_at_ms, result_blob_id, conversation_id, run_json,
                      origin_conversation_id, requested_by, attachments_json, schedule_id, created_by_app_key_id)
  SELECT id, project_id, employee_id, parent_task_id, title, brief_blob_id, acceptance_blob_id, status, waiting_reason,
         priority, version, created_at_ms, updated_at_ms, result_blob_id, conversation_id, run_json,
         origin_conversation_id, requested_by, attachments_json, schedule_id, created_by_app_key_id
  FROM tasks;
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;
CREATE INDEX tasks_project_status ON tasks(project_id,status,priority);
CREATE INDEX tasks_parent ON tasks(parent_task_id);
CREATE INDEX tasks_schedule ON tasks(schedule_id) WHERE schedule_id IS NOT NULL;
CREATE INDEX tasks_app_key ON tasks(created_by_app_key_id, created_at_ms) WHERE created_by_app_key_id IS NOT NULL;
CREATE INDEX tasks_employee ON tasks(employee_id, status);
CREATE INDEX tasks_ticket ON tasks(ticket_id) WHERE ticket_id IS NOT NULL;

-- A project's own dispatcher (NULL: the company's).
ALTER TABLE projects ADD COLUMN dispatcher_employee_id TEXT REFERENCES employees(id);

-- Seed: the global scope (if the database has none yet), the company and its six
-- columns. Ids are random UUIDs (version 4) like every other id. The default
-- dispatcher is chosen by the fresh start's import, not here.
INSERT INTO scopes(id, kind)
  SELECT lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-'
         || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6))),
         'global'
  WHERE NOT EXISTS (SELECT 1 FROM scopes WHERE kind = 'global');
INSERT INTO company(id, scope_id, name, dispatcher_employee_id, version, created_at_ms, updated_at_ms)
  SELECT lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-'
         || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6))),
         (SELECT id FROM scopes WHERE kind = 'global'), 'Company', NULL, 1,
         CAST(strftime('%s', 'now') AS INTEGER) * 1000, CAST(strftime('%s', 'now') AS INTEGER) * 1000;
INSERT INTO board_columns(id, company_id, kind, name, position, version)
  SELECT lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-'
         || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6))),
         (SELECT id FROM company), c.kind, c.name, c.position, 1
  FROM (SELECT 'backlog' AS kind, 'Backlog' AS name, 0 AS position
        UNION ALL SELECT 'todo', 'Todo', 1
        UNION ALL SELECT 'doing', 'Doing', 2
        UNION ALL SELECT 'plan_review', 'Plan review', 3
        UNION ALL SELECT 'review', 'Review', 4
        UNION ALL SELECT 'done', 'Done', 5) c;

-- Departments are gone (protocol 4): one org chart through reports_to. Employees
-- and proposals lose department_id, scopes lose the kind 'department', and the
-- departments table goes. A workspace with data takes the fresh start (which
-- moves department memory itself); for any other database, department memory
-- moves to the company (global) scope with locator_json.from_department, and
-- what else pointed at a department scope is moved or dropped, so no reference
-- breaks.
UPDATE memory_items
  SET locator_json = json_set(CASE WHEN json_type(locator_json) = 'object' THEN locator_json ELSE '{}' END, '$.from_department',
                              COALESCE((SELECT d.name FROM departments d WHERE d.scope_id = memory_items.scope_id), 'department')),
      scope_id = (SELECT id FROM scopes WHERE kind = 'global')
  WHERE scope_id IN (SELECT id FROM scopes WHERE kind = 'department');
UPDATE retrieval_chunks SET scope_id = (SELECT id FROM scopes WHERE kind = 'global')
  WHERE scope_id IN (SELECT id FROM scopes WHERE kind = 'department');
UPDATE shares SET destination_scope_id = (SELECT id FROM scopes WHERE kind = 'global')
  WHERE destination_scope_id IN (SELECT id FROM scopes WHERE kind = 'department');
DELETE FROM config_overrides WHERE scope_id IN (SELECT id FROM scopes WHERE kind = 'department');
DELETE FROM file_grants WHERE scope_id IN (SELECT id FROM scopes WHERE kind = 'department');

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
  CHECK(reports_to IS NULL OR reports_to <> id)
) STRICT;
INSERT INTO employees_new(id, scope_id, name, role, identity_blob_id, instructions_blob_id, config_json, status, version,
                          created_at_ms, updated_at_ms, folder, title, reports_to)
  SELECT id, scope_id, name, role, identity_blob_id, instructions_blob_id, config_json, status, version,
         created_at_ms, updated_at_ms, folder, title, reports_to
  FROM employees;
DROP TABLE employees;
ALTER TABLE employees_new RENAME TO employees;
CREATE INDEX employees_reports_to ON employees(reports_to);

CREATE TABLE proposals_new (
  id TEXT PRIMARY KEY,
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
INSERT INTO proposals_new(id, proposed_by, status, name, role, title, instructions_blob_id, reports_to, reason, decision_note,
                          created_employee_id, origin_conversation_id, created_at_ms, decided_at_ms, kind, target_employee_id,
                          base_version, evidence_json)
  SELECT id, proposed_by, status, name, role, title, instructions_blob_id, reports_to, reason, decision_note,
         created_employee_id, origin_conversation_id, created_at_ms, decided_at_ms, kind, target_employee_id,
         base_version, evidence_json
  FROM proposals;
DROP TABLE proposals;
ALTER TABLE proposals_new RENAME TO proposals;
CREATE INDEX proposals_status ON proposals(status, created_at_ms);
CREATE INDEX proposals_proposer ON proposals(proposed_by, status);
CREATE INDEX proposals_target ON proposals(target_employee_id, status);

DROP TABLE departments;
CREATE TABLE scopes_new (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('global','project','employee')),
  parent_id TEXT REFERENCES scopes(id), owner_id TEXT,
  CHECK((kind='global' AND parent_id IS NULL AND owner_id IS NULL)
     OR (kind IN ('project','employee') AND parent_id IS NOT NULL AND owner_id IS NOT NULL)),
  UNIQUE(kind, owner_id)
) STRICT;
INSERT INTO scopes_new(id, kind, parent_id, owner_id)
  SELECT id, kind, parent_id, owner_id FROM scopes WHERE kind <> 'department';
DROP TABLE scopes;
ALTER TABLE scopes_new RENAME TO scopes;
CREATE UNIQUE INDEX one_global_scope ON scopes(kind) WHERE kind='global';
