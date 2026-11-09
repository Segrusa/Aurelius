//! Migration 33: an employee has a description (P7). A schema 32 store keeps its
//! employees, none of them with a description.

use crate::{Store, employees, migrate};
use rusqlite::Connection;

const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000331";
const CHAT: &str = "00000000-0000-7000-8000-000000000332";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

#[test]
fn a_schema_32_store_keeps_its_employees_and_none_of_them_has_a_description() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 32).unwrap());
    let conn = db(dir.path());
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es', 'employee', ?1, ?2)", [&global, EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES (?1, 'es', 'Ann', 'Dev', '{}', 'active', 3, 1, 2)", [EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES (?1, 'employee', NULL, ?2, 'Ann', 1, 1)", [CHAT, EMPLOYEE]).unwrap();
    assert!(conn.prepare("SELECT description FROM employees").is_err(), "the column is new in 33");
    drop(conn);

    let store = Store::open(dir.path()).unwrap();
    let listed = store.read(|c| employees::list_employees(c, &|_| None)).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].name.as_str(), listed[0].version, listed[0].description.as_deref()), ("Ann", 3, None));

    let id = listed[0].id;
    store.write(move |tx| employees::set_description(tx, id, Some("Reviews Rust."))).unwrap();
    assert_eq!(store.read(|c| employees::list_employees(c, &|_| None)).unwrap()[0].description.as_deref(), Some("Reviews Rust."));
    store.write(move |tx| employees::set_description(tx, id, None)).unwrap();
    assert_eq!(store.read(|c| employees::list_employees(c, &|_| None)).unwrap()[0].description, None, "empty is none");
    assert!(migrate::supported_version() >= 33);
}
