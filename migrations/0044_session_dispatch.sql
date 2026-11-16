-- v0.28 (protocol 5, feature `session_dispatch`): the dispatcher chooses who takes a
-- work session, as it chooses the owner of a ticket.
--
-- * work_sessions.dispatch: 'choosing' while the dispatcher's job runs, 'nobody' when it
--   chose nobody (a board member chooses then); NULL: an employee works in it. Until
--   one is chosen, the conversation's employee is the dispatcher.
-- * session_dispatch_runs: the dispatcher's runs for a session. They hang on the
--   session's conversation (a run needs a task or a conversation) but answer no
--   message, so the chat runs' restart cleanup leaves them to the dispatcher's job.
ALTER TABLE work_sessions ADD COLUMN dispatch TEXT CHECK(dispatch IN ('choosing','nobody'));

CREATE TABLE session_dispatch_runs (
  run_id TEXT PRIMARY KEY REFERENCES runs(id),
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX session_dispatch_runs_conversation ON session_dispatch_runs(conversation_id);
