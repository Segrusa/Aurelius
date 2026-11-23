-- Protocol 5 (additive, feature `rail_pins`): each board member pins their own
-- workspaces to the rail, on every device they use.
--
-- * user_rails: a board member who chose their rail. Without a row the client pins
--   every open workspace, so no rail starts empty; with one, an empty list means none.
-- * user_rail_pins: the pinned workspaces in rail order (position from 0). A whole
--   list is written at once (user.set_rail_pins), so positions have no gaps.
--
-- Additive only: nothing is rebuilt, changed or deleted.
CREATE TABLE user_rails (
  user_id TEXT PRIMARY KEY REFERENCES users(id),
  chosen_at_ms INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE user_rail_pins (
  user_id TEXT NOT NULL REFERENCES user_rails(user_id),
  project_id TEXT NOT NULL REFERENCES projects(id),
  position INTEGER NOT NULL CHECK(position >= 0 AND position < 500),
  PRIMARY KEY(user_id, project_id),
  UNIQUE(user_id, position)
) STRICT, WITHOUT ROWID;
