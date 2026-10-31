//! A run that started twice under one lease (compacted first), and a run nobody
//! drives any more: neither may keep its conversation (AUR-BUG-05).

use super::*;
use crate::tests::conversation;
use aurelius_domain::RunId;
use aurelius_storage::queries;
use crate::run::{ChatRunPlan, RunOutcome, lifecycle};

fn ctx() -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: None }
}

fn send(service: &CommandService, conversation_id: aurelius_domain::ConversationId, text: &str) -> RunId {
    let send = CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: None,
        command: Command::SendChatMessage(aurelius_domain::conversation::SendChatMessage { conversation_id, text: text.into(), images: Vec::new(), interrupt: false }),
    };
    let CommandResult::ChatMessageSent { run_id, .. } = service.execute(&ctx(), send).unwrap().receipt.result else { panic!() };
    run_id
}

/// Claim the next due job as the daemon does; the lease ran out `expired` ago when given.
fn claim(service: &CommandService, expired: Option<i64>) -> Option<queries::JobRow> {
    let lease = expired.map_or(45_000, |ago| -ago);
    service.store().write(move |tx| queries::claim_due_job(tx, "test", aurelius_storage::now_ms(), lease, 4)).unwrap()
}

fn plan(service: &CommandService, job: &queries::JobRow) -> ChatRunPlan {
    service.prepare_chat_run(&job.payload).unwrap().expect("a run to prepare")
}

fn run_state(service: &CommandService, run: RunId) -> (String, Option<String>, String) {
    service
        .store()
        .read(|c| Ok(c.query_row("SELECT r.status, r.error_code, j.state FROM runs r JOIN jobs j ON j.id = r.id WHERE r.id = ?1", [run.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?))
        .unwrap()
}

/// (status, text) of the run's answer.
fn answer(service: &CommandService, run: RunId) -> (String, String) {
    let (status, key): (String, Option<String>) = service
        .store()
        .read(|c| Ok(c.query_row("SELECT m.status, b.storage_key FROM messages m LEFT JOIN blobs b ON b.id = m.content_blob_id WHERE m.run_id = ?1", [run.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap();
    let text = key.and_then(|k| service.store().blobs().read(&k).ok()).map(|b| String::from_utf8(b).unwrap()).unwrap_or_default();
    (status, text)
}

#[test]
fn a_run_that_compacted_first_starts_its_turn_under_the_same_lease() {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(aurelius_storage::Store::open(dir.path()).unwrap()));
    let run = send(&service, conversation(&service), "hello");
    let job = claim(&service, None).expect("the run's job");
    let plan = plan(&service, &job);
    // Once while it compacts (the answer shows as running), once when its turn starts.
    let first = service.start_chat_run(&plan, job.lease_epoch).expect("the first start");
    let again = service.start_chat_run(&plan, job.lease_epoch).expect("the turn starts after compacting, under the same lease");
    assert_eq!(first.map(|n| n.len() > 0), Some(true));
    assert!(again.is_some(), "still holds the lease");
    let started: i64 = service
        .store()
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM events WHERE aggregate_id = ?1 AND type = 'run.started'", [run.to_string()], |r| r.get(0))?))
        .unwrap();
    assert_eq!(started, 1, "the start is recorded once");
    assert_eq!(run_state(&service, run).0, "running");
    service.finish_chat_run(&plan, RunOutcome::Completed, "done", job.lease_epoch).unwrap();
    assert_eq!(run_state(&service, run), ("succeeded".into(), None, "done".into()));
}

#[test]
fn a_run_nobody_drives_ends_with_a_note_and_the_next_message_goes() {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(aurelius_storage::Store::open(dir.path()).unwrap()));
    let conversation_id = conversation(&service);
    let run = send(&service, conversation_id, "hello");
    // Its driver claimed and started it, wrote some text, then went away: the lease ran out.
    let job = claim(&service, Some(60_000)).expect("the run's job");
    let plan = plan(&service, &job);
    service.start_chat_run(&plan, job.lease_epoch).unwrap().expect("started");
    service.checkpoint_chat_run(&plan, "Half an answ", job.lease_epoch).unwrap();
    let waiting = send(&service, conversation_id, "Hello?");
    assert!(claim(&service, None).is_none(), "the conversation is still held");

    // Still driven here: nothing happens.
    let driven: std::collections::HashSet<(String, i64)> = [(job.id.clone(), job.lease_epoch)].into();
    assert!(service.end_abandoned_chat_runs(aurelius_storage::now_ms(), &driven).unwrap().is_empty());
    assert_eq!(run_state(&service, run).0, "running");

    let notices = service.end_abandoned_chat_runs(aurelius_storage::now_ms(), &Default::default()).unwrap();
    assert!(!notices.is_empty());
    assert_eq!(run_state(&service, run), ("failed".into(), Some("abandoned".into()), "done".into()));
    let (status, text) = answer(&service, run);
    assert_eq!(status, "failed");
    assert!(text.starts_with("Half an answ") && text.contains("Aurelius stopped this answer"), "{text}");
    // The waiting message goes next.
    let next = claim(&service, None).expect("the waiting message's run is claimed");
    assert_eq!(next.id, waiting.to_string());
}

#[test]
fn a_stopped_run_nobody_drives_ends_as_stopped_and_one_that_never_started_goes_again() {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(aurelius_storage::Store::open(dir.path()).unwrap()));
    // Stopped while its driver was gone ("Stop" did nothing): it ends as stopped.
    let run = send(&service, conversation(&service), "hello");
    let job = claim(&service, Some(60_000)).unwrap();
    let plan = plan(&service, &job);
    service.start_chat_run(&plan, job.lease_epoch).unwrap().unwrap();
    service.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'cancelling' WHERE id = ?1", [run.to_string()])?)).unwrap();
    service.end_abandoned_chat_runs(aurelius_storage::now_ms(), &Default::default()).unwrap();
    assert_eq!(run_state(&service, run), ("cancelled".into(), None, "done".into()));
    assert_eq!(answer(&service, run).0, "interrupted");

    // Claimed but never started: it is simply tried again.
    let fresh = send(&service, conversation(&service), "again");
    claim(&service, Some(60_000)).unwrap();
    service.end_abandoned_chat_runs(aurelius_storage::now_ms(), &Default::default()).unwrap();
    assert_eq!(run_state(&service, fresh), ("created".into(), None, "ready".into()));
}

#[test]
fn waiting_on_an_approval_is_not_hanging_an_open_task_is_and_the_limit_is_a_setting() {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(aurelius_storage::Store::open(dir.path()).unwrap()));
    let run = send(&service, conversation(&service), "hello");
    assert!(!service.run_waits_on_others(run));
    // A task delegated from this conversation and still running: the run does not
    // block on it (delegate_task answers at once), so the watchdog stays on. A
    // manager's conversation nearly always has one; this is where runs hung.
    service
        .store()
        .write(move |tx| {
            tx.execute(
                "INSERT INTO tasks(id, project_id, title, brief_blob_id, status, version, created_at_ms, updated_at_ms, origin_conversation_id)
                 SELECT 't1', c.project_id, 'Child', (SELECT id FROM blobs LIMIT 1), 'running', 1, 0, 0, c.id
                 FROM runs r JOIN conversations c ON c.id = r.conversation_id WHERE r.id = ?1",
                [run.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(!service.run_waits_on_others(run), "an open delegated task is no reason to wait");
    // The run's own task waits for its colleagues while this run is live (a message
    // to it started the run): the run works, it does not wait on them.
    service
        .store()
        .write(move |tx| {
            tx.execute(
                "INSERT INTO tasks(id, project_id, title, brief_blob_id, status, waiting_reason, version, created_at_ms, updated_at_ms)
                 SELECT 'own', project_id, 'Own', brief_blob_id, 'waiting', 'input', 1, 0, 0 FROM tasks WHERE id = 't1'",
                [],
            )?;
            tx.execute("UPDATE runs SET task_id = 'own' WHERE id = ?1", [run.to_string()])?;
            Ok(())
        })
        .unwrap();
    assert!(!service.run_waits_on_others(run), "its task waits for the user's input");
    service.store().write(|tx| Ok(tx.execute("UPDATE tasks SET waiting_reason = 'dependency' WHERE id = 'own'", [])?)).unwrap();
    assert!(!service.run_waits_on_others(run), "its task waits for its colleagues");
    service
        .store()
        .write(move |tx| {
            // Any blob and event will do for the references.
            tx.execute(
                "INSERT INTO approvals (id, run_id, call_id, tool, effect, action_blob_id, action_digest, request_event_id, status, expires_at_ms)
                 VALUES ('a1', ?1, 'c1', 'Bash', 'execute', (SELECT id FROM blobs LIMIT 1), 'd', (SELECT id FROM events LIMIT 1), 'pending', 9999999999999)",
                [run.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(service.run_waits_on_others(run), "a pending approval");
    service.store().write(|tx| Ok(tx.execute("UPDATE approvals SET status = 'rejected'", [])?)).unwrap();
    assert!(!service.run_waits_on_others(run), "a decided one no longer");

    assert_eq!(service.run_idle_limit_ms(), lifecycle::DEFAULT_RUN_IDLE_MS);
    service.store().write(|tx| aurelius_storage::settings::put(tx, lifecycle::RUN_IDLE_TIMEOUT, "120", 0)).unwrap();
    assert_eq!(service.run_idle_limit_ms(), 120_000);
    service.store().write(|tx| aurelius_storage::settings::put(tx, lifecycle::RUN_IDLE_TIMEOUT, "1", 0)).unwrap();
    assert_eq!(service.run_idle_limit_ms(), 30_000, "never below 30 s");
}

#[test]
fn when_it_cannot_tell_whether_a_run_waits_it_does_not_stop_it() {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(aurelius_storage::Store::open(dir.path()).unwrap()));
    let run = send(&service, conversation(&service), "hello");
    assert!(!service.run_waits_on_others(run));
    // The question fails (here: its table is gone): in doubt the run is not stopped.
    service.store().write(|tx| Ok(tx.execute("ALTER TABLE approvals RENAME TO approvals_gone", [])?)).unwrap();
    assert!(service.run_waits_on_others(run), "a read error is no reason to stop a run");
}

#[test]
fn an_old_driver_of_a_job_claimed_again_does_not_count_and_the_new_one_does() {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(aurelius_storage::Store::open(dir.path()).unwrap()));
    let run = send(&service, conversation(&service), "hello");
    // Claimed, deferred (put back for a retry), and claimed again: a new lease epoch.
    let old = claim(&service, Some(60_000)).expect("the run's job");
    let (id, epoch) = (old.id.clone(), old.lease_epoch);
    assert!(service.store().write(move |tx| queries::reschedule_job(tx, &id, epoch, 0, "retry", false)).unwrap());
    let new = claim(&service, Some(60_000)).expect("claimed again");
    assert_eq!((new.id.as_str(), new.lease_epoch), (old.id.as_str(), old.lease_epoch + 1));
    let plan = plan(&service, &new);
    service.start_chat_run(&plan, new.lease_epoch).unwrap().expect("the new driver started it");

    // Both drivers are known, then the old one ends late: the new one still drives it.
    let both: std::collections::HashSet<(String, i64)> = [(old.id.clone(), old.lease_epoch), (new.id.clone(), new.lease_epoch)].into();
    assert!(service.end_abandoned_chat_runs(aurelius_storage::now_ms(), &both).unwrap().is_empty());
    let new_only: std::collections::HashSet<(String, i64)> = [(new.id.clone(), new.lease_epoch)].into();
    assert!(service.end_abandoned_chat_runs(aurelius_storage::now_ms(), &new_only).unwrap().is_empty());
    assert_eq!(run_state(&service, run).0, "running", "not ended while its driver works on it");

    // Only the old driver left: it can no longer write, so nobody drives the run.
    let old_only: std::collections::HashSet<(String, i64)> = [(old.id.clone(), old.lease_epoch)].into();
    assert!(!service.end_abandoned_chat_runs(aurelius_storage::now_ms(), &old_only).unwrap().is_empty());
    assert_eq!(run_state(&service, run), ("failed".into(), Some("abandoned".into()), "done".into()));
}
