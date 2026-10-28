-- The runtime session (Claude Code's own transcript) a conversation goes on in
-- with `--resume`, instead of sending its whole history again each run.
-- `busy_run_id`: the run going on in it (or waiting for a usage limit).
-- `seen_count`/`seen_digest`: the messages the session has (in order) up to its
-- last answer `answer_id`; empty digest = not usable yet (its first run goes on).
-- `model`, `folder`, `profile`: what it was started with; any change starts a new one.
-- `context_json`: mode, task status, memory and MCP list it last knew, for the change note.
CREATE TABLE conversation_sessions (
  conversation_id TEXT PRIMARY KEY REFERENCES conversations(id),
  session_id TEXT NOT NULL,
  busy_run_id TEXT,
  seen_count INTEGER NOT NULL DEFAULT 0,
  seen_digest TEXT NOT NULL DEFAULT '',
  answer_id TEXT,
  model TEXT NOT NULL DEFAULT '',
  folder TEXT NOT NULL DEFAULT '',
  profile TEXT NOT NULL DEFAULT '',
  context_json TEXT NOT NULL DEFAULT '{}',
  updated_at_ms INTEGER NOT NULL
) STRICT;
