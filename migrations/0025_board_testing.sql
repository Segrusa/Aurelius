-- v0.16 (protocol 5, feature `board_testing`, ticket #5): the Testing column.
-- When the company names a tester, a ticket whose main task reports goes to
-- Testing first: the tester runs the full test suite once in a test sub-task,
-- and its verdict moves the card on (green: Review; red: back to Doing).
--
-- Runs with foreign keys off (Migration::rebuild): board_columns is rebuilt so
-- its CHECK takes 'testing'. Every row is copied with its id, so every
-- tickets.column_id stays valid; nothing is deleted.

CREATE TABLE board_columns_new (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES company(id),
  kind TEXT NOT NULL CHECK(kind IN ('backlog','todo','doing','plan_review','testing','review','done')),
  name TEXT NOT NULL,
  position INTEGER NOT NULL,
  version INTEGER NOT NULL,
  UNIQUE(company_id, kind)
) STRICT;
INSERT INTO board_columns_new(id, company_id, kind, name, position, version)
  SELECT id, company_id, kind, name, position, version FROM board_columns;
DROP TABLE board_columns;
ALTER TABLE board_columns_new RENAME TO board_columns;

-- Testing goes just before Review (between Plan review and Review on a board in
-- its first order); Review and every column after it move one place right. The
-- board version is its highest column version, so the moved columns count up.
UPDATE board_columns SET position = position + 1, version = version + 1
  WHERE position >= (SELECT position FROM board_columns WHERE kind = 'review');
INSERT INTO board_columns(id, company_id, kind, name, position, version)
  SELECT lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-'
         || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6))),
         c.id, 'testing', 'Testing', (SELECT position - 1 FROM board_columns WHERE kind = 'review'),
         (SELECT MAX(version) FROM board_columns)
  FROM company c;

-- The company's tester (NULL: none, no Testing step; cards go from Doing to Review as before).
ALTER TABLE company ADD COLUMN tester_employee_id TEXT REFERENCES employees(id);

-- The ticket's latest test sub-task (made by the engine when the card went to
-- Testing). Only that task's run may give the verdict, and only while it is open.
ALTER TABLE tickets ADD COLUMN test_task_id TEXT REFERENCES tasks(id);
