-- Management channel and webhooks for app keys (features `management_channel`
-- and `webhooks`). Existing keys stay audience 'all' and have no webhook.
ALTER TABLE app_keys ADD COLUMN audience TEXT NOT NULL DEFAULT 'all' CHECK(audience IN ('all','management'));
ALTER TABLE app_keys ADD COLUMN webhook_url TEXT;
-- Kept as is (not hashed): Aurelius needs it to sign every delivery. Never listed.
ALTER TABLE app_keys ADD COLUMN webhook_secret TEXT;

-- The conversations an app wrote into (`ask_management`): the app gets the replies
-- to them (`manager.replied`) and may read them. `awaiting`: it wrote and no reply
-- was delivered yet.
CREATE TABLE app_key_threads (
  key_id TEXT NOT NULL REFERENCES app_keys(id),
  conversation_id TEXT NOT NULL,
  employee_id TEXT NOT NULL,
  awaiting INTEGER NOT NULL DEFAULT 0 CHECK(awaiting IN (0,1)),
  created_at_ms INTEGER NOT NULL,
  last_sent_ms INTEGER NOT NULL,
  PRIMARY KEY (key_id, conversation_id)
) STRICT;
CREATE INDEX app_key_threads_conversation ON app_key_threads(conversation_id);

-- Events waiting to be posted to an app's webhook. A row is made when the
-- event happens and kept until it is delivered or given up, so a restart
-- loses nothing. `id` is the delivery id (the receiver's idempotency id).
CREATE TABLE webhook_outbox (
  id TEXT PRIMARY KEY,
  key_id TEXT NOT NULL REFERENCES app_keys(id),
  event TEXT NOT NULL,
  body_json TEXT NOT NULL CHECK(json_valid(body_json)),
  created_at_ms INTEGER NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_attempt_ms INTEGER NOT NULL,
  delivered_at_ms INTEGER,
  gave_up_at_ms INTEGER,
  last_status INTEGER,
  last_error TEXT
) STRICT;
CREATE INDEX webhook_outbox_due ON webhook_outbox(next_attempt_ms) WHERE delivered_at_ms IS NULL AND gave_up_at_ms IS NULL;
CREATE INDEX webhook_outbox_key ON webhook_outbox(key_id, created_at_ms);
