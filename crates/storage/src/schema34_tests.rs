//! Migration 34: an employee has an access level (C3). A schema 33 store keeps its
//! employees, all of them on `inherit`.

use crate::{Store, employees, migrate};
use aurelius_domain::employee::AccessLevel;
use rusqlite::Connection;

const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000341";
const CHAT: &str = "00000000-0000-7000-8000-000000000342";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

#[test]
fn a_schema_33_store_keeps_its_employees_and_they_all_inherit() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 33).unwrap());
    let conn = db(dir.path());
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es', 'employee', ?1, ?2)", [&global, EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES (?1, 'es', 'Ann', 'Dev', '{}', 'active', 3, 1, 2)", [EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES (?1, 'employee', NULL, ?2, 'Ann', 1, 1)", [CHAT, EMPLOYEE]).unwrap();
    assert!(conn.prepare("SELECT access_level FROM employees").is_err(), "the column is new in 34");
    drop(conn);

    let store = Store::open(dir.path()).unwrap();
    let listed = store.read(|c| employees::list_employees(c, &|_| None)).unwrap();
    assert_eq!((listed[0].name.as_str(), listed[0].version, listed[0].access_level), ("Ann", 3, AccessLevel::Inherit));

    let (id, chat) = (listed[0].id, listed[0].conversation_id);
    store.write(move |tx| employees::set_access_level(tx, id, AccessLevel::ReadOnly)).unwrap();
    assert_eq!(store.read(|c| employees::list_employees(c, &|_| None)).unwrap()[0].access_level, AccessLevel::ReadOnly);
    assert_eq!(store.read(move |c| employees::access_level_of(c, chat)).unwrap(), AccessLevel::ReadOnly);
    assert!(migrate::supported_version() >= 34);
}

#[test]
fn a_level_this_build_does_not_know_is_not_inherit() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es', 'employee', ?1, ?2)", [&global, EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms, access_level) VALUES (?1, 'es', 'Ann', 'Dev', '{}', 'active', 1, 1, 2, 'from_the_future')", [EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES (?1, 'employee', NULL, ?2, 'Ann', 1, 1)", [CHAT, EMPLOYEE]).unwrap();
    drop(conn);

    let store = Store::open(dir.path()).unwrap();
    let chat = aurelius_domain::ConversationId::parse(CHAT).unwrap();
    assert_eq!(store.read(move |c| employees::access_level_of(c, chat)).unwrap(), AccessLevel::Unknown);
}
