//! Migration 37: a board member's chat with an employee and the tickets made from it
//! (both gone again with migration 45). A schema 36 store keeps its conversations,
//! which belong to no board member.

use crate::Store;
use rusqlite::Connection;

const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000371";
const MAIN: &str = "00000000-0000-7000-8000-000000000372";
const OLDER: &str = "00000000-0000-7000-8000-000000000373";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn employee(conn: &Connection) {
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es', 'employee', ?1, ?2)", [&global, EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES (?1, 'es', 'Ann', 'Dev', '{}', 'active', 3, 1, 2)", [EMPLOYEE]).unwrap();
    for (id, at) in [(MAIN, 1), (OLDER, 2)] {
        conn.execute("INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES (?1, 'employee', NULL, ?2, 'Ann', 1, ?3)", rusqlite::params![id, EMPLOYEE, at]).unwrap();
    }
}

#[test]
fn a_schema_36_store_keeps_its_conversations_and_they_belong_to_no_board_member() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 36).unwrap());
    let conn = db(dir.path());
    employee(&conn);
    assert!(conn.prepare("SELECT ticket_id FROM chat_tickets").is_err(), "new in 37");
    drop(conn);

    let store = Store::open(dir.path()).unwrap();
    let owners: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM conversations WHERE kind = 'employee' AND board_member_id IS NOT NULL", [], |r| r.get(0))?)).unwrap();
    assert_eq!(owners, 0);
    let kept: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM conversations WHERE kind = 'employee' AND employee_id = ?1", [EMPLOYEE], |r| r.get(0))?)).unwrap();
    assert_eq!(kept, 2, "the main chat and the older one stay");
}
