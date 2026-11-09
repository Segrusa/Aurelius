//! Migration 37: a board member's chat with an employee and the tickets made from it.
//! A schema 36 store keeps its conversations, which belong to no board member.

use crate::{Store, employee_chats as chats};
use aurelius_domain::{ConversationId, EmployeeId, UserId};
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

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

fn user(store: &Store, name: &str) -> UserId {
    let (id, name) = (UserId::new(), name.to_owned());
    store.write(move |tx| crate::users::insert(tx, &crate::users::NewUser { id: &id.to_string(), username: &name, created_by_user: None, now_ms: 1 }, PHC)).unwrap();
    id
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
    let (owners, links): (i64, i64) = store
        .read(|c| Ok(c.query_row("SELECT (SELECT COUNT(*) FROM conversations WHERE kind = 'employee' AND board_member_id IS NOT NULL), (SELECT COUNT(*) FROM chat_tickets)", [], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap();
    assert_eq!((owners, links), (0, 0));
    let main = ConversationId::parse(MAIN).unwrap();
    assert!(store.read(|c| chats::of_conversation(c, main)).unwrap().is_none(), "the main chat is nobody's own");
    assert!(!store.read(|c| chats::hidden_from(c, main, None)).unwrap());
}

#[test]
fn a_board_member_has_one_chat_per_employee_and_the_main_chat_stays_the_oldest() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 36).unwrap());
    let conn = db(dir.path());
    employee(&conn);
    drop(conn);
    let store = Store::open(dir.path()).unwrap();
    let e = EmployeeId::parse(EMPLOYEE).unwrap();
    let (grace, robin) = (user(&store, "grace"), user(&store, "robin"));

    let first = store.write(move |tx| chats::ensure(tx, e, grace)).unwrap().unwrap();
    let again = store.write(move |tx| chats::ensure(tx, e, grace)).unwrap().unwrap();
    let other = store.write(move |tx| chats::ensure(tx, e, robin)).unwrap().unwrap();
    assert_eq!(first, again);
    assert_ne!(first, other);
    assert_eq!(store.read(move |c| crate::employees::main_chat(c, e)).unwrap().map(|c| c.to_string()).as_deref(), Some(MAIN));
    assert_eq!(store.read(move |c| chats::of_conversation(c, first)).unwrap().map(|c| (c.employee, c.user, c.username)), Some((e, grace, "grace".to_owned())));
    assert!(!store.read(move |c| chats::hidden_from(c, first, Some(grace))).unwrap());
    assert!(store.read(move |c| chats::hidden_from(c, first, Some(robin))).unwrap());
    assert!(store.read(move |c| chats::hidden_from(c, first, None)).unwrap());
    assert!(store.read(move |c| chats::list(c, grace, |_| None)).unwrap().is_empty(), "an empty chat is not listed");
}
