//! Migration 23: guidelines. A phase 4 database keeps every row and gains five
//! empty, STRICT tables; the constraints hold in the database itself.

use crate::{Store, migrate};
use rusqlite::{Connection, params};

/// 0022 was released with phase 4: its bytes never change.
const RELEASED: [(i64, &str); 1] = [(22, "18b6441dcbc0ede8fccab00d49767b542a6a01927f7796ada8d3f25abe3aeb04")];

const P: &str = "00000000-0000-7000-8000-000000000001";
const B: &str = "00000000-0000-7000-8000-000000000002";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const E1: &str = "00000000-0000-7000-8000-000000000004";
const TASK: &str = "00000000-0000-7000-8000-000000000021";
const RUN: &str = "00000000-0000-7000-8000-000000000031";
const G: &str = "00000000-0000-7000-8000-000000000041";

const TABLES: [&str; 5] = ["guideline_links", "guideline_proposals", "guideline_versions", "guidelines", "run_guidelines"];

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn strings(conn: &Connection, sql: &str) -> Vec<String> {
    conn.prepare(sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

#[test]
fn phase_4_migrations_are_unchanged_and_23_is_additive() {
    for (version, sum) in RELEASED {
        let m = migrate::MIGRATIONS.iter().find(|m| m.version == version).unwrap();
        assert_eq!(migrate::checksum(m.sql), sum, "migration {version} changed");
    }
    let last = migrate::MIGRATIONS.last().unwrap();
    assert_eq!((last.version, last.rebuild, last.then.is_some()), (23, false, false));
}

#[test]
fn migration_23_has_no_seed_content() {
    let sql = migrate::MIGRATIONS.iter().find(|m| m.version == 23).unwrap().sql.to_lowercase();
    // Only DDL: no row is written (B7: the starter set is a seed step).
    assert!(!sql.contains("insert"), "0023 writes rows");
    for title in ["rust style", "code review", "layers and boundaries"] {
        assert!(!sql.contains(title), "0023 holds seed text: {title}");
    }
}

/// A schema 22 database with a board member, a project, an employee with a
/// task and its run, and a vault item.
fn phase_4_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 22).unwrap());
    let conn = db(dir);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es', 'employee', '{global}', '{E1}');
         INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms)
           VALUES ('{E1}', 'es', 'Jordan', 'Dev', '{{}}', 'active', 1, 1, 1);
         INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES ('{B}', 'k', 's', 0, 'text/plain', 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms, password_changed_at_ms)
           VALUES ('{ALEX}', '{company}', 'Alex', '$argon2id$x', 1, 1, 5);
         INSERT INTO tasks(id, project_id, employee_id, title, brief_blob_id, status, version, created_at_ms, updated_at_ms)
           VALUES ('{TASK}', '{P}', '{E1}', 'Old work', '{B}', 'succeeded', 3, 5, 6);
         INSERT INTO runs(id, task_id, attempt, runtime_kind, model_id, resolved_config_json, status)
           VALUES ('{RUN}', '{TASK}', 1, 'claude_code', 'm', '{{}}', 'succeeded');
         INSERT INTO vault_items(id, company_id, kind, name, env_name, ciphertext, nonce, key_id, version, created_by_user, updated_by_user, created_at_ms, updated_at_ms)
           VALUES ('v1', '{company}', 'secret', 'Deploy key', 'DEPLOY_KEY', x'01', x'00', 'k1', 1, '{ALEX}', '{ALEX}', 1, 1);"
    ))
    .unwrap();
}

/// Every row of the tables phase 4 data lives in, as text.
fn snapshot(conn: &Connection) -> Vec<String> {
    let mut all = Vec::new();
    for table in ["users", "projects", "employees", "tasks", "runs", "vault_items", "settings", "company"] {
        let columns = strings(conn, &format!("SELECT name FROM pragma_table_info('{table}') ORDER BY cid"));
        let row = columns.iter().map(|c| format!("COALESCE(quote(\"{c}\"), 'NULL')")).collect::<Vec<_>>().join(" || '|' || ");
        all.extend(strings(conn, &format!("SELECT '{table}:' || {row} FROM {table} ORDER BY 1")));
    }
    all
}

#[test]
fn a_phase_4_database_migrates_without_losing_anything() {
    let dir = tempfile::tempdir().unwrap();
    phase_4_database(dir.path());
    let before = snapshot(&db(dir.path()));
    assert!(before.iter().any(|r| r.starts_with("vault_items:")) && before.iter().any(|r| r.starts_with("runs:")));

    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    assert_eq!(conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get::<_, i64>(0)).unwrap(), 23);
    assert!(strings(&conn, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());
    assert_eq!(snapshot(&conn), before);
    for table in TABLES {
        assert_eq!(count(&conn, table), 0, "{table}");
        let strict: i64 = conn.query_row("SELECT strict FROM pragma_table_list WHERE name = ?1", [table], |r| r.get(0)).unwrap();
        assert_eq!(strict, 1, "{table} is STRICT");
    }
    let indexes = strings(
        &conn,
        "SELECT name FROM sqlite_master WHERE type = 'index' AND (name LIKE 'guideline%' OR name LIKE 'run_guidelines%') AND sql IS NOT NULL ORDER BY name",
    );
    assert_eq!(
        indexes,
        ["guideline_links_by_employee", "guideline_links_by_project", "guideline_links_employee", "guideline_links_project",
         "guideline_proposals_guideline", "guideline_proposals_proposer", "guideline_proposals_status", "guidelines_company"]
    );
    let triggers = strings(&conn, "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'guideline%' ORDER BY name");
    assert_eq!(
        triggers,
        ["guideline_versions_no_delete", "guideline_versions_no_update", "guidelines_no_delete", "guidelines_version_forward"]
    );
}

/// A store at 23 with the rows of [`phase_4_database`] and one guideline (v1).
fn with_guideline() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    phase_4_database(dir.path());
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    conn.execute_batch(&format!(
        "INSERT INTO guidelines(id, company_id, current_version, created_by_user, updated_by_user, created_at_ms, updated_at_ms)
           SELECT '{G}', id, 1, '{ALEX}', '{ALEX}', 1, 1 FROM company;
         INSERT INTO guideline_versions(guideline_id, version, title, text, source, by_user, created_at_ms)
           VALUES ('{G}', 1, 'Style', 'Be brief.', 'created', '{ALEX}', 1);"
    ))
    .unwrap();
    (dir, conn)
}

fn version(conn: &Connection, v: i64, title: &str, text: &str, source: &str) -> rusqlite::Result<usize> {
    let proposal: Option<&str> = None;
    let restored: Option<i64> = (source == "restored").then_some(1);
    let by: Option<&str> = (source != "seed").then_some(ALEX);
    conn.execute(
        "INSERT INTO guideline_versions(guideline_id, version, title, text, source, proposal_id, restored_from, by_user, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 2)",
        params![G, v, title, text, source, proposal, restored, by],
    )
}

#[test]
fn versions_are_append_only_and_guidelines_are_never_deleted() {
    let (_dir, conn) = with_guideline();
    let err = conn.execute("UPDATE guideline_versions SET text = 'Changed.'", []).unwrap_err().to_string();
    assert!(err.contains("append-only"), "{err}");
    let err = conn.execute("DELETE FROM guideline_versions", []).unwrap_err().to_string();
    assert!(err.contains("append-only"), "{err}");
    let err = conn.execute("DELETE FROM guidelines", []).unwrap_err().to_string();
    assert!(err.contains("archived, not deleted"), "{err}");
    assert_eq!(count(&conn, "guideline_versions"), 1);

    // The current version moves to the next stored version only.
    assert!(conn.execute("UPDATE guidelines SET current_version = 2", []).is_err(), "version 2 is not stored yet");
    version(&conn, 2, "Style", "Be briefer.", "edited").unwrap();
    version(&conn, 3, "Style", "Be brief.", "restored").unwrap();
    assert!(conn.execute("UPDATE guidelines SET current_version = 3", []).is_err(), "skips a version");
    conn.execute("UPDATE guidelines SET current_version = 2", []).unwrap();
    assert!(conn.execute("UPDATE guidelines SET current_version = 1", []).is_err(), "goes back");
    // Other columns change freely (archive).
    conn.execute("UPDATE guidelines SET archived_at_ms = 5", []).unwrap();
}

#[test]
fn a_version_checks_its_title_text_and_source() {
    let (_dir, conn) = with_guideline();
    let long_title = "t".repeat(120);
    let longest = "é".repeat(8000);
    // Limits count characters, not bytes: 8,000 two-byte characters fit.
    version(&conn, 2, &long_title, &longest, "edited").unwrap();
    assert!(version(&conn, 3, &"t".repeat(121), "x", "edited").is_err(), "title over 120");
    assert!(version(&conn, 3, "", "x", "edited").is_err(), "empty title");
    assert!(version(&conn, 3, "  ", "x", "edited").is_err(), "blank title");
    assert!(version(&conn, 3, "T", &"x".repeat(8001), "edited").is_err(), "text over 8,000");
    assert!(version(&conn, 3, "T", "", "edited").is_err(), "empty text");
    assert!(version(&conn, 3, "T", "x", "imported").is_err(), "unknown source");
    assert!(version(&conn, 0, "T", "x", "edited").is_err(), "version 0");
    assert!(version(&conn, 2, "T", "x", "edited").is_err(), "version 2 twice");
    // A proposal names its proposal, a restore its version; only the seed has no user.
    assert!(version(&conn, 3, "T", "x", "proposal").is_err(), "proposal without proposal_id");
    assert!(
        conn.execute("INSERT INTO guideline_versions(guideline_id, version, title, text, source, by_user, created_at_ms) VALUES (?1, 3, 'T', 'x', 'edited', NULL, 2)", [G]).is_err(),
        "an edit without a user"
    );
    version(&conn, 3, "T", "x", "seed").unwrap();
    for source in ["created", "edited", "restored"] {
        let v: i64 = conn.query_row("SELECT MAX(version) + 1 FROM guideline_versions", [], |r| r.get(0)).unwrap();
        version(&conn, v, "T", "x", source).unwrap();
    }
}

#[test]
fn a_link_has_exactly_one_target_once() {
    let (_dir, conn) = with_guideline();
    let link = |employee: Option<&str>, project: Option<&str>| {
        conn.execute(
            "INSERT INTO guideline_links(guideline_id, employee_id, project_id, created_at_ms) VALUES (?1, ?2, ?3, 1)",
            params![G, employee, project],
        )
    };
    link(Some(E1), None).unwrap();
    link(None, Some(P)).unwrap();
    assert!(link(Some(E1), Some(P)).is_err(), "both");
    assert!(link(None, None).is_err(), "neither");
    assert!(link(Some(E1), None).is_err(), "the employee twice");
    assert!(link(None, Some(P)).is_err(), "the project twice");
    assert!(link(Some("00000000-0000-7000-8000-0000000000ff"), None).is_err(), "an unknown employee");
}

#[test]
fn a_proposal_checks_its_lengths_json_and_decision() {
    let (_dir, conn) = with_guideline();
    let insert = |id: &str, guideline: Option<&str>, base: Option<i64>, text: &str, reason: &str, evidence: &str| {
        conn.execute(
            "INSERT INTO guideline_proposals(id, guideline_id, base_version, proposed_by, title, text, reason, evidence_json, status, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, 'Style', ?5, ?6, ?7, 'pending', 1)",
            params![id, guideline, base, E1, text, reason, evidence],
        )
    };
    insert("p1", Some(G), Some(1), &"x".repeat(8000), &"r".repeat(2000), "[]").unwrap();
    insert("p2", None, None, "x", "r", r#"[{"kind":"message","id":"m1","label":"a"}]"#).unwrap();
    assert!(insert("p3", Some(G), Some(1), &"x".repeat(8001), "r", "[]").is_err(), "text over 8,000");
    assert!(insert("p3", Some(G), Some(1), "x", &"r".repeat(2001), "[]").is_err(), "reason over 2,000");
    assert!(insert("p3", Some(G), Some(1), "x", "r", "not json").is_err(), "evidence not json");
    assert!(insert("p3", Some(G), Some(1), "x", "r", "{}").is_err(), "evidence not a list");
    assert!(insert("p3", Some(G), None, "x", "r", "[]").is_err(), "an existing guideline needs its base version");
    assert!(insert("p3", None, Some(1), "x", "r", "[]").is_err(), "a new one has none");

    assert!(conn.execute("UPDATE guideline_proposals SET status = 'approved', decided_at_ms = 2 WHERE id = 'p1'", []).is_err(), "approved without a board member");
    assert!(conn.execute("UPDATE guideline_proposals SET status = 'rejected', decided_by_user = ?1 WHERE id = 'p1'", [ALEX]).is_err(), "decided without a time");
    assert!(conn.execute("UPDATE guideline_proposals SET status = 'maybe' WHERE id = 'p1'", []).is_err());
    conn.execute("UPDATE guideline_proposals SET status = 'approved', decided_by_user = ?1, decided_at_ms = 2 WHERE id = 'p1'", [ALEX]).unwrap();
    conn.execute("UPDATE guideline_proposals SET status = 'withdrawn', decided_at_ms = 2 WHERE id = 'p2'", []).unwrap();
}

#[test]
fn a_run_snapshot_checks_its_level_and_goes_with_its_run() {
    let (_dir, conn) = with_guideline();
    let insert = |guideline: &str, level: &str| {
        conn.execute(
            "INSERT INTO run_guidelines(run_id, guideline_id, version, title, level, chars) VALUES (?1, ?2, 7, 'Style', ?3, 9)",
            params![RUN, guideline, level],
        )
    };
    // No foreign key on the version (7 does not exist) or on the guideline.
    insert(G, "employee").unwrap();
    insert("g-gone", "project").unwrap();
    assert!(insert("g-other", "company").is_err(), "unknown level");
    assert!(insert(G, "project").is_err(), "a guideline twice in one run");
    assert!(
        conn.execute("INSERT INTO run_guidelines(run_id, guideline_id, version, title, level, chars) VALUES ('no-run', ?1, 1, 'S', 'employee', 1)", [G]).is_err(),
        "the run must exist"
    );
    // Deleting a run takes its snapshot along instead of blocking.
    conn.execute("DELETE FROM runs WHERE id = ?1", [RUN]).unwrap();
    assert_eq!(count(&conn, "run_guidelines"), 0);
}
