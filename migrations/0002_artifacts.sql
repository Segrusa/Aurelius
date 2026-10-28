-- Claude Code artifacts (claude.ai pages) that runs published, one row per URL.
CREATE TABLE artifacts (
  url TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  conversation_id TEXT NOT NULL REFERENCES conversations(id),
  task_id TEXT REFERENCES tasks(id),
  project_id TEXT REFERENCES projects(id),
  -- Claude Code's version number of the latest publish (its `seq`).
  version INTEGER NOT NULL,
  first_published_at_ms INTEGER NOT NULL,
  last_published_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX artifacts_conversation ON artifacts(conversation_id, last_published_at_ms);
CREATE INDEX artifacts_task ON artifacts(task_id, last_published_at_ms);
CREATE INDEX artifacts_project ON artifacts(project_id, last_published_at_ms);
