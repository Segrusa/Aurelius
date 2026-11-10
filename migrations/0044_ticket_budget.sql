-- A budget per ticket: how much employees may set in motion on a ticket between two
-- actions of a board member. Until now a ticket could go round between its owner, a
-- reviewer and the tester without end; only a count per hour slowed it down.
--
-- * projects.ticket_budget: how many wakes of employees a ticket of this project may
--   have since a board member last acted on it. NULL: the default (25); 0: no limit.
--
-- What is spent is counted from the mailbox (`employee_inbox`) and the ticket's events,
-- so nothing else is stored. Additive only: one nullable column.
ALTER TABLE projects ADD COLUMN ticket_budget INTEGER CHECK(ticket_budget IS NULL OR ticket_budget >= 0);
