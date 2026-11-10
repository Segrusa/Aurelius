-- An administrator among the board members. Until now every board member could
-- reset another's password, end their sessions and disable them; with a temporary
-- password that is signing in as them, on their Claude login and in their private
-- chats. From here only an administrator does those three for someone else, and
-- an administrator makes or unmakes another one.
--
-- * users.admin: 1 for an administrator. The first board member of a server (the
--   onboarding) is one; a server that already has board members gets its oldest
--   active one, so there is always someone who can.
--
-- Additive only: one column with a default.
ALTER TABLE users ADD COLUMN admin INTEGER NOT NULL DEFAULT 0 CHECK(admin IN (0, 1));
UPDATE users SET admin = 1
 WHERE id = (SELECT id FROM users WHERE disabled_at_ms IS NULL ORDER BY created_at_ms, id LIMIT 1);
