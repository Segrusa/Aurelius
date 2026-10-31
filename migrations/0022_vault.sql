-- v0.14 (protocol 5, phase 4): the vault. Items of the company (project_id
-- NULL) or of one project, of kind text, url or secret. Every value is stored
-- encrypted, text and url too (B1): the database holds only its ciphertext, the
-- nonce and the id of the key; the key itself is never in the database or in a
-- backup. Storage never sees a plaintext value: the daemon encrypts before it
-- writes and decrypts after it reads.
--
-- No value is ever in plaintext and none is in an FTS table: no FTS table or
-- trigger reads a vault table.
--
-- Additive only: three new tables, no row of an older table changes.

-- An item is never deleted: "Delete" archives it, so its audit trail keeps its
-- item. `version` is the version of the value: it goes up by one with every new
-- ciphertext (a "replace"), not with a change of name or description.
CREATE TABLE vault_items (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES company(id),
  -- NULL: a company item, for every project.
  project_id TEXT REFERENCES projects(id),
  kind TEXT NOT NULL CHECK(kind IN ('text','url','secret')),
  name TEXT NOT NULL CHECK(length(trim(name)) > 0 AND length(name) <= 200),
  -- The environment variable a run gets the value in: UPPER_SNAKE (A-Z, 0-9 and
  -- _, not starting with a digit). A secret must have one; a text or url may.
  env_name TEXT CHECK(env_name IS NULL OR (length(env_name) BETWEEN 1 AND 128
                      AND env_name NOT GLOB '*[^A-Z0-9_]*' AND env_name NOT GLOB '[0-9]*')),
  description TEXT,
  ciphertext BLOB NOT NULL CHECK(length(ciphertext) > 0),
  nonce BLOB NOT NULL CHECK(length(nonce) > 0),
  key_id TEXT NOT NULL CHECK(length(key_id) > 0),
  version INTEGER NOT NULL CHECK(version > 0),
  created_by_user TEXT NOT NULL REFERENCES users(id),
  updated_by_user TEXT NOT NULL REFERENCES users(id),
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  archived_at_ms INTEGER,
  CHECK(kind <> 'secret' OR env_name IS NOT NULL)
) STRICT;

-- Names and environment variables are unique per scope among the items that
-- are not archived (an archived name can be used again). '' stands for the
-- company scope, so NULL project_ids collide with each other. A name compares
-- without case ("GitHub token" = "github token").
CREATE UNIQUE INDEX vault_items_name ON vault_items(company_id, COALESCE(project_id, ''), name COLLATE NOCASE)
  WHERE archived_at_ms IS NULL;
CREATE UNIQUE INDEX vault_items_env_name ON vault_items(company_id, COALESCE(project_id, ''), env_name)
  WHERE archived_at_ms IS NULL AND env_name IS NOT NULL;
-- The list per scope, and the run's set (company + its project).
CREATE INDEX vault_items_scope ON vault_items(company_id, project_id, archived_at_ms);
-- Key rotation: every row with one key.
CREATE INDEX vault_items_key ON vault_items(key_id);

-- B7, defence in depth: the kind changes between text and url only. A secret
-- never becomes readable text, and a text never becomes a secret (whose value
-- may already be in a prompt): make a new item.
CREATE TRIGGER vault_items_kind_secret BEFORE UPDATE OF kind ON vault_items
  WHEN OLD.kind <> NEW.kind AND (OLD.kind = 'secret' OR NEW.kind = 'secret')
BEGIN
  SELECT RAISE(ABORT, 'vault item kind cannot change to or from secret');
END;
-- The scope is part of what the value is bound to; an item never moves.
CREATE TRIGGER vault_items_scope_fixed BEFORE UPDATE OF company_id, project_id ON vault_items
  WHEN OLD.company_id IS NOT NEW.company_id OR OLD.project_id IS NOT NEW.project_id
BEGIN
  SELECT RAISE(ABORT, 'vault item scope cannot change');
END;
-- Items are archived, never deleted.
CREATE TRIGGER vault_items_no_delete BEFORE DELETE ON vault_items BEGIN
  SELECT RAISE(ABORT, 'vault items are archived, not deleted');
END;

-- The values an item had before (E16). A new ciphertext moves the old one here,
-- still encrypted under its own key, with the kind and version it was sealed
-- for: the redactor keeps them, since a run or a resumed session may still hold
-- them. A row has its own id: nothing assumes (item, version) is unique, since
-- a restored backup can bring older version numbers back. Rows are not deleted;
-- only a key rotation changes one (its ciphertext, nonce and key id).
CREATE TABLE vault_item_versions (
  id INTEGER PRIMARY KEY,
  item_id TEXT NOT NULL REFERENCES vault_items(id),
  kind TEXT NOT NULL CHECK(kind IN ('text','url','secret')),
  version INTEGER NOT NULL CHECK(version > 0),
  ciphertext BLOB NOT NULL CHECK(length(ciphertext) > 0),
  nonce BLOB NOT NULL CHECK(length(nonce) > 0),
  key_id TEXT NOT NULL CHECK(length(key_id) > 0),
  replaced_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX vault_item_versions_item ON vault_item_versions(item_id, id);
CREATE INDEX vault_item_versions_key ON vault_item_versions(key_id);

CREATE TRIGGER vault_item_versions_fixed BEFORE UPDATE OF item_id, kind, version, replaced_at_ms ON vault_item_versions
  WHEN OLD.item_id IS NOT NEW.item_id OR OLD.kind IS NOT NEW.kind OR OLD.version IS NOT NEW.version
       OR OLD.replaced_at_ms IS NOT NEW.replaced_at_ms
BEGIN
  SELECT RAISE(ABORT, 'a previous vault value only changes its key');
END;
CREATE TRIGGER vault_item_versions_no_delete BEFORE DELETE ON vault_item_versions BEGIN
  SELECT RAISE(ABORT, 'previous vault values are kept');
END;

-- Who read or changed what, append-only like events (0014). Never a value: an
-- edit records only the names of the fields that changed ("name,env_name").
-- A run reads an item once per run (one 'deliver' or 'tool_read' row per item).
-- The ids of the employee, run, task, ticket and session are kept as they were
-- (no foreign key): the audit outlives them and never blocks their cleanup.
CREATE TABLE vault_audit (
  -- The order rows were written in, and the protocol's cursor ("Load older":
  -- before a seq). An INTEGER PRIMARY KEY, so VACUUM never renumbers it.
  seq INTEGER PRIMARY KEY,
  id TEXT NOT NULL UNIQUE,
  -- NULL only for a key export (it concerns no single item).
  item_id TEXT REFERENCES vault_items(id),
  company_id TEXT NOT NULL REFERENCES company(id),
  -- The item's scope (NULL: company), copied so the log filters per scope.
  project_id TEXT,
  action TEXT NOT NULL CHECK(action IN ('create','edit','replace','archive','reveal','copy','deliver','tool_read','key_export')),
  changed_fields TEXT CHECK(changed_fields IS NULL OR (length(changed_fields) BETWEEN 1 AND 200
                            AND changed_fields NOT GLOB '*[^a-z_,]*')),
  -- Who: a board member, or the employee with its run.
  user_id TEXT REFERENCES users(id),
  -- "Via": the board member's session and its device, at the time.
  session_id TEXT,
  via TEXT,
  employee_id TEXT,
  run_id TEXT,
  task_id TEXT,
  ticket_id TEXT,
  -- The version of the item's value that was read or written (E15: which value
  -- a run got). NULL for a key export.
  version INTEGER CHECK(version IS NULL OR version > 0),
  at_ms INTEGER NOT NULL,
  CHECK(action <> 'key_export' OR version IS NULL),
  CHECK(user_id IS NOT NULL OR employee_id IS NOT NULL OR run_id IS NOT NULL),
  CHECK((action = 'key_export') = (item_id IS NULL)),
  CHECK(action <> 'key_export' OR user_id IS NOT NULL)
) STRICT;
-- The log pages newest first by seq; every index ends in it (the rowid), so a
-- list per item, scope, board member or employee walks its index backwards.
CREATE INDEX vault_audit_item ON vault_audit(item_id) WHERE item_id IS NOT NULL;
CREATE INDEX vault_audit_scope ON vault_audit(company_id, project_id);
CREATE INDEX vault_audit_user ON vault_audit(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX vault_audit_employee ON vault_audit(employee_id) WHERE employee_id IS NOT NULL;
-- "Last 30 days", and the cursor by time.
CREATE INDEX vault_audit_at ON vault_audit(at_ms DESC, id DESC);

CREATE TRIGGER vault_audit_no_update BEFORE UPDATE ON vault_audit BEGIN
  SELECT RAISE(ABORT, 'vault_audit is append-only');
END;
CREATE TRIGGER vault_audit_no_delete BEFORE DELETE ON vault_audit BEGIN
  SELECT RAISE(ABORT, 'vault_audit is append-only');
END;
