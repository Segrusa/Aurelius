-- The file each publish of a Claude Code artifact sent to claude.ai, so clients
-- can show it without a claude.ai login. The last 20 versions per page are kept.
CREATE TABLE artifact_versions (
  url TEXT NOT NULL REFERENCES artifacts(url),
  version INTEGER NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('html','markdown','text')),
  file_name TEXT NOT NULL,
  content TEXT NOT NULL,
  published_at_ms INTEGER NOT NULL,
  PRIMARY KEY (url, version)
) STRICT;
