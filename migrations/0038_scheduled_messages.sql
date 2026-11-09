-- v0.23 (protocol 5, R7 and C7): messages an employee schedules, and its scratchpad.
--
-- * scheduled_messages: what `schedule_message` planned. `state`: scheduled, delivered,
--   cancelled, not_delivered (text, so a later value needs no rebuild). It stays after
--   delivery: the Scheduled page shows the outcome. `login_user` is the login of the
--   run that planned it (assumption 5), NULL when that run had none: such a message
--   waits for a login like any run, it never borrows someone else's. `discussion_id`
--   is a ticket's discussion or a channel; `chat_id` the employee's own chat otherwise.
-- * employee_scratchpads: one text per employee, in its prompt at every run.
--
-- Additive only: two new tables.
CREATE TABLE scheduled_messages (
  id TEXT PRIMARY KEY,
  from_employee_id TEXT NOT NULL REFERENCES employees(id),
  to_employee_id TEXT NOT NULL REFERENCES employees(id),
  content TEXT NOT NULL,
  discussion_id TEXT REFERENCES conversations(id) ON DELETE CASCADE,
  chat_id TEXT REFERENCES conversations(id) ON DELETE CASCADE,
  thread_key TEXT NOT NULL,
  ticket_id TEXT,
  thread_parent TEXT,
  place TEXT NOT NULL,
  hop INTEGER NOT NULL DEFAULT 0,
  login_user TEXT,
  planned_by_run TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'scheduled',
  due_at_ms INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  settled_at_ms INTEGER,
  outcome TEXT,
  run_id TEXT
) STRICT;
CREATE INDEX scheduled_messages_due ON scheduled_messages(state, due_at_ms);
CREATE INDEX scheduled_messages_from ON scheduled_messages(from_employee_id, state);
CREATE INDEX scheduled_messages_thread ON scheduled_messages(thread_key, state);

CREATE TABLE employee_scratchpads (
  employee_id TEXT PRIMARY KEY REFERENCES employees(id),
  text TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  updated_by_run TEXT
) STRICT;
