-- What became of a ticket's branch when a board member decided on it: merged into the
-- branch it was made from, or pushed for a pull request. Only a board member does
-- either (`ticket.land`); the server never does by itself.
--
-- * tickets.landed: 'merged' or 'pushed'; NULL: the branch is only kept.
-- * tickets.landed_commit: the merge commit, or the commit that was pushed.
-- * tickets.landed_url: where the pull request is opened, when the remote is one we know.
--
-- Additive only: nullable columns.
ALTER TABLE tickets ADD COLUMN landed TEXT CHECK(landed IN ('merged', 'pushed'));
ALTER TABLE tickets ADD COLUMN landed_commit TEXT;
ALTER TABLE tickets ADD COLUMN landed_url TEXT;
