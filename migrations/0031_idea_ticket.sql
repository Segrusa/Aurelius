-- v0.22 (protocol 5, feature `ticket_from_idea`, ticket #16): an idea becomes a
-- ticket instead of a task.
--
-- * ideas.ticket_id: the ticket made from the idea (`ticket.create { from_idea }`),
--   set in the same transaction that makes the ticket. Every idea so far has none;
--   those that became a task keep their task_id.
--
-- Additive only: one column without a value. Nothing is rebuilt, changed or deleted.
ALTER TABLE ideas ADD COLUMN ticket_id TEXT REFERENCES tickets(id);
