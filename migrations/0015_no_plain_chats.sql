-- v0.9 (protocol 3.0): the plain chat is gone. Every conversation belongs to a
-- project (its assistant), a task or an employee. Standalone chats (kind 'chat')
-- are deleted with everything that hangs off them (the daemon backs the
-- database up before it migrates), the stored Chat run defaults go, and the
-- conversations table is rebuilt without 'chat' and without web research.
-- Runs with foreign keys off (Migration::rebuild); the check after it must be clean.

CREATE TEMP TABLE gone_conversations AS SELECT id FROM conversations WHERE kind = 'chat';
CREATE TEMP TABLE gone_runs AS SELECT id FROM runs WHERE conversation_id IN (SELECT id FROM gone_conversations);

DELETE FROM run_activity WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM message_sources WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM usage_records WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM heartbeat_wakes WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM approvals WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM run_traces WHERE run_id IN (SELECT id FROM gone_runs) OR conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM jobs WHERE id IN (SELECT id FROM gone_runs)
   OR json_extract(payload_json, '$.conversation_id') IN (SELECT id FROM gone_conversations);
DELETE FROM messages WHERE conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM runs WHERE id IN (SELECT id FROM gone_runs);
DELETE FROM attachments WHERE conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM shares WHERE destination_conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM conversation_sessions WHERE conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM artifact_versions WHERE url IN (SELECT url FROM artifacts WHERE conversation_id IN (SELECT id FROM gone_conversations));
DELETE FROM artifacts WHERE conversation_id IN (SELECT id FROM gone_conversations);
UPDATE tasks SET origin_conversation_id = NULL WHERE origin_conversation_id IN (SELECT id FROM gone_conversations);
UPDATE tasks SET conversation_id = NULL WHERE conversation_id IN (SELECT id FROM gone_conversations);
UPDATE proposals SET origin_conversation_id = NULL WHERE origin_conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM conversations WHERE id IN (SELECT id FROM gone_conversations);
DROP TABLE gone_runs;
DROP TABLE gone_conversations;

-- The Chat purpose's run defaults (model, effort, persona for plain chats).
DELETE FROM config_overrides WHERE purpose = 'chat';

CREATE TABLE conversations_new (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('project','task','employee')),
  project_id TEXT REFERENCES projects(id), employee_id TEXT REFERENCES employees(id),
  title TEXT NOT NULL, version INTEGER NOT NULL, created_at_ms INTEGER NOT NULL,
  archived_at_ms INTEGER,
  agent_mode TEXT NOT NULL DEFAULT 'ask' CHECK(agent_mode IN ('plan','ask','accept_edits','auto')),
  browser_allowed INTEGER NOT NULL DEFAULT 0 CHECK(browser_allowed IN (0,1)),
  screen_allowed INTEGER NOT NULL DEFAULT 0 CHECK(screen_allowed IN (0,1)),
  run_json TEXT,
  CHECK((kind='project' AND project_id IS NOT NULL AND employee_id IS NULL) OR
        (kind='task' AND project_id IS NOT NULL AND employee_id IS NOT NULL) OR
        (kind='employee' AND project_id IS NULL AND employee_id IS NOT NULL))
) STRICT;
INSERT INTO conversations_new(id, kind, project_id, employee_id, title, version, created_at_ms, archived_at_ms,
                              agent_mode, browser_allowed, screen_allowed, run_json)
  SELECT id, kind, project_id, employee_id, title, version, created_at_ms, archived_at_ms,
         agent_mode, browser_allowed, screen_allowed, run_json
  FROM conversations;
DROP TABLE conversations;
ALTER TABLE conversations_new RENAME TO conversations;
CREATE INDEX conversations_employee ON conversations(employee_id, kind, created_at_ms);
CREATE INDEX conversations_project ON conversations(project_id, kind);
