//! Migration 28: who read or dismissed a notification for everyone (AUR-FA-A0).
//! A schema 27 store keeps every row, what was read stays read, and the two new
//! STRICT tables hold their keys in the database.

use crate::{Store, migrate, timeline};
use rusqlite::Connection;

const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const BEA: &str = "00000000-0000-7000-8000-000000000005";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

/// A schema 27 store: Alex and Bea; for everyone one read and one unread
/// notification; Bea's own, unread.
fn schema_27_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 27).unwrap());
    let conn = db(dir);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{ALEX}', '{company}', 'alex', '$argon2id$x', 1, 1), ('{BEA}', '{company}', 'bea', '$argon2id$x', 1, 1);"
    ))
    .unwrap();
    for (id, user, read) in [("read", None, Some(5_i64)), ("open", None, None), ("own", Some(BEA), None)] {
        conn.execute(
            "INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms)
             VALUES (?1, 'task', ?1, 1, 'task.succeeded', 1, 'system', ?1, '{}', ?1, 10, 10)",
            [id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO notifications(id, event_id, dedupe_key, severity, target_json, created_at_ms, user_id, read_at_ms) VALUES (?1, ?1, ?1, 'info', '{}', 10, ?2, ?3)",
            rusqlite::params![id, user, read],
        )
        .unwrap();
    }
}

/// Every notification with who it is for and when it was read.
fn rows(conn: &Connection) -> Vec<(String, Option<String>, Option<i64>)> {
    let mut stmt = conn.prepare("SELECT id, user_id, read_at_ms FROM notifications ORDER BY id").unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().collect::<Result<_, _>>().unwrap()
}

#[test]
fn migration_28_is_plain_and_additive() {
    let m = migrate::MIGRATIONS.iter().find(|m| m.version == 28).unwrap();
    assert!(!m.rebuild && m.then.is_none());
    for line in m.sql.lines().map(str::trim).filter(|l| !l.starts_with("--")) {
        let upper = line.to_ascii_uppercase();
        for word in ["DROP ", "DELETE ", " RENAME ", "UPDATE ", "INSERT ", "ALTER "] {
            let allowed = word == "DELETE " && upper.contains("ON DELETE CASCADE");
            assert!(allowed || !upper.contains(word), "not additive: {line}");
        }
    }
}

#[test]
fn a_schema_27_store_keeps_its_notifications_and_what_was_read_stays_read() {
    let dir = tempfile::tempdir().unwrap();
    schema_27_database(dir.path());
    let before = rows(&db(dir.path()));

    let store = Store::open(dir.path()).unwrap();
    let conn = db(dir.path());
    assert_eq!(rows(&conn), before, "no row changed");
    for table in ["notification_reads", "notification_dismissals"] {
        let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "{table}");
    }
    let shown = |user| {
        let mut rows: Vec<_> = store.read(|c| timeline::list_notifications_for(c, 50, Some(user), None)).unwrap().into_iter().map(|n| (n.id, n.read)).collect();
        rows.sort();
        rows
    };
    assert_eq!(shown(ALEX), [("open".to_owned(), false), ("read".to_owned(), true)]);
    assert_eq!(shown(BEA), [("open".to_owned(), false), ("own".to_owned(), false), ("read".to_owned(), true)]);
    assert_eq!(store.read(|c| timeline::unread_count_for(c, Some(ALEX), None)).unwrap(), 1);
    assert_eq!(store.read(|c| timeline::unread_count_for(c, Some(BEA), None)).unwrap(), 2);
}

#[test]
fn the_new_tables_are_strict_and_hold_their_keys() {
    let dir = tempfile::tempdir().unwrap();
    schema_27_database(dir.path());
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    for (table, at) in [("notification_reads", "read_at_ms"), ("notification_dismissals", "dismissed_at_ms")] {
        let insert = format!("INSERT INTO {table}(notification_id, user_id, {at}) VALUES (?1, ?2, ?3)");
        conn.execute(&insert, rusqlite::params!["open", ALEX, 20]).unwrap();
        assert!(conn.execute(&insert, rusqlite::params!["open", ALEX, 21]).is_err(), "once per board member");
        assert!(conn.execute(&insert, rusqlite::params!["nope", ALEX, 21]).is_err(), "a notification that exists");
        assert!(conn.execute(&insert, rusqlite::params!["open", "nobody", 21]).is_err(), "a board member that exists");
        assert!(conn.execute(&insert, rusqlite::params!["read", ALEX, "x"]).is_err(), "STRICT");
    }
    conn.execute("DELETE FROM notifications WHERE id = 'open'", []).unwrap();
    for table in ["notification_reads", "notification_dismissals"] {
        let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "{table} goes with its notification");
    }
}
