-- v0.25 (protocol 5, additive): links to Claude Design artifacts.
--
-- * projects.styleguide_url: the workspace's styleguide (an artifact of type Design System).
-- * tickets.design_url: the design of this ticket (an artifact of type Design).
--
-- Only the link is kept, never what the artifact holds: a run reads it itself with Claude
-- Code's Artifact tool, on the Claude login of the board member it runs on. Both optional,
-- additive only.
ALTER TABLE projects ADD COLUMN styleguide_url TEXT;
ALTER TABLE tickets ADD COLUMN design_url TEXT;
