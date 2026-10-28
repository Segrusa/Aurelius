-- Idea attachments (feature `idea_attachments`): like a task's, copies in the
-- project's uploads area, as JSON {folder, files}.
ALTER TABLE ideas ADD COLUMN attachments_json TEXT;
