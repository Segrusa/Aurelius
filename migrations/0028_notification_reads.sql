-- v0.19 (protocol 5, AUR-FA-A0): the Inbox per board member. A notification for
-- everyone (notifications.user_id IS NULL) is one shared row, so reading or
-- dismissing it may not touch that row: who read or dismissed it is kept here.
-- A board member's own notification keeps working on its row (read_at_ms, DELETE).
-- The read_at_ms already on a shared row stays and counts for everyone: who read
-- it was not kept.
--
-- Additive only: two new tables. Nothing is rebuilt, changed or deleted.
CREATE TABLE IF NOT EXISTS notification_reads (
  notification_id TEXT NOT NULL REFERENCES notifications(id) ON DELETE CASCADE,
  user_id TEXT NOT NULL REFERENCES users(id),
  read_at_ms INTEGER NOT NULL,
  PRIMARY KEY(notification_id, user_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS notification_dismissals (
  notification_id TEXT NOT NULL REFERENCES notifications(id) ON DELETE CASCADE,
  user_id TEXT NOT NULL REFERENCES users(id),
  dismissed_at_ms INTEGER NOT NULL,
  PRIMARY KEY(notification_id, user_id)
) STRICT, WITHOUT ROWID;
