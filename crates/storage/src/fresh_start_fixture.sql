-- A protocol 3 workspace (schema 17) with a row in every table: what the
-- fresh start keeps, pauses, rebuilds and leaves behind. Test data only.
-- Ids: 0a.. scopes, 0b.. blobs, 0c.. projects/employees/departments,
-- 0d.. memories, 0e.. events, 0f.. work and history.

INSERT INTO scopes(id, kind) VALUES ('0a000000-0000-4000-8000-000000000001', 'global');
INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES
  ('0a000000-0000-4000-8000-000000000002', 'project', '0a000000-0000-4000-8000-000000000001', '0c000000-0000-4000-8000-000000000001'),
  ('0a000000-0000-4000-8000-000000000003', 'employee', '0a000000-0000-4000-8000-000000000001', '0c000000-0000-4000-8000-000000000002'),
  ('0a000000-0000-4000-8000-000000000004', 'employee', '0a000000-0000-4000-8000-000000000001', '0c000000-0000-4000-8000-000000000003'),
  ('0a000000-0000-4000-8000-000000000005', 'department', '0a000000-0000-4000-8000-000000000001', '0c000000-0000-4000-8000-000000000004');

-- Blobs (the files are written by the test): 1 project description, 2 Orion's
-- instructions, 3 Ada's identity, 4 a company fact, 5 the same fact in the
-- department, 6 a department-only fact, 7 a forgotten memory, 8 a project fact,
-- 9 an older instruction version, 10 a task brief, 11 a message, 12 a proposal,
-- 13 an employee memory, 14 a rejected memory.
INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms)
  SELECT printf('0b000000-0000-4000-8000-%012d', n), printf('fx/blob%02d', n), 'x', 1, 'text/plain', 1
  FROM (WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c WHERE n < 14) SELECT n FROM c);

INSERT INTO projects(id, scope_id, name, description_blob_id, status, version, created_at_ms, updated_at_ms, folder) VALUES
  ('0c000000-0000-4000-8000-000000000001', '0a000000-0000-4000-8000-000000000002', 'Alpha', '0b000000-0000-4000-8000-000000000001', 'paused', 4, 100, 200, '/work/alpha');

INSERT INTO departments(id, scope_id, name, description, head_id, status, version, created_at_ms, updated_at_ms) VALUES
  ('0c000000-0000-4000-8000-000000000004', '0a000000-0000-4000-8000-000000000005', 'Engineering', NULL, '0c000000-0000-4000-8000-000000000002', 'active', 1, 10, 10);

INSERT INTO employees(id, scope_id, name, role, identity_blob_id, instructions_blob_id, config_json, status, version, created_at_ms, updated_at_ms, folder, title, reports_to, department_id) VALUES
  ('0c000000-0000-4000-8000-000000000002', '0a000000-0000-4000-8000-000000000003', 'Orion', 'pm', NULL, '0b000000-0000-4000-8000-000000000002', '{"identity":""}', 'active', 3, 20, 30, NULL, 'Project Manager', NULL, '0c000000-0000-4000-8000-000000000004'),
  ('0c000000-0000-4000-8000-000000000003', '0a000000-0000-4000-8000-000000000004', 'Ada', 'engineer', '0b000000-0000-4000-8000-000000000003', NULL, '{"identity":"ada"}', 'active', 2, 21, 31, '/work/ada', 'Engineer', '0c000000-0000-4000-8000-000000000002', '0c000000-0000-4000-8000-000000000004');

INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, occurred_at_ms, recorded_at_ms, dedupe_key)
  SELECT printf('0e000000-0000-4000-8000-%012d', n), 'fixture', 'f', n, 'project.created', 1, 'user', 'c', n, n, printf('fixture-%d', n)
  FROM (WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c WHERE n < 3) SELECT n FROM c);

INSERT INTO proposals(id, department_id, proposed_by, status, name, role, title, instructions_blob_id, reports_to, reason, created_at_ms, kind) VALUES
  ('0f000000-0000-4000-8000-000000000020', '0c000000-0000-4000-8000-000000000004', '0c000000-0000-4000-8000-000000000002', 'approved', 'Ada', 'engineer', NULL, '0b000000-0000-4000-8000-000000000012', NULL, 'more hands', 15, 'colleague');

INSERT INTO employee_instruction_versions(employee_id, version, instructions_blob_id, source, proposal_id, restored_from, created_at_ms) VALUES
  ('0c000000-0000-4000-8000-000000000002', 1, '0b000000-0000-4000-8000-000000000009', 'initial', NULL, NULL, 20),
  ('0c000000-0000-4000-8000-000000000002', 2, '0b000000-0000-4000-8000-000000000002', 'proposal', '0f000000-0000-4000-8000-000000000020', NULL, 25);

-- Memories: company fact (oldest), the same fact in the department (a
-- duplicate), a department-only fact, a forgotten one, a project one, an
-- employee one (superseded) and a rejected one.
INSERT INTO memory_items(id, scope_id, kind, content_blob_id, status, confidence, importance, pinned, valid_from_ms, supersedes_id, extraction_version, created_at_ms, locator_json) VALUES
  ('0d000000-0000-4000-8000-000000000001', '0a000000-0000-4000-8000-000000000001', 'decision', '0b000000-0000-4000-8000-000000000004', 'active', 0.9, 0.5, 0, 1, NULL, 'v1', 40, '{}'),
  ('0d000000-0000-4000-8000-000000000002', '0a000000-0000-4000-8000-000000000005', 'decision', '0b000000-0000-4000-8000-000000000005', 'active', 0.9, 0.5, 1, 1, NULL, 'v1', 50, '{"task":"t"}'),
  ('0d000000-0000-4000-8000-000000000003', '0a000000-0000-4000-8000-000000000005', 'project_knowledge', '0b000000-0000-4000-8000-000000000006', 'active', 0.8, 0.5, 0, 1, NULL, 'v1', 51, '{}'),
  ('0d000000-0000-4000-8000-000000000004', '0a000000-0000-4000-8000-000000000002', 'decision', '0b000000-0000-4000-8000-000000000007', 'forgotten', 0.8, 0.5, 0, 1, NULL, 'v1', 52, '{}'),
  ('0d000000-0000-4000-8000-000000000005', '0a000000-0000-4000-8000-000000000002', 'project_knowledge', '0b000000-0000-4000-8000-000000000008', 'active', 0.8, 0.5, 0, 1, NULL, 'v1', 53, '{}'),
  ('0d000000-0000-4000-8000-000000000006', '0a000000-0000-4000-8000-000000000004', 'episodic', '0b000000-0000-4000-8000-000000000013', 'superseded', 0.8, 0.5, 0, 1, '0d000000-0000-4000-8000-000000000004', 'v1', 54, '{}'),
  ('0d000000-0000-4000-8000-000000000007', '0a000000-0000-4000-8000-000000000001', 'decision', '0b000000-0000-4000-8000-000000000014', 'rejected', 0.8, 0.5, 0, 1, NULL, 'v1', 55, '{}');
INSERT INTO memory_sources(memory_id, event_id, locator_json) VALUES ('0d000000-0000-4000-8000-000000000001', '0e000000-0000-4000-8000-000000000001', '{}');
INSERT INTO retrieval_chunks(id, scope_id, source_event_id, memory_id, blob_id, locator_json, content_hash, token_count) VALUES
  ('0f000000-0000-4000-8000-000000000001', '0a000000-0000-4000-8000-000000000001', '0e000000-0000-4000-8000-000000000001', '0d000000-0000-4000-8000-000000000001', '0b000000-0000-4000-8000-000000000004', '{}', 'h', 3);
INSERT INTO chunk_fts(chunk_id, text) VALUES ('0f000000-0000-4000-8000-000000000001', 'old index text');

INSERT INTO config_overrides(scope_id, purpose, version, config_json, source_event_id) VALUES
  ('0a000000-0000-4000-8000-000000000001', 'run', 1, '{"model":"opus"}', '0e000000-0000-4000-8000-000000000001'),
  ('0a000000-0000-4000-8000-000000000005', 'run', 1, '{"model":"haiku"}', '0e000000-0000-4000-8000-000000000001');
INSERT INTO settings(key, value_json, updated_at_ms) VALUES
  ('follow_org_chart', 'false', 1), ('trace_mode', '"full"', 1), ('webhook_cursor', '3', 1), ('sessions_root', '"/old/root"', 1), ('sessions_owned_since_ms', '5', 1);
INSERT INTO connections(id, provider_kind, auth_kind, display_name, secret_ref, status, config_json, updated_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000002', 'anthropic', 'api_key', 'Claude', 'keychain:claude', 'ready', '{}', 1);
INSERT INTO employee_plugins(employee_id, plugin_id, enabled_at_ms) VALUES ('0c000000-0000-4000-8000-000000000003', 'browser', 1);
INSERT INTO project_plugins(project_id, plugin_id, enabled_at_ms) VALUES ('0c000000-0000-4000-8000-000000000001', 'git', 1);
INSERT INTO remote_clients(id, principal_id, name, platform, credential_verifier, capabilities_json, created_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000003', 'p', 'Phone', 'android', 'verifier', '[]', 1);
INSERT INTO remote_sessions(id, client_id, token_verifier, created_at_ms, expires_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000004', '0f000000-0000-4000-8000-000000000003', 'token', 1, 9999999999999);
INSERT INTO push_registrations(client_id, platform, token, environment, updated_at_ms) VALUES ('0f000000-0000-4000-8000-000000000003', 'fcm', 'push', 'production', 1);
INSERT INTO app_keys(id, name, key_verifier, scopes_json, created_at_ms, audience, webhook_url, webhook_secret) VALUES
  ('0f000000-0000-4000-8000-000000000005', 'Slack', 'appkey', '[]', 1, 'management', 'https://example.invalid/hook', 'secret');
INSERT INTO agent_functions(id, project_id, employee_id, name, slug, instruction_template, enabled, created_at_ms, updated_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000006', '0c000000-0000-4000-8000-000000000001', '0c000000-0000-4000-8000-000000000003', 'Summarize', 'summarize', 'Summarize {x}', 1, 1, 1);
INSERT INTO api_keys(id, name, key_verifier, created_at_ms) VALUES ('0f000000-0000-4000-8000-000000000007', 'CI', 'apikey', 1);
INSERT INTO api_key_functions(api_key_id, function_id) VALUES ('0f000000-0000-4000-8000-000000000007', '0f000000-0000-4000-8000-000000000006');
INSERT INTO notes(id, project_id, body, pinned, revision, created_at_ms, updated_at_ms) VALUES ('0f000000-0000-4000-8000-000000000008', '0c000000-0000-4000-8000-000000000001', 'A project note', 1, 1, 1, 1);
INSERT INTO heartbeats(id, employee_id, rule_json, timezone, next_due_ms, enabled, version, cursor_seq) VALUES
  ('0f000000-0000-4000-8000-000000000009', '0c000000-0000-4000-8000-000000000002', '{}', 'UTC', 100, 1, 1, 3);
INSERT INTO recurring_tasks(id, project_id, employee_id, title, goal, recurrence_json, paused, anchor_ms, next_run_ms, last_task_id, created_at_ms, updated_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000010', '0c000000-0000-4000-8000-000000000001', '0c000000-0000-4000-8000-000000000003', 'Weekly', 'Report', '{}', 0, 1, 2, '0f000000-0000-4000-8000-000000000011', 1, 1);

-- Work and history: none of it comes over.
INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000012', 'employee', NULL, '0c000000-0000-4000-8000-000000000002', 'Orion', 1, 1),
  ('0f000000-0000-4000-8000-000000000013', 'task', '0c000000-0000-4000-8000-000000000001', '0c000000-0000-4000-8000-000000000003', 'Do it', 1, 1);
INSERT INTO tasks(id, project_id, employee_id, title, brief_blob_id, status, version, created_at_ms, updated_at_ms, conversation_id) VALUES
  ('0f000000-0000-4000-8000-000000000011', '0c000000-0000-4000-8000-000000000001', '0c000000-0000-4000-8000-000000000003', 'Do it', '0b000000-0000-4000-8000-000000000010', 'succeeded', 1, 1, 1, '0f000000-0000-4000-8000-000000000013'),
  ('0f000000-0000-4000-8000-000000000014', '0c000000-0000-4000-8000-000000000001', '0c000000-0000-4000-8000-000000000003', 'Then this', '0b000000-0000-4000-8000-000000000010', 'queued', 1, 1, 1, NULL);
INSERT INTO ideas(id, project_id, title, status, employee_id, task_id, version, created_at_ms, updated_at_ms, kind) VALUES
  ('0f000000-0000-4000-8000-000000000015', '0c000000-0000-4000-8000-000000000001', 'Open idea', 'open', '0c000000-0000-4000-8000-000000000003', '0f000000-0000-4000-8000-000000000011', 1, 1, 1, 'feature'),
  ('0f000000-0000-4000-8000-000000000016', '0c000000-0000-4000-8000-000000000001', 'Done idea', 'done', NULL, NULL, 1, 1, 1, 'bug');
INSERT INTO task_dependencies(task_id, requires_task_id) VALUES ('0f000000-0000-4000-8000-000000000014', '0f000000-0000-4000-8000-000000000011');
INSERT INTO runs(id, task_id, employee_id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES
  ('0f000000-0000-4000-8000-000000000017', '0f000000-0000-4000-8000-000000000011', '0c000000-0000-4000-8000-000000000003', '0f000000-0000-4000-8000-000000000013', 1, 'claude', 'opus', '{}', 'succeeded');
INSERT INTO messages(id, conversation_id, ordinal, role, content_blob_id, source_event_id, status, input_kind, created_at_ms, run_id) VALUES
  ('0f000000-0000-4000-8000-000000000018', '0f000000-0000-4000-8000-000000000013', 1, 'assistant', '0b000000-0000-4000-8000-000000000011', '0e000000-0000-4000-8000-000000000002', 'complete', 'text', 1, '0f000000-0000-4000-8000-000000000017');
INSERT INTO message_sources(run_id, ordinal, url, sha256, fetched_at_ms) VALUES ('0f000000-0000-4000-8000-000000000017', 1, 'https://example.invalid', 's', 1);
INSERT INTO run_activity(run_id, call_id, message_id, tool, input_json, text_offset, started_at_ms) VALUES ('0f000000-0000-4000-8000-000000000017', 'c1', '0f000000-0000-4000-8000-000000000018', 'Read', '{}', 0, 1);
INSERT INTO run_traces(run_id, conversation_id, created_at_ms, trace_json) VALUES ('0f000000-0000-4000-8000-000000000017', '0f000000-0000-4000-8000-000000000013', 1, '{}');
INSERT INTO conversation_sessions(conversation_id, session_id, updated_at_ms) VALUES ('0f000000-0000-4000-8000-000000000013', 'session', 1);
INSERT INTO approvals(id, run_id, call_id, tool, effect, action_blob_id, action_digest, request_event_id, status, expires_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000019', '0f000000-0000-4000-8000-000000000017', 'c1', 'Bash', 'execute', '0b000000-0000-4000-8000-000000000011', 'd', '0e000000-0000-4000-8000-000000000002', 'approved', 1);
INSERT INTO artifacts(url, title, conversation_id, task_id, project_id, version, first_published_at_ms, last_published_at_ms) VALUES
  ('https://example.invalid/a', 'Report', '0f000000-0000-4000-8000-000000000013', '0f000000-0000-4000-8000-000000000011', '0c000000-0000-4000-8000-000000000001', 1, 1, 1);
INSERT INTO artifact_versions(url, version, kind, file_name, content, published_at_ms) VALUES ('https://example.invalid/a', 1, 'markdown', 'r.md', '# r', 1);
INSERT INTO attachments(id, conversation_id, name, media_kind, original_blob_id, text_blob_id, byte_length, truncated, source_event_id, created_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000021', '0f000000-0000-4000-8000-000000000013', 'a.txt', 'text', '0b000000-0000-4000-8000-000000000011', '0b000000-0000-4000-8000-000000000011', 1, 0, '0e000000-0000-4000-8000-000000000002', 1);
INSERT INTO shares(id, source_event_id, destination_scope_id, snapshot_blob_id, grant_event_id) VALUES
  ('0f000000-0000-4000-8000-000000000022', '0e000000-0000-4000-8000-000000000002', '0a000000-0000-4000-8000-000000000002', '0b000000-0000-4000-8000-000000000011', '0e000000-0000-4000-8000-000000000002');
INSERT INTO usage_records(id, run_id, source_event_id, estimate, provider_record_key, recorded_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000023', '0f000000-0000-4000-8000-000000000017', '0e000000-0000-4000-8000-000000000002', 0, 'k', 1);
INSERT INTO jobs(id, kind, subject_id, source_event_id, dedupe_key, payload_json, state, not_before_ms) VALUES
  ('0f000000-0000-4000-8000-000000000024', 'task.run', '0f000000-0000-4000-8000-000000000014', '0e000000-0000-4000-8000-000000000002', 'job', '{}', 'ready', 1);
INSERT INTO notifications(id, event_id, dedupe_key, severity, target_json, created_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000025', '0e000000-0000-4000-8000-000000000002', 'n', 'info', '{}', 1);
INSERT INTO command_receipts(command_id, request_hash, method, result_json, committed_seq, created_at_ms) VALUES ('cmd', 'h', 'm', '{}', 1, 1);
INSERT INTO function_calls(id, function_id, api_key_id, task_id, created_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000026', '0f000000-0000-4000-8000-000000000006', '0f000000-0000-4000-8000-000000000007', '0f000000-0000-4000-8000-000000000011', 1);
INSERT INTO heartbeat_wakes(run_id, schedule_id, occurrence_key, reasons_json, missed, created_at_ms) VALUES
  ('0f000000-0000-4000-8000-000000000017', '0f000000-0000-4000-8000-000000000009', 'o', '[]', 0, 1);
INSERT INTO file_grants(id, scope_id, canonical_path, platform_identity_json, access, recursive, source_event_id) VALUES
  ('0f000000-0000-4000-8000-000000000027', '0a000000-0000-4000-8000-000000000002', '/work', '{}', 'read', 1, '0e000000-0000-4000-8000-000000000002');
INSERT INTO app_key_threads(key_id, conversation_id, employee_id, created_at_ms, last_sent_ms) VALUES
  ('0f000000-0000-4000-8000-000000000005', '0f000000-0000-4000-8000-000000000012', '0c000000-0000-4000-8000-000000000002', 1, 1);
INSERT INTO webhook_outbox(id, key_id, event, body_json, created_at_ms, next_attempt_ms) VALUES
  ('0f000000-0000-4000-8000-000000000028', '0f000000-0000-4000-8000-000000000005', 'manager.replied', '{}', 1, 1);
INSERT INTO task_variant_sets(task_id, phase, count, created_at_ms, updated_at_ms) VALUES ('0f000000-0000-4000-8000-000000000011', 'done', 2, 1, 1);
INSERT INTO task_variants(task_id, n, hint, status, created_at_ms) VALUES ('0f000000-0000-4000-8000-000000000011', 1, 'fast', 'done', 1);
