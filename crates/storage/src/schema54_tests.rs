//! Migration 54: each board member's own run defaults. A board member of schema
//! 53 chose none after the upgrade, the employee's and the company's run defaults
//! stay as they were, and the table refuses what no run could use.

use crate::{Store, user_run_defaults};
use aurelius_domain::autonomy::AgentMode;
use aurelius_domain::user::UserRunDefaults;
use aurelius_domain::{EventId, UserId};
use rusqlite::Connection;

const USER: &str = "00000000-0000-7000-8000-000000000521";

/// A schema 53 store with the board member Alex and a company default model
/// (without its event: foreign keys stay off while the rows go in).
fn schema_53_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 53).unwrap());
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
    assert!(conn.prepare("SELECT user_id FROM user_run_defaults").is_err(), "new in 54");
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{USER}', '{company}', 'alex', '$argon2id$x', 1, 1);
         INSERT INTO config_overrides(scope_id, purpose, version, config_json, source_event_id) VALUES ('{global}', 'employee', 1, '{{\"model\":{{\"value\":\"company-model\"}}}}', 'e1');"
    ))
    .unwrap();
}

fn user() -> UserId {
    UserId::parse(USER).unwrap()
}

#[test]
fn a_schema_53_board_member_has_no_run_defaults_and_the_company_default_stays() {
    let dir = tempfile::tempdir().unwrap();
    schema_53_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.read(move |c| user_run_defaults::get(c, user())).unwrap(), UserRunDefaults::default());
    let kept: String = store.read(|c| Ok(c.query_row("SELECT config_json FROM config_overrides WHERE purpose = 'employee'", [], |r| r.get(0))?)).unwrap();
    assert!(kept.contains("company-model"), "{kept}");
}

#[test]
fn run_defaults_are_stored_and_replaced_as_a_whole() {
    let dir = tempfile::tempdir().unwrap();
    schema_53_database(dir.path());
    let store = Store::open(dir.path()).unwrap();
    let first = UserRunDefaults { model: Some("claude-opus-5-5".into()), effort: Some("high".into()), mode: Some(AgentMode::Plan), subagent_model: Some("claude-haiku-5-5".into()) };
    let set = first.clone();
    store.write(move |tx| user_run_defaults::set(tx, user(), &set, EventId::new(), 5)).unwrap();
    assert_eq!(store.read(move |c| user_run_defaults::get(c, user())).unwrap(), first);
    let second = UserRunDefaults { mode: Some(AgentMode::Auto), ..Default::default() };
    let set = second.clone();
    store.write(move |tx| user_run_defaults::set(tx, user(), &set, EventId::new(), 6)).unwrap();
    assert_eq!(store.read(move |c| user_run_defaults::get(c, user())).unwrap(), second);
}

#[test]
fn the_table_refuses_an_unknown_effort_or_mode_and_an_empty_model() {
    let dir = tempfile::tempdir().unwrap();
    schema_53_database(dir.path());
    drop(Store::open(dir.path()).unwrap());
    let conn = Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    for (column, value) in [("effort", "extreme"), ("mode", "yolo"), ("model", "")] {
        let sql = format!("INSERT INTO user_run_defaults(user_id, {column}, source_event_id, updated_at_ms) VALUES ('{USER}', ?1, 'e', 1)");
        assert!(conn.execute(&sql, [value]).is_err(), "{column} = {value:?}");
    }
}
