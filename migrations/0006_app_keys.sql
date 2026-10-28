-- App keys (feature `app_keys`): outside apps and agents that use Aurelius
-- through aurelius-mcp. Only the SHA-256 of the secret is kept.
CREATE TABLE app_keys (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  key_verifier TEXT NOT NULL UNIQUE,
  scopes_json TEXT NOT NULL CHECK(json_valid(scopes_json)),
  limit_per_hour INTEGER,
  limit_concurrent INTEGER,
  created_at_ms INTEGER NOT NULL,
  last_used_ms INTEGER,
  revoked_at_ms INTEGER
) STRICT;
-- The app whose key created a task.
ALTER TABLE tasks ADD COLUMN created_by_app_key_id TEXT REFERENCES app_keys(id);
CREATE INDEX tasks_app_key ON tasks(created_by_app_key_id, created_at_ms) WHERE created_by_app_key_id IS NOT NULL;
