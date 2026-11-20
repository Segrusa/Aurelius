//! `work_sessions::history` (feature `session_history`): every board member's
//! sessions on the day of their last activity, with filters, keyset paging and
//! the state of their summary.

use crate::{Store, events, work_sessions};
use aurelius_domain::history::HistorySummaryState;
use aurelius_domain::{ConversationId, EmployeeId, ProjectId};

const ALPHA: &str = "00000000-0000-7000-8000-0000000004a1";
const BETA: &str = "00000000-0000-7000-8000-0000000004a2";
const ORION: &str = "00000000-0000-7000-8000-0000000004b1";
const FLUX: &str = "00000000-0000-7000-8000-0000000004b2";
const ALEX: &str = "00000000-0000-7000-8000-0000000004c1";
const SAM: &str = "00000000-0000-7000-8000-0000000004c2";
/// Alex with Orion in Alpha; its last message at 300.
const S1: &str = "00000000-0000-7000-8000-0000000004d1";
/// Sam with Flux in Beta, no messages: last active at its start, 200.
const S2: &str = "00000000-0000-7000-8000-0000000004d2";
/// Alex with Orion in Alpha, its last message at 100.
const S3: &str = "00000000-0000-7000-8000-0000000004d3";
/// Deleted (archived): never in the History.
const S4: &str = "00000000-0000-7000-8000-0000000004d4";
const KIND: &str = "session.summarize";

fn conv(id: &str) -> ConversationId {
    ConversationId::parse(id).unwrap()
}

fn sessions() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|tx| {
            let company: String = tx.query_row("SELECT id FROM company", [], |r| r.get(0))?;
            let global: String = tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?;
            tx.execute_batch(&format!(
                "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('pa', 'project', '{global}', '{ALPHA}'), ('pb', 'project', '{global}', '{BETA}'),
                   ('eo', 'employee', '{global}', '{ORION}'), ('ef', 'employee', '{global}', '{FLUX}');
                 INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES
                   ('{ALPHA}', 'pa', 'Alpha', 'active', 1, 1, 1), ('{BETA}', 'pb', 'Beta', 'active', 1, 1, 1);
                 INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES
                   ('{ORION}', 'eo', 'Orion', 'PM', '{{}}', 'active', 1, 1, 1), ('{FLUX}', 'ef', 'Flux', 'Dev', '{{}}', 'active', 1, 1, 1);
                 INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES
                   ('{ALEX}', '{company}', 'alex', '$argon2id$x', 1, 1), ('{SAM}', '{company}', 'sam', '$argon2id$x', 1, 1);
                 INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms, archived_at_ms) VALUES
                   ('{S1}', 'session', '{ALPHA}', '{ORION}', 'Fix the login', 1, 10, NULL),
                   ('{S2}', 'session', '{BETA}', '{FLUX}', 'New logo', 1, 200, NULL),
                   ('{S3}', 'session', '{ALPHA}', '{ORION}', 'Release notes', 1, 20, NULL),
                   ('{S4}', 'session', '{ALPHA}', '{ORION}', '', 1, 30, 40);
                 INSERT INTO work_sessions(conversation_id, started_by, worktree, branch, created_at_ms) VALUES
                   ('{S1}', '{ALEX}', '/w/s1', 'session/s1', 10), ('{S2}', '{SAM}', NULL, NULL, 200),
                   ('{S3}', '{ALEX}', NULL, NULL, 20), ('{S4}', '{ALEX}', NULL, NULL, 30);
                 INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms) VALUES
                   ('e1', 'conversation', '{S1}', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'k1', 1, 1),
                   ('e2', 'conversation', '{S1}', 2, 'message.user_committed', 1, 'user', 'c', '{{}}', 'k2', 1, 1),
                   ('e3', 'conversation', '{S3}', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'k3', 1, 1),
                   ('e4', 'conversation', '{S4}', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'k4', 1, 1);
                 INSERT INTO messages(id, conversation_id, ordinal, role, source_event_id, status, input_kind, created_at_ms) VALUES
                   ('m1', '{S1}', 1, 'user', 'e1', 'complete', 'text', 150), ('m2', '{S1}', 2, 'assistant', 'e2', 'complete', 'text', 300),
                   ('m3', '{S3}', 1, 'user', 'e3', 'complete', 'text', 100), ('m4', '{S4}', 1, 'user', 'e4', 'complete', 'text', 999);"
            ))?;
            Ok(())
        })
        .unwrap();
    (dir, store)
}

fn read(store: &Store, filter: work_sessions::HistoryFilter, limit: u32) -> (Vec<String>, bool) {
    let (entries, more) = store.read(|c| work_sessions::history(c, &filter, limit, KIND)).unwrap();
    (entries.into_iter().map(|e| e.title).collect(), more)
}

#[test]
fn every_board_members_session_shows_by_its_last_activity_and_a_deleted_one_never() {
    let (_dir, store) = sessions();
    let (entries, more) = store.read(|c| work_sessions::history(c, &Default::default(), 10, KIND)).unwrap();
    assert!(!more);
    let titles: Vec<_> = entries.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(titles, ["Fix the login", "New logo", "Release notes"]);
    assert_eq!(entries.iter().map(|e| e.last_activity_ms).collect::<Vec<_>>(), [300, 200, 100]);
    assert_eq!(entries[0].started_by_name.as_deref(), Some("alex"));
    assert_eq!(entries[1].started_by_name.as_deref(), Some("sam"));
    assert_eq!(entries[0].branch.as_deref(), Some("session/s1"));
    assert_eq!(entries[0].employee_name, "Orion");
    assert_eq!(entries[1].project_name, "Beta");
}

#[test]
fn the_filters_narrow_on_project_employee_period_and_text() {
    let (_dir, store) = sessions();
    let project = |p: &str| work_sessions::HistoryFilter { project: Some(ProjectId::parse(p).unwrap()), ..Default::default() };
    assert_eq!(read(&store, project(BETA), 10).0, ["New logo"]);
    let employee = work_sessions::HistoryFilter { employee: Some(EmployeeId::parse(ORION).unwrap()), ..Default::default() };
    assert_eq!(read(&store, employee, 10).0, ["Fix the login", "Release notes"]);
    let period = work_sessions::HistoryFilter { from_ms: Some(150), to_ms: Some(300), ..Default::default() };
    assert_eq!(read(&store, period, 10).0, ["New logo"], "from included, to excluded");
    let text = work_sessions::HistoryFilter { query: Some("  LOGIN ".into()), ..Default::default() };
    assert_eq!(read(&store, text, 10).0, ["Fix the login"]);
}

#[test]
fn the_search_finds_a_session_by_its_summary() {
    let (_dir, store) = sessions();
    store.write(|tx| work_sessions::set_summary(tx, conv(S3), "Wrote the 0.9 changelog.", 100)).unwrap();
    let text = work_sessions::HistoryFilter { query: Some("changelog".into()), ..Default::default() };
    assert_eq!(read(&store, text, 10).0, ["Release notes"]);
}

#[test]
fn pages_follow_on_from_the_last_row() {
    let (_dir, store) = sessions();
    let (first, more) = store.read(|c| work_sessions::history(c, &Default::default(), 2, KIND)).unwrap();
    assert!(more);
    assert_eq!(first.len(), 2);
    let last = first.last().unwrap();
    let next = work_sessions::HistoryFilter { before: Some((last.last_activity_ms, last.conversation_id)), ..Default::default() };
    assert_eq!(read(&store, next, 2), (vec!["Release notes".to_owned()], false));
}

#[test]
fn the_summary_state_follows_the_column_and_the_job() {
    let (_dir, store) = sessions();
    let state = |store: &Store, id: &str| {
        let (entries, _) = store.read(|c| work_sessions::history(c, &Default::default(), 10, KIND)).unwrap();
        let e = entries.into_iter().find(|e| e.conversation_id == conv(id)).unwrap();
        (e.summary_state, e.summary)
    };
    assert_eq!(state(&store, S1), (HistorySummaryState::BeforeHistory, None));
    store
        .write(|tx| {
            work_sessions::mark_summary_pending(tx, conv(S1))?;
            tx.execute(
                "INSERT INTO jobs(id, kind, subject_id, source_event_id, dedupe_key, payload_json, state, not_before_ms) VALUES ('j1', ?1, 'j1', 'e1', 'j1', json_object('conversation_id', ?2), 'ready', 0)",
                [KIND, S1],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(state(&store, S1).0, HistorySummaryState::Summarizing);
    store.write(|tx| Ok(tx.execute("UPDATE jobs SET state = 'done' WHERE id = 'j1'", [])?)).unwrap();
    assert_eq!(state(&store, S1).0, HistorySummaryState::NotAvailable, "pending without a job never waits forever");
    assert!(store.write(|tx| work_sessions::set_summary_failed(tx, conv(S1))).unwrap());
    assert_eq!(state(&store, S1).0, HistorySummaryState::NotAvailable);
    assert!(store.write(|tx| work_sessions::set_summary(tx, conv(S1), "Fixed the login.", 300)).unwrap());
    assert_eq!(state(&store, S1), (HistorySummaryState::Ready, Some("Fixed the login.".into())));
    // A failed re-summary keeps the text; a new ask does not drop it either.
    assert!(!store.write(|tx| work_sessions::set_summary_failed(tx, conv(S1))).unwrap());
    store.write(|tx| work_sessions::mark_summary_pending(tx, conv(S1))).unwrap();
    assert_eq!(state(&store, S1), (HistorySummaryState::Ready, Some("Fixed the login.".into())));
    assert_eq!(store.read(|c| work_sessions::summary_so_far(c, conv(S1))).unwrap(), (Some("Fixed the login.".into()), Some(300)));
}

#[test]
fn a_ready_job_is_postponed_and_a_taken_one_is_not() {
    let (_dir, store) = sessions();
    store
        .write(|tx| {
            tx.execute_batch(&format!(
                "INSERT INTO jobs(id, kind, subject_id, source_event_id, dedupe_key, payload_json, state, not_before_ms) VALUES
                   ('ready', '{KIND}', 'ready', 'e1', 'ready', json_object('conversation_id', '{S1}'), 'ready', 10),
                   ('leased', '{KIND}', 'leased', 'e3', 'leased', json_object('conversation_id', '{S3}'), 'leased', 10);"
            ))?;
            Ok(())
        })
        .unwrap();
    assert!(store.write(|tx| events::postpone_ready_job(tx, KIND, "conversation_id", S1, 500)).unwrap());
    assert!(!store.write(|tx| events::postpone_ready_job(tx, KIND, "conversation_id", S3, 500)).unwrap());
    assert!(!store.write(|tx| events::postpone_ready_job(tx, "other.kind", "conversation_id", S1, 900)).unwrap());
    let due: Vec<i64> = store.read(|c| Ok(c.prepare("SELECT not_before_ms FROM jobs ORDER BY id")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)).unwrap();
    assert_eq!(due, [10, 500], "leased stays, ready moves");
}
