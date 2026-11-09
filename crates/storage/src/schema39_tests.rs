//! Migration 39: the activity log. A schema 38 store keeps what it has; a row has no
//! foreign keys, finds its way per thread, employee and run, and goes after 90 days.

use crate::activity_log::{self, Filter};
use crate::{Store, employees, events, inbox, migrate, now_ms};
use aurelius_domain::employee::ActivityState;
use aurelius_domain::{ActorKind, ConversationId, EmployeeId, EventId, RunId, UserId};
use rusqlite::Connection;

const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000391";
const CHAT: &str = "00000000-0000-7000-8000-000000000392";
const PRIVATE: &str = "00000000-0000-7000-8000-000000000393";
const OWNER: &str = "00000000-0000-7000-8000-000000000394";

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
    conn.execute("INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) SELECT ?1, id, 'sam', 'x', 1, 1 FROM company", [OWNER]).unwrap();
    conn.execute("INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms, board_member_id) VALUES (?1, 'employee', NULL, ?2, 'Ann', 1, 1, ?3)", [PRIVATE, EMPLOYEE, OWNER]).unwrap();
}

fn ann() -> EmployeeId {
    EmployeeId::parse(EMPLOYEE).unwrap()
}

fn owner() -> UserId {
    UserId::parse(OWNER).unwrap()
}

/// An item for `run` in `conversation`, as the worker writes it.
fn deliver(store: &Store, thread: &'static str, conversation: &str, run: RunId) -> String {
    let conversation = ConversationId::parse(conversation).unwrap();
    store
        .write(move |tx| {
            let appended = events::append(tx, events::NewEvent {
                aggregate_type: "run", aggregate_id: run.to_string(), aggregate_version: 1, event_type: "run.queued", project_id: None,
                actor_kind: ActorKind::User, actor_id: None, correlation_id: "test".into(), causation_id: None,
                payload_blob_id: None, metadata: serde_json::json!({}), dedupe_key: format!("q-{run}"),
            })?;
            let source_event: EventId = appended.id;
            let payload = serde_json::json!({});
            inbox::insert(tx, inbox::NewItem { employee: ann(), thread_key: thread, ticket: None, conversation, kind: "message", source: "board", hop: 2, payload: &payload, login_user: None, task: None, source_event, run })?;
            Ok(())
        })
        .unwrap();
    store.read(|c| inbox::due(c, i64::MAX / 2)).unwrap().into_iter().find(|i| i.run == run).unwrap().id
}

fn steps(store: &Store, reader: Option<UserId>, filter: Filter) -> Vec<String> {
    let (rows, _) = store.read(move |c| activity_log::list(c, reader, &filter, 50)).unwrap();
    rows.into_iter().map(|r| r.step).collect()
}

#[test]
fn a_schema_38_store_gets_an_empty_log_and_keeps_what_it_has() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 38).unwrap());
    let conn = db(dir.path());
    assert!(conn.prepare("SELECT seq FROM activity_log").is_err(), "new in 39");
    employee(&conn);
    drop(conn);

    let store = Store::open(dir.path()).unwrap();
    assert!(steps(&store, None, Filter::default()).is_empty());
    assert_eq!(store.read(|c| employees::activity_state(c, ann())).unwrap(), ActivityState::Idle);
    assert!(migrate::supported_version() >= 39);
}

#[test]
fn a_delivery_and_its_queue_steps_are_found_per_employee_thread_and_run() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    employee(&db(dir.path()));
    let reserved = RunId::new();
    let item = deliver(&store, "chat:one", CHAT, reserved);
    let other = RunId::new();
    deliver(&store, "chat:two", CHAT, other);
    assert_eq!(store.read(|c| employees::activity_state(c, ann())).unwrap(), ActivityState::Queued);

    let run = RunId::new();
    let items = vec![item.clone()];
    store.write(move |tx| inbox::claim(tx, &items, run)).unwrap();

    let all = Filter { employee: Some(ann()), ..Default::default() };
    assert_eq!(steps(&store, None, all), ["claimed", "delivered", "delivered"]);
    let one = Filter { thread: Some("chat:one".into()), ..Default::default() };
    assert_eq!(steps(&store, None, one), ["claimed", "delivered"]);
    // The run also has what its item went through before the run existed.
    let of_run = Filter { run: Some(run), ..Default::default() };
    assert_eq!(steps(&store, None, of_run), ["claimed", "delivered"]);
    let queue = Filter { category: Some(activity_log::QUEUE.into()), ..Default::default() };
    assert_eq!(steps(&store, None, queue), ["claimed"]);

    let (rows, more) = store.read(|c| activity_log::list(c, None, &Filter::default(), 1)).unwrap();
    assert!(more);
    assert_eq!((rows[0].step.as_str(), rows[0].employee_name.as_deref(), rows[0].hop), ("claimed", Some("Ann"), Some(2)));
    assert_eq!(rows[0].place.as_ref().unwrap().label, "A removed chat");
    let before = Filter { before_seq: Some(rows[0].seq), ..Default::default() };
    assert_eq!(steps(&store, None, before), ["delivered", "delivered"]);
}

#[test]
fn a_cancelled_item_and_a_parked_thread_leave_a_step() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    employee(&db(dir.path()));
    let run = RunId::new();
    deliver(&store, "chat:one", CHAT, run);
    store.write(move |tx| inbox::cancel(tx, run)).unwrap();
    store.write(|tx| inbox::unpark(tx, EMPLOYEE, "chat:one")).unwrap();
    let thread = Filter { thread: Some("chat:one".into()), ..Default::default() };
    assert_eq!(steps(&store, None, thread), ["cancelled", "delivered"], "nothing was parked, so nothing opened again");
}

#[test]
fn someone_elses_private_chat_is_not_in_the_log_or_the_queue() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    employee(&db(dir.path()));
    let private: &'static str = Box::leak(format!("chat:{PRIVATE}").into_boxed_str());
    deliver(&store, private, PRIVATE, RunId::new());

    assert_eq!(steps(&store, Some(owner()), Filter::default()), ["delivered"]);
    assert_eq!(steps(&store, Some(UserId::new()), Filter::default()), Vec::<String>::new());
    assert_eq!(steps(&store, None, Filter::default()), Vec::<String>::new());

    let now = now_ms();
    let theirs = store.read(move |c| inbox::queue_of(c, ann(), Some(owner()), now)).unwrap();
    assert_eq!((theirs.len(), theirs[0].place.label.as_str(), theirs[0].state.as_str()), (1, "Chat with sam", "queued"));
    assert_eq!(theirs[0].place.chat_id, ConversationId::parse(PRIVATE).ok());
    assert!(store.read(move |c| inbox::queue_of(c, ann(), Some(UserId::new()), now)).unwrap().is_empty());
}

#[test]
fn a_row_needs_nothing_else_and_goes_after_90_days() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let nobody = EmployeeId::new();
    let conn = db(dir.path());
    conn.execute(
        "INSERT INTO activity_log(at_ms, category, step, employee_id, thread_key, run_id) VALUES (1, 'queue', 'old', ?1, 'ticket:gone', 'no-run'), (?2, 'queue', 'new', ?1, 'ticket:gone', 'no-run')",
        rusqlite::params![nobody.to_string(), now_ms()],
    )
    .unwrap();
    drop(conn);

    let (rows, _) = store.read(|c| activity_log::list(c, None, &Filter::default(), 10)).unwrap();
    assert_eq!(rows[0].place.as_ref().unwrap().label, "A removed ticket");
    assert_eq!(rows[0].employee_name, None, "a removed employee has no name, the row stays");
    assert_eq!(store.write(|tx| activity_log::prune(tx, now_ms())).unwrap(), 1);
    assert_eq!(steps(&store, None, Filter::default()), ["new"]);
    // Everything old goes also when nothing newer holds the place.
    assert_eq!(store.write(|tx| activity_log::prune(tx, now_ms() + activity_log::KEEP_MS + 1_000)).unwrap(), 1);
    assert_eq!(steps(&store, None, Filter::default()), Vec::<String>::new());
}
