//! `work_sessions::list`: the order follows what last happened in a session (an
//! answer that keeps building counts from its latest step, not its start), and
//! `unseen` says a run ended after the board member last opened the session.

use crate::{Store, discussions, work_sessions};
use aurelius_domain::{ConversationId, UserId};

const ALPHA: &str = "00000000-0000-7000-8000-0000000005a1";
const ORION: &str = "00000000-0000-7000-8000-0000000005b1";
const ALEX: &str = "00000000-0000-7000-8000-0000000005c1";
/// Its last message started at 300; nothing ran after it.
const S1: &str = "00000000-0000-7000-8000-0000000005d1";
/// Its answer started at 100 and kept building: a step at 500, the run ended at 600.
const S2: &str = "00000000-0000-7000-8000-0000000005d2";

fn conv(id: &str) -> ConversationId {
    ConversationId::parse(id).unwrap()
}

fn alex() -> UserId {
    UserId::parse(ALEX).unwrap()
}

fn sessions() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|tx| {
            let company: String = tx.query_row("SELECT id FROM company", [], |r| r.get(0))?;
            let global: String = tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?;
            tx.execute_batch(&format!(
                "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('pa', 'project', '{global}', '{ALPHA}'), ('eo', 'employee', '{global}', '{ORION}');
                 INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{ALPHA}', 'pa', 'Alpha', 'active', 1, 1, 1);
                 INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES
                   ('{ORION}', 'eo', 'Orion', 'PM', '{{}}', 'active', 1, 1, 1);
                 INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{ALEX}', '{company}', 'alex', '$argon2id$x', 1, 1);
                 INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES
                   ('{S1}', 'session', '{ALPHA}', '{ORION}', 'Quick question', 1, 10),
                   ('{S2}', 'session', '{ALPHA}', '{ORION}', 'Long refactor', 1, 20);
                 INSERT INTO work_sessions(conversation_id, started_by, created_at_ms) VALUES ('{S1}', '{ALEX}', 10), ('{S2}', '{ALEX}', 20);
                 INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms) VALUES
                   ('e1', 'conversation', '{S1}', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'k1', 1, 1),
                   ('e2', 'conversation', '{S2}', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'k2', 1, 1);
                 INSERT INTO messages(id, conversation_id, ordinal, role, source_event_id, status, input_kind, created_at_ms) VALUES
                   ('m1', '{S1}', 1, 'user', 'e1', 'complete', 'text', 300),
                   ('m2', '{S2}', 1, 'assistant', 'e2', 'complete', 'text', 100);
                 INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status, started_at_ms, ended_at_ms)
                   VALUES ('r2', '{S2}', 1, 'fake', 'm', '{{}}', 'succeeded', 100, 600);
                 INSERT INTO run_activity(run_id, call_id, message_id, tool, input_json, text_offset, started_at_ms, ended_at_ms)
                   VALUES ('r2', 'c1', 'm2', 'Bash', '{{}}', 0, 450, 500);"
            ))?;
            Ok(())
        })
        .unwrap();
    (dir, store)
}

fn listed(store: &Store) -> Vec<(String, i64, bool)> {
    store
        .read(|c| work_sessions::list(c, alex(), None))
        .unwrap()
        .into_iter()
        .map(|s| (s.title, s.last_activity_ms, s.unseen))
        .collect()
}

#[test]
fn an_answer_that_kept_building_counts_from_its_last_step_not_its_start() {
    let (_dir, store) = sessions();
    let rows = listed(&store);
    assert_eq!(rows.iter().map(|r| (r.0.as_str(), r.1)).collect::<Vec<_>>(), [("Long refactor", 600), ("Quick question", 300)]);
    let (history, _) = store.read(|c| work_sessions::history(c, &Default::default(), 10, "session.summarize")).unwrap();
    assert_eq!(history.iter().map(|e| e.last_activity_ms).collect::<Vec<_>>(), [600, 300], "the History orders the same way");
}

#[test]
fn a_step_of_a_running_answer_moves_the_session_up() {
    let (_dir, store) = sessions();
    store
        .write(|tx| {
            tx.execute_batch(&format!(
                "INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status, started_at_ms)
                   VALUES ('r1', '{S1}', 1, 'fake', 'm', '{{}}', 'running', 300);
                 INSERT INTO run_activity(run_id, call_id, message_id, tool, input_json, text_offset, started_at_ms) VALUES ('r1', 'c1', 'm1', 'Read', '{{}}', 0, 700);"
            ))?;
            Ok(())
        })
        .unwrap();
    assert_eq!(listed(&store)[0], ("Quick question".to_owned(), 700, false), "running: not unseen");
}

#[test]
fn a_session_is_unseen_when_a_run_ended_after_it_was_last_opened() {
    let (_dir, store) = sessions();
    assert!(listed(&store).iter().all(|r| !r.2), "never opened here: no mark (sessions from before the mark)");
    store
        .write(|tx| {
            tx.execute(
                "INSERT INTO discussion_reads(user_id, conversation_id, read_through, read_at_ms) VALUES (?1, ?2, 1, 550)",
                [ALEX, S2],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(listed(&store)[0], ("Long refactor".to_owned(), 600, true), "the run ended at 600, opened at 550");
    store.write(|tx| discussions::mark_read(tx, alex(), conv(S2))).unwrap();
    assert!(!listed(&store)[0].2, "opening it clears the mark");
}
