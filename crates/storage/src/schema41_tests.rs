//! Migration 41: quick tasks (ticket #53). A schema 40 store keeps every ticket as
//! it was; each becomes a normal ticket without a merge commit, and the new
//! columns refuse what their CHECKs do not allow.

use crate::{Store, tickets};
use aurelius_domain::{TicketId, ticket::TicketKind};
use rusqlite::Connection;

const P: &str = "00000000-0000-7000-8000-000000000001";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const T1: &str = "00000000-0000-7000-8000-000000000011";
const T2: &str = "00000000-0000-7000-8000-000000000012";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn column(conn: &Connection, kind: &str) -> String {
    conn.query_row("SELECT id FROM board_columns WHERE kind = ?1", [kind], |r| r.get(0)).unwrap()
}

const TICKET_COLUMNS: &str = "id, number, title, column_id, position, priority, created_by_user, version, created_at_ms, updated_at_ms, done_at_ms, conversation_id, login_user, design_url";

fn ticket_rows(conn: &Connection) -> Vec<String> {
    let sql = format!("SELECT json_array({TICKET_COLUMNS}) FROM tickets ORDER BY id");
    conn.prepare(&sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

/// A schema 40 store with Alex, #1 in Doing (with a design link) and #2 on Done.
fn schema_40_database(dir: &std::path::Path) -> Vec<String> {
    drop(Store::open_until(dir, 40).unwrap());
    let conn = db(dir);
    assert!(conn.prepare("SELECT kind FROM tickets").is_err(), "new in 41");
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    let (doing, done) = (column(&conn, "doing"), column(&conn, "done"));
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{ALEX}', '{company}', 'alex', '$argon2id$x', 1, 1);
         INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES
           ('c1', 'ticket', '{P}', '#1 Fix the login', 1, 10), ('c2', 'ticket', '{P}', '#2 Write the docs', 1, 20);
         INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, priority, created_by_user,
                             version, created_at_ms, updated_at_ms, done_at_ms, conversation_id, login_user, design_url) VALUES
           ('{T1}', '{company}', '{P}', 1, 'Fix the login', '{doing}', 1.0, 'high', '{ALEX}', 4, 10, 11, NULL, 'c1', '{ALEX}', 'https://claude.ai/design/x'),
           ('{T2}', '{company}', '{P}', 2, 'Write the docs', '{done}', 1.0, 'normal', '{ALEX}', 2, 20, 21, 21, 'c2', '{ALEX}', NULL);"
    ))
    .unwrap();
    ticket_rows(&conn)
}

#[test]
fn a_schema_40_store_keeps_its_tickets_and_each_becomes_a_normal_ticket() {
    let dir = tempfile::tempdir().unwrap();
    let before = schema_40_database(dir.path());

    let store = Store::open(dir.path()).unwrap();
    let conn = db(dir.path());
    assert_eq!(ticket_rows(&conn), before, "every ticket keeps what it had");
    let kinds: Vec<(String, Option<String>)> =
        conn.prepare("SELECT kind, merge_commit FROM tickets ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap();
    assert_eq!(kinds, vec![("normal".to_owned(), None), ("normal".to_owned(), None)]);

    let id = TicketId::parse(T1).unwrap();
    let state = store.read(move |c| tickets::state(c, id)).unwrap().unwrap();
    assert_eq!(state.kind, TicketKind::Normal);
    let view = store.read(move |c| tickets::view(c, id, &|_| None)).unwrap().unwrap();
    assert_eq!((view.kind, view.merge_commit), (None, None), "a normal ticket's view is as before");
}

#[test]
fn the_new_columns_refuse_what_their_checks_do_not_allow() {
    let dir = tempfile::tempdir().unwrap();
    schema_40_database(dir.path());
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    assert!(conn.execute("UPDATE tickets SET kind = 'urgent' WHERE id = ?1", [T1]).is_err(), "only normal or quick");
    assert!(conn.execute("UPDATE tickets SET kind = NULL WHERE id = ?1", [T1]).is_err(), "never empty");
    assert!(conn.execute("UPDATE tickets SET merge_commit = 'abc' WHERE id = ?1", [T1]).is_err(), "a commit is at least 7 characters");
    conn.execute("UPDATE tickets SET kind = 'quick', merge_commit = 'a1b2c3d4e5f6' WHERE id = ?1", [T1]).unwrap();

    let store = Store::open(dir.path()).unwrap();
    let id = TicketId::parse(T1).unwrap();
    let view = store.read(move |c| tickets::view(c, id, &|_| None)).unwrap().unwrap();
    assert_eq!((view.kind.as_deref(), view.merge_commit.as_deref()), (Some("quick"), Some("a1b2c3d4e5f6")));
    assert!(view.is_quick());
}
