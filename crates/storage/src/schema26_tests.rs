//! Migration 26: ticket channels (AUR-CHAT-001). A schema 25 store keeps every
//! row; every ticket's channel gets its members, the channels of closed tickets
//! are archived, and the backfill can run again without changing anything.

use crate::{Store, migrate};
use rusqlite::Connection;

const P: &str = "00000000-0000-7000-8000-000000000001";
const B: &str = "00000000-0000-7000-8000-000000000002";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const BEA: &str = "00000000-0000-7000-8000-000000000005";
const E1: &str = "00000000-0000-7000-8000-000000000004";
const E2: &str = "00000000-0000-7000-8000-000000000006";
const T1: &str = "00000000-0000-7000-8000-000000000011";
const T2: &str = "00000000-0000-7000-8000-000000000012";
const T3: &str = "00000000-0000-7000-8000-000000000013";
const TASK: &str = "00000000-0000-7000-8000-000000000021";
const EVENT: &str = "00000000-0000-7000-8000-000000000031";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn strings(conn: &Connection, sql: &str) -> Vec<String> {
    conn.prepare(sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn column(conn: &Connection, kind: &str) -> String {
    conn.query_row("SELECT id FROM board_columns WHERE kind = ?1", [kind], |r| r.get(0)).unwrap()
}

/// A schema 25 store: Alex (made in the onboarding: the Owner) and Bea (made by
/// Alex), two employees, and three tickets: #1 in Doing by Bea, owned by Pulse,
/// its main task with Nova; #2 on Done; #3 archived in Todo. #1's channel has a message.
fn schema_25_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 25).unwrap());
    let conn = db(dir);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    let (doing, done, todo) = (column(&conn, "doing"), column(&conn, "done"), column(&conn, "todo"));
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es1', 'employee', '{global}', '{E1}'), ('es2', 'employee', '{global}', '{E2}');
         INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES
           ('{E1}', 'es1', 'Pulse', 'QA', '{{}}', 'active', 1, 1, 1), ('{E2}', 'es2', 'Nova', 'BA', '{{}}', 'active', 1, 1, 1);
         INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES ('{B}', 'k', 's', 0, 'text/plain', 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{ALEX}', '{company}', 'Alex', '$argon2id$x', 1, 1);
         INSERT INTO users(id, company_id, username, password_hash, created_by_user, created_at_ms, updated_at_ms) VALUES ('{BEA}', '{company}', 'Bea', '$argon2id$x', '{ALEX}', 2, 2);
         INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES
           ('c1', 'ticket', '{P}', '#1 Fix the login', 1, 10), ('c2', 'ticket', '{P}', '#2 Write the docs', 1, 20), ('c3', 'ticket', '{P}', '#3 Old work', 1, 30);
         INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, priority, created_by_user, owner_employee_id,
                             version, created_at_ms, updated_at_ms, done_at_ms, archived_at_ms, conversation_id, login_user) VALUES
           ('{T1}', '{company}', '{P}', 1, 'Fix the login', '{doing}', 1.0, 'high', '{BEA}', '{E1}', 4, 10, 11, NULL, NULL, 'c1', '{BEA}'),
           ('{T2}', '{company}', '{P}', 2, 'Write the docs', '{done}', 1.0, 'normal', '{ALEX}', NULL, 2, 20, 21, 22, NULL, 'c2', '{ALEX}'),
           ('{T3}', '{company}', '{P}', 3, 'Old work', '{todo}', 2.0, 'low', '{ALEX}', NULL, 2, 30, 31, NULL, 33, 'c3', '{ALEX}');
         INSERT INTO tasks(id, project_id, employee_id, title, brief_blob_id, status, version, created_at_ms, updated_at_ms, ticket_id)
           VALUES ('{TASK}', '{P}', '{E2}', 'Fix the login', '{B}', 'running', 3, 5, 6, '{T1}');
         UPDATE tickets SET root_task_id = '{TASK}' WHERE id = '{T1}';
         INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id,
                            metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms)
           VALUES ('{EVENT}', 'conversation', 'c1', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'k', 12, 12);
         INSERT INTO messages(id, conversation_id, ordinal, role, source_event_id, status, input_kind, created_at_ms)
           VALUES ('m1', 'c1', 1, 'user', '{EVENT}', 'complete', 'text', 12);"
    ))
    .unwrap();
}

/// The rows a migration must keep: every table's count, and the rows of the
/// conversations, tickets and messages.
fn snapshot(conn: &Connection) -> Vec<String> {
    let mut out = Vec::new();
    for table in strings(conn, "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT IN ('schema_migrations', 'channel_members', 'message_reactions') ORDER BY name") {
        out.push(format!("{table}: {}", count(conn, &format!("SELECT COUNT(*) FROM \"{table}\""))));
    }
    out.extend(strings(conn, "SELECT id || ' ' || kind || ' ' || COALESCE(project_id, '-') || ' ' || title || ' ' || version || ' ' || created_at_ms FROM conversations ORDER BY id"));
    out.extend(strings(conn, "SELECT id || ' ' || number || ' ' || title || ' ' || column_id || ' ' || version || ' ' || conversation_id FROM tickets ORDER BY id"));
    out.extend(strings(conn, "SELECT id || ' ' || conversation_id || ' ' || ordinal || ' ' || role || ' ' || status FROM messages ORDER BY id"));
    out
}

const MEMBERS: &str = "SELECT conversation_id || ' ' || COALESCE(u.username, e.name) || ' ' || m.joined_at_ms
    FROM channel_members m LEFT JOIN users u ON u.id = m.user_id LEFT JOIN employees e ON e.id = m.employee_id ORDER BY 1";
const ARCHIVED: &str = "SELECT id || ' ' || COALESCE(archived_at_ms, '-') FROM conversations WHERE kind = 'ticket' ORDER BY id";

/// The part of 0026 that may run again (after its marker).
fn backfill() -> &'static str {
    let sql = migrate::MIGRATIONS.iter().find(|m| m.version == 26).unwrap().sql;
    let after = sql.split_once("-- backfill (idempotent)").unwrap().1;
    after.split_once('\n').unwrap().1
}

#[test]
fn migration_26_is_additive() {
    let m = migrate::MIGRATIONS.iter().find(|m| m.version == 26).unwrap();
    assert!(!m.rebuild && m.then.is_none());
    for line in m.sql.lines().map(str::trim).filter(|l| !l.starts_with("--")) {
        let upper = line.to_ascii_uppercase();
        assert!(!upper.contains("DROP ") && !upper.contains("DELETE ") && !upper.contains(" RENAME "), "not additive: {line}");
    }
}

#[test]
fn a_schema_25_store_migrates_without_losing_a_row() {
    let dir = tempfile::tempdir().unwrap();
    schema_25_database(dir.path());
    let before = snapshot(&db(dir.path()));
    assert!(before.contains(&"messages: 1".to_owned()) && before.contains(&"tickets: 3".to_owned()));

    drop(Store::open_until(dir.path(), 26).unwrap());
    let conn = db(dir.path());
    assert_eq!(count(&conn, "SELECT MAX(version) FROM schema_migrations"), 26);
    assert!(strings(&conn, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());
    assert_eq!(snapshot(&conn), before);
}

#[test]
fn every_ticket_channel_gets_its_creator_the_owner_and_its_employees() {
    let dir = tempfile::tempdir().unwrap();
    schema_25_database(dir.path());
    drop(Store::open_until(dir.path(), 26).unwrap());
    let conn = db(dir.path());
    assert_eq!(
        strings(&conn, MEMBERS),
        // #1: Bea made it, Alex is the Owner, Pulse owns it, Nova has its main task.
        ["c1 Alex 10", "c1 Bea 10", "c1 Nova 10", "c1 Pulse 10", "c2 Alex 20", "c3 Alex 30"]
    );
}

#[test]
fn the_channels_of_tickets_on_done_or_archived_are_archived() {
    let dir = tempfile::tempdir().unwrap();
    schema_25_database(dir.path());
    drop(Store::open_until(dir.path(), 26).unwrap());
    let conn = db(dir.path());
    assert_eq!(strings(&conn, ARCHIVED), ["c1 -", "c2 22", "c3 33"]);
}

#[test]
fn the_backfill_runs_again_without_changing_anything() {
    let dir = tempfile::tempdir().unwrap();
    schema_25_database(dir.path());
    drop(Store::open_until(dir.path(), 26).unwrap());
    let conn = db(dir.path());
    let before = (snapshot(&conn), strings(&conn, MEMBERS), strings(&conn, ARCHIVED));
    conn.execute_batch(backfill()).unwrap();
    conn.execute_batch(backfill()).unwrap();
    assert_eq!((snapshot(&conn), strings(&conn, MEMBERS), strings(&conn, ARCHIVED)), before);
}

#[test]
fn an_archive_time_already_there_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    schema_25_database(dir.path());
    db(dir.path()).execute("UPDATE conversations SET archived_at_ms = 5 WHERE id = 'c2'", []).unwrap();
    drop(Store::open_until(dir.path(), 26).unwrap());
    assert_eq!(strings(&db(dir.path()), ARCHIVED), ["c1 -", "c2 5", "c3 33"]);
}

#[test]
fn a_member_and_a_reaction_are_one_person_each_and_once() {
    let dir = tempfile::tempdir().unwrap();
    schema_25_database(dir.path());
    drop(Store::open_until(dir.path(), 26).unwrap());
    let conn = db(dir.path());
    let both = conn.execute("INSERT INTO channel_members(conversation_id, user_id, employee_id, joined_at_ms) VALUES ('c1', ?1, ?2, 1)", [ALEX, E1]);
    assert!(both.is_err(), "a board member or an employee, not both");
    assert!(conn.execute("INSERT INTO channel_members(conversation_id, joined_at_ms) VALUES ('c1', 1)", []).is_err(), "nobody");
    assert!(conn.execute("INSERT INTO channel_members(conversation_id, user_id, joined_at_ms) VALUES ('c1', ?1, 99)", [ALEX]).is_err(), "once");
    conn.execute("INSERT INTO message_reactions(message_id, user_id, emoji, created_at_ms) VALUES ('m1', ?1, '👍', 1)", [ALEX]).unwrap();
    assert!(conn.execute("INSERT INTO message_reactions(message_id, user_id, emoji, created_at_ms) VALUES ('m1', ?1, '👍', 2)", [ALEX]).is_err());
    conn.execute("INSERT INTO message_reactions(message_id, employee_id, emoji, created_at_ms) VALUES ('m1', ?1, '👍', 3)", [E1]).unwrap();
    assert!(conn.execute("UPDATE messages SET thread_parent_id = 'nope' WHERE id = 'm1'", []).is_err(), "a thread's parent is a message");
}
