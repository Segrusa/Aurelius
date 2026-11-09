-- v0.22 (protocol 5, R1-R3 and R6): the mailbox of an employee. Everything that
-- wakes an employee is an item here; the worker of the employee takes the oldest
-- one, merges what waits in the same thread and only then makes the run and its
-- job (the job is the run's execution record: lease, epoch, recovery).
--
-- * employee_inbox: the items. `state`: queued, claimed (a run took it), done,
--   cancelled. `kind`: message, task, resume, heartbeat. `source`: board, employee,
--   system. Text columns, so a later value needs no rebuild. `run_id` is the id
--   the run gets; items merged into a run point at that run afterwards.
-- * employee_threads: per employee and thread (`ticket:<id>`, `channel:<conversation>`,
--   `chat:<conversation>`) its own conversation, and with it its Claude Code session
--   (R6), and whether the thread is parked on a question (R3).
-- * run_triggers: which items a run answers.
-- * conversations.thread_key: marks a thread's own hidden conversation. Every
--   conversation so far is NULL.
--
-- Additive only: new tables and one nullable column. Runs and jobs already queued
-- finish as they are.
CREATE TABLE employee_inbox (
  id TEXT PRIMARY KEY,
  employee_id TEXT NOT NULL REFERENCES employees(id),
  thread_key TEXT NOT NULL,
  ticket_id TEXT REFERENCES tickets(id),
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'queued',
  source TEXT NOT NULL,
  hop INTEGER NOT NULL DEFAULT 0,
  payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
  login_user TEXT,
  task_id TEXT,
  source_event_id TEXT NOT NULL REFERENCES events(id),
  priority INTEGER NOT NULL DEFAULT 0,
  not_before_ms INTEGER NOT NULL,
  last_error_code TEXT,
  run_id TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX employee_inbox_due ON employee_inbox(state, not_before_ms);
CREATE INDEX employee_inbox_conversation ON employee_inbox(conversation_id, state);
CREATE INDEX employee_inbox_run ON employee_inbox(run_id);

CREATE TABLE employee_threads (
  employee_id TEXT NOT NULL REFERENCES employees(id),
  thread_key TEXT NOT NULL,
  conversation_id TEXT REFERENCES conversations(id) ON DELETE SET NULL,
  state TEXT NOT NULL DEFAULT 'open',
  parked_task_id TEXT,
  parked_run_id TEXT,
  parked_at_ms INTEGER,
  created_at_ms INTEGER NOT NULL,
  PRIMARY KEY (employee_id, thread_key)
) STRICT;

CREATE TABLE run_triggers (
  run_id TEXT NOT NULL,
  item_id TEXT NOT NULL REFERENCES employee_inbox(id) ON DELETE CASCADE,
  created_at_ms INTEGER NOT NULL,
  PRIMARY KEY (run_id, item_id)
) STRICT;
CREATE INDEX run_triggers_item ON run_triggers(item_id);

ALTER TABLE conversations ADD COLUMN thread_key TEXT;
