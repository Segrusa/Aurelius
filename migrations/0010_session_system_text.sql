-- The system text a conversation's session was started with, given again as it is
-- on every `--resume`: Claude Code does not keep it with the session, and a changed
-- text means the whole session is written to the prompt cache again. What changed
-- since goes in front of the new messages instead. `system_at_ms`: when it was
-- frozen (a session older than a day starts over); 0 for sessions from before.
ALTER TABLE conversation_sessions ADD COLUMN system_text TEXT NOT NULL DEFAULT '';
ALTER TABLE conversation_sessions ADD COLUMN system_at_ms INTEGER NOT NULL DEFAULT 0;
