-- Instruction proposals (Harness² B): a manager proposes better instructions for
-- itself or a direct report, the user decides. A proposal row is now either a
-- colleague (as before) or new instructions for `target_manager_id`, made on
-- version `base_version` of its instructions, with the tasks and memories it
-- rests on in `evidence_json` ([{kind, id, label}]).
ALTER TABLE manager_proposals ADD COLUMN kind TEXT NOT NULL DEFAULT 'colleague' CHECK(kind IN ('colleague','instructions'));
ALTER TABLE manager_proposals ADD COLUMN target_manager_id TEXT REFERENCES managers(id);
ALTER TABLE manager_proposals ADD COLUMN base_version INTEGER;
ALTER TABLE manager_proposals ADD COLUMN evidence_json TEXT CHECK(evidence_json IS NULL OR json_valid(evidence_json));
CREATE INDEX manager_proposals_target ON manager_proposals(target_manager_id, status);

-- Every version of a manager's instructions, so a change can be seen and put
-- back. `source`: initial (as it was when versions began), created, edited,
-- proposal (an approved proposal, `proposal_id`), restored (`restored_from`).
-- A NULL blob: the manager had no instructions.
CREATE TABLE manager_instruction_versions (
  manager_id TEXT NOT NULL REFERENCES managers(id),
  version INTEGER NOT NULL CHECK(version > 0),
  instructions_blob_id TEXT REFERENCES blobs(id),
  source TEXT NOT NULL CHECK(source IN ('initial','created','edited','proposal','restored')),
  proposal_id TEXT REFERENCES manager_proposals(id),
  restored_from INTEGER,
  created_at_ms INTEGER NOT NULL,
  PRIMARY KEY(manager_id, version)
) STRICT;

-- The managers there are now start at version 1 with their current instructions.
INSERT INTO manager_instruction_versions(manager_id, version, instructions_blob_id, source, created_at_ms)
  SELECT id, 1, instructions_blob_id, 'initial', updated_at_ms FROM managers;
