-- Aurelius v0.7.0: the one schema. There is no migration path from v0.6; the
-- one-time `aurelius-import` tool copies what matters from a v0.6 database.
-- Connection PRAGMAs (foreign_keys, WAL, synchronous, busy_timeout) are applied by
-- storage::open_connection, because journal_mode cannot change inside a migration transaction.
-- The events table is the audit log (append-only); projections are ordinary tables.
-- Operational tables (jobs, receipts, remote sessions) are not replayed.

CREATE TABLE schema_migrations (
  version INTEGER PRIMARY KEY, checksum TEXT NOT NULL, applied_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE blobs (
  id TEXT PRIMARY KEY, storage_key TEXT NOT NULL UNIQUE,
  sha256 TEXT NOT NULL, byte_length INTEGER NOT NULL CHECK(byte_length >= 0),
  media_type TEXT NOT NULL, encryption_key_ref TEXT,
  created_at_ms INTEGER NOT NULL, deleted_at_ms INTEGER
) STRICT;

CREATE TABLE events (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  id TEXT NOT NULL UNIQUE,
  aggregate_type TEXT NOT NULL, aggregate_id TEXT NOT NULL,
  aggregate_version INTEGER NOT NULL CHECK(aggregate_version > 0),
  type TEXT NOT NULL, schema_version INTEGER NOT NULL CHECK(schema_version > 0),
  project_id TEXT, manager_id TEXT, task_id TEXT, run_id TEXT,
  actor_kind TEXT NOT NULL CHECK(actor_kind IN ('user','manager','system','runtime')),
  actor_id TEXT, correlation_id TEXT NOT NULL,
  causation_id TEXT REFERENCES events(id),
  payload_blob_id TEXT REFERENCES blobs(id),
  metadata_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(metadata_json)),
  occurred_at_ms INTEGER NOT NULL, recorded_at_ms INTEGER NOT NULL,
  dedupe_key TEXT NOT NULL UNIQUE,
  UNIQUE(aggregate_type, aggregate_id, aggregate_version)
) STRICT;
CREATE INDEX events_project_seq ON events(project_id, seq);
CREATE INDEX events_correlation ON events(correlation_id, seq);
CREATE INDEX events_causation ON events(causation_id);
CREATE INDEX events_task_seq ON events(task_id, seq);
CREATE TRIGGER events_no_update BEFORE UPDATE ON events BEGIN
  SELECT RAISE(ABORT, 'events are append-only');
END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events BEGIN
  SELECT RAISE(ABORT, 'events are append-only');
END;

-- One receipt per write (idempotency key). `method` is the protocol method that was called.
CREATE TABLE command_receipts (
  command_id TEXT PRIMARY KEY, request_hash TEXT NOT NULL,
  method TEXT NOT NULL,
  result_json TEXT NOT NULL CHECK(json_valid(result_json)),
  committed_seq INTEGER NOT NULL REFERENCES events(seq), created_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE scopes (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('global','project','manager')),
  parent_id TEXT REFERENCES scopes(id), owner_id TEXT,
  CHECK((kind='global' AND parent_id IS NULL AND owner_id IS NULL)
     OR (kind IN ('project','manager') AND parent_id IS NOT NULL AND owner_id IS NOT NULL)),
  UNIQUE(kind, owner_id)
) STRICT;
CREATE UNIQUE INDEX one_global_scope ON scopes(kind) WHERE kind='global';

CREATE TABLE projects (
  id TEXT PRIMARY KEY, scope_id TEXT NOT NULL UNIQUE REFERENCES scopes(id),
  name TEXT NOT NULL, description_blob_id TEXT REFERENCES blobs(id),
  status TEXT NOT NULL CHECK(status IN ('active','paused','archived')),
  version INTEGER NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL,
  folder TEXT,
  -- "Follow the org chart": delegation, hand-overs and escalation follow who reports to whom.
  follow_org_chart INTEGER NOT NULL DEFAULT 0 CHECK(follow_org_chart IN (0,1))
) STRICT;

CREATE TABLE managers (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id),
  scope_id TEXT NOT NULL UNIQUE REFERENCES scopes(id), name TEXT NOT NULL, role TEXT NOT NULL,
  identity_blob_id TEXT REFERENCES blobs(id), instructions_blob_id TEXT REFERENCES blobs(id),
  config_json TEXT NOT NULL CHECK(json_valid(config_json)),
  status TEXT NOT NULL CHECK(status IN ('active','paused','archived')),
  version INTEGER NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL,
  folder TEXT,
  -- The org chart: a position title (at most 80 characters) and who this manager reports to.
  title TEXT,
  reports_to TEXT REFERENCES managers(id),
  CHECK(reports_to IS NULL OR reports_to <> id)
) STRICT;
CREATE INDEX managers_project ON managers(project_id);
CREATE INDEX managers_reports_to ON managers(reports_to);

-- A colleague a manager proposed; the user decides (always, also in Auto).
CREATE TABLE manager_proposals (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  proposed_by TEXT NOT NULL REFERENCES managers(id),
  status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected','withdrawn')),
  name TEXT NOT NULL, role TEXT NOT NULL, title TEXT,
  instructions_blob_id TEXT NOT NULL REFERENCES blobs(id),
  reports_to TEXT REFERENCES managers(id),
  reason TEXT NOT NULL,
  decision_note TEXT,
  created_manager_id TEXT REFERENCES managers(id),
  origin_conversation_id TEXT REFERENCES conversations(id),
  created_at_ms INTEGER NOT NULL, decided_at_ms INTEGER
) STRICT;
CREATE INDEX manager_proposals_project ON manager_proposals(project_id, status, created_at_ms);
CREATE INDEX manager_proposals_proposer ON manager_proposals(proposed_by, status);

-- Claude Code plugins a manager has enabled (`name@marketplace`).
CREATE TABLE manager_plugins (
  manager_id TEXT NOT NULL REFERENCES managers(id),
  plugin_id TEXT NOT NULL,
  enabled_at_ms INTEGER NOT NULL,
  PRIMARY KEY(manager_id, plugin_id)
) STRICT;

CREATE TABLE connections (
  id TEXT PRIMARY KEY, provider_kind TEXT NOT NULL, auth_kind TEXT NOT NULL,
  display_name TEXT NOT NULL, account_fingerprint TEXT,
  secret_ref TEXT, runtime_path TEXT,
  status TEXT NOT NULL CHECK(status IN ('disconnected','ready','needs_auth','unavailable')),
  config_json TEXT NOT NULL CHECK(json_valid(config_json)), updated_at_ms INTEGER NOT NULL
) STRICT;
CREATE TABLE config_overrides (
  scope_id TEXT NOT NULL REFERENCES scopes(id), purpose TEXT NOT NULL,
  version INTEGER NOT NULL, config_json TEXT NOT NULL CHECK(json_valid(config_json)),
  source_event_id TEXT NOT NULL REFERENCES events(id), PRIMARY KEY(scope_id,purpose)
) STRICT;
CREATE TABLE file_grants (
  id TEXT PRIMARY KEY, scope_id TEXT NOT NULL REFERENCES scopes(id),
  canonical_path TEXT NOT NULL, platform_identity_json TEXT NOT NULL CHECK(json_valid(platform_identity_json)),
  access TEXT NOT NULL CHECK(access IN ('read','stage_write','write')),
  recursive INTEGER NOT NULL CHECK(recursive IN (0,1)), expires_at_ms INTEGER, revoked_at_ms INTEGER,
  source_event_id TEXT NOT NULL REFERENCES events(id)
) STRICT;

CREATE TABLE conversations (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('chat','project','task')),
  project_id TEXT REFERENCES projects(id), manager_id TEXT REFERENCES managers(id),
  title TEXT NOT NULL, version INTEGER NOT NULL, created_at_ms INTEGER NOT NULL,
  archived_at_ms INTEGER,
  web_research INTEGER NOT NULL DEFAULT 0,
  agent_mode TEXT NOT NULL DEFAULT 'ask' CHECK(agent_mode IN ('plan','ask','accept_edits','auto')),
  browser_allowed INTEGER NOT NULL DEFAULT 0 CHECK(browser_allowed IN (0,1)),
  screen_allowed INTEGER NOT NULL DEFAULT 0 CHECK(screen_allowed IN (0,1)),
  run_json TEXT,
  CHECK((kind='chat' AND project_id IS NULL AND manager_id IS NULL) OR
        (kind='project' AND project_id IS NOT NULL) OR
        (kind='task' AND project_id IS NOT NULL AND manager_id IS NOT NULL))
) STRICT;
CREATE TABLE messages (
  id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id),
  ordinal INTEGER NOT NULL, role TEXT NOT NULL CHECK(role IN ('user','assistant','tool')),
  content_blob_id TEXT REFERENCES blobs(id), source_event_id TEXT NOT NULL REFERENCES events(id),
  status TEXT NOT NULL CHECK(status IN ('partial','complete','interrupted','failed','redacted')),
  input_kind TEXT NOT NULL CHECK(input_kind IN ('text','tool','share')),
  created_at_ms INTEGER NOT NULL,
  run_id TEXT REFERENCES runs(id), reply_to_id TEXT REFERENCES messages(id),
  meta_json TEXT CHECK(meta_json IS NULL OR json_valid(meta_json)),
  UNIQUE(conversation_id,ordinal)
) STRICT;
CREATE INDEX messages_conversation_ordinal ON messages(conversation_id, ordinal);

CREATE TABLE tasks (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id),
  manager_id TEXT REFERENCES managers(id), parent_task_id TEXT REFERENCES tasks(id),
  title TEXT NOT NULL, brief_blob_id TEXT NOT NULL REFERENCES blobs(id),
  acceptance_blob_id TEXT REFERENCES blobs(id),
  status TEXT NOT NULL CHECK(status IN ('draft','queued','running','waiting','review','succeeded','failed','cancelled')),
  waiting_reason TEXT CHECK(waiting_reason IN ('approval','input','dependency','quota','offline','paused','reconcile')),
  priority INTEGER NOT NULL DEFAULT 0, version INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL,
  result_blob_id TEXT REFERENCES blobs(id),
  conversation_id TEXT REFERENCES conversations(id),
  run_json TEXT,
  origin_conversation_id TEXT REFERENCES conversations(id),
  requested_by TEXT REFERENCES managers(id),
  attachments_json TEXT,
  -- The recurring task this one was made from.
  schedule_id TEXT,
  CHECK((status='waiting' AND waiting_reason IS NOT NULL) OR (status<>'waiting' AND waiting_reason IS NULL))
) STRICT;
CREATE INDEX tasks_project_status ON tasks(project_id,status,priority);
CREATE INDEX tasks_parent ON tasks(parent_task_id);
CREATE INDEX tasks_schedule ON tasks(schedule_id) WHERE schedule_id IS NOT NULL;
CREATE TABLE task_dependencies (
  task_id TEXT NOT NULL REFERENCES tasks(id), requires_task_id TEXT NOT NULL REFERENCES tasks(id),
  PRIMARY KEY(task_id,requires_task_id), CHECK(task_id <> requires_task_id)
) STRICT;

CREATE TABLE runs (
  id TEXT PRIMARY KEY, task_id TEXT REFERENCES tasks(id),
  manager_id TEXT REFERENCES managers(id), conversation_id TEXT REFERENCES conversations(id),
  attempt INTEGER NOT NULL CHECK(attempt > 0), runtime_kind TEXT NOT NULL,
  connection_id TEXT REFERENCES connections(id), model_id TEXT NOT NULL,
  resolved_config_json TEXT NOT NULL CHECK(json_valid(resolved_config_json)),
  context_blob_id TEXT REFERENCES blobs(id),
  status TEXT NOT NULL CHECK(status IN ('created','starting','running','waiting','cancelling','succeeded','failed','cancelled','unknown')),
  external_session_id TEXT, external_turn_id TEXT, runtime_version TEXT,
  last_provider_cursor TEXT, lease_epoch INTEGER NOT NULL DEFAULT 0,
  started_at_ms INTEGER, ended_at_ms INTEGER, error_code TEXT,
  CHECK(task_id IS NOT NULL OR conversation_id IS NOT NULL)
) STRICT;
CREATE INDEX runs_status ON runs(status);
CREATE INDEX runs_conversation ON runs(conversation_id);
CREATE INDEX runs_task ON runs(task_id);

CREATE TABLE jobs (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, subject_id TEXT NOT NULL,
  source_event_id TEXT NOT NULL REFERENCES events(id), dedupe_key TEXT NOT NULL UNIQUE,
  payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
  state TEXT NOT NULL CHECK(state IN ('ready','leased','done','dead','cancelled')),
  not_before_ms INTEGER NOT NULL, priority INTEGER NOT NULL DEFAULT 0,
  attempts INTEGER NOT NULL DEFAULT 0, max_attempts INTEGER NOT NULL DEFAULT 3,
  lease_owner TEXT, lease_until_ms INTEGER, lease_epoch INTEGER NOT NULL DEFAULT 0,
  last_error_code TEXT
) STRICT;
CREATE INDEX jobs_due ON jobs(state,not_before_ms,priority);

-- A manager's heartbeat (a periodic wake-up with quiet hours and a token budget).
CREATE TABLE heartbeats (
  id TEXT PRIMARY KEY, manager_id TEXT NOT NULL REFERENCES managers(id),
  rule_json TEXT NOT NULL CHECK(json_valid(rule_json)), timezone TEXT NOT NULL,
  next_due_ms INTEGER, last_occurrence_key TEXT, enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
  version INTEGER NOT NULL,
  cursor_seq INTEGER NOT NULL DEFAULT 0, last_evaluated_ms INTEGER, waiting_reason TEXT
) STRICT;
CREATE UNIQUE INDEX heartbeats_manager ON heartbeats(manager_id);
CREATE INDEX heartbeats_due ON heartbeats(enabled, next_due_ms);
CREATE TABLE heartbeat_wakes (
  run_id TEXT PRIMARY KEY REFERENCES runs(id),
  schedule_id TEXT NOT NULL REFERENCES heartbeats(id),
  occurrence_key TEXT NOT NULL UNIQUE,
  reasons_json TEXT NOT NULL CHECK(json_valid(reasons_json)),
  missed INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  outcome TEXT CHECK(outcome IS NULL OR outcome IN ('reported','silent','failed'))
) STRICT;
CREATE INDEX heartbeat_wakes_schedule ON heartbeat_wakes(schedule_id, created_at_ms);

-- Recurring tasks: a template that makes a normal task on a recurrence.
CREATE TABLE recurring_tasks (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  manager_id TEXT NOT NULL REFERENCES managers(id),
  title TEXT NOT NULL,
  goal TEXT NOT NULL,
  criteria_json TEXT NOT NULL DEFAULT '[]',
  attachments_json TEXT,
  run_json TEXT,
  recurrence_json TEXT NOT NULL,
  paused INTEGER NOT NULL DEFAULT 0,
  anchor_ms INTEGER NOT NULL,
  next_run_ms INTEGER,
  last_run_ms INTEGER,
  last_task_id TEXT,
  last_skipped_ms INTEGER,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX recurring_tasks_due ON recurring_tasks(next_run_ms) WHERE paused = 0;

-- Questions and permission requests from a run, decided by the user.
CREATE TABLE approvals (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(id),
  call_id TEXT NOT NULL,
  tool TEXT NOT NULL,
  effect TEXT NOT NULL CHECK(effect IN ('read','local_write','external_write','execute')),
  -- The exact action as shown to the user (tool, arguments, reasons), as JSON.
  action_blob_id TEXT NOT NULL REFERENCES blobs(id),
  action_digest TEXT NOT NULL,
  request_event_id TEXT NOT NULL REFERENCES events(id),
  status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected','expired','invalidated','consumed')),
  expires_at_ms INTEGER NOT NULL, decision_event_id TEXT REFERENCES events(id),
  decided_at_ms INTEGER, answer_json TEXT CHECK(answer_json IS NULL OR json_valid(answer_json)),
  UNIQUE(run_id, call_id)
) STRICT;
CREATE INDEX approvals_pending ON approvals(status,expires_at_ms);

CREATE TABLE memory_items (
  id TEXT PRIMARY KEY, scope_id TEXT NOT NULL REFERENCES scopes(id),
  kind TEXT NOT NULL CHECK(kind IN ('identity','project_knowledge','decision','episodic','conversation_summary')),
  content_blob_id TEXT NOT NULL REFERENCES blobs(id),
  status TEXT NOT NULL CHECK(status IN ('proposed','active','superseded','rejected','forgotten')),
  confidence REAL NOT NULL CHECK(confidence >= 0 AND confidence <= 1),
  importance REAL NOT NULL CHECK(importance >= 0 AND importance <= 1),
  pinned INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN (0,1)),
  valid_from_ms INTEGER NOT NULL, valid_until_ms INTEGER,
  supersedes_id TEXT REFERENCES memory_items(id),
  extraction_version TEXT NOT NULL, created_at_ms INTEGER NOT NULL,
  content_hash TEXT NOT NULL DEFAULT '',
  conflicts_with_id TEXT REFERENCES memory_items(id),
  decided_by TEXT,
  locator_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(locator_json))
) STRICT;
CREATE INDEX memory_scope_kind ON memory_items(scope_id,status,kind);
CREATE INDEX memory_hash ON memory_items(scope_id, content_hash);
CREATE TABLE memory_sources (
  memory_id TEXT NOT NULL REFERENCES memory_items(id), event_id TEXT NOT NULL REFERENCES events(id),
  locator_json TEXT NOT NULL CHECK(json_valid(locator_json)), PRIMARY KEY(memory_id,event_id)
) STRICT;
CREATE TABLE retrieval_chunks (
  id TEXT PRIMARY KEY, scope_id TEXT NOT NULL REFERENCES scopes(id),
  source_event_id TEXT NOT NULL REFERENCES events(id), memory_id TEXT REFERENCES memory_items(id),
  blob_id TEXT NOT NULL REFERENCES blobs(id), locator_json TEXT NOT NULL CHECK(json_valid(locator_json)),
  content_hash TEXT NOT NULL, token_count INTEGER NOT NULL CHECK(token_count >= 0),
  embedding_model TEXT, embedding BLOB, deleted_at_ms INTEGER
) STRICT;
CREATE INDEX chunks_scope ON retrieval_chunks(scope_id, deleted_at_ms);
CREATE INDEX chunks_memory ON retrieval_chunks(memory_id);
-- FTS data is sensitive derived content, and must participate in deletion/backup policy.
CREATE VIRTUAL TABLE chunk_fts USING fts5(chunk_id UNINDEXED, text, tokenize='unicode61');

CREATE TABLE shares (
  id TEXT PRIMARY KEY, source_event_id TEXT NOT NULL REFERENCES events(id),
  destination_scope_id TEXT REFERENCES scopes(id), destination_conversation_id TEXT REFERENCES conversations(id),
  snapshot_blob_id TEXT NOT NULL REFERENCES blobs(id),
  grant_event_id TEXT NOT NULL REFERENCES events(id), revoked_at_ms INTEGER,
  mode TEXT NOT NULL DEFAULT 'context' CHECK(mode IN ('context','memory')),
  title TEXT NOT NULL DEFAULT '', source_label TEXT NOT NULL DEFAULT '',
  memory_id TEXT REFERENCES memory_items(id), created_at_ms INTEGER NOT NULL DEFAULT 0,
  CHECK((destination_scope_id IS NOT NULL) <> (destination_conversation_id IS NOT NULL))
) STRICT;
CREATE INDEX shares_scope ON shares(destination_scope_id, revoked_at_ms);
CREATE INDEX shares_conversation ON shares(destination_conversation_id, revoked_at_ms);

CREATE TABLE notifications (
  id TEXT PRIMARY KEY, event_id TEXT NOT NULL REFERENCES events(id),
  dedupe_key TEXT NOT NULL UNIQUE, severity TEXT NOT NULL,
  target_json TEXT NOT NULL CHECK(json_valid(target_json)),
  created_at_ms INTEGER NOT NULL, delivered_at_ms INTEGER, read_at_ms INTEGER
) STRICT;
CREATE TABLE usage_records (
  id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(id),
  source_event_id TEXT NOT NULL REFERENCES events(id),
  input_tokens INTEGER, output_tokens INTEGER, cached_tokens INTEGER,
  cost_microunits INTEGER, currency TEXT, estimate INTEGER NOT NULL CHECK(estimate IN (0,1)),
  provider_record_key TEXT NOT NULL UNIQUE, recorded_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE attachments (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id),
  name TEXT NOT NULL,
  media_kind TEXT NOT NULL,
  original_blob_id TEXT NOT NULL REFERENCES blobs(id),
  text_blob_id TEXT NOT NULL REFERENCES blobs(id),
  byte_length INTEGER NOT NULL CHECK(byte_length >= 0),
  truncated INTEGER NOT NULL CHECK(truncated IN (0,1)),
  source_event_id TEXT NOT NULL REFERENCES events(id),
  created_at_ms INTEGER NOT NULL,
  removed_at_ms INTEGER
) STRICT;
CREATE INDEX attachments_conversation ON attachments(conversation_id);
CREATE TABLE message_sources (
  run_id TEXT NOT NULL REFERENCES runs(id),
  ordinal INTEGER NOT NULL,
  url TEXT NOT NULL,
  title TEXT,
  sha256 TEXT NOT NULL,
  text_blob_id TEXT REFERENCES blobs(id),
  error TEXT,
  fetched_at_ms INTEGER NOT NULL,
  PRIMARY KEY(run_id, ordinal)
) STRICT;
CREATE TABLE run_activity (
  run_id TEXT NOT NULL REFERENCES runs(id),
  call_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  tool TEXT NOT NULL,
  input_json TEXT NOT NULL CHECK(json_valid(input_json)),
  output TEXT,
  ok INTEGER CHECK(ok IS NULL OR ok IN (0,1)),
  text_offset INTEGER NOT NULL,
  started_at_ms INTEGER NOT NULL,
  ended_at_ms INTEGER, parent_call_id TEXT,
  PRIMARY KEY (run_id, call_id)
) STRICT;
CREATE INDEX run_activity_message ON run_activity(message_id);
CREATE INDEX run_activity_parent ON run_activity(run_id, parent_call_id);

CREATE TABLE remote_clients (
  id TEXT PRIMARY KEY,
  principal_id TEXT NOT NULL,
  name TEXT NOT NULL,
  platform TEXT NOT NULL CHECK(platform IN ('ios','android','windows','macos','linux')),
  credential_verifier TEXT NOT NULL,
  capabilities_json TEXT NOT NULL CHECK(json_valid(capabilities_json)),
  created_at_ms INTEGER NOT NULL,
  last_seen_ms INTEGER,
  revoked_at_ms INTEGER
) STRICT;
CREATE TABLE remote_sessions (
  id TEXT PRIMARY KEY,
  client_id TEXT NOT NULL REFERENCES remote_clients(id),
  token_verifier TEXT NOT NULL UNIQUE,
  created_at_ms INTEGER NOT NULL,
  expires_at_ms INTEGER NOT NULL,
  revoked_at_ms INTEGER
) STRICT;
CREATE INDEX remote_sessions_client ON remote_sessions(client_id);
CREATE TABLE push_registrations (
  client_id TEXT PRIMARY KEY REFERENCES remote_clients(id),
  platform TEXT NOT NULL CHECK(platform IN ('apns','fcm')),
  token TEXT NOT NULL,
  environment TEXT NOT NULL CHECK(environment IN ('sandbox','production')),
  updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE ideas (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  title TEXT NOT NULL,
  note TEXT,
  status TEXT NOT NULL CHECK(status IN ('open','done','dropped')),
  manager_id TEXT REFERENCES managers(id),
  task_id TEXT REFERENCES tasks(id),
  version INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  kind TEXT NOT NULL DEFAULT 'feature' CHECK(kind IN ('feature','bug'))
) STRICT;
CREATE INDEX ideas_project ON ideas(project_id, status, created_at_ms);

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE agent_functions (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  manager_id TEXT NOT NULL REFERENCES managers(id),
  name TEXT NOT NULL,
  slug TEXT NOT NULL UNIQUE,
  description TEXT NOT NULL DEFAULT '',
  instruction_template TEXT NOT NULL,
  params_json TEXT NOT NULL DEFAULT '[]',
  mode TEXT,
  limit_per_day INTEGER,
  limit_per_week INTEGER,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX agent_functions_project ON agent_functions(project_id);
CREATE TABLE api_keys (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  key_verifier TEXT NOT NULL UNIQUE,
  created_at_ms INTEGER NOT NULL,
  last_used_ms INTEGER,
  revoked_at_ms INTEGER
) STRICT;
CREATE TABLE api_key_functions (
  api_key_id TEXT NOT NULL REFERENCES api_keys(id),
  function_id TEXT NOT NULL REFERENCES agent_functions(id),
  PRIMARY KEY (api_key_id, function_id)
) STRICT;
CREATE TABLE function_calls (
  id TEXT PRIMARY KEY,
  function_id TEXT NOT NULL REFERENCES agent_functions(id),
  api_key_id TEXT NOT NULL REFERENCES api_keys(id),
  task_id TEXT NOT NULL REFERENCES tasks(id),
  created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX function_calls_quota ON function_calls(function_id, created_at_ms);
CREATE INDEX function_calls_task ON function_calls(task_id);
