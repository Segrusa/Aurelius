//! Migration 38: scheduled messages and the scratchpad, and who waits for a thread.
//! A schema 37 store keeps what it has.

use crate::{Store, inbox, migrate, scheduled, scratchpads};
use aurelius_domain::{ConversationId, EmployeeId, MessageId, RunId};
use rusqlite::Connection;

const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000381";
const CHAT: &str = "00000000-0000-7000-8000-000000000382";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn employee(conn: &Connection) {
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es', 'employee', ?1, ?2)", [&global, EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES (?1, 'es', 'Ann', 'Dev', '{}', 'active', 3, 1, 2)", [EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES (?1, 'employee', NULL, ?2, 'Ann', 1, 1)", [CHAT, EMPLOYEE]).unwrap();
}

fn ids() -> (EmployeeId, ConversationId) {
    (EmployeeId::parse(EMPLOYEE).unwrap(), ConversationId::parse(CHAT).unwrap())
}

fn new(thread_key: &str, due_at_ms: i64, hop: u32) -> scheduled::NewScheduled<'_> {
    let (ann, chat) = ids();
    scheduled::NewScheduled { from: ann, to: ann, content: "Check it", discussion: None, chat: Some(chat), thread_key, ticket: None, thread_parent: None, place: "Ann's own chat", hop, login_user: None, planned_by_run: RunId::new(), due_at_ms }
}

#[test]
fn a_schema_37_store_gets_empty_tables_and_keeps_what_it_has() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 37).unwrap());
    let conn = db(dir.path());
    employee(&conn);
    assert!(conn.prepare("SELECT id FROM scheduled_messages").is_err(), "new in 38");
    drop(conn);

    let store = Store::open(dir.path()).unwrap();
    assert!(store.read(|c| scheduled::due(c, i64::MAX / 2)).unwrap().is_empty());
    assert!(store.read(|c| scratchpads::get(c, ids().0)).unwrap().is_none());
    assert!(migrate::supported_version() >= 38);
}

#[test]
fn a_scheduled_message_is_due_once_and_settled_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    employee(&db(dir.path()));
    let id = store.write(|tx| scheduled::insert(tx, new("chat:a", 1_000, 2))).unwrap();
    assert!(store.read(|c| scheduled::due(c, 999)).unwrap().is_empty(), "not yet");
    let due = store.read(|c| scheduled::due(c, 1_000)).unwrap();
    assert_eq!((due.len(), due[0].id, due[0].hop, due[0].content.as_str()), (1, id, 2, "Check it"));
    assert_eq!(store.read(|c| scheduled::pending_of(c, ids().0)).unwrap(), 1);

    let run = RunId::new();
    store.write(move |tx| scheduled::settle(tx, id, "delivered", Some(run), None)).unwrap();
    assert!(store.read(|c| scheduled::due(c, 2_000)).unwrap().is_empty());
    store.write(move |tx| scheduled::settle(tx, id, "not_delivered", None, Some("late"))).unwrap();
    let all = store.read(|c| scheduled::list(c, None, 10)).unwrap();
    assert_eq!((all.len(), all[0].state.as_str(), all[0].run_id, all[0].outcome.as_deref()), (1, "delivered", Some(run), None), "the first settling stands");
    assert!(!store.write(move |tx| scheduled::cancel(tx, id, None)).unwrap(), "it no longer waits");
}

#[test]
fn a_scratchpad_is_one_text_per_employee_and_empty_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    employee(&db(dir.path()));
    let ann = ids().0;
    let run = RunId::new();
    store.write(move |tx| scratchpads::set(tx, ann, "First", run)).unwrap();
    store.write(move |tx| scratchpads::set(tx, ann, "Second", run)).unwrap();
    assert_eq!(store.read(move |c| scratchpads::get(c, ann)).unwrap().map(|(t, _)| t).as_deref(), Some("Second"));
    store.write(move |tx| scratchpads::set(tx, ann, "", run)).unwrap();
    assert!(store.read(move |c| scratchpads::get(c, ann)).unwrap().is_none());
}

/// A queued item of Ann in `thread`, not due before `not_before`.
fn item(store: &Store, thread: &'static str, not_before: i64, error: Option<&'static str>) {
    let (ann, chat) = ids();
    let run = RunId::new();
    store
        .write(move |tx| {
            let appended = crate::events::append(tx, crate::events::NewEvent {
                aggregate_type: "run", aggregate_id: run.to_string(), aggregate_version: 1, event_type: "run.queued", project_id: None,
                actor_kind: aurelius_domain::ActorKind::User, actor_id: None, correlation_id: "test".into(), causation_id: None,
                payload_blob_id: None, metadata: serde_json::json!({}), dedupe_key: format!("q-{run}"),
            })?;
            inbox::insert(tx, inbox::NewItem {
                employee: ann, thread_key: thread, ticket: None, conversation: chat, kind: "message", source: "board", hop: 0,
                payload: &serde_json::json!({ "reply_to": MessageId::new() }), login_user: None, task: None, source_event: appended.id, run,
            })?;
            tx.execute("UPDATE employee_inbox SET not_before_ms = ?2, last_error_code = ?3 WHERE run_id = ?1", rusqlite::params![run.to_string(), not_before, error])?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn a_waiter_is_behind_the_other_threads_that_go_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    employee(&db(dir.path()));
    let waiters = |thread: &'static str, now: i64| store.read(move |c| inbox::waiters(c, thread, now)).unwrap();
    assert!(waiters("ticket:a", 100).is_empty(), "nobody waits");

    // Thread b first (two items, one run), then a: Ann is behind b.
    item(&store, "ticket:b", 0, None);
    item(&store, "ticket:b", 0, None);
    item(&store, "ticket:a", 0, None);
    let found = waiters("ticket:a", 100);
    assert_eq!((found.len(), found[0].employee_name.as_str(), found[0].behind, found[0].reason.as_deref()), (1, "Ann", 1, None));
    assert_eq!(waiters("ticket:b", 100)[0].behind, 0, "nothing before it");

    // An item held for a usage limit says so, and is not in front of anyone.
    item(&store, "ticket:c", 500, Some("quota"));
    let found = waiters("ticket:c", 100);
    assert_eq!(found[0].reason.as_deref(), Some("limit"));
    assert_eq!(found[0].behind, 2, "b and a go first");
    assert_eq!(waiters("ticket:a", 100)[0].behind, 1, "c does not count: it is not due");
}
