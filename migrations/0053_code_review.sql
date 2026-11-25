-- Protocol 5 (additive, feature `code_review`, ticket #84): reviewing a branch in the
-- Code tab as a pull request is reviewed.
--
-- * code_review_threads: a comment thread on one line of a branch's diff. `snippet` keeps
--   the diff lines it is about (a JSON array of strings), so it still shows its code after
--   the line changes. `resolved` folds it.
-- * code_reviews: a board member finished their review of a branch at a head, with a
--   verdict. Append-only: a later review is a new row.
-- * code_review_comments: the comments of a thread. `review_id` NULL: pending, only its
--   writer sees it until they finish their review.
-- * code_viewed_files: the files a board member marked viewed, per branch head.
--
-- Additive only: nothing is rebuilt, changed or deleted. 0052 is taken on another branch.
CREATE TABLE code_review_threads (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  branch TEXT NOT NULL CHECK(length(branch) BETWEEN 1 AND 255),
  path TEXT NOT NULL CHECK(length(path) BETWEEN 1 AND 1024),
  line INTEGER NOT NULL CHECK(line BETWEEN 1 AND 10000000),
  side TEXT NOT NULL CHECK(side IN ('new', 'old')),
  commit_sha TEXT NOT NULL CHECK(length(commit_sha) BETWEEN 7 AND 64),
  snippet TEXT NOT NULL CHECK(length(snippet) <= 4000 AND json_valid(snippet)),
  resolved INTEGER NOT NULL DEFAULT 0 CHECK(resolved IN (0, 1)),
  created_by TEXT NOT NULL REFERENCES users(id),
  created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX code_review_threads_branch ON code_review_threads(project_id, branch, created_at_ms);

CREATE TABLE code_reviews (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  branch TEXT NOT NULL CHECK(length(branch) BETWEEN 1 AND 255),
  head TEXT NOT NULL CHECK(length(head) BETWEEN 7 AND 64),
  user_id TEXT NOT NULL REFERENCES users(id),
  verdict TEXT NOT NULL CHECK(verdict IN ('comment', 'approved', 'changes')),
  body TEXT NOT NULL CHECK(length(body) <= 8000),
  created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX code_reviews_branch ON code_reviews(project_id, branch, created_at_ms);
CREATE TRIGGER code_reviews_no_update BEFORE UPDATE ON code_reviews BEGIN
  SELECT RAISE(ABORT, 'code_reviews is append-only');
END;
CREATE TRIGGER code_reviews_no_delete BEFORE DELETE ON code_reviews BEGIN
  SELECT RAISE(ABORT, 'code_reviews is append-only');
END;

CREATE TABLE code_review_comments (
  id TEXT PRIMARY KEY,
  thread_id TEXT NOT NULL REFERENCES code_review_threads(id),
  user_id TEXT NOT NULL REFERENCES users(id),
  body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 8000),
  review_id TEXT REFERENCES code_reviews(id),
  created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX code_review_comments_thread ON code_review_comments(thread_id, created_at_ms);
CREATE INDEX code_review_comments_pending ON code_review_comments(user_id) WHERE review_id IS NULL;

CREATE TABLE code_viewed_files (
  user_id TEXT NOT NULL REFERENCES users(id),
  project_id TEXT NOT NULL REFERENCES projects(id),
  branch TEXT NOT NULL CHECK(length(branch) BETWEEN 1 AND 255),
  head TEXT NOT NULL CHECK(length(head) BETWEEN 7 AND 64),
  path TEXT NOT NULL CHECK(length(path) BETWEEN 1 AND 1024),
  PRIMARY KEY(user_id, project_id, branch, head, path)
) STRICT, WITHOUT ROWID;
