-- Plugins a project has enabled: its project assistant loads them.
CREATE TABLE project_plugins (
  project_id TEXT NOT NULL REFERENCES projects(id),
  plugin_id TEXT NOT NULL,
  enabled_at_ms INTEGER NOT NULL,
  PRIMARY KEY(project_id, plugin_id)
) STRICT;
