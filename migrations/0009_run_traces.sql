-- Tokens written to the prompt cache, apart: they were (and stay) counted in
-- `input_tokens`, which is input not read from the cache.
ALTER TABLE usage_records ADD COLUMN cache_creation_tokens INTEGER NOT NULL DEFAULT 0;
CREATE INDEX usage_records_run ON usage_records(run_id);

-- Trace mode (off by default): what each run sent and what each of its model
-- calls cost. Kept for the latest runs only (see `storage::traces`).
CREATE TABLE run_traces (
  run_id TEXT PRIMARY KEY,
  conversation_id TEXT,
  created_at_ms INTEGER NOT NULL,
  trace_json TEXT NOT NULL CHECK(json_valid(trace_json))
) STRICT;
CREATE INDEX run_traces_created ON run_traces(created_at_ms);
CREATE INDEX run_traces_conversation ON run_traces(conversation_id);
