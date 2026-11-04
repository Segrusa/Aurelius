-- A browser pairs like a phone (the React web client): `web` is a platform of a paired device.
--
-- Runs with foreign keys off (Migration::rebuild): remote_clients is rebuilt so its CHECK takes
-- 'web'. Every row is copied with its id, so remote_sessions, push_registrations and
-- user_sessions stay valid; nothing is deleted.

CREATE TABLE remote_clients_new (
  id TEXT PRIMARY KEY,
  principal_id TEXT NOT NULL,
  name TEXT NOT NULL,
  platform TEXT NOT NULL CHECK(platform IN ('ios','android','windows','macos','linux','web')),
  credential_verifier TEXT NOT NULL,
  capabilities_json TEXT NOT NULL CHECK(json_valid(capabilities_json)),
  created_at_ms INTEGER NOT NULL,
  last_seen_ms INTEGER,
  revoked_at_ms INTEGER,
  user_id TEXT REFERENCES users(id)
) STRICT;
INSERT INTO remote_clients_new(id, principal_id, name, platform, credential_verifier, capabilities_json, created_at_ms, last_seen_ms, revoked_at_ms, user_id)
  SELECT id, principal_id, name, platform, credential_verifier, capabilities_json, created_at_ms, last_seen_ms, revoked_at_ms, user_id FROM remote_clients;
DROP TABLE remote_clients;
ALTER TABLE remote_clients_new RENAME TO remote_clients;
