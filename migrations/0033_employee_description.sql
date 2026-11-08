-- v0.23 (protocol 5, feature set "employees as bots", P7): what other employees read
-- about an employee to know when to hand work to it.
--
-- * employees.description: one line, at most 160 characters (the server checks it).
--   Empty until a board member writes one; the roster in a run's prompt then shows
--   the first line of the instructions, as it did before this column.
--
-- Additive only: one column with a default. Nothing is rebuilt, changed or deleted.
ALTER TABLE employees ADD COLUMN description TEXT NOT NULL DEFAULT '';
