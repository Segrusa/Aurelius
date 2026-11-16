-- v0.27 (protocol 5, feature `work_sessions`): work sessions, as in Claude Desktop.
--
-- * conversations.kind 'session': a conversation of one employee in one project's
--   folder, started by a board member, without a ticket. It replaces the quick task
--   for new work; quick tickets made before stay what they are.
-- * work_sessions: who started it and, when it asked for one, its own worktree and
--   branch (NULL: it works in the project's folder itself).
--
-- The conversations table is rebuilt for the new kind (its CHECK); every row and
-- index stays as it was.
CREATE TABLE conversations_new (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('project','task','employee','group','ticket','session')),
  project_id TEXT REFERENCES projects(id), employee_id TEXT REFERENCES employees(id),
  title TEXT NOT NULL, version INTEGER NOT NULL, created_at_ms INTEGER NOT NULL,
  archived_at_ms INTEGER,
  agent_mode TEXT NOT NULL DEFAULT 'ask' CHECK(agent_mode IN ('plan','ask','accept_edits','auto')),
  browser_allowed INTEGER NOT NULL DEFAULT 0 CHECK(browser_allowed IN (0,1)),
  screen_allowed INTEGER NOT NULL DEFAULT 0 CHECK(screen_allowed IN (0,1)),
  run_json TEXT, quick_answer INTEGER NOT NULL DEFAULT 0 CHECK(quick_answer IN (0,1)), thread_key TEXT, board_member_id TEXT,
  CHECK((kind='project' AND project_id IS NOT NULL AND employee_id IS NULL) OR
        (kind='task' AND project_id IS NOT NULL AND employee_id IS NOT NULL) OR
        (kind='employee' AND project_id IS NULL AND employee_id IS NOT NULL) OR
        (kind='group' AND employee_id IS NULL) OR
        (kind='ticket' AND project_id IS NOT NULL AND employee_id IS NULL) OR
        (kind='session' AND project_id IS NOT NULL AND employee_id IS NOT NULL))
) STRICT;
INSERT INTO conversations_new(id, kind, project_id, employee_id, title, version, created_at_ms, archived_at_ms,
                              agent_mode, browser_allowed, screen_allowed, run_json, quick_answer, thread_key, board_member_id)
  SELECT id, kind, project_id, employee_id, title, version, created_at_ms, archived_at_ms,
         agent_mode, browser_allowed, screen_allowed, run_json, quick_answer, thread_key, board_member_id
  FROM conversations;
DROP TABLE conversations;
ALTER TABLE conversations_new RENAME TO conversations;
CREATE INDEX conversations_employee ON conversations(employee_id, kind, created_at_ms);
CREATE INDEX conversations_project ON conversations(project_id, kind);
CREATE UNIQUE INDEX one_general_channel ON conversations(kind) WHERE kind = 'group' AND project_id IS NULL;
CREATE UNIQUE INDEX one_project_channel ON conversations(project_id) WHERE kind = 'group' AND project_id IS NOT NULL;
CREATE UNIQUE INDEX conversations_board_member_chat ON conversations(employee_id, board_member_id) WHERE board_member_id IS NOT NULL;

CREATE TABLE work_sessions (
  conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
  started_by TEXT NOT NULL,
  worktree TEXT,
  branch TEXT,
  created_at_ms INTEGER NOT NULL,
  CHECK((worktree IS NULL) = (branch IS NULL))
) STRICT;
CREATE INDEX work_sessions_started_by ON work_sessions(started_by, created_at_ms);
