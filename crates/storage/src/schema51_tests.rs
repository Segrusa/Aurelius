//! Migration 51: each board member pins their own workspaces to the rail. A board
//! member of schema 49 has not chosen yet, a choice keeps its order, and a pin names
//! a real project of a real board member.

use crate::{Store, rail_pins};
use aurelius_domain::{ProjectId, UserId};
use rusqlite::Connection;

const ALPHA: &str = "00000000-0000-7000-8000-000000000511";
const BETA: &str = "00000000-0000-7000-8000-000000000512";
const USER: &str = "00000000-0000-7000-8000-000000000513";

/// A schema 49 store with the projects Alpha and Beta and the board member Alex.
fn schema_49_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 49).unwrap());
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    assert!(conn.prepare("SELECT user_id FROM user_rails").is_err(), "new in 51");
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('pa', 'project', '{global}', '{ALPHA}'), ('pb', 'project', '{global}', '{BETA}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{ALPHA}', 'pa', 'Alpha', 'active', 1, 1, 1), ('{BETA}', 'pb', 'Beta', 'archived', 1, 1, 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{USER}', '{company}', 'alex', '$argon2id$x', 1, 1);"
    ))
    .unwrap();
}

fn ids() -> (UserId, ProjectId, ProjectId) {
    (UserId::parse(USER).unwrap(), ProjectId::parse(ALPHA).unwrap(), ProjectId::parse(BETA).unwrap())
}

#[test]
fn a_schema_49_board_member_has_not_chosen_a_rail_after_the_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    schema_49_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let (user, _, _) = ids();
    let rail = store.read(move |c| rail_pins::get(c, user)).unwrap();
    assert!(!rail.chosen);
    assert!(rail.project_ids.is_empty());
}

#[test]
fn a_chosen_rail_keeps_its_order_and_the_same_list_again_is_no_change() {
    let dir = tempfile::tempdir().unwrap();
    schema_49_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let (user, alpha, beta) = ids();
    assert!(store.write(move |tx| rail_pins::set(tx, user, &[beta, alpha], 5)).unwrap());
    assert!(!store.write(move |tx| rail_pins::set(tx, user, &[beta, alpha], 6)).unwrap());
    let rail = store.read(move |c| rail_pins::get(c, user)).unwrap();
    assert!(rail.chosen);
    assert_eq!(rail.project_ids, [beta, alpha]);
}

#[test]
fn unpinning_everything_is_a_choice_of_none() {
    let dir = tempfile::tempdir().unwrap();
    schema_49_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let (user, alpha, _) = ids();
    store.write(move |tx| rail_pins::set(tx, user, &[alpha], 5)).unwrap();
    assert!(store.write(move |tx| rail_pins::set(tx, user, &[], 6)).unwrap());
    let rail = store.read(move |c| rail_pins::get(c, user)).unwrap();
    assert!(rail.chosen && rail.project_ids.is_empty());
}

#[test]
fn a_pin_of_an_unknown_project_or_board_member_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    schema_49_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let (user, alpha, _) = ids();
    let nowhere = ProjectId::new();
    assert_eq!(store.read(move |c| rail_pins::unknown_projects(c, &[alpha, nowhere])).unwrap(), [nowhere]);
    assert!(store.write(move |tx| rail_pins::set(tx, user, &[nowhere], 5)).is_err());
    assert!(store.write(move |tx| rail_pins::set(tx, UserId::new(), &[alpha], 5)).is_err());
}
