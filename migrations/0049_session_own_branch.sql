-- Protocol 5 (additive, feature `session_own_branch`): a work session can work in the
-- project's folder itself on a branch of its own, instead of in a worktree of its own.
--
-- Such a session keeps the project's folder in `worktree` and its branch in `branch`
-- (so the CHECK that both are set or both are NULL still holds); `own_branch` says
-- that the daemon checks the branch out in that folder instead of making a worktree.
-- Older sessions have no branch of their own.
ALTER TABLE work_sessions ADD COLUMN own_branch INTEGER NOT NULL DEFAULT 0 CHECK(own_branch IN (0, 1));
