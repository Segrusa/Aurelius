-- v0.12 (protocol 4, phase 2): discussions. The Team chat gets channels: the
-- one group chat of 0017 stays the general channel (kind 'group', no project),
-- and every project gets its own (kind 'group' with the project). Every ticket
-- gets its own discussion (kind 'ticket', with the ticket's project), made with
-- the ticket. A message records the board member who wrote it.
--
-- Runs with foreign keys off (Migration::rebuild): conversations and tickets
-- are rebuilt. Every row is copied; nothing is deleted.

CREATE TABLE conversations_new (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('project','task','employee','group','ticket')),
  project_id TEXT REFERENCES projects(id), employee_id TEXT REFERENCES employees(id),
  title TEXT NOT NULL, version INTEGER NOT NULL, created_at_ms INTEGER NOT NULL,
  archived_at_ms INTEGER,
  agent_mode TEXT NOT NULL DEFAULT 'ask' CHECK(agent_mode IN ('plan','ask','accept_edits','auto')),
  browser_allowed INTEGER NOT NULL DEFAULT 0 CHECK(browser_allowed IN (0,1)),
  screen_allowed INTEGER NOT NULL DEFAULT 0 CHECK(screen_allowed IN (0,1)),
  run_json TEXT,
  CHECK((kind='project' AND project_id IS NOT NULL AND employee_id IS NULL) OR
        (kind='task' AND project_id IS NOT NULL AND employee_id IS NOT NULL) OR
        (kind='employee' AND project_id IS NULL AND employee_id IS NOT NULL) OR
        (kind='group' AND employee_id IS NULL) OR
        (kind='ticket' AND project_id IS NOT NULL AND employee_id IS NULL))
) STRICT;
INSERT INTO conversations_new(id, kind, project_id, employee_id, title, version, created_at_ms, archived_at_ms,
                              agent_mode, browser_allowed, screen_allowed, run_json)
  SELECT id, kind, project_id, employee_id, title, version, created_at_ms, archived_at_ms,
         agent_mode, browser_allowed, screen_allowed, run_json
  FROM conversations;
DROP TABLE conversations;
ALTER TABLE conversations_new RENAME TO conversations;
CREATE INDEX conversations_employee ON conversations(employee_id, kind, created_at_ms);
CREATE INDEX conversations_project ON conversations(project_id, kind);
-- One general channel (the group chat of 0017) and one channel per project.
CREATE UNIQUE INDEX one_general_channel ON conversations(kind) WHERE kind = 'group' AND project_id IS NULL;
CREATE UNIQUE INDEX one_project_channel ON conversations(project_id) WHERE kind = 'group' AND project_id IS NOT NULL;

-- A channel for every project there is (archived ones too: theirs is read-only),
-- titled with the project's name. Ids are random UUIDs (version 4).
INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms)
  SELECT lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-'
         || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6))),
         'group', p.id, p.name, 1, CAST(strftime('%s', 'now') AS INTEGER) * 1000
  FROM projects p;

-- Tickets: `conversation_id`, the ticket's discussion (exactly one per ticket).
-- Each existing ticket gets the id of its discussion here, and the row below.
CREATE TABLE tickets_new (
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
  -- The ticket's discussion (kind 'ticket'), made in the same transaction.
  conversation_id TEXT NOT NULL UNIQUE REFERENCES conversations(id),
  UNIQUE(company_id, number)
) STRICT;
INSERT INTO tickets_new(id, company_id, project_id, number, title, description_blob_id, column_id, position, priority,
                        created_by_user, owner_employee_id, run_json, labels_json, attachments_json, deadline_ms,
                        root_task_id, review_feedback, version, created_at_ms, updated_at_ms, started_at_ms, done_at_ms,
                        archived_at_ms, conversation_id)
  SELECT id, company_id, project_id, number, title, description_blob_id, column_id, position, priority,
         created_by_user, owner_employee_id, run_json, labels_json, attachments_json, deadline_ms,
         root_task_id, review_feedback, version, created_at_ms, updated_at_ms, started_at_ms, done_at_ms,
         archived_at_ms,
         lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-'
         || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6)))
  FROM tickets;
INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms)
  SELECT conversation_id, 'ticket', project_id, '#' || number || ' ' || title, 1, created_at_ms FROM tickets_new;
DROP TABLE tickets;
ALTER TABLE tickets_new RENAME TO tickets;
CREATE INDEX tickets_column ON tickets(column_id, position);
CREATE INDEX tickets_project ON tickets(project_id, archived_at_ms);
CREATE INDEX tickets_root_task ON tickets(root_task_id) WHERE root_task_id IS NOT NULL;

-- The board member who wrote a message (NULL: an employee, Aurelius, or a
-- message from before phase 2).
ALTER TABLE messages ADD COLUMN author_user_id TEXT REFERENCES users(id);
-- The posts that woke colleagues, counted per run and per employee over all
-- discussions (RULE 08): only these few rows, not every message.
CREATE INDEX messages_waking ON messages(created_at_ms) WHERE role = 'tool' AND json_extract(meta_json, '$.woke') > 0;
