//! Migration 47: a session's title made from its content. A schema 46 session keeps
//! its title and has not asked for a made one yet, so it asks after its next answer.

use crate::{Store, work_sessions};
use aurelius_domain::ConversationId;
use rusqlite::Connection;

const P: &str = "00000000-0000-7000-8000-000000000471";
const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000472";
const SESSION: &str = "00000000-0000-7000-8000-000000000473";
const USER: &str = "00000000-0000-7000-8000-000000000474";

/// A schema 46 store with Orion working in one session of project Alpha, started by Alex.
fn schema_46_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 46).unwrap());
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    assert!(conn.prepare("SELECT title_asked_at_ms FROM work_sessions").is_err(), "new in 47");
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}'), ('es', 'employee', '{global}', '{EMPLOYEE}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES ('{EMPLOYEE}', 'es', 'Orion', 'PM', '{{}}', 'active', 1, 1, 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{USER}', '{company}', 'alex', '$argon2id$x', 1, 1);
         INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES ('{SESSION}', 'session', '{P}', '{EMPLOYEE}', 'kan je even kijken naar', 1, 10);
         INSERT INTO work_sessions(conversation_id, started_by, worktree, branch, created_at_ms, dispatch, summary_state) VALUES ('{SESSION}', '{USER}', NULL, NULL, 10, NULL, 'pending');"
    ))
    .unwrap();
}

#[test]
fn a_schema_46_session_keeps_its_title_and_asks_for_a_made_one_once() {
    let dir = tempfile::tempdir().unwrap();
    schema_46_database(dir.path());

    let store = Store::open(dir.path()).unwrap();
    let id = ConversationId::parse(SESSION).unwrap();
    let row: (String, Option<i64>, Option<String>) = store
        .read(|c| {
            Ok(c.query_row(
                "SELECT c.title, s.title_asked_at_ms, s.summary_state FROM work_sessions s JOIN conversations c ON c.id = s.conversation_id WHERE s.conversation_id = ?1",
                [SESSION],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .unwrap();
    assert_eq!(row, ("kan je even kijken naar".to_owned(), None, Some("pending".to_owned())));

    assert!(store.write(move |tx| work_sessions::mark_title_asked(tx, id)).unwrap(), "the first ask");
    assert!(!store.write(move |tx| work_sessions::mark_title_asked(tx, id)).unwrap(), "asked once");
    assert!(store.write(move |tx| work_sessions::set_title(tx, id, "Login fix")).unwrap());
    assert!(!store.write(move |tx| work_sessions::set_title(tx, id, "Login fix")).unwrap(), "the same title changes nothing");
}

#[test]
fn the_title_column_refuses_a_negative_moment() {
    let dir = tempfile::tempdir().unwrap();
    schema_46_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let negative = store.write(|tx| Ok(tx.execute("UPDATE work_sessions SET title_asked_at_ms = -1", [])?));
    assert!(negative.is_err());
}

#[test]
fn a_deleted_session_gets_no_made_title() {
    let dir = tempfile::tempdir().unwrap();
    schema_46_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    store.write(|tx| Ok(tx.execute("UPDATE conversations SET archived_at_ms = 20, title = '' WHERE id = ?1", [SESSION])?)).unwrap();
    let id = ConversationId::parse(SESSION).unwrap();
    assert!(!store.write(move |tx| work_sessions::set_title(tx, id, "Login fix")).unwrap());
}
