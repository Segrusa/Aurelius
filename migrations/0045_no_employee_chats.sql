-- v0.12: chatting with an employee is gone. A board member's own chat with an
-- employee (conversations.board_member_id, chat_tickets) and the Team chat (the
-- General channel and the project channels, kind 'group', and the employees' threads
-- of them, thread key 'channel:...') are deleted with everything that hangs off them
-- (the daemon backs the database up before it migrates). Kept: every employee's main
-- chat (kind 'employee', no thread, no board member: it holds the employee's settings),
-- the discussions of tickets and their threads, tasks, project conversations and work
-- sessions. The conversations table keeps its shape (the 'group' kind and board_member_id
-- stay as dead letters); the events log is append-only and untouched.
-- Runs with foreign keys off (Migration::rebuild); the check after it must be clean.

CREATE TEMP TABLE gone_conversations AS
  SELECT id FROM conversations
  WHERE kind = 'group'
     OR (kind = 'employee' AND thread_key LIKE 'channel:%')
     OR board_member_id IS NOT NULL;
CREATE TEMP TABLE gone_runs AS SELECT id FROM runs WHERE conversation_id IN (SELECT id FROM gone_conversations);
CREATE TEMP TABLE gone_messages AS SELECT id FROM messages WHERE conversation_id IN (SELECT id FROM gone_conversations);

-- What is left of a run in a conversation that stays, but was posted by a gone run.
UPDATE messages SET run_id = NULL WHERE run_id IN (SELECT id FROM gone_runs) AND id NOT IN (SELECT id FROM gone_messages);
-- What a message that stays says about one that goes (a reply, a thread of a gone root).
UPDATE messages SET reply_to_id = NULL WHERE reply_to_id IN (SELECT id FROM gone_messages) AND id NOT IN (SELECT id FROM gone_messages);
UPDATE messages SET thread_parent_id = NULL WHERE thread_parent_id IN (SELECT id FROM gone_messages) AND id NOT IN (SELECT id FROM gone_messages);
UPDATE messages SET reply_to_id = NULL, thread_parent_id = NULL WHERE id IN (SELECT id FROM gone_messages);

UPDATE task_variants SET run_id = NULL WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM run_activity WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM message_sources WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM usage_records WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM heartbeat_wakes WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM approvals WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM run_guidelines WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM run_traces WHERE run_id IN (SELECT id FROM gone_runs) OR conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM run_triggers WHERE run_id IN (SELECT id FROM gone_runs)
   OR item_id IN (SELECT id FROM employee_inbox WHERE conversation_id IN (SELECT id FROM gone_conversations) OR thread_key LIKE 'channel:%' OR run_id IN (SELECT id FROM gone_runs));
DELETE FROM employee_inbox WHERE conversation_id IN (SELECT id FROM gone_conversations) OR thread_key LIKE 'channel:%' OR run_id IN (SELECT id FROM gone_runs);
DELETE FROM jobs WHERE id IN (SELECT id FROM gone_runs)
   OR json_extract(payload_json, '$.conversation_id') IN (SELECT id FROM gone_conversations)
   OR json_extract(payload_json, '$.group.conversation_id') IN (SELECT id FROM gone_conversations);
DELETE FROM message_reactions WHERE message_id IN (SELECT id FROM gone_messages);
DELETE FROM thread_reads WHERE root_message_id IN (SELECT id FROM gone_messages);
DELETE FROM discussion_reads WHERE conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM channel_members WHERE conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM notifications WHERE conversation_id IN (SELECT id FROM gone_conversations) OR message_id IN (SELECT id FROM gone_messages);
DELETE FROM scheduled_messages WHERE discussion_id IN (SELECT id FROM gone_conversations) OR chat_id IN (SELECT id FROM gone_conversations) OR thread_key LIKE 'channel:%';
DELETE FROM employee_threads WHERE thread_key LIKE 'channel:%' OR conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM activity_log WHERE thread_key LIKE 'channel:%' OR thread_key IN (SELECT 'chat:' || id FROM gone_conversations) OR run_id IN (SELECT id FROM gone_runs);
DELETE FROM messages WHERE conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM session_dispatch_runs WHERE run_id IN (SELECT id FROM gone_runs);
DELETE FROM runs WHERE id IN (SELECT id FROM gone_runs);
DELETE FROM attachments WHERE conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM shares WHERE destination_conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM conversation_sessions WHERE conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM artifact_versions WHERE url IN (SELECT url FROM artifacts WHERE conversation_id IN (SELECT id FROM gone_conversations));
DELETE FROM artifacts WHERE conversation_id IN (SELECT id FROM gone_conversations);
UPDATE tasks SET origin_conversation_id = NULL WHERE origin_conversation_id IN (SELECT id FROM gone_conversations);
UPDATE tasks SET conversation_id = NULL WHERE conversation_id IN (SELECT id FROM gone_conversations);
UPDATE task_variants SET conversation_id = NULL WHERE conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM app_key_threads WHERE conversation_id IN (SELECT id FROM gone_conversations);
UPDATE proposals SET origin_conversation_id = NULL WHERE origin_conversation_id IN (SELECT id FROM gone_conversations);
DELETE FROM chat_tickets;
DELETE FROM conversations WHERE id IN (SELECT id FROM gone_conversations);
DROP TABLE gone_messages;
DROP TABLE gone_runs;
DROP TABLE gone_conversations;

DROP TABLE chat_tickets;
