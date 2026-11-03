//! Migration 31: an idea can point at a ticket (ticket #16). A schema 30 store keeps
//! its ideas, status and task links, none of them with a ticket.

use crate::{Store, ideas, migrate};
use rusqlite::Connection;

const PROJECT: &str = "00000000-0000-7000-8000-000000000031";
const IDEA: &str = "00000000-0000-7000-8000-000000000032";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

#[test]
fn a_schema_30_store_keeps_its_ideas_and_none_of_them_has_a_ticket() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 30).unwrap());
    let conn = db(dir.path());
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', ?1, ?2)", [&global, PROJECT]).unwrap();
    conn.execute("INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES (?1, 'ps', 'Atlas', 'active', 1, 1, 1)", [PROJECT]).unwrap();
    conn.execute("INSERT INTO ideas(id, project_id, title, note, status, kind, version, created_at_ms, updated_at_ms) VALUES (?1, ?2, 'Old idea', 'Its note', 'dropped', 'bug', 3, 1, 2)", [IDEA, PROJECT]).unwrap();
    assert!(conn.prepare("SELECT ticket_id FROM ideas").is_err(), "the column is new in 31");
    drop(conn);
    let store = Store::open(dir.path()).unwrap();
    let project = aurelius_domain::ProjectId::parse(PROJECT).unwrap();
    let listed = store.read(move |c| ideas::list(c, project)).unwrap();
    assert_eq!(listed.len(), 1);
    let idea = &listed[0];
    assert_eq!((idea.title.as_str(), idea.note.as_deref(), idea.status), ("Old idea", Some("Its note"), aurelius_domain::idea::IdeaStatus::Dropped));
    assert_eq!((idea.ticket_id, idea.ticket_number), (None, None));
    assert!(migrate::supported_version() >= 31);
}
