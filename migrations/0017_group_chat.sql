-- v0.10: one group chat ("Team chat") for the whole company: the user and every
-- employee. Its kind is 'group', with neither a project nor an employee. The
-- employees answer from their own (hidden) main chat; their answers are copied
-- here as authored messages (role 'tool', meta {kind: "group", from: <name>}).
--
-- Runs with foreign keys off (Migration::rebuild): the conversations table is
-- rebuilt with the new kind. Every row is copied; nothing is deleted.

CREATE TABLE conversations_new (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('project','task','employee','group')),
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
        (kind='group' AND project_id IS NULL AND employee_id IS NULL))
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
-- One group chat per company (one company per server).
CREATE UNIQUE INDEX one_group_conversation ON conversations(kind) WHERE kind = 'group';

-- The group chat itself, made here (not lazily) so reading it never writes.
-- Its id is a random UUID (version 4) like every other id.
INSERT INTO conversations(id, kind, title, version, created_at_ms)
  SELECT lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-'
         || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6))),
         'group', 'Team chat', 1, CAST(strftime('%s', 'now') AS INTEGER) * 1000;
