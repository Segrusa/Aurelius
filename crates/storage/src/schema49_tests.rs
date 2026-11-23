//! Migration 49: a work session can work on its own branch in the project's folder. A
//! schema 48 session has no branch of its own, and the column takes nothing but 0 or 1.

use crate::{Store, work_sessions};
use aurelius_domain::{ConversationId, UserId};
use rusqlite::Connection;

const P: &str = "00000000-0000-7000-8000-000000000491";
const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000492";
const SESSION: &str = "00000000-0000-7000-8000-000000000493";
const USER: &str = "00000000-0000-7000-8000-000000000494";

/// A schema 48 store with Orion working in a worktree session of project Alpha, started by Alex.
fn schema_48_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 48).unwrap());
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    assert!(conn.prepare("SELECT own_branch FROM work_sessions").is_err(), "new in 49");
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}'), ('es', 'employee', '{global}', '{EMPLOYEE}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES ('{EMPLOYEE}', 'es', 'Orion', 'PM', '{{}}', 'active', 1, 1, 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{USER}', '{company}', 'alex', '$argon2id$x', 1, 1);
         INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES ('{SESSION}', 'session', '{P}', '{EMPLOYEE}', 'Login fix', 1, 10);
         INSERT INTO work_sessions(conversation_id, started_by, worktree, branch, created_at_ms, dispatch, design) VALUES ('{SESSION}', '{USER}', '/data/worktrees/session-00000493', 'session/00000493', 10, NULL, 0);"
    ))
    .unwrap();
}

#[test]
fn a_schema_48_session_keeps_its_worktree_and_has_no_branch_of_its_own_after_the_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    schema_48_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let id = ConversationId::parse(SESSION).unwrap();
    let session = store.read(|c| work_sessions::of_conversation(c, id)).unwrap().unwrap();
    assert!(!session.own_branch);
    assert_eq!(session.worktree, Some(("/data/worktrees/session-00000493".to_owned(), "session/00000493".to_owned())));
    let listed = store.read(|c| work_sessions::list(c, UserId::parse(USER).unwrap(), None)).unwrap();
    assert_eq!(listed.iter().map(|s| (s.title.as_str(), s.own_branch)).collect::<Vec<_>>(), [("Login fix", false)]);
}

#[test]
fn the_own_branch_column_takes_only_zero_or_one() {
    let dir = tempfile::tempdir().unwrap();
    schema_48_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let refused = store.write(|tx| Ok(tx.execute("UPDATE work_sessions SET own_branch = 2", [])?));
    assert!(refused.is_err(), "{refused:?}");
}
