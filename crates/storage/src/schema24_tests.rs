//! Migration 24: the server's name. A schema 23 database keeps every row and
//! gains one empty, STRICT table whose constraints hold in the database itself.

use crate::{Store, migrate, server_name};
use aurelius_domain::UserId;
use rusqlite::{Connection, params};

/// 0023 was released with the guidelines: its bytes never change.
const RELEASED: [(i64, &str); 1] = [(23, "1ca8661be176db94067c6b766895e81b26d96743ab997fccec152dd47a1a173b")];

const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const G: &str = "00000000-0000-7000-8000-000000000041";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn strings(conn: &Connection, sql: &str) -> Vec<String> {
    conn.prepare(sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

/// A schema 23 database with a board member, a guideline and a setting.
fn guidelines_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 23).unwrap());
    let conn = db(dir);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms, password_changed_at_ms)
           VALUES ('{ALEX}', '{company}', 'Alex', '$argon2id$x', 1, 1, 5);
         INSERT INTO guidelines(id, company_id, current_version, created_by_user, updated_by_user, created_at_ms, updated_at_ms)
           VALUES ('{G}', '{company}', 1, '{ALEX}', '{ALEX}', 1, 1);
         INSERT INTO guideline_versions(guideline_id, version, title, text, source, by_user, created_at_ms)
           VALUES ('{G}', 1, 'Style', 'Be brief.', 'created', '{ALEX}', 1);
         INSERT OR REPLACE INTO settings(key, value_json, updated_at_ms) VALUES ('sn01_probe', '\"kept\"', 1);"
    ))
    .unwrap();
}

/// Every row of the tables schema 23 data lives in, as text.
fn snapshot(conn: &Connection) -> Vec<String> {
    let mut all = Vec::new();
    for table in ["users", "company", "settings", "guidelines", "guideline_versions"] {
        let columns = strings(conn, &format!("SELECT name FROM pragma_table_info('{table}') ORDER BY cid"));
        let row = columns.iter().map(|c| format!("COALESCE(quote(\"{c}\"), 'NULL')")).collect::<Vec<_>>().join(" || '|' || ");
        all.extend(strings(conn, &format!("SELECT '{table}:' || {row} FROM {table} ORDER BY 1")));
    }
    all
}

#[test]
fn migration_23_is_unchanged_and_24_is_plain() {
    for (version, sum) in RELEASED {
        let m = migrate::MIGRATIONS.iter().find(|m| m.version == version).unwrap();
        assert_eq!(migrate::checksum(m.sql), sum, "migration {version} changed");
    }
    let m24 = migrate::MIGRATIONS.iter().find(|m| m.version == 24).unwrap();
    assert_eq!((m24.rebuild, m24.then.is_some()), (false, false));
    assert!(!m24.sql.to_lowercase().contains("insert"), "0024 writes no rows");
}

#[test]
fn a_schema_23_database_migrates_without_losing_anything() {
    let dir = tempfile::tempdir().unwrap();
    guidelines_database(dir.path());
    let before = snapshot(&db(dir.path()));
    assert!(before.iter().any(|r| r.starts_with("guideline_versions:")) && before.iter().any(|r| r.contains("sn01_probe")));
    assert_eq!(server_name::get(&db(dir.path())).unwrap(), server_name::StoredServerName::default(), "before 24: no name");

    drop(Store::open_until(dir.path(), 24).unwrap());
    let conn = db(dir.path());
    assert_eq!(conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get::<_, i64>(0)).unwrap(), 24);
    assert!(strings(&conn, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());
    assert_eq!(snapshot(&conn), before);
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM server_name", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    let strict: i64 = conn.query_row("SELECT strict FROM pragma_table_list WHERE name = 'server_name'", [], |r| r.get(0)).unwrap();
    assert_eq!(strict, 1, "server_name is STRICT");
    assert_eq!(server_name::get(&conn).unwrap(), server_name::StoredServerName::default(), "no row: the computer's name");
}

fn put(conn: &Connection, id: i64, name: Option<&str>) -> rusqlite::Result<usize> {
    conn.execute("INSERT OR REPLACE INTO server_name(id, name, version, updated_by_user, updated_at_ms) VALUES (?1, ?2, 1, ?3, 1)", params![id, name, ALEX])
}

#[test]
fn the_table_checks_the_names_length_and_characters() {
    let dir = tempfile::tempdir().unwrap();
    guidelines_database(dir.path());
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    // Characters, not bytes: 40 two-byte characters fit.
    put(&conn, 1, Some(&"é".repeat(40))).unwrap();
    put(&conn, 1, Some("Thuis-PC 🏠")).unwrap();
    put(&conn, 1, None).unwrap();
    assert!(put(&conn, 1, Some(&"x".repeat(41))).is_err(), "over 40");
    assert!(put(&conn, 1, Some("")).is_err(), "empty");
    assert!(put(&conn, 1, Some(" Kantoor")).is_err(), "not trimmed");
    assert!(put(&conn, 1, Some("Kan\ntoor")).is_err(), "line feed");
    assert!(put(&conn, 1, Some("Kan\rtoor")).is_err(), "carriage return");
    assert!(put(&conn, 2, Some("Second")).is_err(), "one row only");
    assert!(conn.execute("UPDATE server_name SET version = 0", []).is_err(), "version 0");
    assert!(conn.execute("UPDATE server_name SET updated_by_user = 'nobody'", []).is_err(), "references users");
}

#[test]
fn set_counts_versions_and_none_puts_the_computers_name_back() {
    let dir = tempfile::tempdir().unwrap();
    guidelines_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let alex = UserId::parse(ALEX).unwrap();
    assert_eq!(store.write(move |tx| server_name::set(tx, Some("Kantoor"), alex)).unwrap(), 1);
    assert_eq!(store.read(server_name::get).unwrap(), server_name::StoredServerName { chosen: Some("Kantoor".into()), version: 1 });
    assert_eq!(store.write(move |tx| server_name::set(tx, None, alex)).unwrap(), 2);
    assert_eq!(store.read(server_name::get).unwrap(), server_name::StoredServerName { chosen: None, version: 2 });
}
