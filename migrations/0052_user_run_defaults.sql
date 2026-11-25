-- Protocol 5 (additive, feature `user_run_defaults`): each board member's own
-- default model, effort, mode and subagent model for the runs on their login.
--
-- * user_run_defaults: one row per board member who set them (user.set_run_defaults).
--   A column left NULL is not chosen: the connection's default model, the model's
--   own effort, Ask. The employee's and the company's model and effort in
--   config_overrides are no longer read for runs, but stay as they are.
--
-- Additive only: nothing is rebuilt, changed or deleted.
CREATE TABLE user_run_defaults (
  user_id TEXT PRIMARY KEY REFERENCES users(id),
  model TEXT CHECK(model IS NULL OR length(model) BETWEEN 1 AND 200),
  effort TEXT CHECK(effort IS NULL OR effort IN ('low','medium','high','xhigh','max')),
  mode TEXT CHECK(mode IS NULL OR mode IN ('plan','ask','accept_edits','auto')),
  subagent_model TEXT CHECK(subagent_model IS NULL OR length(subagent_model) BETWEEN 1 AND 200),
  source_event_id TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
