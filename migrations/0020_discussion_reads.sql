-- v0.12 (protocol 4, phase 2): how far each board member has read each
-- discussion (a channel of the Team chat, a ticket's discussion), for the
-- unread and mention counts of `channel.list`. Reading the discussion
-- (`group_chat.get { channel }`, `conversation.get`) moves it forward; it never
-- goes back. A read marker is not an event: it changes nothing anyone else sees.
CREATE TABLE discussion_reads (
  user_id TEXT NOT NULL REFERENCES users(id),
  conversation_id TEXT NOT NULL REFERENCES conversations(id),
  -- The highest message ordinal read (messages.ordinal).
  read_through INTEGER NOT NULL,
  read_at_ms INTEGER NOT NULL,
  PRIMARY KEY(user_id, conversation_id)
) STRICT, WITHOUT ROWID;
