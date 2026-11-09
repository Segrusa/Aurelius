//! Migration 36: the employees' mailbox. A schema 35 store keeps what it has, and
//! its conversations are no thread's own.

use crate::{Store, inbox, migrate};
use aurelius_domain::{ConversationId, EmployeeId, MessageId, RunId};
use rusqlite::Connection;

const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000361";
const CHAT: &str = "00000000-0000-7000-8000-000000000362";

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

#[test]
fn a_schema_35_store_gets_an_empty_mailbox_and_keeps_its_conversations() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 35).unwrap());
    let conn = db(dir.path());
    employee(&conn);
    assert!(conn.prepare("SELECT id FROM employee_inbox").is_err(), "new in 36");
    drop(conn);

    let store = Store::open(dir.path()).unwrap();
    let conn = db(dir.path());
    let (threads, key): (i64, Option<String>) = conn.query_row("SELECT (SELECT COUNT(*) FROM employee_threads), thread_key FROM conversations WHERE id = ?1", [CHAT], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((threads, key), (0, None));
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM employee_inbox", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    assert!(store.read(|c| inbox::due(c, i64::MAX / 2)).unwrap().is_empty());
    assert!(migrate::supported_version() >= 36);
}

#[test]
fn a_thread_has_one_hidden_conversation_and_an_item_points_at_the_run_it_goes_into() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let conn = db(dir.path());
    employee(&conn);
    drop(conn);
    let ann = EmployeeId::parse(EMPLOYEE).unwrap();
    let chat = ConversationId::parse(CHAT).unwrap();
    let (a, b) = store
        .write(move |tx| {
            let a = inbox::thread_conversation(tx, ann, "channel:x", "the x channel")?.unwrap();
            let b = inbox::thread_conversation(tx, ann, "channel:x", "the x channel")?.unwrap();
            Ok((a, b))
        })
        .unwrap();
    assert_eq!(a, b);
    assert_ne!(a, chat, "not the main chat");
    let main = store.read(move |c| crate::employees::main_chat(c, ann)).unwrap().unwrap();
    assert_eq!(main, chat, "the main chat stays the first conversation without a thread");
    assert_eq!(store.read(move |c| crate::conversations::employee_conversations(c, ann)).unwrap().len(), 1, "no list shows it");

    let run = RunId::new();
    store
        .write(move |tx| {
            let appended = crate::events::append(tx, crate::events::NewEvent {
                aggregate_type: "run", aggregate_id: run.to_string(), aggregate_version: 1, event_type: "run.queued", project_id: None,
                actor_kind: aurelius_domain::ActorKind::User, actor_id: None, correlation_id: "test".into(), causation_id: None,
                payload_blob_id: None, metadata: serde_json::json!({}), dedupe_key: format!("q-{run}"),
            })?;
            inbox::insert(tx, inbox::NewItem {
                employee: ann, thread_key: "chat:x", ticket: None, conversation: chat, kind: "message", source: "board", hop: 0,
                payload: &serde_json::json!({ "reply_to": MessageId::new() }), login_user: None, task: None, source_event: appended.id, run,
            })
        })
        .unwrap();
    assert_eq!(store.read(move |c| crate::conversations::run_status(c, run)).unwrap().as_deref(), Some("created"), "a queued item counts as a run not yet started");
    let due = store.read(|c| inbox::due(c, i64::MAX / 2)).unwrap();
    assert_eq!((due.len(), due[0].run), (1, run));
    let id = due[0].id.clone();
    let merged_into = RunId::new();
    store.write(move |tx| inbox::claim(tx, &[id], merged_into)).unwrap();
    assert!(store.read(|c| inbox::due(c, i64::MAX / 2)).unwrap().is_empty());
    let (item_run, trigger): (String, String) = db(dir.path()).query_row("SELECT i.run_id, t.run_id FROM employee_inbox i JOIN run_triggers t ON t.item_id = i.id", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((item_run, trigger), (merged_into.to_string(), merged_into.to_string()));
}
