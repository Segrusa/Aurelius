-- How big the session's context was at its last answer (input + cache reads +
-- cache writes of the last model call), so the next run can compact it first
-- when it grew past the server's threshold. 0: unknown (a session from before).
ALTER TABLE conversation_sessions ADD COLUMN context_tokens INTEGER NOT NULL DEFAULT 0;
