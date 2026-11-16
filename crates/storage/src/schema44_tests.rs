//! Migration 44: the dispatcher of a work session. A schema 43 store keeps its
//! sessions as they were (an employee works in each), and the new column and
//! table refuse what their checks do not allow.

use crate::Store;
use rusqlite::Connection;

const P: &str = "00000000-0000-7000-8000-000000000441";
const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000442";
const SESSION: &str = "00000000-0000-7000-8000-000000000443";
const USER: &str = "00000000-0000-7000-8000-000000000444";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

/// A schema 43 store with Orion working in one session of project Alpha.
fn schema_43_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 43).unwrap());
    let conn = db(dir);
    assert!(conn.prepare("SELECT dispatch FROM work_sessions").is_err(), "new in 44");
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}'), ('es', 'employee', '{global}', '{EMPLOYEE}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES ('{EMPLOYEE}', 'es', 'Orion', 'PM', '{{}}', 'active', 1, 1, 1);
         INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES ('{SESSION}', 'session', '{P}', '{EMPLOYEE}', 'Fix the login', 1, 10);
         INSERT INTO work_sessions(conversation_id, started_by, worktree, branch, created_at_ms) VALUES ('{SESSION}', '{USER}', '/w/session-x', 'session/x', 10);"
    ))
    .unwrap();
}

#[test]
fn a_schema_43_session_keeps_its_employee_and_has_no_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    schema_43_database(dir.path());

    let store = Store::open(dir.path()).unwrap();
    let row: (String, Option<String>, Option<String>, Option<String>) = store
        .read(|c| Ok(c.query_row("SELECT started_by, worktree, branch, dispatch FROM work_sessions WHERE conversation_id = ?1", [SESSION], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?))
        .unwrap();
    assert_eq!(row, (USER.to_owned(), Some("/w/session-x".to_owned()), Some("session/x".to_owned()), None));
    let runs: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM session_dispatch_runs", [], |r| r.get(0))?)).unwrap();
    assert_eq!(runs, 0);
}

#[test]
fn the_dispatch_column_takes_only_choosing_or_nobody() {
    let dir = tempfile::tempdir().unwrap();
    schema_43_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    for ok in ["choosing", "nobody"] {
        store.write(move |tx| Ok(tx.execute("UPDATE work_sessions SET dispatch = ?1", [ok])?)).unwrap();
    }
    let wrong = store.write(|tx| Ok(tx.execute("UPDATE work_sessions SET dispatch = 'chosen'", [])?));
    assert!(wrong.is_err());
    // A dispatcher's run row needs a real run.
    let orphan = store.write(|tx| Ok(tx.execute("INSERT INTO session_dispatch_runs(run_id, conversation_id, created_at_ms) VALUES ('r', ?1, 1)", [SESSION])?));
    assert!(orphan.is_err());
}
