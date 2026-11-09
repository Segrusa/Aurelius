-- v0.24 (protocol 5, R8): the activity log.
--
-- One row per delivery, queue step and run step of an employee, findable per thread,
-- employee and run. Deliberately without foreign keys: a row outlives what it is about,
-- and writing one never depends on another table. Rows go after 90 days.
--
-- * category: delivery, queue or run (text, so a later value needs no rebuild).
-- * step: what happened, per category; see `aurelius_storage::activity_log`.
-- * detail: a short code or a tool's name. Never the text of a message or a tool's input.
-- * item_id: the mailbox item a delivery or queue step is about.
-- * private_to: the board member whose private chat this is; others never read the row.
--
-- Additive only: one new table.
CREATE TABLE activity_log (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  at_ms INTEGER NOT NULL,
  category TEXT NOT NULL,
  step TEXT NOT NULL,
  detail TEXT,
  employee_id TEXT,
  thread_key TEXT,
  item_id TEXT,
  run_id TEXT,
  hop INTEGER,
  private_to TEXT
) STRICT;
CREATE INDEX activity_log_employee ON activity_log(employee_id, seq);
CREATE INDEX activity_log_thread ON activity_log(thread_key, seq);
CREATE INDEX activity_log_run ON activity_log(run_id, seq);
CREATE INDEX activity_log_item ON activity_log(item_id) WHERE item_id IS NOT NULL;
