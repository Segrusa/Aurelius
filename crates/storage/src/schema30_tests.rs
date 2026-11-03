//! Migration 30: quick answers (ticket #15). A schema 29 store keeps every
//! conversation, none of them a quick-answer conversation. Making the hidden
//! conversation of a real employee is tested in the application's discussion tests.

use crate::{Store, conversations, migrate};
use aurelius_domain::EmployeeId;
use rusqlite::Connection;

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

#[test]
fn a_schema_29_store_upgrades_and_none_of_its_conversations_is_a_quick_answer() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 29).unwrap());
    let conn = db(dir.path());
    let before: i64 = conn.query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0)).unwrap();
    assert!(conn.prepare("SELECT quick_answer FROM conversations").is_err(), "the column is new in 30");
    drop(conn);
    let store = Store::open(dir.path()).unwrap();
    let (after, quick): (i64, i64) = store.read(|c| Ok((c.query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0))?, c.query_row("SELECT COUNT(*) FROM conversations WHERE quick_answer = 1", [], |r| r.get(0))?))).unwrap();
    assert_eq!((after, quick), (before, 0));
    assert!(migrate::supported_version() >= 30);
}

#[test]
fn an_unknown_employee_has_no_quick_answer_conversation_and_the_cost_starts_at_zero() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let nobody = EmployeeId::new();
    assert_eq!(store.write(move |tx| conversations::quick_answer_chat(tx, nobody)).unwrap(), None, "an unknown employee has none");
    assert_eq!(store.read(|c| conversations::quick_answer_cost_since(c, 0)).unwrap(), 0);
}
