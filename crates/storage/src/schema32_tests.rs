//! Migration 32: `web` is a platform of a paired device. A schema 31 store keeps every device
//! with its sessions, and its keys still hold.

use crate::{Store, migrate};
use rusqlite::Connection;

const INSERT: &str = "INSERT INTO remote_clients(id, principal_id, name, platform, credential_verifier, capabilities_json, created_at_ms)";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

#[test]
fn every_device_stays_and_a_browser_can_be_one() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 31).unwrap());
    let conn = db(dir.path());
    assert!(conn.execute(&format!("{INSERT} VALUES ('b', 'p', 'Browser', 'web', 'v', '[]', 1)"), []).is_err(), "schema 31 has no browsers");
    conn.execute_batch(&format!(
        "{INSERT} VALUES ('phone', 'p', 'iPhone', 'ios', 'v1', '[\"chat\"]', 5);
         INSERT INTO remote_sessions(id, client_id, token_verifier, created_at_ms, expires_at_ms) VALUES ('s', 'phone', 't', 6, 7);"
    ))
    .unwrap();
    drop(conn);

    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    let phone: (String, String, String, i64) = conn
        .query_row("SELECT name, platform, capabilities_json, created_at_ms FROM remote_clients WHERE id = 'phone'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap();
    assert_eq!(phone, ("iPhone".to_owned(), "ios".to_owned(), "[\"chat\"]".to_owned(), 5));

    conn.execute(&format!("{INSERT} VALUES ('b', 'p', 'Browser', 'web', 'v2', '[]', 8)"), []).unwrap();
    assert!(conn.execute(&format!("{INSERT} VALUES ('x', 'p', 'X', 'fridge', 'v3', '[]', 9)"), []).is_err(), "the CHECK still holds");
    assert!(conn.execute("INSERT INTO remote_sessions(id, client_id, token_verifier, created_at_ms, expires_at_ms) VALUES ('s2', 'nobody', 't2', 1, 2)", []).is_err(), "the key still holds");
    assert_eq!(conn.query_row("SELECT client_id FROM remote_sessions WHERE id = 's'", [], |r| r.get::<_, String>(0)).unwrap(), "phone");
    assert!(conn.prepare("PRAGMA foreign_key_check").unwrap().query([]).unwrap().next().unwrap().is_none());
    assert!(migrate::supported_version() >= 32);
}
