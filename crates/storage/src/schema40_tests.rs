//! Migration 40: a project's styleguide link and a ticket's design link. A schema 39
//! store keeps what it has; both start empty.

use crate::Store;
use rusqlite::Connection;

const PROJECT: &str = "00000000-0000-7000-8000-000000000401";

fn db(dir: &std::path::Path) -> Connection {
    Connection::open(dir.join("data/aurelius.sqlite")).unwrap()
}

#[test]
fn a_schema_39_store_keeps_its_projects_and_its_links_start_empty() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 39).unwrap());
    let conn = db(dir.path());
    assert!(conn.prepare("SELECT styleguide_url FROM projects").is_err(), "new in 40");
    assert!(conn.prepare("SELECT design_url FROM tickets").is_err(), "new in 40");
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', ?1, ?2)", [&global, PROJECT]).unwrap();
    conn.execute("INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES (?1, 'ps', 'Atlas', 'active', 3, 1, 1)", [PROJECT]).unwrap();
    drop(conn);

    let store = Store::open(dir.path()).unwrap();
    let read_blob = |_: &str| None;
    let projects = store.read(|c| crate::queries::list_projects(c, &read_blob)).unwrap();
    let atlas = projects.iter().find(|p| p.id.to_string() == PROJECT).unwrap();
    assert_eq!((atlas.name.as_str(), atlas.version, atlas.styleguide_url.as_deref()), ("Atlas", 3, None));

    let id = atlas.id;
    let link = "https://claude.ai/artifact/abc";
    let version = store.write(move |tx| crate::company::update_project(tx, id, None, Some(Some(link)), None, None)).unwrap();
    assert_eq!(version, 4, "one version for the change");
    assert_eq!(store.read(move |c| crate::company::project_styleguide(c, id)).unwrap().as_deref(), Some(link));
    store.write(move |tx| crate::company::update_project(tx, id, None, Some(None), None, None).map(drop)).unwrap();
    assert_eq!(store.read(move |c| crate::company::project_styleguide(c, id)).unwrap(), None, "cleared");
}
