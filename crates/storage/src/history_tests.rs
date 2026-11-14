//! The History's read and writes (ticket #57): only Done, newest first, every filter,
//! keyset paging without gaps or doubles, and a summary that belongs to one Done.

use crate::{Store, events, history, tickets};
use aurelius_domain::ActorKind;
use aurelius_domain::history::HistorySummaryState;
use aurelius_domain::{EmployeeId, ProjectId, TicketId};

const P1: &str = "00000000-0000-7000-8000-000000000001";
const P2: &str = "00000000-0000-7000-8000-000000000002";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const ANN: &str = "00000000-0000-7000-8000-000000000004";
const KIND: &str = "memory.extract";

fn ticket_id(number: u32) -> String {
    format!("00000000-0000-7000-8000-0000000001{number:02}")
}

/// Two projects, Ann, and tickets #1..#6: #1..#5 on Done at 100·n (#5 at the same
/// moment as #4, archived, in Beta, without an owner), #6 in Doing.
fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|tx| {
            let company: String = tx.query_row("SELECT id FROM company", [], |r| r.get(0))?;
            let global: String = tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?;
            let column = |kind: &str| -> rusqlite::Result<String> { tx.query_row("SELECT id FROM board_columns WHERE kind = ?1", [kind], |r| r.get(0)) };
            let (doing, done) = (column("doing")?, column("done")?);
            tx.execute_batch(&format!(
                "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('s1', 'project', '{global}', '{P1}'), ('s2', 'project', '{global}', '{P2}');
                 INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES
                   ('{P1}', 's1', 'Alpha', 'active', 1, 1, 1), ('{P2}', 's2', 'Beta', 'archived', 1, 1, 1);
                 INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{ALEX}', '{company}', 'alex', '$argon2id$x', 1, 1);"
            ))?;
            tx.execute_batch(&format!(
                "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es', 'employee', '{global}', '{ANN}');
                 INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES ('{ANN}', 'es', 'Ann', 'Dev', '{{}}', 'archived', 1, 1, 1);"
            ))?;
            for n in 1..=6u32 {
                let (column, done, project, owner, archived) = match n {
                    5 => (&done, Some(400), P2, None, Some(500)),
                    6 => (&doing, None, P1, Some(ANN), None),
                    _ => (&done, Some(100 * n as i64), P1, Some(ANN), None),
                };
                tx.execute("INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES (?1, 'ticket', ?2, ?1, 1, 1)", rusqlite::params![format!("c{n}"), project])?;
                let title = if n == 2 { "Export the report".to_owned() } else { format!("Ticket {n}") };
                tx.execute(
                    &format!(
                        "INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, priority, created_by_user, owner_employee_id,
                                             version, created_at_ms, updated_at_ms, done_at_ms, archived_at_ms, login_user, conversation_id)
                         VALUES (?1, '{company}', ?2, ?3, ?4, ?5, ?3, 'normal', '{ALEX}', ?6, 1, 1, 1, ?7, ?8, '{ALEX}', 'c' || ?3)"
                    ),
                    rusqlite::params![ticket_id(n), project, n, title, column, owner, done, archived],
                )?;
            }
            Ok(())
        })
        .unwrap();
    (dir, store)
}

fn id(number: u32) -> TicketId {
    TicketId::parse(&ticket_id(number)).unwrap()
}

fn numbers(store: &Store, filter: &history::Filter, limit: u32) -> (Vec<u32>, bool) {
    let (rows, more) = store.read(|c| history::list(c, filter, limit, KIND)).unwrap();
    (rows.iter().map(|r| r.number).collect(), more)
}

#[test]
fn the_history_has_only_done_newest_first_with_archived_ones() {
    let (_dir, store) = store();
    let (rows, more) = store.read(|c| history::list(c, &history::Filter::default(), 50, KIND)).unwrap();
    assert!(!more);
    assert_eq!(rows.iter().map(|r| r.number).collect::<Vec<_>>(), vec![5, 4, 3, 2, 1], "#6 is in Doing; #5 ties #4 and has the higher number");
    let five = &rows[0];
    assert_eq!((five.project_name.as_str(), five.owner_employee_id, five.owner_name.as_deref()), ("Beta", None, None), "archived ticket and project stay");
    assert_eq!(rows[1].owner_name.as_deref(), Some("Ann"), "an archived employee keeps its name");
    assert_eq!(five.summary_state, HistorySummaryState::BeforeHistory);
}

#[test]
fn every_filter_narrows_and_they_add_up() {
    let (_dir, store) = store();
    let p1 = ProjectId::parse(P1).unwrap();
    assert_eq!(numbers(&store, &history::Filter { project: Some(p1), ..Default::default() }, 50).0, vec![4, 3, 2, 1]);
    let ann: EmployeeId = store.read(|c| Ok(c.query_row("SELECT owner_employee_id FROM tickets WHERE number = 1", [], |r| r.get::<_, String>(0))?)).map(|e| EmployeeId::parse(&e).unwrap()).unwrap();
    assert_eq!(numbers(&store, &history::Filter { employee: Some(ann), ..Default::default() }, 50).0, vec![4, 3, 2, 1], "the owner");
    assert_eq!(numbers(&store, &history::Filter { from_ms: Some(200), to_ms: Some(400), ..Default::default() }, 50).0, vec![3, 2], "from included, to excluded");
    assert_eq!(numbers(&store, &history::Filter { query: Some("  REPORT ".into()), ..Default::default() }, 50).0, vec![2], "the title, case-insensitive");
    store.write(|tx| history::set_summary(tx, id(3), "It added 100% of the CSV_export.")).unwrap();
    assert_eq!(numbers(&store, &history::Filter { query: Some("csv_".into()), ..Default::default() }, 50).0, vec![3], "the summary; _ is no wildcard");
    assert_eq!(numbers(&store, &history::Filter { query: Some("%".into()), ..Default::default() }, 50).0, vec![3], "% is no wildcard");
    assert_eq!(numbers(&store, &history::Filter { project: Some(p1), query: Some("ticket".into()), from_ms: Some(300), ..Default::default() }, 50).0, vec![4, 3]);
    assert_eq!(numbers(&store, &history::Filter { query: Some("nothing like it".into()), ..Default::default() }, 50), (vec![], false));
}

#[test]
fn keyset_pages_follow_each_other_without_gaps_or_doubles() {
    let (_dir, store) = store();
    let mut seen = Vec::new();
    let mut before = None;
    loop {
        let (rows, more) = store.read(|c| history::list(c, &history::Filter { before, ..Default::default() }, 2, KIND)).unwrap();
        seen.extend(rows.iter().map(|r| r.number));
        before = rows.last().map(|r| (r.done_at_ms, r.number));
        if !more {
            break;
        }
    }
    assert_eq!(seen, vec![5, 4, 3, 2, 1], "#4 and #5 share a moment and are split across pages");
}

#[test]
fn a_summary_belongs_to_one_done_and_a_new_done_starts_again() {
    let (_dir, store) = store();
    let state = |n: u32| store.read(move |c| history::list(c, &history::Filter::default(), 50, KIND)).unwrap().0.into_iter().find(|r| r.number == n).map(|r| (r.summary_state, r.summary));
    // Done: pending; without a job that is "not available", never an endless wait.
    store.write(|tx| tickets::mark_done(tx, id(1))).unwrap();
    assert_eq!(state(1), Some((HistorySummaryState::NotAvailable, None)));
    store
        .write(|tx| {
            let source = events::append(tx, events::NewEvent {
                aggregate_type: "ticket", aggregate_id: ticket_id(1), aggregate_version: 1, event_type: "ticket.accepted", project_id: None,
                actor_kind: ActorKind::User, actor_id: None, correlation_id: "test".into(), causation_id: None,
                payload_blob_id: None, metadata: serde_json::json!({}), dedupe_key: "accepted-1".into(),
            })?
            .id;
            events::insert_job(tx, "extract-1", KIND, source, "extract-1", &serde_json::json!({ "ticket_id": ticket_id(1) }))
        })
        .unwrap();
    assert_eq!(state(1).map(|s| s.0), Some(HistorySummaryState::Summarizing), "its extraction is queued");
    store.write(|tx| Ok(tx.execute("UPDATE jobs SET state = 'done' WHERE id = 'extract-1'", [])?)).unwrap();
    assert!(store.write(|tx| history::set_summary(tx, id(1), "It shipped.")).unwrap());
    assert!(!store.write(|tx| history::set_summary(tx, id(1), "A second text.")).unwrap(), "a job that runs again does not overwrite it");
    assert!(!store.write(|tx| history::set_failed(tx, id(1))).unwrap(), "nor does a late failure");
    assert_eq!(state(1), Some((HistorySummaryState::Ready, Some("It shipped.".into()))));
    assert!(!store.write(|tx| history::set_summary(tx, id(6), "Not done.")).unwrap(), "only on Done");

    // B2: a new Done clears it and has a new moment; its new summary is written.
    let before: i64 = store.read(|c| Ok(c.query_row("SELECT done_at_ms FROM tickets WHERE number = 1", [], |r| r.get(0))?)).unwrap();
    store.write(|tx| tickets::mark_done(tx, id(1))).unwrap();
    assert_eq!(state(1).map(|s| s.1), Some(None));
    let after: i64 = store.read(|c| Ok(c.query_row("SELECT done_at_ms FROM tickets WHERE number = 1", [], |r| r.get(0))?)).unwrap();
    assert!(after >= before);
    assert!(store.write(|tx| history::set_failed(tx, id(1))).unwrap());
    assert!(store.write(|tx| history::set_summary(tx, id(1), "It shipped again.")).unwrap(), "after a failure the retry's text comes");
    assert_eq!(state(1), Some((HistorySummaryState::Ready, Some("It shipped again.".into()))));
}

#[test]
fn a_search_finds_a_title_with_an_accented_capital() {
    let (_dir, store) = store();
    store.write(|tx| Ok(tx.execute("UPDATE tickets SET title = 'Écran de connexion' WHERE number = 1", [])?)).unwrap();
    let found = |q: &str| numbers(&store, &history::Filter { query: Some(q.into()), ..Default::default() }, 50).0;
    // SQLite's lower() folds only ASCII, so the typed text is folded the same way.
    assert_eq!(found("Écran"), vec![1], "as shown");
    assert_eq!(found("ÉCRAN DE"), vec![1], "the ASCII letters in any case");
    assert_eq!(found("connexion"), vec![1]);
}
