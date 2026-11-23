-- Protocol 5 (additive): a work session can be a design session.
--
-- A board member starts a design as a type of its own next to a session and a
-- ticket. It is a work session whose employee makes HTML pages; the app opens it
-- with its preview and lists it under Designs from the start. Older sessions are
-- no design.
ALTER TABLE work_sessions ADD COLUMN design INTEGER NOT NULL DEFAULT 0 CHECK(design IN (0, 1));
