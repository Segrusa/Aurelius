-- A git worktree and a branch per ticket. Until now every run of every ticket worked
-- in the project's one folder, on whatever branch it was on: two tickets at once
-- wrote through each other, and what a ticket changed could not be told apart.
--
-- * tickets.branch: the ticket's own branch (`ticket/12-fix-the-login`), made from
--   `base_branch` at `base_commit` when its first run starts.
-- * tickets.worktree: where that branch is checked out, the folder its runs work in
--   (below the server's data, never inside the project's folder). NULL: none (yet, or
--   removed once the ticket closed with nothing uncommitted in it; the branch stays).
-- * projects.ticket_worktrees: off for a project whose tickets keep working in its
--   folder as before. On by default; it only does anything in a git repository.
--
-- Additive only: columns with defaults.
ALTER TABLE tickets ADD COLUMN branch TEXT;
ALTER TABLE tickets ADD COLUMN base_branch TEXT;
ALTER TABLE tickets ADD COLUMN base_commit TEXT;
ALTER TABLE tickets ADD COLUMN worktree TEXT;
ALTER TABLE projects ADD COLUMN ticket_worktrees INTEGER NOT NULL DEFAULT 1 CHECK(ticket_worktrees IN (0, 1));
