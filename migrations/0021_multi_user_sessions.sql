-- v0.13 (protocol 5, phase 3): board members log in. A login is a user
-- session (only the verifier of its token is stored); a board member can be
-- disabled and its password reset (a temporary one must be changed at the next
-- login). Every board member links its own Claude login: the database holds
-- only its status, never a token or a path (the folder is always
-- <data>/logins/<user_id>/). Runs, tickets, app keys, recurring tasks and
-- heartbeats record whose login they run on; without one, a task waits with
-- the new waiting reason 'login'.
--
-- Runs with foreign keys off (Migration::rebuild): tasks (the waiting reason)
-- and tickets (login_user NOT NULL) are rebuilt. Every row is copied; nothing
-- is deleted. A run's waiting reason is its error_code, which has no CHECK, so
-- runs only gain a column.

-- The sessions of the board members, like remote_sessions (0001). `client_id`
-- NULL: the local IPC (the desktop on the server itself). It expires 90 days
-- after it was made (`expires_at_ms`) or 30 days after it was last used.
CREATE TABLE user_sessions (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id),
  client_id TEXT REFERENCES remote_clients(id),
  device_name TEXT NOT NULL,
  -- SHA-256 of a random 32-byte token; the token itself is only with the client.
  token_verifier TEXT NOT NULL UNIQUE,
  created_at_ms INTEGER NOT NULL,
  last_seen_ms INTEGER NOT NULL,
  expires_at_ms INTEGER NOT NULL,
  revoked_at_ms INTEGER
) STRICT;
CREATE INDEX user_sessions_user ON user_sessions(user_id, revoked_at_ms);
CREATE INDEX user_sessions_client ON user_sessions(client_id) WHERE client_id IS NOT NULL;

-- The board member who last used the device, for "my devices".
ALTER TABLE remote_clients ADD COLUMN user_id TEXT REFERENCES users(id);

-- A user is never deleted; it can be disabled. `must_change_password` (0/1):
-- the password was reset to a temporary one, to be changed at the next login.
ALTER TABLE users ADD COLUMN disabled_at_ms INTEGER;
ALTER TABLE users ADD COLUMN password_changed_at_ms INTEGER;
ALTER TABLE users ADD COLUMN must_change_password INTEGER NOT NULL DEFAULT 0 CHECK(must_change_password IN (0,1));

-- The Claude login of each board member: its status and what `claude auth
-- status` says about it. No token and no path.
CREATE TABLE claude_logins (
  user_id TEXT PRIMARY KEY REFERENCES users(id),
  status TEXT NOT NULL CHECK(status IN ('linked','missing','expired')),
  email TEXT,
  plan TEXT,
  linked_at_ms INTEGER,
  checked_at_ms INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- Whose Claude login a run uses (NULL: from before phase 3, or not Claude).
ALTER TABLE runs ADD COLUMN login_user TEXT REFERENCES users(id);
CREATE INDEX runs_login_user ON runs(login_user) WHERE login_user IS NOT NULL;

-- Whose login the tasks of an app key, an API key (agent functions), a recurring
-- task or a heartbeat run on (NULL: none chosen; such a task waits with 'login').
ALTER TABLE app_keys ADD COLUMN login_user TEXT REFERENCES users(id);
ALTER TABLE api_keys ADD COLUMN login_user TEXT REFERENCES users(id);
ALTER TABLE recurring_tasks ADD COLUMN login_user TEXT REFERENCES users(id);
ALTER TABLE heartbeats ADD COLUMN login_user TEXT REFERENCES users(id);

-- The "company login": the login a board member chose for the background jobs
-- that have no source (memory consolidation). NULL: they do not run.
ALTER TABLE company ADD COLUMN background_login_user TEXT REFERENCES users(id);

-- A notification for one board member only (NULL: for everyone), e.g. "connect
-- your Claude login" for the creator of a ticket that waits for it.
ALTER TABLE notifications ADD COLUMN user_id TEXT REFERENCES users(id);

-- Tasks: the waiting reason 'login'. Every row is copied.
CREATE TABLE tasks_new (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id),
  employee_id TEXT REFERENCES "employees"(id), parent_task_id TEXT REFERENCES tasks(id),
  title TEXT NOT NULL, brief_blob_id TEXT NOT NULL REFERENCES blobs(id),
  acceptance_blob_id TEXT REFERENCES blobs(id),
  status TEXT NOT NULL CHECK(status IN ('draft','queued','running','waiting','review','succeeded','failed','cancelled')),
  waiting_reason TEXT CHECK(waiting_reason IN ('approval','input','dependency','quota','offline','paused','reconcile','plan_review','login')),
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
  -- Whose Claude login a task without a ticket runs on: the board member who
  -- made it, the run that delegated it, its app key's or schedule's (a task of a
  -- ticket follows the ticket's login_user).
  login_user TEXT REFERENCES users(id),
  CHECK((status='waiting' AND waiting_reason IS NOT NULL) OR (status<>'waiting' AND waiting_reason IS NULL))
) STRICT;
INSERT INTO tasks_new(id, project_id, employee_id, parent_task_id, title, brief_blob_id, acceptance_blob_id, status, waiting_reason,
                      priority, version, created_at_ms, updated_at_ms, result_blob_id, conversation_id, run_json,
                      origin_conversation_id, requested_by, attachments_json, schedule_id, created_by_app_key_id, ticket_id)
  SELECT id, project_id, employee_id, parent_task_id, title, brief_blob_id, acceptance_blob_id, status, waiting_reason,
         priority, version, created_at_ms, updated_at_ms, result_blob_id, conversation_id, run_json,
         origin_conversation_id, requested_by, attachments_json, schedule_id, created_by_app_key_id, ticket_id
  FROM tasks;
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;
CREATE INDEX tasks_project_status ON tasks(project_id,status,priority);
CREATE INDEX tasks_parent ON tasks(parent_task_id);
CREATE INDEX tasks_schedule ON tasks(schedule_id) WHERE schedule_id IS NOT NULL;
CREATE INDEX tasks_app_key ON tasks(created_by_app_key_id, created_at_ms) WHERE created_by_app_key_id IS NOT NULL;
CREATE INDEX tasks_employee ON tasks(employee_id, status);
CREATE INDEX tasks_ticket ON tasks(ticket_id) WHERE ticket_id IS NOT NULL;

-- Tickets: `login_user`, whose Claude login its runs use. It starts as the
-- creator and changes only to a board member who asks for it themselves ("Run on
-- my login"); `created_by_user` never changes. Each ticket gets its creator here.
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
  -- Whose Claude login the ticket's runs use (at first its creator).
  login_user TEXT NOT NULL REFERENCES users(id),
  UNIQUE(company_id, number)
) STRICT;
INSERT INTO tickets_new(id, company_id, project_id, number, title, description_blob_id, column_id, position, priority,
                        created_by_user, owner_employee_id, run_json, labels_json, attachments_json, deadline_ms,
                        root_task_id, review_feedback, version, created_at_ms, updated_at_ms, started_at_ms, done_at_ms,
                        archived_at_ms, conversation_id, login_user)
  SELECT id, company_id, project_id, number, title, description_blob_id, column_id, position, priority,
         created_by_user, owner_employee_id, run_json, labels_json, attachments_json, deadline_ms,
         root_task_id, review_feedback, version, created_at_ms, updated_at_ms, started_at_ms, done_at_ms,
         archived_at_ms, conversation_id, created_by_user
  FROM tickets;
DROP TABLE tickets;
ALTER TABLE tickets_new RENAME TO tickets;
CREATE INDEX tickets_column ON tickets(column_id, position);
CREATE INDEX tickets_project ON tickets(project_id, archived_at_ms);
CREATE INDEX tickets_root_task ON tickets(root_task_id) WHERE root_task_id IS NOT NULL;
CREATE INDEX tickets_login_user ON tickets(login_user);

-- The Claude Code session a conversation goes on in, per board member's login:
-- its transcript is in that login's folder, so a conversation that ran on Alex's
-- login never resumes on Robin's; Robin's run starts a session of its own (R4).
-- '' is a session from before phase 3 (in the daemon user's ~/.claude, never
-- resumed again). Every row is copied.
CREATE TABLE conversation_sessions_new (
  conversation_id TEXT NOT NULL REFERENCES conversations(id),
  login_user TEXT NOT NULL DEFAULT '',
  session_id TEXT NOT NULL,
  busy_run_id TEXT,
  seen_count INTEGER NOT NULL DEFAULT 0,
  seen_digest TEXT NOT NULL DEFAULT '',
  answer_id TEXT,
  model TEXT NOT NULL DEFAULT '',
  folder TEXT NOT NULL DEFAULT '',
  profile TEXT NOT NULL DEFAULT '',
  context_json TEXT NOT NULL DEFAULT '{}',
  updated_at_ms INTEGER NOT NULL,
  system_text TEXT NOT NULL DEFAULT '',
  system_at_ms INTEGER NOT NULL DEFAULT 0,
  context_tokens INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (conversation_id, login_user)
) STRICT;
INSERT INTO conversation_sessions_new(conversation_id, login_user, session_id, busy_run_id, seen_count, seen_digest, answer_id, model, folder,
                                      profile, context_json, updated_at_ms, system_text, system_at_ms, context_tokens)
  SELECT conversation_id, '', session_id, busy_run_id, seen_count, seen_digest, answer_id, model, folder,
         profile, context_json, updated_at_ms, system_text, system_at_ms, context_tokens
  FROM conversation_sessions;
DROP TABLE conversation_sessions;
ALTER TABLE conversation_sessions_new RENAME TO conversation_sessions;
