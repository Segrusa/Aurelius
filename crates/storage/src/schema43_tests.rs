//! Migration 43: work sessions. The conversations table is rebuilt for the kind
//! `session`; a schema 42 store keeps every conversation as it was.

use crate::Store;
use rusqlite::Connection;

const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000431";
const CHAT: &str = "00000000-0000-7000-8000-000000000432";
const BOARD: &str = "00000000-0000-7000-8000-000000000433";
const USER: &str = "00000000-0000-7000-8000-000000000434";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

#[test]
fn a_schema_42_store_keeps_its_conversations_and_takes_sessions() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 42).unwrap());
    let conn = db(dir.path());
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es', 'employee', ?1, ?2)", [&global, EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES (?1, 'es', 'Orion', 'PM', '{}', 'active', 1, 1, 1)", [EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO conversations(id, kind, employee_id, title, version, created_at_ms, agent_mode, board_member_id) VALUES (?1, 'employee', ?2, 'Orion', 4, 7, 'auto', NULL)", [CHAT, EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO conversations(id, kind, employee_id, title, version, created_at_ms, board_member_id) VALUES (?1, 'employee', ?2, 'Orion', 1, 8, ?3)", [BOARD, EMPLOYEE, USER]).unwrap();
    assert!(conn.prepare("SELECT conversation_id FROM work_sessions").is_err(), "new in 43");
    drop(conn);

    let store = Store::open(dir.path()).unwrap();
    let kept: (String, i64, String, i64) = store
        .read(|c| Ok(c.query_row("SELECT kind, version, agent_mode, created_at_ms FROM conversations WHERE id = ?1", [CHAT], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?))
        .unwrap();
    assert_eq!(kept, ("employee".into(), 4, "auto".into(), 7));
    let board: Option<String> = store.read(|c| Ok(c.query_row("SELECT board_member_id FROM conversations WHERE id = ?1", [BOARD], |r| r.get(0))?)).unwrap();
    assert_eq!(board.as_deref(), Some(USER), "the board member's chat stays theirs");
    // One chat per board member per employee still holds after the rebuild.
    let twice = store.write(|tx| Ok(tx.execute("INSERT INTO conversations(id, kind, employee_id, title, version, created_at_ms, board_member_id) VALUES ('x', 'employee', ?1, 'Orion', 1, 9, ?2)", [EMPLOYEE, USER])?));
    assert!(twice.is_err());
    // A session needs a project and an employee.
    let without_project = store.write(|tx| Ok(tx.execute("INSERT INTO conversations(id, kind, employee_id, title, version, created_at_ms) VALUES ('s', 'session', ?1, 'S', 1, 9)", [EMPLOYEE])?));
    assert!(without_project.is_err());
}
