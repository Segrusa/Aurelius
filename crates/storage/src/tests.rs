use super::*;
use crate::events::{NewEvent, append};
use aurelius_domain::{ActorKind, ConversationId, EmployeeId, ProposalId, ProjectId, proposal::ProposalStatus};

fn event(aggregate_id: &str, version: u64, dedupe: &str) -> NewEvent<'static> {
    NewEvent {
        aggregate_type: "project",
        aggregate_id: aggregate_id.into(),
        aggregate_version: version,
        event_type: "project.created",
        project_id: None,
        actor_kind: ActorKind::User,
        actor_id: None,
        correlation_id: "test".into(),
        causation_id: None,
        payload_blob_id: None,
        metadata: serde_json::json!({}),
        dedupe_key: dedupe.into(),
    }
}

#[test]
fn migration_is_idempotent_and_detects_tampering() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path()).unwrap());
    // Reopening applies nothing new.
    drop(Store::open(dir.path()).unwrap());

    let conn = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    conn.execute("UPDATE schema_migrations SET checksum = 'x' WHERE version = 1", []).unwrap();
    drop(conn);
    assert!(matches!(Store::open(dir.path()), Err(StorageError::MigrationChecksum { version: 1 })));
}

#[test]
fn newer_database_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path()).unwrap());
    let conn = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    conn.execute("INSERT INTO schema_migrations VALUES (99, 'future', 0)", []).unwrap();
    drop(conn);
    assert!(matches!(Store::open(dir.path()), Err(StorageError::DatabaseTooNew { found: 99, .. })));
}

#[test]
fn a_database_of_an_older_release_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path()).unwrap());
    let conn = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    conn.execute("INSERT INTO schema_migrations VALUES (28, 'v0.6', 0)", []).unwrap();
    drop(conn);
    assert!(matches!(Store::open(dir.path()), Err(StorageError::DatabaseTooNew { found: 28, supported }) if supported == crate::migrate::supported_version()));
}

#[test]
fn only_known_event_kinds_enter_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let err = store.write(|tx| append(tx, NewEvent { event_type: "plan.proposed", ..event("a", 1, "k1") }).map(|_| ())).unwrap_err();
    assert!(err.to_string().contains("unknown event type plan.proposed"), "{err}");
    assert_eq!(store.read(queries::last_seq).unwrap(), 0);
}

#[test]
fn events_after_gives_typed_notices() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.write(|tx| append(tx, event("a", 1, "k1")).map(|_| ())).unwrap();
    let notices = store.read(|c| queries::events_after(c, 0, 10)).unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!((notices[0].seq, notices[0].kind, notices[0].aggregate_id.as_str()), (1, aurelius_domain::EventKind::ProjectCreated, "a"));
}

#[test]
fn a_receipt_keeps_the_method_it_answered() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|tx| {
            let appended = append(tx, event("a", 1, "k1"))?;
            events::insert_receipt(tx, "cmd-1", "hash", "project.create", "{}", appended.seq)
        })
        .unwrap();
    let found = store.read(|c| events::find_receipt(c, "cmd-1")).unwrap().unwrap();
    assert_eq!((found.method.as_str(), found.request_hash.as_str(), found.committed_seq), ("project.create", "hash", 1));
}

#[test]
fn events_are_append_only() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.write(|tx| append(tx, event("a", 1, "k1")).map(|_| ())).unwrap();

    let update = store.write(|tx| Ok(tx.execute("UPDATE events SET type = 'x'", [])?));
    assert!(update.unwrap_err().to_string().contains("append-only"));
    let delete = store.write(|tx| Ok(tx.execute("DELETE FROM events", [])?));
    assert!(delete.unwrap_err().to_string().contains("append-only"));
}

#[test]
fn aggregate_versions_must_be_gap_free() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let err = store.write(|tx| append(tx, event("a", 2, "k1")).map(|_| ())).unwrap_err();
    assert!(matches!(err, StorageError::AggregateVersion { expected: 1, actual: 0, .. }));
}

#[test]
fn failed_write_rolls_back_event_and_projection_together() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let pid = ProjectId::new();

    // Simulates a crash/failure after event + projection were written but before commit.
    let result = store.write(move |tx| {
        append(tx, event(&pid.to_string(), 1, "k1"))?;
        events::insert_project_projection(tx, pid, "Atlas", None)?;
        Err::<(), _>(StorageError::Rejected("injected fault before commit".into()))
    });
    assert!(result.is_err());

    let (events, projects, scopes) = store
        .read(|c| {
            Ok((
                queries::last_seq(c)?,
                c.query_row("SELECT COUNT(*) FROM projects", [], |r| r.get::<_, i64>(0))?,
                // Migration 18 seeds the global scope (the company's).
                c.query_row("SELECT COUNT(*) FROM scopes WHERE kind <> 'global'", [], |r| r.get::<_, i64>(0))?,
            ))
        })
        .unwrap();
    assert_eq!((events, projects, scopes), (0, 0, 0));
}

#[test]
fn blob_is_durable_and_hashed() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let blob = store.blobs().put(b"hello", "text/plain").unwrap();
    assert_eq!(blob.sha256, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    assert_eq!(store.blobs().read(&blob.storage_key).unwrap(), b"hello");
    assert!(std::fs::read_dir(dir.path().join("blobs/tmp")).unwrap().next().is_none());
}

#[test]
fn job_lease_epoch_fences_stale_owner() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|tx| {
            let appended = append(tx, event("job-1", 1, "k1"))?;
            events::insert_job(tx, "job-1", "memory.extract", appended.id, "x:1", &serde_json::json!({}))
        })
        .unwrap();

    let first = store.write(|tx| queries::claim_due_job(tx, "a", now_ms(), -1, 4)).unwrap().unwrap();
    // Lease already expired (negative lease): reconciliation releases it and a new owner claims.
    assert_eq!(store.write(|tx| queries::release_leases(tx, "memory.extract", now_ms())).unwrap(), 1);
    let second = store.write(|tx| queries::claim_due_job(tx, "b", now_ms(), 45_000, 4)).unwrap().unwrap();
    assert_eq!(second.lease_epoch, first.lease_epoch + 1);

    let stale_epoch = first.lease_epoch;
    assert!(!store.write(move |tx| queries::complete_job(tx, "job-1", stale_epoch)).unwrap());
    let epoch = second.lease_epoch;
    assert!(store.write(move |tx| queries::complete_job(tx, "job-1", epoch)).unwrap());
}

fn run_job(store: &Store, id: &str, conversation: &str) {
    let (id, conversation) = (id.to_owned(), conversation.to_owned());
    store
        .write(move |tx| {
            let appended = append(tx, event(&id, 1, &format!("k-{id}")))?;
            events::insert_job(tx, &id, "run.chat", appended.id, &format!("d-{id}"), &serde_json::json!({ "conversation_id": conversation }))
        })
        .unwrap();
}

#[test]
fn claims_respect_per_conversation_and_global_limits() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    run_job(&store, "a1", "conv-a");
    run_job(&store, "a2", "conv-a");
    run_job(&store, "b1", "conv-b");
    let now = now_ms();
    let claim = |limit: u32| store.write(move |tx| queries::claim_due_job(tx, "w", now, 45_000, limit)).unwrap().map(|j| j.id);

    assert_eq!(claim(4).as_deref(), Some("a1"));
    // a2 shares the conversation with the running a1: b1 goes first.
    assert_eq!(claim(4).as_deref(), Some("b1"));
    assert_eq!(claim(4), None, "conv-a is busy, nothing else is due");

    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    run_job(&store, "x", "c1");
    run_job(&store, "y", "c2");
    let now = now_ms() + 1;
    let first = store.write(move |tx| queries::claim_due_job(tx, "w", now, 45_000, 1)).unwrap();
    assert!(first.is_some());
    let second = store.write(move |tx| queries::claim_due_job(tx, "w", now, 45_000, 1)).unwrap();
    assert!(second.is_none(), "global limit of one active run");
}

#[test]
fn task_starts_count_as_runs_and_a_zero_limit_holds_runs_only() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let job = |id: &'static str, kind: &'static str| {
        store
            .write(move |tx| {
                let appended = append(tx, event(id, 1, &format!("k-{id}")))?;
                events::insert_job(tx, id, kind, appended.id, &format!("d-{id}"), &serde_json::json!({ "conversation_id": id }))
            })
            .unwrap()
    };
    job("t", "task.run");
    job("c", "run.chat");
    job("f", "memory.extract");
    let now = now_ms() + 1;
    let claim = |limit: u32| store.write(move |tx| queries::claim_due_job(tx, "w", now, 45_000, limit)).unwrap().map(|j| j.id);

    // Held back: the runs wait, other work still goes.
    assert_eq!(claim(0).as_deref(), Some("f"));
    assert_eq!(claim(0), None);
    assert_eq!(store.write(move |tx| queries::mark_held_runs(tx, "load:cpu", now)).unwrap(), vec!["c".to_string()]);
    assert!(store.write(move |tx| queries::mark_held_runs(tx, "load:cpu", now)).unwrap().is_empty(), "told once per reason");

    // A leased task start takes the only place.
    assert!(claim(1).is_some());
    assert_eq!(store.read(queries::running_runs).unwrap(), 1);
    assert_eq!(claim(1), None);
    assert!(claim(2).is_some());
    assert_eq!(store.read(queries::running_runs).unwrap(), 2);
}

#[test]
fn reschedule_uses_fake_clock_and_parking_keeps_attempts() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    run_job(&store, "j", "c");
    let t0 = now_ms() + 1_000; // fake clock, after the job became due
    let job = store.write(move |tx| queries::claim_due_job(tx, "w", t0, 45_000, 4)).unwrap().unwrap();
    let epoch = job.lease_epoch;
    assert!(store.write(move |tx| queries::reschedule_job(tx, "j", epoch, t0 + 5_000, "net", true)).unwrap());
    // Not due before its time on the fake clock.
    assert!(store.write(move |tx| queries::claim_due_job(tx, "w", t0 + 4_999, 45_000, 4)).unwrap().is_none());
    let job = store.write(move |tx| queries::claim_due_job(tx, "w", t0 + 5_000, 45_000, 4)).unwrap().unwrap();
    let epoch = job.lease_epoch;
    assert_eq!(store.write(|tx| queries::job_attempts(tx, "j")).unwrap().0, 2);
    // Parking for auth does not burn an attempt, and a stale epoch cannot park.
    assert!(!store.write(move |tx| queries::reschedule_job(tx, "j", epoch - 1, i64::MAX / 2, "auth", false)).unwrap());
    assert!(store.write(move |tx| queries::reschedule_job(tx, "j", epoch, i64::MAX / 2, "auth", false)).unwrap());
    assert_eq!(store.write(|tx| queries::job_attempts(tx, "j")).unwrap().0, 1);
    assert_eq!(store.write(move |tx| queries::wake_parked(tx, "auth", t0)).unwrap(), 1);
    assert!(store.write(move |tx| queries::claim_due_job(tx, "w", t0 + 1, 45_000, 4)).unwrap().is_some());
}

#[test]
fn the_current_schema_needs_no_backup_first() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path()).unwrap());
    assert!(crate::backup::before_migration(dir.path(), "test", "dev").unwrap().is_none());
}

/// A project and an employee (company-wide, titled "Head").
fn project_with_employee(store: &Store, name: &str) -> (ProjectId, EmployeeId) {
    let (project, employee) = (ProjectId::new(), EmployeeId::new());
    let name = name.to_owned();
    store
        .write(move |tx| {
            events::insert_project_projection(tx, project, "Atlas", None)?;
            employees::insert_employee(tx, employee, &name, "Lead", Some("Head"), None, None, &serde_json::json!({}), ConversationId::new())
        })
        .unwrap();
    (project, employee)
}

fn blob(store: &Store, id: &str) {
    let id = id.to_owned();
    store.write(move |tx| Ok(tx.execute("INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES (?1, ?1, '', 0, 'text/plain', 0)", [&id])?)).unwrap();
}

#[test]
fn the_org_chart_plugins_and_the_follow_setting_are_stored() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (project, boss) = project_with_employee(&store, "Jean");
    let report = EmployeeId::new();
    store
        .write(move |tx| employees::insert_employee(tx, report, "Simon", "Dev", None, Some(boss), None, &serde_json::json!({}), ConversationId::new()))
        .unwrap();
    let read = |_: &str| -> Option<String> { None };
    let listed = store.read(|c| employees::list_employees(c, &read)).unwrap();
    let by_name = |name: &str| listed.iter().find(|m| m.name == name).unwrap();
    assert_eq!((by_name("Jean").title.as_deref(), by_name("Jean").reports_to), (Some("Head"), None));
    assert_eq!((by_name("Simon").title.as_deref(), by_name("Simon").reports_to), (None, Some(boss)));
    // Names are unique in the company (any case), and its persistent chat has no project.
    assert!(store.read(|c| employees::name_taken(c, " simon ", None)).unwrap());
    assert!(!store.read(|c| employees::name_taken(c, "Simon", Some(report))).unwrap());
    let chat = store.read(|c| employees::main_chat(c, report)).unwrap().unwrap();
    assert_eq!(store.read(|c| conversations::owner_of(c, chat)).unwrap(), (Some(report), None));
    let entries = |store: &Store| {
        let mut e: Vec<(EmployeeId, Option<EmployeeId>)> = store.read(employees::org_entries).unwrap();
        e.sort();
        e
    };
    let mut expected = vec![(boss, None), (report, Some(boss))];
    expected.sort();
    assert_eq!(entries(&store), expected);

    store.write(move |tx| employees::set_reports_to(tx, report, None)).unwrap();
    assert!(entries(&store).contains(&(report, None)));

    store.write(move |tx| employees::set_plugins(tx, boss, &["a@x".to_string(), "b@x".to_string()])).unwrap();
    store.write(move |tx| employees::set_plugins(tx, report, &["b@x".to_string()])).unwrap();
    assert_eq!(store.read(|c| employees::plugins_of(c, boss)).unwrap(), vec!["a@x".to_string(), "b@x".to_string()]);
    let users = store.read(employees::plugin_users).unwrap();
    assert_eq!(users.iter().filter(|(p, _)| p == "b@x").count(), 2);
    store.write(move |tx| employees::set_plugins(tx, boss, &[])).unwrap();
    assert!(store.read(|c| employees::plugins_of(c, boss)).unwrap().is_empty());

    // A project's own list, for its project assistant and the runs in its tasks, apart from the employees'.
    store.write(move |tx| employees::set_project_plugins(tx, project, &["c@x".to_string(), "a@x".to_string(), "c@x".to_string()])).unwrap();
    assert_eq!(store.read(|c| employees::project_plugins_of(c, project)).unwrap(), vec!["c@x".to_string(), "a@x".to_string()]);
    assert_eq!(store.read(employees::project_plugin_users).unwrap(), vec![("a@x".to_string(), project), ("c@x".to_string(), project)]);
    assert!(store.read(|c| employees::plugins_of(c, boss)).unwrap().is_empty());
    store.write(move |tx| employees::set_project_plugins(tx, project, &[])).unwrap();
    assert!(store.read(|c| employees::project_plugins_of(c, project)).unwrap().is_empty());

    // "Follow the org chart" is a server setting, on unless turned off.
    assert!(store.read(employees::follow_org_chart).unwrap());
    store.write(move |tx| employees::set_follow_org_chart(tx, false)).unwrap();
    assert!(!store.read(employees::follow_org_chart).unwrap());
    store.write(move |tx| employees::set_follow_org_chart(tx, true)).unwrap();
    assert!(store.read(employees::follow_org_chart).unwrap());
}

#[test]
fn a_queued_task_whose_start_is_planned_is_scheduled_and_not_at_work() {
    use aurelius_domain::{TaskId, task::TaskStatus};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (project, lead) = project_with_employee(&store, "Jean");
    let colleague = EmployeeId::new();
    blob(&store, "g");
    let (parent, child) = (TaskId::new(), TaskId::new());
    store
        .write(move |tx| {
            employees::insert_employee(tx, colleague, "Simon", "Dev", None, None, None, &serde_json::json!({}), ConversationId::new())?;
            tasks::insert_task(tx, parent, project, Some(lead), "Parent", "g", "g", TaskStatus::Running, None)?;
            tasks::insert_task(tx, child, project, Some(colleague), "Part", "g", "g", TaskStatus::Queued, Some(parent))?;
            let appended = append(tx, event(&child.to_string(), 1, "k-child"))?;
            events::insert_job(tx, &child.to_string(), "task.run", appended.id, "d-child", &serde_json::json!({}))
        })
        .unwrap();
    let read = |_: &str| -> Option<String> { None };
    let view = |store: &Store| store.read(|c| tasks::task_view(c, child, &read)).unwrap().unwrap();
    let delegated = |store: &Store| store.read(|c| employees::running_agents(c, lead)).unwrap().iter().filter(|a| a.kind == "delegated").count();

    // About to start: at work.
    assert_eq!((view(&store).waiting_reason, view(&store).starts_at_ms), (None, None));
    assert_eq!(delegated(&store), 1);

    // Its start is planned for later: scheduled, with the time, and not in the agents bar.
    let at = now_ms() + 3_600_000;
    store.write(move |tx| Ok(tx.execute("UPDATE jobs SET not_before_ms = ?2 WHERE id = ?1", rusqlite::params![child.to_string(), at])?)).unwrap();
    assert_eq!((view(&store).waiting_reason.as_deref(), view(&store).starts_at_ms), (Some("scheduled"), Some(at)));
    assert_eq!(delegated(&store), 0);

    // Parked until something wakes it: scheduled without a time.
    store.write(move |tx| Ok(tx.execute("UPDATE jobs SET not_before_ms = ?2 WHERE id = ?1", rusqlite::params![child.to_string(), i64::MAX / 2])?)).unwrap();
    assert_eq!((view(&store).waiting_reason.as_deref(), view(&store).starts_at_ms), (Some("scheduled"), None));
}

#[test]
fn a_finished_task_with_a_run_in_its_conversation_is_live_running_or_queued() {
    use aurelius_domain::{TaskId, task::TaskStatus, views::LiveRun};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (project, employee) = project_with_employee(&store, "Jean");
    blob(&store, "g");
    let (task, conversation, run) = (TaskId::new(), ConversationId::new(), aurelius_domain::RunId::new());
    store
        .write(move |tx| {
            // The conversation of the task is a chat of the employee (as in `project_with_employee`).
            employees::insert_employee(tx, EmployeeId::new(), "Simon", "Dev", None, None, None, &serde_json::json!({}), conversation)?;
            tasks::insert_task(tx, task, project, Some(employee), "Done", "g", "g", TaskStatus::Succeeded, None)?;
            tasks::set_task_conversation(tx, task, conversation)?;
            let appended = append(tx, event(&run.to_string(), 1, "k-run"))?;
            events::insert_job(tx, &run.to_string(), "run.chat", appended.id, "d-run", &serde_json::json!({}))
        })
        .unwrap();
    let read = |_: &str| -> Option<String> { None };
    let view = |store: &Store| store.read(|c| tasks::task_view(c, task, &read)).unwrap().unwrap();
    let set_run = |store: &Store, status: &'static str| {
        store
            .write(move |tx| {
                tx.execute("DELETE FROM runs WHERE id = ?1", [run.to_string()])?;
                Ok(tx.execute(
                    "INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES (?1, ?2, 1, 'fake', 'm', '{}', ?3)",
                    rusqlite::params![run.to_string(), conversation.to_string(), status],
                )?)
            })
            .unwrap();
    };
    let set_job = |store: &Store, state: &'static str| store.write(move |tx| Ok(tx.execute("UPDATE jobs SET state = ?2 WHERE id = ?1", rusqlite::params![run.to_string(), state])?)).unwrap();

    // No run: nothing live, and the status is as it was.
    assert_eq!(view(&store).live_run, None);
    // Queued and not claimed: queued.
    set_run(&store, "created");
    assert_eq!(view(&store).live_run, Some(LiveRun::Queued));
    // Claimed but not started: running. Then going, and stopping.
    set_job(&store, "leased");
    assert_eq!(view(&store).live_run, Some(LiveRun::Running));
    for status in ["starting", "running", "waiting", "cancelling"] {
        set_run(&store, status);
        assert_eq!(view(&store).live_run, Some(LiveRun::Running), "{status}");
    }
    // Ended: gone again. The task stays finished throughout.
    for status in ["succeeded", "failed", "cancelled"] {
        set_run(&store, status);
        assert_eq!(view(&store).live_run, None, "{status}");
    }
    assert_eq!(view(&store).status, TaskStatus::Succeeded);
    // A task that is not finished says it with its status: no live run.
    set_run(&store, "running");
    store.write(move |tx| Ok(tx.execute("UPDATE tasks SET status = 'running' WHERE id = ?1", [task.to_string()])?)).unwrap();
    assert_eq!(view(&store).live_run, None);
}

#[test]
fn proposals_are_listed_pending_first_decided_once_and_withdrawn_with_their_proposer() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (_, boss) = project_with_employee(&store, "Jean");
    blob(&store, "instr");
    let propose = |name: &'static str| {
        let id = ProposalId::new();
        store
            .write(move |tx| {
                proposals::insert(
                    tx,
                    proposals::NewProposal {
                        id,
                        proposed_by: boss,
                        name,
                        role: "Designer",
                        title: None,
                        instructions_blob_id: "instr",
                        reports_to: Some(boss),
                        reason: "We need design",
                        origin_conversation_id: None,
                        kind: aurelius_domain::proposal::ProposalKind::Colleague,
                        target_employee_id: None,
                        base_version: None,
                        evidence: Vec::new(),
                    },
                )
            })
            .unwrap();
        id
    };
    let (first, second) = (propose("Eva"), propose("Finn"));
    assert_eq!(store.read(|c| proposals::open_count(c, boss)).unwrap(), 2);

    assert!(store.write(move |tx| proposals::decide(tx, first, ProposalStatus::Rejected, Some("not now"), None)).unwrap());
    assert!(!store.write(move |tx| proposals::decide(tx, first, ProposalStatus::Approved, None, None)).unwrap(), "decided once");
    assert_eq!(store.read(|c| proposals::open_count(c, boss)).unwrap(), 1);

    let read = |key: &str| Some(format!("text of {key}"));
    let listed = store.read(|c| proposals::list(c, &read)).unwrap();
    assert_eq!(listed.iter().map(|p| (p.id, p.status)).collect::<Vec<_>>(), vec![(second, ProposalStatus::Pending), (first, ProposalStatus::Rejected)]);
    assert_eq!((listed[0].proposed_by_name.as_str(), listed[0].reports_to_name.as_deref(), listed[0].instructions.as_str()), ("Jean", Some("Jean"), "text of instr"));
    assert_eq!(listed[1].decision_note.as_deref(), Some("not now"));

    assert_eq!(store.write(move |tx| proposals::withdraw_for(tx, boss)).unwrap(), vec![second]);
    let row = store.read(|c| proposals::load(c, second)).unwrap().unwrap();
    assert_eq!(row.status, ProposalStatus::Withdrawn);
}

// ── The conversion of a 2.x database (migration 14) ─────────────────────

/// Ids of the 2.x fixture: two projects; Jean, Simon and Tom in both (merged by
/// name, Tom with another role too), Max only in Demo. Simon has a task in Aurelius.
struct Fixture {
    aurelius: ProjectId,
    demo: ProjectId,
    jean_a: EmployeeId,
    simon_b: EmployeeId,
    simon_a: EmployeeId,
    jean_b: EmployeeId,
    max_b: EmployeeId,
    tom_a: EmployeeId,
    tom_b: EmployeeId,
    simon_b_chat: String,
    simon_a_chat: String,
    task: String,
    task_conversation: String,
    memory: String,
    proposal: String,
}

/// A data folder holding a 2.x database (the last protocol 2 schema) with the
/// fixture's data, written the way a 2.x server wrote it.
fn two_x_fixture(root: &std::path::Path) -> Fixture {
    let f = Fixture {
        aurelius: ProjectId::new(),
        demo: ProjectId::new(),
        jean_a: EmployeeId::new(),
        simon_b: EmployeeId::new(),
        simon_a: EmployeeId::new(),
        jean_b: EmployeeId::new(),
        max_b: EmployeeId::new(),
        tom_a: EmployeeId::new(),
        tom_b: EmployeeId::new(),
        simon_b_chat: ConversationId::new().to_string(),
        simon_a_chat: ConversationId::new().to_string(),
        task: aurelius_domain::TaskId::new().to_string(),
        task_conversation: ConversationId::new().to_string(),
        memory: aurelius_domain::MemoryId::new().to_string(),
        proposal: ProposalId::new().to_string(),
    };
    let store = Store::open_until(root, crate::migrate::LAST_PROTOCOL_2_VERSION).unwrap();
    for id in ["i1", "i2", "g", "m"] {
        blob(&store, id);
    }
    let (aurelius, demo) = (f.aurelius, f.demo);
    // (id, project, name, role, created, reports_to, main conversation)
    let managers = vec![
        (f.jean_a, aurelius, "Jean", "Product manager", 10, Some(f.simon_a), ConversationId::new().to_string()),
        (f.simon_b, demo, "Simon", "Lead server", 20, Some(f.jean_b), f.simon_b_chat.clone()),
        (f.simon_a, aurelius, "simon", "lead server ", 30, None, f.simon_a_chat.clone()),
        (f.jean_b, demo, "Jean", "Product manager", 40, None, ConversationId::new().to_string()),
        (f.max_b, demo, "Max", "Lead mobile", 50, Some(f.simon_b), ConversationId::new().to_string()),
        (f.tom_a, aurelius, "Tom", "Tester", 60, Some(f.simon_a), ConversationId::new().to_string()),
        (f.tom_b, demo, "Tom", "Support", 70, Some(f.jean_b), ConversationId::new().to_string()),
    ];
    let (simon_a, simon_b) = (f.simon_a, f.simon_b);
    let (task, task_conversation, memory, proposal) = (f.task.clone(), f.task_conversation.clone(), f.memory.clone(), f.proposal.clone());
    store
        .write(move |tx| {
            events::insert_project_projection(tx, aurelius, "Aurelius", None)?;
            events::insert_project_projection(tx, demo, "Demo", None)?;
            tx.execute("UPDATE projects SET follow_org_chart = 1, created_at_ms = 1 WHERE id = ?1", [aurelius.to_string()])?;
            tx.execute("UPDATE projects SET created_at_ms = 2 WHERE id = ?1", [demo.to_string()])?;
            // Managers first without bosses (a boss may come later), then the bosses.
            for (id, project, name, role, created, _, chat) in &managers {
                let scope = aurelius_domain::ScopeId::new().to_string();
                let project_scope: String = tx.query_row("SELECT scope_id FROM projects WHERE id = ?1", [project.to_string()], |r| r.get(0))?;
                tx.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'manager', ?2, ?3)", rusqlite::params![scope, project_scope, id.to_string()])?;
                tx.execute(
                    "INSERT INTO managers(id, project_id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, '{}', 'active', 1, ?6, ?6)",
                    rusqlite::params![id.to_string(), project.to_string(), scope, name, role, created],
                )?;
                tx.execute(
                    "INSERT INTO conversations(id, kind, project_id, manager_id, title, version, created_at_ms) VALUES (?1, 'project', ?2, ?3, ?4, 1, ?5)",
                    rusqlite::params![chat, project.to_string(), id.to_string(), name, created],
                )?;
            }
            for (id, _, _, _, _, boss, _) in &managers {
                tx.execute("UPDATE managers SET reports_to = ?2 WHERE id = ?1", rusqlite::params![id.to_string(), boss.map(|b| b.to_string())])?;
            }
            // Simon's work in Aurelius: a task with its conversation, a memory, plugins,
            // a proposal and instruction versions; Simon in Demo has its own.
            tx.execute(
                "INSERT INTO tasks(id, project_id, manager_id, title, brief_blob_id, status, version, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, ?3, 'Server work', 'g', 'running', 1, 100, 100)",
                rusqlite::params![task, aurelius.to_string(), simon_a.to_string()],
            )?;
            tx.execute(
                "INSERT INTO conversations(id, kind, project_id, manager_id, title, version, created_at_ms) VALUES (?1, 'task', ?2, ?3, 'Server work', 1, 100)",
                rusqlite::params![task_conversation, aurelius.to_string(), simon_a.to_string()],
            )?;
            tx.execute("UPDATE tasks SET conversation_id = ?2 WHERE id = ?1", rusqlite::params![task, task_conversation])?;
            let simon_a_scope: String = tx.query_row("SELECT scope_id FROM managers WHERE id = ?1", [simon_a.to_string()], |r| r.get(0))?;
            tx.execute(
                "INSERT INTO memory_items(id, scope_id, kind, content_blob_id, status, confidence, importance, valid_from_ms, extraction_version, created_at_ms)
                 VALUES (?1, ?2, 'decision', 'm', 'active', 1.0, 0.5, 0, 'manager', 0)",
                rusqlite::params![memory, simon_a_scope],
            )?;
            for (manager, plugin) in [(simon_a, "x@m"), (simon_b, "y@m"), (simon_b, "x@m")] {
                tx.execute("INSERT INTO manager_plugins(manager_id, plugin_id, enabled_at_ms) VALUES (?1, ?2, 0)", rusqlite::params![manager.to_string(), plugin])?;
            }
            tx.execute(
                "INSERT INTO manager_proposals(id, project_id, proposed_by, status, name, role, instructions_blob_id, reports_to, reason, created_at_ms)
                 VALUES (?1, ?2, ?3, 'pending', 'Eva', 'Designer', 'i1', ?3, 'design', 5)",
                rusqlite::params![proposal, aurelius.to_string(), simon_a.to_string()],
            )?;
            tx.execute("UPDATE managers SET instructions_blob_id = 'i1' WHERE id = ?1", [simon_b.to_string()])?;
            tx.execute("UPDATE managers SET instructions_blob_id = 'i2' WHERE id = ?1", [simon_a.to_string()])?;
            for (manager, blob) in [(simon_b, "i1"), (simon_a, "i2")] {
                tx.execute(
                    "INSERT INTO manager_instruction_versions(manager_id, version, instructions_blob_id, source, created_at_ms) VALUES (?1, 1, ?2, 'created', 0)",
                    rusqlite::params![manager.to_string(), blob],
                )?;
            }
            // An old event in the 2.x names.
            tx.execute(
                "INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, manager_id, actor_kind, actor_id, correlation_id,
                     metadata_json, occurred_at_ms, recorded_at_ms, dedupe_key)
                 VALUES (?1, 'manager', ?2, 1, 'manager.created', 1, ?2, 'manager', ?2, 'old', ?3, 0, 0, 'old-manager-created')",
                rusqlite::params![aurelius_domain::EventId::new().to_string(), simon_a.to_string(), format!("{{\"manager_id\":\"{simon_a}\"}}")],
            )?;
            Ok(())
        })
        .unwrap();
    f
}

#[test]
fn a_two_x_database_becomes_the_company() {
    let dir = tempfile::tempdir().unwrap();
    let f = two_x_fixture(dir.path());
    // The update makes a backup of the old database first, then converts on open.
    assert!(crate::backup::before_migration(dir.path(), "test", "dev").unwrap().is_some());

    // Up to schema 17 the conversion's departments are still there (18 drops them).
    let store = Store::open_until(dir.path(), LAST_DEPARTMENT_SCHEMA).unwrap();
    let department_name = |id: EmployeeId| -> Option<String> {
        store
            .read(|c| Ok(c.query_row("SELECT d.name FROM employees e LEFT JOIN departments d ON d.id = e.department_id WHERE e.id = ?1", [id.to_string()], |r| r.get(0))?))
            .unwrap()
    };
    // Each project seeded a department; a project's single root is its head (the survivor).
    let departments = converted_departments(&store, false);
    let head = |name: &str| departments.iter().find(|d| d.1 == name).unwrap().2;
    assert_eq!(departments.len(), 2);
    assert_eq!((head("Aurelius"), head("Demo")), (Some(f.simon_a), Some(f.jean_a)));
    assert_eq!(department_name(f.jean_a).as_deref(), Some("Aurelius"));
    assert_eq!(department_name(f.simon_a).as_deref(), Some("Aurelius"));
    assert_eq!((department_name(f.tom_a).as_deref(), department_name(f.max_b).as_deref()), (Some("Aurelius"), Some("Demo")));
    assert_eq!(department_name(f.simon_b), None);
    let proposal_department: Option<String> =
        store.read(|c| Ok(c.query_row("SELECT department_id FROM proposals WHERE id = ?1", [&f.proposal], |r| r.get(0))?)).unwrap();
    assert_eq!(proposal_department, departments.iter().find(|d| d.1 == "Aurelius").map(|d| d.0.clone()));
    // Memory a 3.0 server kept in the Aurelius department.
    let department_memory = aurelius_domain::MemoryId::new().to_string();
    let aurelius_department = departments.iter().find(|d| d.1 == "Aurelius").unwrap().0.clone();
    let memory = department_memory.clone();
    store
        .write(move |tx| {
            let scope: String = tx.query_row("SELECT scope_id FROM departments WHERE id = ?1", [&aurelius_department], |r| r.get(0))?;
            tx.execute(
                "INSERT INTO memory_items(id, scope_id, kind, content_blob_id, status, confidence, importance, valid_from_ms, extraction_version, created_at_ms)
                 VALUES (?1, ?2, 'decision', 'm', 'active', 1.0, 0.5, 0, 'department', 0)",
                rusqlite::params![memory, scope],
            )?;
            Ok(())
        })
        .unwrap();
    drop(store);

    // The rest is checked on the current schema.
    let store = Store::open(dir.path()).unwrap();
    let status = |id: EmployeeId| -> String { store.read(|c| Ok(c.query_row("SELECT status FROM employees WHERE id = ?1", [id.to_string()], |r| r.get(0))?)).unwrap() };
    let boss = |id: EmployeeId| -> Option<EmployeeId> {
        store
            .read(|c| Ok(c.query_row("SELECT reports_to FROM employees WHERE id = ?1", [id.to_string()], |r| r.get::<_, Option<String>>(0))?))
            .unwrap()
            .and_then(|b| EmployeeId::parse(&b).ok())
    };

    // Departments are gone; their memory is the company's, saying where it came from.
    assert!(!has_departments(&store));
    let (scope_kind, locator): (String, String) = store
        .read(move |c| {
            Ok(c.query_row(
                "SELECT s.kind, m.locator_json FROM memory_items m JOIN scopes s ON s.id = m.scope_id WHERE m.id = ?1",
                [&department_memory],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap();
    let locator: serde_json::Value = serde_json::from_str(&locator).unwrap();
    assert_eq!((scope_kind.as_str(), locator["from_department"].as_str()), ("global", Some("Aurelius")));

    // Every id is kept; the same name merged into the one with the most tasks
    // (Simon of Aurelius) or else the oldest, the others archived.
    let all: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM employees", [], |r| r.get(0))?)).unwrap();
    assert_eq!(all, 7);
    for kept in [f.jean_a, f.simon_a, f.max_b, f.tom_a] {
        assert_eq!(status(kept), "active");
    }
    for gone in [f.jean_b, f.simon_b, f.tom_b] {
        assert_eq!(status(gone), "archived");
    }
    let simon: String = store.read(|c| Ok(c.query_row("SELECT name FROM employees WHERE id = ?1", [f.simon_a.to_string()], |r| r.get(0))?)).unwrap();
    assert_eq!(simon, "simon", "the survivor keeps its own plain name");

    // Bosses point at the survivors.
    assert_eq!((boss(f.jean_a), boss(f.simon_a)), (Some(f.simon_a), None));
    assert_eq!((boss(f.max_b), boss(f.tom_a)), (Some(f.simon_a), Some(f.simon_a)));

    // The merged one's work: its task, task conversation, memory, plugins, proposal and versions.
    let (task_employee, conv): (String, (String, String, Option<String>)) = store
        .read(|c| {
            let e = c.query_row("SELECT employee_id FROM tasks WHERE id = ?1", [&f.task], |r| r.get(0))?;
            let conv = c.query_row("SELECT kind, employee_id, project_id FROM conversations WHERE id = ?1", [&f.task_conversation], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok((e, conv))
        })
        .unwrap();
    assert_eq!(task_employee, f.simon_a.to_string());
    assert_eq!(conv, ("task".to_string(), f.simon_a.to_string(), Some(f.aurelius.to_string())));
    let memory_scope: String = store.read(|c| Ok(c.query_row("SELECT scope_id FROM memory_items WHERE id = ?1", [&f.memory], |r| r.get(0))?)).unwrap();
    let simon_scope: String = store.read(|c| Ok(c.query_row("SELECT scope_id FROM employees WHERE id = ?1", [f.simon_a.to_string()], |r| r.get(0))?)).unwrap();
    assert_eq!(memory_scope, simon_scope);
    let mut plugins = store.read(|c| employees::plugins_of(c, f.simon_a)).unwrap();
    plugins.sort();
    assert_eq!(plugins, vec!["x@m".to_string(), "y@m".to_string()]);
    let read = |key: &str| Some(key.to_string());
    let proposal = store.read(|c| proposals::list(c, &read)).unwrap().into_iter().find(|p| p.id.to_string() == f.proposal).unwrap();
    assert_eq!(proposal.proposed_by, f.simon_a);
    let versions: Vec<(i64, Option<String>, String)> = store
        .read(|c| {
            let mut stmt = c.prepare("SELECT version, instructions_blob_id, source FROM employee_instruction_versions WHERE employee_id = ?1 ORDER BY version")?;
            let rows = stmt.query_map([f.simon_a.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows.collect::<Result<_, _>>()?)
        })
        .unwrap();
    assert_eq!(versions, vec![(1, Some("i2".into()), "created".into()), (2, Some("i1".into()), "created".into()), (3, Some("i2".into()), "restored".into())]);
    assert_eq!(store.read(|c| crate::instruction_versions::current(c, f.simon_a)).unwrap(), 3);

    // The managers' conversations are employee chats without a project; the oldest is the persistent one.
    let chat: (String, Option<String>, String) = store
        .read(|c| Ok(c.query_row("SELECT kind, project_id, employee_id FROM conversations WHERE id = ?1", [&f.simon_b_chat], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?))
        .unwrap();
    assert_eq!(chat, ("employee".to_string(), None, f.simon_a.to_string()));
    assert_eq!(store.read(|c| employees::main_chat(c, f.simon_a)).unwrap().map(|c| c.to_string()), Some(f.simon_b_chat.clone()));
    let older = ConversationId::parse(&f.simon_a_chat).unwrap();
    assert!(store.read(|c| conversations::is_older_employee_chat(c, older)).unwrap());

    // The org chart setting came from the projects (Aurelius had it on).
    assert!(store.read(employees::follow_org_chart).unwrap());
    let has_column: bool = store.read(|c| Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('projects') WHERE name = 'follow_org_chart')", [], |r| r.get(0))?)).unwrap();
    assert!(!has_column);

    // The audit log speaks of employees now.
    let (kind, actor, metadata): (String, String, String) = store
        .read(|c| Ok(c.query_row("SELECT type, actor_kind, metadata_json FROM events WHERE dedupe_key = 'old-manager-created'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?))
        .unwrap();
    assert_eq!((kind.as_str(), actor.as_str()), ("employee.created", "employee"));
    assert!(metadata.contains("\"employee_id\""), "{metadata}");

    // One report says what happened.
    let report = store.read(company_convert::conversion_report).unwrap().unwrap();
    assert_eq!(report.departments.len(), 2);
    let mut merged: Vec<(EmployeeId, Vec<EmployeeId>)> = report.merged.iter().map(|m| (m.employee_id, m.merged_ids.clone())).collect();
    merged.sort();
    let mut expected = vec![(f.jean_a, vec![f.jean_b]), (f.simon_a, vec![f.simon_b]), (f.tom_a, vec![f.tom_b])];
    expected.sort();
    assert_eq!(merged, expected);
    assert!(report.suffixed.is_empty() && report.archived.is_empty());
    assert_eq!(report.roots, 1);
    assert!(report.summary().contains("1 root(s) left"), "{}", report.summary());
    let events: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM events WHERE type = 'company.converted'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(events, 1);
    drop(store);

    // Opening again converts nothing more.
    let store = Store::open(dir.path()).unwrap();
    let (all, events): (i64, i64) = store
        .read(|c| {
            let all = c.query_row("SELECT COUNT(*) FROM employees", [], |r| r.get(0))?;
            Ok((all, c.query_row("SELECT COUNT(*) FROM events WHERE type = 'company.converted'", [], |r| r.get(0))?))
        })
        .unwrap();
    assert_eq!((all, events), (7, 1));
}

/// The last schema with departments: 0018 drops them (protocol 4), so the
/// conversion's departments are checked on a store opened up to here.
const LAST_DEPARTMENT_SCHEMA: i64 = 17;

/// Whether anything of departments is left: the table, a department scope or
/// a `department_id` column.
fn has_departments(store: &Store) -> bool {
    store
        .read(|c| {
            Ok(c.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'departments')
                     OR EXISTS(SELECT 1 FROM scopes WHERE kind = 'department')
                     OR EXISTS(SELECT 1 FROM pragma_table_info('employees') WHERE name = 'department_id')
                     OR EXISTS(SELECT 1 FROM pragma_table_info('proposals') WHERE name = 'department_id')",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}

/// The departments the conversion seeded, by name: (id, name, head); archived
/// ones too with `archived`. Read from the old table on a store opened up to
/// `LAST_DEPARTMENT_SCHEMA`: departments are no longer part of the model.
fn converted_departments(store: &Store, archived: bool) -> Vec<(String, String, Option<EmployeeId>)> {
    store
        .read(|c| {
            let mut stmt = c.prepare("SELECT id, name, head_id FROM departments WHERE ?1 OR status = 'active' ORDER BY lower(name), id")?;
            let rows = stmt.query_map([archived], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?.into_iter().map(|(id, name, head)| (id, name, head.and_then(|h| EmployeeId::parse(&h).ok()))).collect())
        })
        .unwrap()
}

#[test]
fn a_two_x_backup_is_not_restored_by_this_build() {
    let dir = tempfile::tempdir().unwrap();
    let f = two_x_fixture(dir.path());
    let backup = crate::backup::before_migration(dir.path(), "test", "dev").unwrap().unwrap();
    // The server converts and goes on working.
    let store = Store::open(dir.path()).unwrap();
    let simon_a = f.simon_a.to_string();
    store.write(move |tx| Ok(tx.execute("UPDATE employees SET name = 'Renamed' WHERE id = ?1", [&simon_a])?)).unwrap();
    drop(store);

    // A 2.x backup is from before v0.11: refused, and the current data stays.
    let err = crate::backup::request_restore(dir.path(), &backup).unwrap_err();
    assert!(err.to_string().contains("before v0.11"), "{err}");
    assert!(crate::backup::perform_pending_restore(dir.path()).unwrap().is_none(), "nothing pending");
    let store = Store::open(dir.path()).unwrap();
    let name: String = store.read(|c| Ok(c.query_row("SELECT name FROM employees WHERE id = ?1", [f.simon_a.to_string()], |r| r.get(0))?)).unwrap();
    assert_eq!(name, "Renamed");
}

/// Tom's live data: Jean in six active projects with six roles and in an
/// archived one; a name only in the archived project; Rex in two projects
/// with bosses that make a loop once merged.
#[test]
fn one_name_in_six_projects_becomes_one_employee() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_until(dir.path(), crate::migrate::LAST_PROTOCOL_2_VERSION).unwrap();
    blob(&store, "g");
    let names = ["Aurelius Desktop", "Project B", "Project C", "Project D", "Project E", "Demo"];
    let roles = ["Product manager", "Developer", "Senior Manager", "Manager", "manager ", "Head of Engineering"];
    let active: Vec<ProjectId> = names.iter().map(|_| ProjectId::new()).collect();
    let test = ProjectId::new();
    let jeans: Vec<EmployeeId> = names.iter().map(|_| EmployeeId::new()).collect();
    let (jean_test, ghost, simon, rex_1, rex_3) = (EmployeeId::new(), EmployeeId::new(), EmployeeId::new(), EmployeeId::new(), EmployeeId::new());
    // (id, project, name, role, created, reports_to)
    let mut managers: Vec<(EmployeeId, ProjectId, &str, &str, i64, Option<EmployeeId>)> = Vec::new();
    for (i, (jean, project)) in jeans.iter().zip(&active).enumerate() {
        // Jean of Project C reports to Rex there.
        managers.push((*jean, *project, "Jean", roles[i], 1 + i as i64, (i == 2).then_some(rex_3)));
    }
    managers.push((jean_test, test, "Jean", "Developer", 0, None));
    managers.push((ghost, test, "Ghost", "Tester", 7, Some(jean_test)));
    managers.push((simon, active[0], "Simon", "Lead server", 8, Some(jeans[0])));
    // Rex of Aurelius Desktop reports to Jean; Rex of Project C is Project C's root.
    managers.push((rex_1, active[0], "Rex", "Architect", 9, Some(jeans[0])));
    managers.push((rex_3, active[2], "Rex", "Architect", 10, None));
    let (active_w, managers_w, jeans_w) = (active.clone(), managers.clone(), jeans.clone());
    store
        .write(move |tx| {
            for (project, name) in active_w.iter().zip(names) {
                events::insert_project_projection(tx, *project, name, None)?;
            }
            events::insert_project_projection(tx, test, "Test", None)?;
            tx.execute("UPDATE projects SET status = 'archived' WHERE id = ?1", [test.to_string()])?;
            for (id, project, name, role, created, _) in &managers_w {
                let scope = aurelius_domain::ScopeId::new().to_string();
                let project_scope: String = tx.query_row("SELECT scope_id FROM projects WHERE id = ?1", [project.to_string()], |r| r.get(0))?;
                tx.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'manager', ?2, ?3)", rusqlite::params![scope, project_scope, id.to_string()])?;
                tx.execute(
                    "INSERT INTO managers(id, project_id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, '{}', 'active', 1, ?6, ?6)",
                    rusqlite::params![id.to_string(), project.to_string(), scope, name, role, created],
                )?;
            }
            for (id, _, _, _, _, boss) in &managers_w {
                tx.execute("UPDATE managers SET reports_to = ?2 WHERE id = ?1", rusqlite::params![id.to_string(), boss.map(|b| b.to_string())])?;
            }
            // Tasks: Jean of Project C has the most (2); one each for Aurelius Desktop and Test.
            for (jean, project) in [(jeans_w[2], active_w[2]), (jeans_w[2], active_w[2]), (jeans_w[0], active_w[0]), (jean_test, test)] {
                tx.execute(
                    "INSERT INTO tasks(id, project_id, manager_id, title, brief_blob_id, status, version, created_at_ms, updated_at_ms)
                     VALUES (?1, ?2, ?3, 'Work', 'g', 'succeeded', 1, 100, 100)",
                    rusqlite::params![aurelius_domain::TaskId::new().to_string(), project.to_string(), jean.to_string()],
                )?;
            }
            Ok(())
        })
        .unwrap();
    drop(store);
    let kept = jeans[2];

    // Up to schema 17: one Jean, in the department where it had the most tasks;
    // the merged ones are in none.
    let store = Store::open_until(dir.path(), LAST_DEPARTMENT_SCHEMA).unwrap();
    let department = |id: EmployeeId| -> Option<String> {
        store
            .read(|c| Ok(c.query_row("SELECT d.name FROM employees e LEFT JOIN departments d ON d.id = e.department_id WHERE e.id = ?1", [id.to_string()], |r| r.get(0))?))
            .unwrap()
    };
    assert_eq!(department(kept).as_deref(), Some("Project C"));
    for other in jeans.iter().filter(|j| **j != kept).chain([&jean_test]) {
        assert_eq!(department(*other), None);
    }
    drop(store);

    let store = Store::open(dir.path()).unwrap();
    assert!(!has_departments(&store));
    let row = |id: EmployeeId| -> (String, String, Option<String>) {
        store
            .read(|c| Ok(c.query_row("SELECT name, status, reports_to FROM employees WHERE id = ?1", [id.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?))
            .unwrap()
    };

    // One Jean with the plain name, the others archived without a boss.
    let (name, status, _) = row(kept);
    assert_eq!((name.as_str(), status.as_str()), ("Jean", "active"));
    for other in jeans.iter().filter(|j| **j != kept).chain([&jean_test]) {
        let (name, status, boss) = row(*other);
        assert_eq!((name.as_str(), status.as_str(), boss), ("Jean", "archived", None));
    }
    let active_jeans: i64 =
        store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM employees WHERE lower(name) = 'jean' AND status <> 'archived'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(active_jeans, 1);
    // All Jean's tasks are the kept one's, also the archived project's.
    let tasks: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM tasks WHERE employee_id = ?1", [kept.to_string()], |r| r.get(0))?)).unwrap();
    assert_eq!(tasks, 4);

    // A name only in the archived project is an archived employee, not a root.
    assert_eq!(row(ghost).1, "archived");

    // Reporting lines point at the kept ones; the Jean ↔ Rex loop clears Jean's boss.
    assert_eq!(row(simon).2, Some(kept.to_string()));
    assert_eq!(row(rex_1).2, Some(kept.to_string()));
    assert_eq!(row(rex_3).1, "archived");
    assert_eq!(row(kept).2, None);

    // The report: the merge, the archived name, the loop and one root left.
    let report = store.read(company_convert::conversion_report).unwrap().unwrap();
    let jean = report.merged.iter().find(|m| m.employee_id == kept).unwrap();
    assert_eq!(jean.merged_ids.len(), 6);
    assert_eq!(jean.projects[0], "Project C");
    assert_eq!(report.archived.iter().map(|a| (a.employee_id, a.project.as_str())).collect::<Vec<_>>(), vec![(ghost, "Test")]);
    assert!(report.cleared_bosses.iter().any(|c| c.employee_id == kept && c.was == rex_1));
    assert!(report.suffixed.is_empty());
    assert_eq!(report.roots, 1);
    let roots: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM employees WHERE status <> 'archived' AND reports_to IS NULL", [], |r| r.get(0))?)).unwrap();
    assert_eq!(roots, 1);
    assert!(report.summary().contains("1 root(s) left"), "{}", report.summary());
}

#[test]
fn a_new_database_has_the_company_and_follows_its_chart() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert!(store.read(employees::follow_org_chart).unwrap());
    assert!(store.read(company_convert::conversion_report).unwrap().is_none());
    let events: i64 = store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?)).unwrap();
    assert_eq!(events, 0, "nothing to convert, no report");
}

/// Spec §16.3: context selection p95 < 300 ms for 50k chunks (no embeddings).
/// Run: cargo test --release -p aurelius-storage -- --ignored --nocapture perf_
#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn perf_retrieval_over_50k_chunks() {
    use crate::memory::{NewMemory, insert, retrieve};
    use aurelius_domain::memory::{MemoryKind, MemoryStatus};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    const WORDS: [&str; 16] = [
        "launch", "pricing", "customer", "roadmap", "design", "budget", "hiring", "security",
        "marketing", "research", "invoice", "supplier", "contract", "onboarding", "release", "support",
    ];
    let started = std::time::Instant::now();
    let scope = store
        .write(|tx| {
            let source = append(tx, event("perf", 1, "perf"))?.id;
            let scope = crate::events::ensure_global_scope(tx)?;
            for i in 0..50_000usize {
                let blob = format!("perf-{i}");
                tx.execute("INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES (?1, ?1, '', 0, 'text/plain', 0)", [&blob])?;
                let text = format!("note {i} about {} and {} for the {} team", WORDS[i % 16], WORDS[(i * 7) % 16], WORDS[(i * 13 + 3) % 16]);
                insert(tx, NewMemory {
                    id: aurelius_domain::MemoryId::new(),
                    scope_id: &scope,
                    kind: if i % 50 == 0 { MemoryKind::Decision } else { MemoryKind::ProjectKnowledge },
                    blob_id: &blob,
                    text: &text,
                    status: MemoryStatus::Active,
                    confidence: 0.8,
                    importance: (i % 10) as f64 / 10.0,
                    pinned: i % 5000 == 0,
                    supersedes: None,
                    conflicts_with: None,
                    decided_by: None,
                    locator: serde_json::json!({}),
                    sources: vec![(source, serde_json::json!({}))],
                    extraction_version: "perf",
                })?;
            }
            Ok(scope)
        })
        .unwrap();
    println!("seeded 50k memories in {:?}", started.elapsed());
    let queries = ["what is the pricing for the launch", "security review of the supplier contract", "hiring budget", "customer onboarding support", "roadmap"];
    let mut times = Vec::new();
    for round in 0..20 {
        let q = queries[round % queries.len()];
        let t = std::time::Instant::now();
        let found = store.read(|c| retrieve(c, std::slice::from_ref(&scope), q)).unwrap();
        times.push(t.elapsed());
        assert!(found.len() <= 500, "bounded candidate set: {}", found.len());
        assert!(found.iter().any(|(_, s)| *s > 0.0), "lexical hits for {q}");
    }
    times.sort();
    let p95 = times[18];
    println!("retrieval over 50k chunks: p50 {:?}, p95 {:?}", times[10], p95);
    assert!(p95 < std::time::Duration::from_millis(300), "p95 {p95:?}");
}

#[test]
fn reads_run_side_by_side() {
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(Store::open(dir.path()).unwrap());
    let one = |c: &rusqlite::Connection| -> Result<i64, StorageError> { Ok(c.query_row("SELECT 1", [], |r| r.get(0))?) };
    // A read on another thread finishes while this one is still open (one shared connection would wait forever).
    let inner = store
        .read(|_| {
            let other = store.clone();
            Ok(std::thread::spawn(move || other.read(one)).join().unwrap())
        })
        .unwrap();
    assert_eq!(inner.unwrap(), 1);
}

#[test]
fn a_cached_blob_is_gone_once_erased() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let blob = store.blobs().put(b"hello", "text/plain").unwrap();
    assert_eq!(store.blobs().read(&blob.storage_key).unwrap(), b"hello");
    // The second read comes from memory, the same bytes.
    assert_eq!(store.blobs().read(&blob.storage_key).unwrap(), b"hello");
    store.blobs().erase(&blob.storage_key).unwrap();
    assert!(store.blobs().read(&blob.storage_key).is_err());
}

/// Measures on a copy of a real data folder (never the live one): the size of
/// a conversation's messages with tool use whole and shortened, and
/// `memory.list` with a file read per item (as before) and with the blob
/// cache. `AURELIUS_COPY_ROOT=<copy> AURELIUS_COPY_CONVERSATION=<id> cargo test
/// -p aurelius-storage load_hang_measure -- --ignored --nocapture`
#[test]
#[ignore]
fn load_hang_measure() {
    let root = std::path::PathBuf::from(std::env::var("AURELIUS_COPY_ROOT").expect("AURELIUS_COPY_ROOT"));
    let store = Store::open(&root).unwrap();
    let text = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());

    if let Ok(id) = std::env::var("AURELIUS_COPY_CONVERSATION") {
        let conversation = ConversationId::parse(&id).unwrap();
        let started = std::time::Instant::now();
        let mut messages = store.read(|c| crate::conversations::list_messages(c, conversation, &text)).unwrap();
        let elapsed = started.elapsed();
        let whole = serde_json::to_vec(&messages).unwrap().len();
        for message in messages.iter_mut() {
            message.activity = std::mem::take(&mut message.activity).into_iter().map(aurelius_domain::activity::preview).collect();
        }
        let short = serde_json::to_vec(&messages).unwrap().len();
        let calls: usize = messages.iter().map(|m| m.activity.len()).sum();
        println!("conversation {id}: {} messages, {calls} tool calls, read in {elapsed:?}; {whole} bytes whole, {short} bytes short", messages.len());
    }

    let projects: Vec<String> = store
        .read(|c| Ok(c.prepare("SELECT id FROM projects")?.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?))
        .unwrap();
    let blobs = root.join("blobs");
    let from_disk = |key: &str| std::fs::read(blobs.join(key)).ok().and_then(|b| String::from_utf8(b).ok());
    for id in projects {
        let project = ProjectId::parse(&id).unwrap();
        let time = |read: &dyn Fn(&str) -> Option<String>| {
            let started = std::time::Instant::now();
            let items = store.read(|c| crate::memory::list(c, crate::memory::Wanted { project: Some(project), employee: None }, read)).unwrap().len();
            (items, started.elapsed())
        };
        let (items, before) = time(&from_disk);
        let (_, first) = time(&text);
        let (_, again) = time(&text);
        println!("memory.list {id}: {items} items; file per item {before:?}, cache first {first:?}, cache again {again:?}");
    }
}

#[test]
fn migration_15_deletes_plain_chats_and_keeps_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_until(dir.path(), 14).unwrap();
    let (chat, kept, project, run) = (ConversationId::new(), ConversationId::new(), ProjectId::new(), aurelius_domain::RunId::new());
    store
        .write(move |tx| {
            events::insert_project_projection(tx, project, "Atlas", None)?;
            tx.execute("INSERT INTO conversations(id, kind, title, version, created_at_ms, web_research) VALUES (?1, 'chat', 'Old chat', 1, 0, 1)", [chat.to_string()])?;
            conversations::insert_project_conversation(tx, kept, project, "Plans")?;
            tx.execute(
                "INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES (?1, ?2, 1, 'fake', '', '{}', 'succeeded')",
                [run.to_string(), chat.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    let count = |sql: &'static str| -> i64 { store.read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).unwrap() };
    // Migration 17 adds the group chat; of the old ones only the project conversation is left.
    assert_eq!(count("SELECT COUNT(*) FROM conversations WHERE kind <> 'group'"), 1, "only the project conversation is left");
    assert_eq!(store.read(|c| conversations::project_of_conversation(c, kept)).unwrap(), Some(project));
    assert_eq!(count("SELECT COUNT(*) FROM runs"), 0);
    assert_eq!(count("SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name = 'web_research'"), 0);
    let refused = store.write(move |tx| Ok(tx.execute("INSERT INTO conversations(id, kind, title, version, created_at_ms) VALUES ('x', 'chat', '', 1, 0)", [])?));
    assert!(refused.is_err(), "kind 'chat' is gone from the schema");
}

#[test]
fn migration_17_adds_one_group_chat_and_keeps_every_conversation_and_message() {
    use crate::conversations::NewMessage;
    use aurelius_domain::{MessageId, conversation::{MessageRole, MessageStatus}};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_until(dir.path(), 16).unwrap();
    let (project, plans, employee, chat) = (ProjectId::new(), ConversationId::new(), EmployeeId::new(), ConversationId::new());
    store
        .write(move |tx| {
            events::insert_project_projection(tx, project, "Atlas", None)?;
            conversations::insert_project_conversation(tx, plans, project, "Plans")?;
            employees::insert_employee(tx, employee, "Simon", "Dev", None, None, None, &serde_json::json!({}), chat)?;
            // The employee's settings live on its main chat row: they must come through.
            tx.execute("UPDATE conversations SET agent_mode = 'auto', browser_allowed = 1, run_json = '{\"model\":\"m\"}' WHERE id = ?1", [chat.to_string()])?;
            tx.execute("UPDATE conversations SET archived_at_ms = 5 WHERE id = ?1", [plans.to_string()])?;
            for (i, conversation) in [plans, chat, chat].into_iter().enumerate() {
                let appended = append(tx, event(&format!("message-{i}"), 1, &format!("k-message-{i}")))?;
                conversations::insert_message(tx, NewMessage {
                    id: MessageId::new(),
                    conversation_id: conversation,
                    role: MessageRole::User,
                    content_blob_id: None,
                    source_event_id: appended.id,
                    status: MessageStatus::Complete,
                    run_id: None,
                    reply_to: None,
                })?;
            }
            Ok(())
        })
        .unwrap();
    let rows = |store: &Store| -> (Vec<String>, Vec<String>) {
        store
            .read(|c| {
                let conversations = c
                    .prepare("SELECT id || ' ' || kind || ' ' || COALESCE(project_id, '-') || ' ' || COALESCE(employee_id, '-') || ' ' || title || ' ' || agent_mode || ' ' || browser_allowed || ' ' || COALESCE(run_json, '-') || ' ' || COALESCE(archived_at_ms, '-') FROM conversations WHERE kind <> 'group' ORDER BY id")?
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                let messages = c
                    .prepare("SELECT id || ' ' || conversation_id || ' ' || ordinal FROM messages ORDER BY id")?
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                Ok((conversations, messages))
            })
            .unwrap()
    };
    let count = |store: &Store, sql: &'static str| -> i64 { store.read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).unwrap() };
    let before = rows(&store);
    let (conversations_before, messages_before) = (count(&store, "SELECT COUNT(*) FROM conversations"), count(&store, "SELECT COUNT(*) FROM messages"));
    assert_eq!((conversations_before, messages_before), (2, 3));
    drop(store);

    // Up to 17 only: 19 adds a channel for the project (schema19_tests).
    let store = Store::open_until(dir.path(), 17).unwrap();
    // Every row, as it was; the only new one is the group chat.
    assert_eq!(rows(&store), before);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM conversations"), conversations_before + 1);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM messages"), messages_before);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM conversations WHERE kind = 'group' AND project_id IS NULL AND employee_id IS NULL"), 1);
    let group = store.read(conversations::group_conversation).unwrap().expect("the group chat exists");
    assert!(group.to_string().len() == 36);
    // The employee still loads through its main chat, with its settings.
    let loaded = store.write(move |tx| employees::load_employee(tx, employee)).unwrap().unwrap();
    assert_eq!(loaded.conversation_id, chat);
    assert_eq!(store.read(|c| conversations::agent_mode(c, chat)).unwrap(), aurelius_domain::autonomy::AgentMode::Auto);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM pragma_foreign_key_check"), 0);

    // One group chat only, and a group belongs to neither a project nor an employee.
    let second = store.write(|tx| Ok(tx.execute("INSERT INTO conversations(id, kind, title, version, created_at_ms) VALUES ('x', 'group', '', 1, 0)", [])?));
    assert!(second.is_err(), "one group chat per company");
    let owned = store.write(move |tx| {
        tx.execute("DELETE FROM conversations WHERE kind = 'group'", [])?;
        Ok(tx.execute("INSERT INTO conversations(id, kind, employee_id, title, version, created_at_ms) VALUES ('y', 'group', ?1, '', 1, 0)", [employee.to_string()])?)
    });
    assert!(owned.is_err(), "a group chat has no employee");
}

#[test]
fn a_deleted_task_conversation_leaves_the_task_but_not_its_conversation_also_after_a_restart() {
    use aurelius_domain::{TaskId, conversation::ConversationKind, task::TaskStatus};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (project, employee) = project_with_employee(&store, "Jean");
    blob(&store, "g");
    let (task, conversation) = (TaskId::new(), ConversationId::new());
    store
        .write(move |tx| {
            assert!(employees::insert_thread(tx, employee, conversation, ConversationKind::Task, Some(project))?);
            tasks::insert_task(tx, task, project, Some(employee), "Done", "g", "g", TaskStatus::Succeeded, None)?;
            tasks::set_task_conversation(tx, task, conversation)
        })
        .unwrap();
    let read = |_: &str| -> Option<String> { None };
    let listed = |store: &Store| store.read(|c| tasks::list_tasks(c, None, &read)).unwrap().into_iter().find(|t| t.id == task);
    assert_eq!(listed(&store).unwrap().conversation_id, Some(conversation));

    store.write(move |tx| conversations::delete_conversation(tx, conversation)).unwrap();
    // The task stays (in the list and on its own), without the deleted conversation.
    assert_eq!(listed(&store).expect("the task stays").conversation_id, None);
    assert_eq!(store.read(|c| tasks::task_view(c, task, &read)).unwrap().unwrap().conversation_id, None);
    // A restart reads the same.
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(listed(&store).expect("the task stays after a restart").conversation_id, None);
    // The employee's main chat was never touched.
    let main: Option<i64> = store.read(|c| Ok(c.query_row("SELECT archived_at_ms FROM conversations WHERE employee_id = ?1 AND kind = 'employee'", [employee.to_string()], |r| r.get(0))?)).unwrap();
    assert_eq!(main, None);
}

#[test]
fn the_group_chat_exists_in_a_fresh_database_and_comes_back_when_its_row_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let fresh = store.read(conversations::group_conversation).unwrap().expect("migration 17 makes the group chat");
    let groups = |store: &Store| store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM conversations WHERE kind = 'group'", [], |r| r.get::<_, i64>(0))?)).unwrap();
    // Opening again keeps the one there is.
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!((store.read(conversations::group_conversation).unwrap(), groups(&store)), (Some(fresh), 1));

    // Its row gone (a database edited or restored by hand): the next open makes it again.
    store.write(|tx| Ok(tx.execute("DELETE FROM conversations WHERE kind = 'group'", [])?)).unwrap();
    assert_eq!(store.read(conversations::group_conversation).unwrap(), None);
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    let again = store.read(conversations::group_conversation).unwrap().expect("made again on open");
    assert_ne!(again, fresh);
    assert_eq!(groups(&store), 1);
}
