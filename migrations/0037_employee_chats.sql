-- v0.23 (protocol 5): one lasting chat per board member per employee, and what ties
-- a ticket to the chat it was made in.
--
-- * conversations.board_member_id: set on such a chat (kind 'employee', thread_key
--   NULL). Every conversation so far is NULL: an employee's own workspace and its
--   older chats stay what they are.
-- * chat_tickets: a ticket made from a chat (recorded by the server when
--   create_ticket runs there). `review_arrivals` counts how often it reached Review,
--   `reviews_told` up to which arrival the chat heard of it: the difference is an
--   update still to deliver, and it stays until it is delivered.
--
-- Additive only: one nullable column, one index and one table.
ALTER TABLE conversations ADD COLUMN board_member_id TEXT;
CREATE UNIQUE INDEX conversations_board_member_chat ON conversations(employee_id, board_member_id) WHERE board_member_id IS NOT NULL;

CREATE TABLE chat_tickets (
  ticket_id TEXT PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  review_arrivals INTEGER NOT NULL DEFAULT 0,
  reviews_told INTEGER NOT NULL DEFAULT 0,
  created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX chat_tickets_waiting ON chat_tickets(conversation_id) WHERE review_arrivals > reviews_told;
