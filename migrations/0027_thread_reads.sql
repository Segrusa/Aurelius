-- v0.18 (protocol 5, features `chat_threads` / `chat_reactions`, AUR-CHAT-002):
-- how far each board member has read each thread, for `MessageView::thread_unread`
-- and the mentions of `channel.list`. Reading the thread (`thread.read`) moves
-- it forward; reading the channel does not. It never goes back.
--
-- Additive only: one new table. Nothing is rebuilt, changed or deleted.
CREATE TABLE IF NOT EXISTS thread_reads (
  user_id TEXT NOT NULL REFERENCES users(id),
  -- The message that starts the thread (messages.thread_parent_id of its replies).
  root_message_id TEXT NOT NULL REFERENCES messages(id),
  -- The highest ordinal of a reply read (messages.ordinal, of the channel's conversation).
  read_through INTEGER NOT NULL,
  read_at_ms INTEGER NOT NULL,
  PRIMARY KEY(user_id, root_message_id)
) STRICT, WITHOUT ROWID;
