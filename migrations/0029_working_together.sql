-- v0.20 (protocol 5, AUR-FA-S1): phase A, working together per person.
--
-- * tickets.responsible_user_id: the board member who is responsible for a ticket,
--   next to the employee who owns its main task. Nobody on existing tickets.
-- * ticket_followers: who follows a ticket and why. Unfollowing removes the row.
--   Every creator follows their tickets that are not archived.
-- * notifications: what a row for one person is about, why they get it and how
--   many later events it stands for. reason is checked on its length only, so a
--   new kind needs no rebuild; conversation_id merges a mention outside a ticket
--   per channel. A queued message can be deleted (conversations::delete_message):
--   its row stays, without the message.
-- * messages: when a message was edited and deleted. The old text stays in the
--   blob the event of the edit or delete carries.
--
-- Additive only: nothing is rebuilt, changed or deleted.
ALTER TABLE tickets ADD COLUMN responsible_user_id TEXT REFERENCES users(id);

CREATE TABLE ticket_followers (
  ticket_id TEXT NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
  user_id TEXT NOT NULL REFERENCES users(id),
  reason TEXT NOT NULL CHECK(reason IN ('created','responsible','mentioned','manual')),
  followed_at_ms INTEGER NOT NULL,
  PRIMARY KEY(ticket_id, user_id)
) STRICT, WITHOUT ROWID;

INSERT INTO ticket_followers(ticket_id, user_id, reason, followed_at_ms)
  SELECT id, created_by_user, 'created', created_at_ms FROM tickets WHERE archived_at_ms IS NULL;

ALTER TABLE notifications ADD COLUMN ticket_id TEXT REFERENCES tickets(id);
ALTER TABLE notifications ADD COLUMN conversation_id TEXT REFERENCES conversations(id);
ALTER TABLE notifications ADD COLUMN message_id TEXT REFERENCES messages(id) ON DELETE SET NULL;
ALTER TABLE notifications ADD COLUMN reason TEXT CHECK(reason IS NULL OR length(reason) <= 32);
ALTER TABLE notifications ADD COLUMN merged_count INTEGER NOT NULL DEFAULT 0 CHECK(merged_count >= 0);
CREATE INDEX notifications_user_ticket ON notifications(user_id, ticket_id);
CREATE INDEX notifications_user_conversation ON notifications(user_id, conversation_id);

ALTER TABLE messages ADD COLUMN edited_at_ms INTEGER;
ALTER TABLE messages ADD COLUMN deleted_at_ms INTEGER;
