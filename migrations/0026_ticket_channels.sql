-- v0.17 (protocol 5, feature `ticket_channels`, AUR-CHAT-001): a channel per
-- ticket. The ticket's discussion (kind 'ticket', 0019) is its channel; its
-- name is computed when read, never stored. A closed ticket's channel is
-- archived (conversations.archived_at_ms), never deleted. Room for threads and
-- reactions (phase 2, not used yet).
--
-- Additive only: new tables, a new column, and a backfill that only inserts
-- (OR IGNORE) and only fills an empty archived_at_ms. Nothing is rebuilt or deleted.

-- Who is in a channel: a board member or an employee (exactly one of them),
-- since when. Nobody is ever removed: the history of who was in it stays.
CREATE TABLE IF NOT EXISTS channel_members (
  conversation_id TEXT NOT NULL REFERENCES conversations(id),
  user_id TEXT REFERENCES users(id),
  employee_id TEXT REFERENCES employees(id),
  joined_at_ms INTEGER NOT NULL,
  CHECK((user_id IS NULL) <> (employee_id IS NULL))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS channel_members_user ON channel_members(conversation_id, user_id) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS channel_members_employee ON channel_members(conversation_id, employee_id) WHERE employee_id IS NOT NULL;

-- Threads (phase 2): the message of the channel a reply answers.
ALTER TABLE messages ADD COLUMN thread_parent_id TEXT REFERENCES messages(id);
CREATE INDEX IF NOT EXISTS messages_thread ON messages(thread_parent_id, ordinal) WHERE thread_parent_id IS NOT NULL;

-- Reactions (phase 2): one emoji of a board member or an employee on a message, once each.
CREATE TABLE IF NOT EXISTS message_reactions (
  message_id TEXT NOT NULL REFERENCES messages(id),
  user_id TEXT REFERENCES users(id),
  employee_id TEXT REFERENCES employees(id),
  emoji TEXT NOT NULL CHECK(length(emoji) BETWEEN 1 AND 64),
  created_at_ms INTEGER NOT NULL,
  CHECK((user_id IS NULL) <> (employee_id IS NULL))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS message_reactions_user ON message_reactions(message_id, user_id, emoji) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS message_reactions_employee ON message_reactions(message_id, employee_id, emoji) WHERE employee_id IS NOT NULL;

-- backfill (idempotent): the statements below may run again and change nothing.

-- Every ticket's channel gets its members as a new ticket would: the board
-- member who made it, the Owner (the board members made in the onboarding,
-- not disabled), the owner employee and the employee of the main task, each
-- as of the ticket's creation.
INSERT OR IGNORE INTO channel_members(conversation_id, user_id, joined_at_ms)
  SELECT t.conversation_id, t.created_by_user, t.created_at_ms FROM tickets t;
INSERT OR IGNORE INTO channel_members(conversation_id, user_id, joined_at_ms)
  SELECT t.conversation_id, u.id, t.created_at_ms FROM tickets t JOIN users u ON u.created_by_user IS NULL AND u.disabled_at_ms IS NULL;
INSERT OR IGNORE INTO channel_members(conversation_id, employee_id, joined_at_ms)
  SELECT t.conversation_id, t.owner_employee_id, t.created_at_ms FROM tickets t WHERE t.owner_employee_id IS NOT NULL;
INSERT OR IGNORE INTO channel_members(conversation_id, employee_id, joined_at_ms)
  SELECT t.conversation_id, k.employee_id, t.created_at_ms FROM tickets t JOIN tasks k ON k.id = t.root_task_id WHERE k.employee_id IS NOT NULL;

-- The channels of tickets already on Done or archived are closed: read-only, archived.
UPDATE conversations SET archived_at_ms = (
    SELECT COALESCE(t.archived_at_ms, t.done_at_ms, t.updated_at_ms) FROM tickets t WHERE t.conversation_id = conversations.id)
  WHERE kind = 'ticket' AND archived_at_ms IS NULL
    AND id IN (SELECT t.conversation_id FROM tickets t JOIN board_columns b ON b.id = t.column_id
               WHERE b.kind = 'done' OR t.archived_at_ms IS NOT NULL);
