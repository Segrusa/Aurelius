-- Task variants (Harness² C, opt-in): a task tried in 2 or 3 ways first, each in
-- a throwaway git worktree, compared by a judge; the chosen diff goes into the
-- work folder. One set per task, one row per variant.
CREATE TABLE task_variant_sets (
  task_id TEXT PRIMARY KEY REFERENCES tasks(id),
  phase TEXT NOT NULL CHECK(phase IN ('proposing','probing','judging','ready','applying','done','stopped','failed')),
  count INTEGER NOT NULL CHECK(count BETWEEN 2 AND 3),
  -- The real work folder (a git repository) and the commit the variants start from.
  repo TEXT,
  base_commit TEXT,
  judge_pick INTEGER,
  compose TEXT,
  chosen INTEGER,
  chosen_by TEXT CHECK(chosen_by IS NULL OR chosen_by IN ('user','judge')),
  error TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE task_variants (
  task_id TEXT NOT NULL REFERENCES task_variant_sets(task_id),
  n INTEGER NOT NULL CHECK(n BETWEEN 1 AND 3),
  hint TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('waiting','running','done','failed','cancelled')),
  -- The variant's own conversation and its run, and its worktree (gone once it is cleaned up).
  conversation_id TEXT REFERENCES conversations(id),
  run_id TEXT REFERENCES runs(id),
  worktree TEXT,
  -- The outcome, collected after the run: diff (blob), its size, the closing answer,
  -- failed tool calls and the last test output.
  collected INTEGER NOT NULL DEFAULT 0 CHECK(collected IN (0,1)),
  files INTEGER NOT NULL DEFAULT 0,
  insertions INTEGER NOT NULL DEFAULT 0,
  deletions INTEGER NOT NULL DEFAULT 0,
  diff_blob_id TEXT REFERENCES blobs(id),
  diff_truncated INTEGER NOT NULL DEFAULT 0 CHECK(diff_truncated IN (0,1)),
  report_blob_id TEXT REFERENCES blobs(id),
  errors INTEGER NOT NULL DEFAULT 0,
  tests TEXT,
  rank INTEGER,
  judge_note TEXT,
  created_at_ms INTEGER NOT NULL,
  ended_at_ms INTEGER,
  PRIMARY KEY(task_id, n)
) STRICT;
CREATE INDEX task_variants_conversation ON task_variants(conversation_id);
