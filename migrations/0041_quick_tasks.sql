-- v0.26 (protocol 5, feature `quick_task`, ticket #53): quick tasks.
--
-- * tickets.kind: 'normal' (every ticket so far) or 'quick': a small piece of work that
--   starts in Doing with its chosen employee, never gets triage, Plan review or Testing,
--   works on its own branch `quick/ticket-<N>` and is merged into the project's local main
--   when a board member accepts it.
-- * tickets.merge_commit: the commit of that merge, once it succeeded.
--
-- Additive only: a new column may carry a CHECK and a NOT NULL with a default, so the
-- table is not rebuilt; every existing ticket becomes 'normal'.
ALTER TABLE tickets ADD COLUMN kind TEXT NOT NULL DEFAULT 'normal' CHECK(kind IN ('normal','quick'));
ALTER TABLE tickets ADD COLUMN merge_commit TEXT CHECK(merge_commit IS NULL OR length(merge_commit) BETWEEN 7 AND 64);
