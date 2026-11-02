//! Migration 29: phase A (AUR-FA-S1). A schema 28 store keeps every row, creators
//! follow their tickets that are not archived, and the new columns start empty.

use crate::{Store, conversations, migrate, tickets, timeline};
use aurelius_domain::{ConversationId, TicketId};
use rusqlite::Connection;

const P: &str = "00000000-0000-7000-8000-000000000001";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const BEA: &str = "00000000-0000-7000-8000-000000000005";
const T1: &str = "00000000-0000-7000-8000-000000000011";
const T2: &str = "00000000-0000-7000-8000-000000000012";
const T3: &str = "00000000-0000-7000-8000-000000000013";
const C1: &str = "00000000-0000-7000-8000-0000000000c1";
const M1: &str = "00000000-0000-7000-8000-0000000000a1";
const M2: &str = "00000000-0000-7000-8000-0000000000a2";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn column(conn: &Connection, kind: &str) -> String {
    conn.query_row("SELECT id FROM board_columns WHERE kind = ?1", [kind], |r| r.get(0)).unwrap()
}

/// A schema 28 store: Alex and Bea; #1 in Doing by Bea, #2 on Done by Alex, #3
/// archived by Bea; two messages in #1's discussion, a notification for everyone
/// and one of Bea's.
fn schema_28_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 28).unwrap());
    let conn = db(dir);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    let (doing, done, todo) = (column(&conn, "doing"), column(&conn, "done"), column(&conn, "todo"));
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES
           ('{ALEX}', '{company}', 'alex', '$argon2id$x', 1, 1), ('{BEA}', '{company}', 'bea', '$argon2id$x', 2, 2);
         INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES
           ('{C1}', 'ticket', '{P}', '#1 Fix the login', 1, 10), ('c2', 'ticket', '{P}', '#2 Write the docs', 1, 20), ('c3', 'ticket', '{P}', '#3 Old work', 1, 30);
         INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, priority, created_by_user,
                             version, created_at_ms, updated_at_ms, done_at_ms, archived_at_ms, conversation_id, login_user) VALUES
           ('{T1}', '{company}', '{P}', 1, 'Fix the login', '{doing}', 1.0, 'high', '{BEA}', 4, 10, 11, NULL, NULL, '{C1}', '{BEA}'),
           ('{T2}', '{company}', '{P}', 2, 'Write the docs', '{done}', 1.0, 'normal', '{ALEX}', 2, 20, 21, 21, NULL, 'c2', '{ALEX}'),
           ('{T3}', '{company}', '{P}', 3, 'Old work', '{todo}', 1.0, 'low', '{BEA}', 1, 30, 31, NULL, 31, 'c3', '{BEA}');
         INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms) VALUES
           ('e1', 'conversation', '{C1}', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'k1', 12, 12),
           ('e2', 'task', 'x', 1, 'task.succeeded', 1, 'system', 'c', '{{}}', 'k2', 13, 13),
           ('e3', 'task', 'y', 1, 'task.failed', 1, 'system', 'c', '{{}}', 'k3', 14, 14);
         INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES ('b1', 'Hello', 's', 0, 'text/plain', 1), ('b2', 'Bye', 's', 0, 'text/plain', 1);
         INSERT INTO messages(id, conversation_id, ordinal, role, content_blob_id, source_event_id, status, input_kind, created_at_ms, author_user_id) VALUES
           ('{M1}', '{C1}', 1, 'user', 'b1', 'e1', 'complete', 'text', 100, '{ALEX}'),
           ('{M2}', '{C1}', 2, 'user', 'b2', 'e1', 'complete', 'text', 200, '{BEA}');
         INSERT INTO notifications(id, event_id, dedupe_key, severity, target_json, created_at_ms, user_id, read_at_ms) VALUES
           ('shared', 'e2', 'event:e2', 'info', '{{\"type\":\"task.succeeded\"}}', 50, NULL, 60),
           ('own', 'e3', 'event:e3', 'warning', '{{\"type\":\"task.failed\"}}', 70, '{BEA}', NULL);"
    ))
    .unwrap();
}

/// Every row of `table` as text, by the columns of schema 28.
fn rows(conn: &Connection, table: &str, columns: &str) -> Vec<String> {
    let sql = format!("SELECT json_array({columns}) FROM {table} ORDER BY id");
    conn.prepare(&sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

const TICKET_COLUMNS: &str = "id, number, title, column_id, position, priority, created_by_user, version, created_at_ms, updated_at_ms, done_at_ms, archived_at_ms, conversation_id, login_user";
const MESSAGE_COLUMNS: &str = "id, conversation_id, ordinal, role, content_blob_id, source_event_id, status, created_at_ms, author_user_id";
const NOTIFICATION_COLUMNS: &str = "id, event_id, dedupe_key, severity, target_json, created_at_ms, user_id, read_at_ms";

fn all_rows(conn: &Connection) -> [Vec<String>; 3] {
    [rows(conn, "tickets", TICKET_COLUMNS), rows(conn, "messages", MESSAGE_COLUMNS), rows(conn, "notifications", NOTIFICATION_COLUMNS)]
}

/// A store at schema 29 with the data of `schema_28_database`, opened by plain connection.
fn upgraded() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    schema_28_database(dir.path());
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    (dir, conn)
}

#[test]
fn migration_29_rebuilds_nothing_and_only_adds() {
    let m = migrate::MIGRATIONS.iter().find(|m| m.version == 29).unwrap();
    assert!(!m.rebuild && m.then.is_none());
    assert_eq!(migrate::supported_version(), 29);
    let mut inserts = 0;
    for line in m.sql.lines().map(str::trim).filter(|l| !l.starts_with("--")) {
        let upper = line.to_ascii_uppercase();
        inserts += upper.matches("INSERT ").count();
        for word in ["DROP ", "DELETE ", " RENAME ", "UPDATE "] {
            let allowed = word == "DELETE " && upper.contains("ON DELETE ");
            assert!(allowed || !upper.contains(word), "not additive: {line}");
        }
    }
    assert_eq!(inserts, 1, "only the backfill of the followers writes rows");
}

#[test]
fn a_schema_28_store_keeps_every_row_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    schema_28_database(dir.path());
    let before = all_rows(&db(dir.path()));
    assert_eq!(before.iter().map(Vec::len).collect::<Vec<_>>(), [3, 2, 2], "the fixture has its rows");

    drop(Store::open(dir.path()).unwrap());
    assert_eq!(all_rows(&db(dir.path())), before);
}

#[test]
fn the_new_columns_are_there_and_start_empty() {
    let (_dir, conn) = upgraded();
    let columns = |table: &str| -> Vec<String> {
        conn.prepare(&format!("SELECT name FROM pragma_table_info('{table}')")).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    };
    for (table, wanted) in [
        ("tickets", &["responsible_user_id"][..]),
        ("notifications", &["ticket_id", "conversation_id", "message_id", "reason", "merged_count"]),
        ("messages", &["edited_at_ms", "deleted_at_ms"]),
    ] {
        let have = columns(table);
        for column in wanted {
            assert!(have.iter().any(|c| c == column), "{table}.{column}");
        }
    }
    let empty = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
    assert_eq!(empty("SELECT COUNT(*) FROM tickets WHERE responsible_user_id IS NOT NULL"), 0);
    assert_eq!(empty("SELECT COUNT(*) FROM messages WHERE edited_at_ms IS NOT NULL OR deleted_at_ms IS NOT NULL"), 0);
    assert_eq!(empty("SELECT COUNT(*) FROM notifications WHERE ticket_id IS NOT NULL OR conversation_id IS NOT NULL OR message_id IS NOT NULL OR reason IS NOT NULL OR merged_count <> 0"), 0);
}

#[test]
fn a_creator_follows_their_tickets_that_are_not_archived_and_no_others() {
    let (_dir, conn) = upgraded();
    let mut stmt = conn.prepare("SELECT ticket_id, user_id, reason, followed_at_ms FROM ticket_followers ORDER BY ticket_id").unwrap();
    let followers: Vec<(String, String, String, i64)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap().collect::<Result<_, _>>().unwrap();
    assert_eq!(
        followers,
        [(T1.to_owned(), BEA.to_owned(), "created".to_owned(), 10), (T2.to_owned(), ALEX.to_owned(), "created".to_owned(), 20)],
        "a ticket on Done still counts; the archived #3 does not"
    );
}

#[test]
fn ticket_followers_is_strict_and_without_rowid_and_holds_its_values() {
    let (_dir, conn) = upgraded();
    let (strict, without_rowid): (bool, bool) = conn.query_row("SELECT strict, wr FROM pragma_table_list WHERE name = 'ticket_followers'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert!(strict && without_rowid);

    let insert = "INSERT INTO ticket_followers(ticket_id, user_id, reason, followed_at_ms) VALUES (?1, ?2, ?3, ?4)";
    for reason in ["responsible", "mentioned", "manual"] {
        conn.execute("DELETE FROM ticket_followers WHERE ticket_id = ?1 AND user_id = ?2", [T1, ALEX]).unwrap();
        conn.execute(insert, rusqlite::params![T1, ALEX, reason, 40]).unwrap();
    }
    assert!(conn.execute(insert, rusqlite::params![T1, ALEX, "manual", 41]).is_err(), "once per board member");
    assert!(conn.execute(insert, rusqlite::params![T3, ALEX, "assigned", 41]).is_err(), "a reason of the four");
    assert!(conn.execute(insert, rusqlite::params!["nope", ALEX, "manual", 41]).is_err(), "a ticket that exists");
    assert!(conn.execute(insert, rusqlite::params![T3, "nobody", "manual", 41]).is_err(), "a board member that exists");
    assert!(conn.execute(insert, rusqlite::params![T3, ALEX, "manual", "x"]).is_err(), "STRICT");
}

#[test]
fn the_checks_on_notifications_refuse_a_long_reason_and_a_negative_count() {
    let (_dir, conn) = upgraded();
    let reason = "UPDATE notifications SET reason = ?1 WHERE id = 'own'";
    conn.execute(reason, ["x".repeat(32)]).unwrap();
    assert!(conn.execute(reason, ["x".repeat(33)]).is_err(), "33 characters");
    conn.execute("UPDATE notifications SET reason = NULL WHERE id = 'own'", []).unwrap();
    assert!(conn.execute("UPDATE notifications SET merged_count = -1 WHERE id = 'own'", []).is_err());
    assert!(conn.execute("UPDATE notifications SET merged_count = NULL WHERE id = 'own'", []).is_err());
    assert!(conn.execute("UPDATE notifications SET ticket_id = 'nope' WHERE id = 'own'", []).is_err(), "a ticket that exists");
    assert!(conn.execute("UPDATE notifications SET conversation_id = 'nope' WHERE id = 'own'", []).is_err(), "a conversation that exists");
    assert!(conn.execute("UPDATE notifications SET message_id = 'nope' WHERE id = 'own'", []).is_err(), "a message that exists");
    assert!(conn.execute("UPDATE tickets SET responsible_user_id = 'nobody' WHERE id = ?1", [T1]).is_err(), "a board member that exists");
}

#[test]
fn the_notification_indexes_are_on_user_then_ticket_or_conversation() {
    let (_dir, conn) = upgraded();
    let columns = |index: &str| -> Vec<String> {
        conn.prepare(&format!("SELECT name FROM pragma_index_info('{index}') ORDER BY seqno")).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(columns("notifications_user_ticket"), ["user_id", "ticket_id"]);
    assert_eq!(columns("notifications_user_conversation"), ["user_id", "conversation_id"]);
}

#[test]
fn deleting_a_queued_message_keeps_the_notification_about_it_without_the_message() {
    let (_dir, conn) = upgraded();
    conn.execute("UPDATE notifications SET message_id = ?1, conversation_id = ?2 WHERE id = 'own'", [M2, C1]).unwrap();
    conn.execute("DELETE FROM messages WHERE id = ?1", [M2]).unwrap();
    let row: (Option<String>, Option<String>) = conn.query_row("SELECT message_id, conversation_id FROM notifications WHERE id = 'own'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(row, (None, Some(C1.to_owned())));
}

#[test]
fn the_reads_show_an_upgraded_row_as_nothing_new() {
    let dir = tempfile::tempdir().unwrap();
    schema_28_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let ticket = store.read(|c| tickets::view(c, TicketId::parse(T1).unwrap(), &|_| None)).unwrap().unwrap();
    assert_eq!((ticket.responsible_user_id, ticket.responsible_username, ticket.responsible_disabled), (None, None, false));

    let messages = store.read(|c| conversations::list_messages(c, ConversationId::parse(C1).unwrap(), &|k| Some(k.to_owned()))).unwrap();
    assert_eq!(messages.iter().map(|m| (m.text.as_str(), m.edited_at_ms, m.deleted_at_ms, m.editable)).collect::<Vec<_>>(), [("Hello", None, None, false), ("Bye", None, None, false)]);

    let shown = store.read(|c| timeline::list_notifications_for(c, 50, Some(BEA))).unwrap();
    assert_eq!(shown.len(), 2);
    for n in shown {
        assert_eq!((n.ticket_id, n.ticket_number, n.conversation_id, n.message_id, n.reason, n.merged_count, n.needs_you), (None, None, None, None, None, 0, false), "{}", n.id);
    }
}
