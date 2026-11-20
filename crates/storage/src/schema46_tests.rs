//! Migration 46: work sessions in the History. A schema 45 store keeps its sessions
//! as they were, without a summary ("No summary yet"), and the new columns refuse
//! what their CHECKs do not allow.

use crate::{Store, work_sessions};
use aurelius_domain::history::HistorySummaryState;
use rusqlite::Connection;

const P: &str = "00000000-0000-7000-8000-000000000461";
const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000462";
const SESSION: &str = "00000000-0000-7000-8000-000000000463";
const USER: &str = "00000000-0000-7000-8000-000000000464";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

/// A schema 45 store with Orion working in one session of project Alpha, started by Alex.
fn schema_45_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 45).unwrap());
    let conn = db(dir);
    assert!(conn.prepare("SELECT summary_text FROM work_sessions").is_err(), "new in 46");
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}'), ('es', 'employee', '{global}', '{EMPLOYEE}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES ('{EMPLOYEE}', 'es', 'Orion', 'PM', '{{}}', 'active', 1, 1, 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{USER}', '{company}', 'alex', '$argon2id$x', 1, 1);
         INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES ('{SESSION}', 'session', '{P}', '{EMPLOYEE}', 'Fix the login', 1, 10);
         INSERT INTO work_sessions(conversation_id, started_by, worktree, branch, created_at_ms, dispatch) VALUES ('{SESSION}', '{USER}', '/w/session-x', 'session/x', 10, NULL);"
    ))
    .unwrap();
}

#[test]
fn a_schema_45_session_keeps_its_row_and_shows_without_a_summary() {
    let dir = tempfile::tempdir().unwrap();
    schema_45_database(dir.path());

    let store = Store::open(dir.path()).unwrap();
    let row: (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<i64>) = store
        .read(|c| {
            Ok(c.query_row(
                "SELECT started_by, branch, dispatch, summary_state, summary_text, summary_through_ms FROM work_sessions WHERE conversation_id = ?1",
                [SESSION],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )?)
        })
        .unwrap();
    assert_eq!(row, (USER.to_owned(), Some("session/x".to_owned()), None, None, None, None));
    let (entries, more) = store.read(|c| work_sessions::history(c, &Default::default(), 10, "session.summarize")).unwrap();
    assert!(!more);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].title, "Fix the login");
    assert_eq!(entries[0].started_by_name.as_deref(), Some("alex"));
    assert_eq!(entries[0].summary_state, HistorySummaryState::BeforeHistory);
}

#[test]
fn the_summary_columns_refuse_what_their_checks_do_not_allow() {
    let dir = tempfile::tempdir().unwrap();
    schema_45_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    for ok in ["pending", "failed"] {
        store.write(move |tx| Ok(tx.execute("UPDATE work_sessions SET summary_state = ?1", [ok])?)).unwrap();
    }
    store.write(|tx| Ok(tx.execute("UPDATE work_sessions SET summary_state = 'ready', summary_text = 'It works.', summary_through_ms = 12", [])?)).unwrap();
    let wrong_state = store.write(|tx| Ok(tx.execute("UPDATE work_sessions SET summary_state = 'done'", [])?));
    assert!(wrong_state.is_err());
    let text_not_ready = store.write(|tx| Ok(tx.execute("UPDATE work_sessions SET summary_state = 'pending', summary_text = 'x'", [])?));
    assert!(text_not_ready.is_err(), "a text only with ready");
    let empty = store.write(|tx| Ok(tx.execute("UPDATE work_sessions SET summary_state = 'ready', summary_text = ''", [])?));
    assert!(empty.is_err());
    let long = "a".repeat(801);
    let too_long = store.write(move |tx| Ok(tx.execute("UPDATE work_sessions SET summary_state = 'ready', summary_text = ?1", [long])?));
    assert!(too_long.is_err());
    let negative = store.write(|tx| Ok(tx.execute("UPDATE work_sessions SET summary_through_ms = -1", [])?));
    assert!(negative.is_err());
}
