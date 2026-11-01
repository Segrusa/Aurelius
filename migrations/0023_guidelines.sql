-- v0.15 (protocol 5, additive): guidelines (docs/product/guidelines, B1–B7).
-- A company library of working rules with append-only versions, linked to
-- employees or projects. A run gets the guidelines of its employee and of its
-- project in its system text and keeps the versions it got (run_guidelines).
-- An employee proposes with propose_guideline (guideline_proposals); only a
-- board member changes, links or decides.
--
-- No content: the starter set comes with a one-time seed step at the first
-- start (storage::guidelines::seed_guidelines_once, B7), never with a migration.
--
-- Additive only: five new tables, no row of an older table changes.

-- A guideline is never deleted (B1): "Delete" archives it, so an old run's
-- snapshot and the versions keep their guideline. Title and text live in the
-- versions; `current_version` points at the newest. NULL users: the seed.
CREATE TABLE guidelines (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES company(id),
  current_version INTEGER NOT NULL CHECK(current_version > 0),
  created_by_user TEXT REFERENCES users(id),
  updated_by_user TEXT REFERENCES users(id),
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  archived_at_ms INTEGER
) STRICT;
-- The library list. A unique title per company among the guidelines that are
-- not archived (NOCASE) spans two tables: storage checks it on every write.
CREATE INDEX guidelines_company ON guidelines(company_id, archived_at_ms);

CREATE TRIGGER guidelines_no_delete BEFORE DELETE ON guidelines BEGIN
  SELECT RAISE(ABORT, 'guidelines are archived, not deleted');
END;
-- The current version only moves forward, one at a time, to a version that is
-- stored (write the version first, then point at it).
CREATE TRIGGER guidelines_version_forward BEFORE UPDATE OF current_version ON guidelines
  WHEN NEW.current_version <> OLD.current_version
   AND (NEW.current_version <> OLD.current_version + 1
        OR NOT EXISTS (SELECT 1 FROM guideline_versions WHERE guideline_id = NEW.id AND version = NEW.current_version))
BEGIN
  SELECT RAISE(ABORT, 'a guideline moves to its next stored version only');
END;

-- What propose_guideline asked (B5, impactanalyse §7). Its own table next to
-- `proposals`: old protocol 5 clients never see an unknown kind. A NULL
-- guideline is a new one, which has no base version. A board member decides
-- (approved/rejected); `withdrawn` is a proposer that was archived first. The
-- run and the conversation are kept as they were, without a foreign key.
CREATE TABLE guideline_proposals (
  id TEXT PRIMARY KEY,
  guideline_id TEXT REFERENCES guidelines(id),
  base_version INTEGER CHECK(base_version IS NULL OR base_version > 0),
  proposed_by TEXT NOT NULL REFERENCES employees(id),
  title TEXT NOT NULL CHECK(length(trim(title)) > 0 AND length(title) <= 120),
  text TEXT NOT NULL CHECK(length(trim(text)) > 0 AND length(text) <= 8000),
  reason TEXT NOT NULL CHECK(length(trim(reason)) > 0 AND length(reason) <= 2000),
  -- [{kind, id, label}], as instruction proposals.
  evidence_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(evidence_json) AND json_type(evidence_json) = 'array'),
  -- For a new guideline: [{kind: employee|project, id}], what the proposer suggests.
  suggested_links_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(suggested_links_json) AND json_type(suggested_links_json) = 'array'),
  status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected','withdrawn')),
  decision_note TEXT CHECK(decision_note IS NULL OR length(decision_note) <= 2000),
  decided_by_user TEXT REFERENCES users(id),
  origin_conversation_id TEXT,
  run_id TEXT,
  created_at_ms INTEGER NOT NULL,
  decided_at_ms INTEGER,
  CHECK((guideline_id IS NULL) = (base_version IS NULL)),
  CHECK((status = 'pending') = (decided_at_ms IS NULL)),
  CHECK(status NOT IN ('approved','rejected') OR decided_by_user IS NOT NULL),
  CHECK(status IN ('approved','rejected') OR decided_by_user IS NULL)
) STRICT;
-- The inbox (pending, oldest first), "1 open per guideline", "2 open new per employee".
CREATE INDEX guideline_proposals_status ON guideline_proposals(status, created_at_ms);
CREATE INDEX guideline_proposals_guideline ON guideline_proposals(guideline_id, status);
CREATE INDEX guideline_proposals_proposer ON guideline_proposals(proposed_by, status);

-- Every version is complete (title and text) and append-only, like the vault's
-- audit. The text is inline: at most 8,000 characters (B4; length() counts
-- characters, like chars().count()). A restore copies an older version as the
-- newest (`restored_from`); an approved proposal names its proposal. Only the
-- seed has no board member.
CREATE TABLE guideline_versions (
  guideline_id TEXT NOT NULL REFERENCES guidelines(id),
  version INTEGER NOT NULL CHECK(version > 0),
  title TEXT NOT NULL CHECK(length(trim(title)) > 0 AND length(title) <= 120),
  text TEXT NOT NULL CHECK(length(trim(text)) > 0 AND length(text) BETWEEN 1 AND 8000),
  source TEXT NOT NULL CHECK(source IN ('seed','created','edited','proposal','restored')),
  proposal_id TEXT REFERENCES guideline_proposals(id),
  restored_from INTEGER CHECK(restored_from IS NULL OR restored_from > 0),
  by_user TEXT REFERENCES users(id),
  created_at_ms INTEGER NOT NULL,
  PRIMARY KEY (guideline_id, version),
  CHECK((source = 'proposal') = (proposal_id IS NOT NULL)),
  CHECK((source = 'restored') = (restored_from IS NOT NULL)),
  CHECK(source = 'seed' OR by_user IS NOT NULL)
) STRICT;

CREATE TRIGGER guideline_versions_no_update BEFORE UPDATE ON guideline_versions BEGIN
  SELECT RAISE(ABORT, 'guideline_versions is append-only');
END;
CREATE TRIGGER guideline_versions_no_delete BEFORE DELETE ON guideline_versions BEGIN
  SELECT RAISE(ABORT, 'guideline_versions is append-only');
END;

-- A guideline holds for an employee or for a project, never a combination
-- (B2). Links have no versions: unlink deletes the row.
CREATE TABLE guideline_links (
  id INTEGER PRIMARY KEY,
  guideline_id TEXT NOT NULL REFERENCES guidelines(id),
  employee_id TEXT REFERENCES employees(id),
  project_id TEXT REFERENCES projects(id),
  -- NULL: the seed.
  created_by_user TEXT REFERENCES users(id),
  created_at_ms INTEGER NOT NULL,
  CHECK((employee_id IS NULL) <> (project_id IS NULL))
) STRICT;
CREATE UNIQUE INDEX guideline_links_employee ON guideline_links(guideline_id, employee_id) WHERE employee_id IS NOT NULL;
CREATE UNIQUE INDEX guideline_links_project ON guideline_links(guideline_id, project_id) WHERE project_id IS NOT NULL;
-- A run's set: the guidelines of its employee and of its project.
CREATE INDEX guideline_links_by_employee ON guideline_links(employee_id) WHERE employee_id IS NOT NULL;
CREATE INDEX guideline_links_by_project ON guideline_links(project_id) WHERE project_id IS NOT NULL;

-- What a run had in its system text (B6), a few rows per run, shown in its
-- trace. No foreign key on the version: the snapshot is a copy (title, level,
-- size) and never blocks anything. A run that is deleted takes its rows along.
CREATE TABLE run_guidelines (
  run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  guideline_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK(version > 0),
  title TEXT NOT NULL,
  level TEXT NOT NULL CHECK(level IN ('employee','project')),
  chars INTEGER NOT NULL CHECK(chars >= 0),
  PRIMARY KEY (run_id, guideline_id)
) STRICT;
