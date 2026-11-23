//! Migration 50: a paired device belongs to the board member who paired it. A schema 49
//! device belongs to the board member who last signed in on it; one nobody signed in on
//! belongs to nobody.

use crate::Store;
use crate::remote::{Devices, list_devices};
use rusqlite::Connection;

const USER: &str = "00000000-0000-7000-8000-000000000501";
const SIGNED_IN: &str = "00000000-0000-7000-8000-000000000502";
const NEVER: &str = "00000000-0000-7000-8000-000000000503";

/// A schema 49 store with Alex's iPhone (Alex signed in on it) and a tablet nobody signed in on.
fn schema_49_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 49).unwrap());
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    assert!(conn.prepare("SELECT paired_by FROM remote_clients").is_err(), "new in 50");
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{USER}', '{company}', 'alex', '$argon2id$x', 1, 1);
         INSERT INTO remote_clients(id, principal_id, name, platform, credential_verifier, capabilities_json, created_at_ms, user_id)
           VALUES ('{SIGNED_IN}', 'p', 'iPhone', 'ios', 'v1', '[\"read\"]', 2, '{USER}'),
                  ('{NEVER}', 'p', 'Tablet', 'android', 'v2', '[\"read\"]', 1, NULL);"
    ))
    .unwrap();
}

#[test]
fn a_schema_49_device_belongs_to_whoever_signed_in_on_it_and_one_nobody_signed_in_on_to_nobody() {
    let dir = tempfile::tempdir().unwrap();
    schema_49_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let alex = store.read(|c| list_devices(c, Devices::PairedBy(USER))).unwrap();
    assert_eq!(alex.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["iPhone"]);
    assert_eq!(alex[0].paired_by.map(|u| u.to_string()).as_deref(), Some(USER));
    let nobody = store.read(|c| list_devices(c, Devices::VisibleTo(None))).unwrap();
    assert_eq!(nobody.iter().map(|d| (d.name.as_str(), d.paired_by)).collect::<Vec<_>>(), [("Tablet", None)]);
}

#[test]
fn a_device_can_only_belong_to_a_board_member_that_exists() {
    let dir = tempfile::tempdir().unwrap();
    schema_49_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let refused = store.write(|tx| Ok(tx.execute(&format!("UPDATE remote_clients SET paired_by = 'nobody' WHERE id = '{NEVER}'"), [])?));
    assert!(refused.is_err(), "{refused:?}");
}
