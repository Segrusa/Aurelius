-- Protocol 5 (additive, feature `session_history`): work sessions in the History.
--
-- A session has no Done: it shows in the History on the day of its last activity,
-- with a short AI summary of what was done in it. The summary is made by the job
-- `session.summarize` once the session has been quiet for a while, and again after
-- new activity; `work_session.retry_summary` asks for one at once.
--
-- * work_sessions.summary_state: NULL: no summary was ever asked for; 'pending':
--   one is asked for; 'ready': summary_text holds it; 'failed': the last try failed
--   and there is no text yet (a failed re-summary keeps the older text, 'ready').
-- * work_sessions.summary_text: the summary, through the vault redactor, flattened
--   and cut like a ticket's (`history::history_summary`).
-- * work_sessions.summary_through_ms: the time of the newest message it covers, so
--   a next summary only reads what came after.
--
-- Additive only: new columns with CHECKs, no rebuild, no data touched.
ALTER TABLE work_sessions ADD COLUMN summary_state TEXT CHECK(summary_state IS NULL OR summary_state IN ('pending','ready','failed'));
ALTER TABLE work_sessions ADD COLUMN summary_text TEXT CHECK(summary_text IS NULL OR (summary_state = 'ready' AND length(summary_text) BETWEEN 1 AND 800));
ALTER TABLE work_sessions ADD COLUMN summary_through_ms INTEGER CHECK(summary_through_ms IS NULL OR summary_through_ms >= 0);
