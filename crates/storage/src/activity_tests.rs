//! `activity.overview` on the bare tables: what another board member may not see,
//! and the due-job claim of a paused employee.

use crate::{Store, activity, events, inbox, now_ms, queries};
use aurelius_domain::{ActorKind, ConversationId, EmployeeId, EventId, RunId, UserId};
use rusqlite::Connection;

const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000501";
const PRIVATE: &str = "00000000-0000-7000-8000-000000000502";
const OWNER: &str = "00000000-0000-7000-8000-000000000503";
const SECRET: &str = "Salary talks";

fn setup() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let conn = Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es', 'employee', ?1, ?2)", [&global, EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES (?1, 'es', 'Ann', 'Dev', '{}', 'active', 3, 1, 2)", [EMPLOYEE]).unwrap();
    conn.execute("INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) SELECT ?1, id, 'sam', 'x', 1, 1 FROM company", [OWNER]).unwrap();
    conn.execute("INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms, board_member_id) VALUES (?1, 'employee', NULL, ?2, ?3, 1, 1, ?4)", [PRIVATE, EMPLOYEE, SECRET, OWNER]).unwrap();
    (dir, store)
}

fn owner() -> UserId {
    UserId::parse(OWNER).unwrap()
}

fn source_event(tx: &rusqlite::Transaction<'_>, run: RunId) -> Result<EventId, crate::StorageError> {
    Ok(events::append(tx, events::NewEvent {
        aggregate_type: "run", aggregate_id: run.to_string(), aggregate_version: 1, event_type: "run.queued", project_id: None,
        actor_kind: ActorKind::User, actor_id: None, correlation_id: "test".into(), causation_id: None,
        payload_blob_id: None, metadata: serde_json::json!({}), dedupe_key: format!("q-{run}"),
    })?.id)
}

fn deliver_private(store: &Store) {
    let run = RunId::new();
    store.write(move |tx| {
        let source_event = source_event(tx, run)?;
        let payload = serde_json::json!({});
        let thread = format!("chat:{PRIVATE}");
        inbox::insert(tx, inbox::NewItem { employee: EmployeeId::parse(EMPLOYEE).unwrap(), thread_key: &thread, ticket: None, conversation: ConversationId::parse(PRIVATE).unwrap(), kind: "message", source: "board", hop: 0, payload: &payload, login_user: None, task: None, source_event, run })
    }).unwrap();
}

#[test]
fn another_board_members_private_chat_is_counted_without_title_or_place() {
    let (_dir, store) = setup();
    deliver_private(&store);
    let stranger = UserId::new();
    let seen = store.read(move |c| activity::overview(c, Some(stranger), now_ms())).unwrap();

    assert_eq!((seen.waiting.len(), seen.waiting[0].employee_name.as_str(), seen.waiting[0].work.private), (1, "Ann", true));
    let wire = serde_json::to_string(&seen).unwrap();
    assert!(!wire.contains(SECRET) && !wire.contains(PRIVATE) && !wire.contains("sam"), "{wire}");
    assert!(seen.tickets.is_empty());
    let anyone = store.read(|c| activity::overview(c, None, now_ms())).unwrap();
    assert!(anyone.waiting[0].work.private, "no board member on the connection sees nothing of it either");
}

#[test]
fn a_board_members_own_private_chat_is_named_for_them() {
    let (_dir, store) = setup();
    deliver_private(&store);
    let mine = store.read(|c| activity::overview(c, Some(owner()), now_ms())).unwrap();
    assert!(!mine.waiting[0].work.private);
    assert_eq!(mine.waiting[0].work.chat_id, ConversationId::parse(PRIVATE).ok());
}

fn run_job(store: &Store) -> String {
    let run = RunId::new();
    store.write(move |tx| {
        let source = source_event(tx, run)?;
        let payload = serde_json::json!({ "employee_id": EMPLOYEE, "conversation_id": PRIVATE });
        events::insert_job(tx, &run.to_string(), "run.chat", source, &format!("run:{run}"), &payload)
    }).unwrap();
    run.to_string()
}

fn pause(store: &Store) {
    store.write(|tx| Ok(tx.execute("UPDATE employees SET status = 'paused' WHERE id = ?1", [EMPLOYEE]).map(|_| ())?)).unwrap();
}

#[test]
fn claim_due_job_skips_a_chat_run_of_a_paused_employee_until_it_is_resumed() {
    let (_dir, store) = setup();
    let job = run_job(&store);
    pause(&store);
    let now = now_ms() + 1;
    assert!(store.write(move |tx| queries::claim_due_job(tx, "t", now, 1_000, 4)).unwrap().is_none());
    assert_eq!(store.read(move |c| queries::held_runs(c, now)).unwrap(), 0, "the machine does not hold it");
    assert!(!store.read(queries::has_work).unwrap(), "a shutdown does not wait for a paused employee");

    store.write(|tx| Ok(tx.execute("UPDATE employees SET status = 'active' WHERE id = ?1", [EMPLOYEE]).map(|_| ())?)).unwrap();
    let claimed = store.write(move |tx| queries::claim_due_job(tx, "t", now, 1_000, 4)).unwrap();
    assert_eq!(claimed.map(|j| j.id), Some(job));
}

fn task_job(store: &Store, parked: bool) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let task = id.clone();
    store.write(move |tx| {
        let id = task;
        tx.execute("INSERT OR IGNORE INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES ('b', 'k', 's', 0, 'text/plain', 1)", [])?;
        let global: String = tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?;
        tx.execute("INSERT OR IGNORE INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', ?1, 'p')", [&global])?;
        tx.execute("INSERT OR IGNORE INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('p', 'ps', 'Alpha', 'active', 1, 1, 1)", [])?;
        tx.execute("INSERT INTO tasks(id, project_id, title, brief_blob_id, status, waiting_reason, version, created_at_ms, updated_at_ms, employee_id) VALUES (?1, 'p', 'Work', 'b', 'waiting', 'paused', 1, 1, 1, ?2)", [&id, &EMPLOYEE.to_owned()])?;
        let source = source_event(tx, RunId::new())?;
        events::insert_job(tx, &id, "task.run", source, &format!("task:{id}"), &serde_json::json!({}))?;
        if parked {
            tx.execute("UPDATE jobs SET last_error_code = 'paused' WHERE id = ?1", [&id])?;
        }
        Ok(())
    }).unwrap();
    id
}

#[test]
fn a_task_job_of_a_paused_employee_is_claimed_once_so_it_can_be_parked() {
    let (_dir, store) = setup();
    let job = task_job(&store, false);
    pause(&store);
    let now = now_ms() + 1;
    let claimed = store.write(move |tx| queries::claim_due_job(tx, "t", now, 1_000, 4)).unwrap();
    assert_eq!(claimed.map(|j| j.id), Some(job), "run_task sets it to waiting; it starts no work");
}

#[test]
fn a_parked_task_job_of_a_paused_employee_is_not_claimed_again() {
    let (_dir, store) = setup();
    task_job(&store, true);
    pause(&store);
    let now = now_ms() + 1;
    assert!(store.write(move |tx| queries::claim_due_job(tx, "t", now, 1_000, 4)).unwrap().is_none());
    let seen = store.read(move |c| activity::overview(c, None, now)).unwrap();
    assert_eq!(seen.waiting[0].waits_for, "paused");
}
