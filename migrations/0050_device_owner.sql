-- Protocol 5 (additive, feature `own_devices`): a paired device belongs to the board member
-- who paired it. Settings → Account lists a board member's own devices; nobody sees or
-- revokes another board member's devices.
--
-- `user_id` stays "the board member who last signed in on it" (push goes there). A device
-- paired before this migration belongs to that board member; one that nobody ever signed
-- in on belongs to nobody and stays visible in the server's Remote access settings.
ALTER TABLE remote_clients ADD COLUMN paired_by TEXT REFERENCES users(id);
UPDATE remote_clients SET paired_by = user_id;
CREATE INDEX remote_clients_paired_by ON remote_clients(paired_by);
