//! Migration 27: the threads' read markers (AUR-CHAT-002). A schema 26 store
//! keeps every row and gains one empty STRICT table whose keys hold in the database.

use crate::{Store, migrate};
use rusqlite::Connection;

const P: &str = "00000000-0000-7000-8000-000000000001";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
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

/// A schema 26 store: Alex, a project channel with a message, a reply in its
/// thread and a reaction.
fn schema_26_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 26).unwrap());
    let conn = db(dir);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{ALEX}', '{company}', 'Alex', '$argon2id$x', 1, 1);
         INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES ('c', 'group', '{P}', 'Alpha', 1, 10);
         INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id,
                            metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms)
           VALUES ('{EVENT}', 'conversation', 'c', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'k', 12, 12);
         INSERT INTO messages(id, conversation_id, ordinal, role, source_event_id, status, input_kind, created_at_ms, author_user_id)
           VALUES ('m1', 'c', 1, 'user', '{EVENT}', 'complete', 'text', 12, '{ALEX}');
         INSERT INTO messages(id, conversation_id, ordinal, role, source_event_id, status, input_kind, created_at_ms, author_user_id, thread_parent_id)
           VALUES ('m2', 'c', 2, 'user', '{EVENT}', 'complete', 'text', 13, '{ALEX}', 'm1');
         INSERT INTO message_reactions(message_id, user_id, emoji, created_at_ms) VALUES ('m1', '{ALEX}', '👍', 14);"
    ))
    .unwrap();
}

/// Every table's row count and the rows of the messages and reactions.
fn snapshot(conn: &Connection) -> Vec<String> {
    let mut out = Vec::new();
    for table in strings(conn, "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT IN ('schema_migrations', 'thread_reads') ORDER BY name") {
        out.push(format!("{table}: {}", count(conn, &format!("SELECT COUNT(*) FROM \"{table}\""))));
    }
    out.extend(strings(conn, "SELECT id || ' ' || ordinal || ' ' || COALESCE(thread_parent_id, '-') FROM messages ORDER BY id"));
    out.extend(strings(conn, "SELECT message_id || ' ' || user_id || ' ' || emoji FROM message_reactions ORDER BY 1"));
    out
}

#[test]
fn migration_27_is_plain_and_additive() {
    let m = migrate::MIGRATIONS.iter().find(|m| m.version == 27).unwrap();
    assert!(!m.rebuild && m.then.is_none());
    assert!(migrate::supported_version() >= 27);
    for line in m.sql.lines().map(str::trim).filter(|l| !l.starts_with("--")) {
        let upper = line.to_ascii_uppercase();
        for word in ["DROP ", "DELETE ", " RENAME ", "UPDATE ", "INSERT ", "ALTER "] {
            assert!(!upper.contains(word), "not additive: {line}");
        }
    }
}

#[test]
fn a_schema_26_store_migrates_without_losing_a_row() {
    let dir = tempfile::tempdir().unwrap();
    schema_26_database(dir.path());
    let before = snapshot(&db(dir.path()));
    assert!(before.contains(&"m2 2 m1".to_owned()));

    drop(Store::open_until(dir.path(), 27).unwrap());
    let conn = db(dir.path());
    assert_eq!(count(&conn, "SELECT MAX(version) FROM schema_migrations"), 27);
    assert!(strings(&conn, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());
    assert_eq!(snapshot(&conn), before);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM thread_reads"), 0);
}

#[test]
fn a_thread_read_marker_is_one_per_board_member_and_root_and_points_at_real_rows() {
    let dir = tempfile::tempdir().unwrap();
    schema_26_database(dir.path());
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    conn.execute("INSERT INTO thread_reads(user_id, root_message_id, read_through, read_at_ms) VALUES (?1, 'm1', 2, 20)", [ALEX]).unwrap();
    assert!(conn.execute("INSERT INTO thread_reads(user_id, root_message_id, read_through, read_at_ms) VALUES (?1, 'm1', 3, 21)", [ALEX]).is_err(), "once per root");
    assert!(conn.execute("INSERT INTO thread_reads(user_id, root_message_id, read_through, read_at_ms) VALUES (?1, 'nope', 1, 21)", [ALEX]).is_err(), "a root is a message");
    assert!(conn.execute("INSERT INTO thread_reads(user_id, root_message_id, read_through, read_at_ms) VALUES ('nobody', 'm1', 1, 21)", []).is_err(), "a reader is a board member");
    assert!(conn.execute("INSERT INTO thread_reads(user_id, root_message_id, read_through, read_at_ms) VALUES (?1, 'm2', 'x', 21)", [ALEX]).is_err(), "STRICT");
}
