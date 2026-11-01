-- v0.16 (protocol 5, additive): the server's name (docs/product/servernaam,
-- SN-01). One name per server (A-01), chosen by a board member; without a row,
-- or with a NULL name, clients see the computer's name (A-03). The history is
-- the `server.renamed` events (OV-03: the last one wins, no version check).
--
-- Additive only: one new table, no row of an older table changes.

-- At most one row (`id` = 1), made by the first change. `version` counts the
-- changes; the name is 1 to 40 characters, trimmed, without line breaks (A-05).
CREATE TABLE server_name (
  id INTEGER PRIMARY KEY CHECK(id = 1),
  name TEXT CHECK(name IS NULL OR (length(name) BETWEEN 1 AND 40 AND name = trim(name)
                                   AND instr(name, char(10)) = 0 AND instr(name, char(13)) = 0)),
  version INTEGER NOT NULL CHECK(version > 0),
  updated_by_user TEXT REFERENCES users(id),
  updated_at_ms INTEGER NOT NULL
) STRICT;
