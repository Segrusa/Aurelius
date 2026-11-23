//! The worker over the mailbox (R2, R3): one run per thread while an employee works
//! on several threads at once, merging per thread, and recovery after a lease ran out. The daemon's `tests/mailbox.rs`
//! does this end to end.

use crate::*;
use crate::run::approvals::RuntimePermission;
use crate::run::inbox::tests_support::use_real_mailbox;
use aurelius_domain::conversation::SendChatMessage;
use aurelius_domain::employee::{CreateEmployee, SendEmployeeMessage};
use aurelius_domain::project::CreateProject;
use aurelius_domain::{ConversationId, EmployeeId, MessageId, RunId, UserId};
use aurelius_domain::task::CreateTask;
use aurelius_storage::{Store, queries};
use std::sync::Arc;

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$CTFhFyS4J4F2nnTR4eM9QwZt1G2y2H7TG3pY1YJ2A5E";

struct Setup {
    _dir: tempfile::TempDir,
    service: crate::CommandService,
    user: UserId,
    project: aurelius_domain::ProjectId,
    alex: EmployeeId,
    chat: ConversationId,
}

fn exec(s: &Setup, command: Command) -> CommandResult {
    s.service.execute(&ctx(Some(s.user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).unwrap().receipt.result
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = crate::CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    let user = UserId::new();
    service.store().write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC)).unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = service.execute(&ctx(Some(user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }) }).unwrap().receipt.result else { panic!() };
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Alex".into(), role: "Backend".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id, .. } = service.execute(&ctx(Some(user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateEmployee(create) }).unwrap().receipt.result else { panic!() };
    use_real_mailbox(employee_id);
    Setup { _dir: dir, service, user, project: project_id, alex: employee_id, chat: conversation_id }
}

fn say(s: &Setup, text: &str) -> (MessageId, RunId) {
    match exec(s, Command::SendEmployeeMessage(SendEmployeeMessage { confirm_secret: false, employee_id: s.alex, text: text.into(), conversation_id: s.chat, images: vec![], files: vec![], interrupt: false })) {
        CommandResult::EmployeeMessageSent { message_id, run_id, .. } => (message_id, run_id),
        other => panic!("{other:?}"),
    }
}

fn claim(s: &Setup) -> Option<queries::JobRow> {
    s.service.claim_next_work("t", 45_000, 4).unwrap().map(|(job, _)| job)
}

fn items(s: &Setup, state: &'static str) -> i64 {
    s.service.store().read(move |c| Ok(c.query_row("SELECT COUNT(*) FROM employee_inbox WHERE state = ?1", [state], |r| r.get(0))?)).unwrap()
}

fn run_rows(s: &Setup) -> i64 {
    s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0))?)).unwrap()
}

#[test]
fn nothing_is_made_until_the_worker_takes_the_item() {
    let s = setup();
    let (_, run) = say(&s, "first");
    assert_eq!((items(&s, "queued"), run_rows(&s)), (1, 0), "no run yet");
    let status = s.service.store().read(move |c| aurelius_storage::conversations::run_status(c, run)).unwrap();
    assert_eq!(status.as_deref(), Some("created"), "but it counts as one that has not started");
    let job = claim(&s).expect("the worker takes it");
    assert_eq!((job.id.clone(), items(&s, "queued"), run_rows(&s)), (run.to_string(), 0, 1));
    assert_eq!(job.lease_epoch, 1);
    assert_eq!(job.payload["employee_id"], s.alex.to_string());
    assert!(job.payload["thread_key"].as_str().unwrap().starts_with("chat:"));
}

#[test]
fn one_run_at_a_time_per_thread_and_the_items_of_a_thread_are_merged() {
    let s = setup();
    let (_, first) = say(&s, "first");
    let job = claim(&s).expect("first");
    assert_eq!(job.id, first.to_string());
    let (second, _) = say(&s, "second");
    let (third, third_run) = say(&s, "third");
    assert!(claim(&s).is_none(), "its thread is busy");

    // The first run ends: the two that waited are one run, answering the newest.
    let epoch = job.lease_epoch;
    let id = job.id.clone();
    s.service.store().write(move |tx| { queries::complete_job(tx, &id, epoch)?; aurelius_storage::conversations::set_run_status(tx, first, "succeeded", None) }).unwrap();
    let job = claim(&s).expect("the merged run");
    assert_eq!(job.id, third_run.to_string());
    assert_eq!(job.payload["reply_to"], third.to_string());
    let (run_count, triggers, answered): (i64, i64, i64) = s.service.store().read(move |c| Ok(c.query_row(
        "SELECT (SELECT COUNT(*) FROM runs), (SELECT COUNT(*) FROM run_triggers WHERE run_id = ?1), (SELECT COUNT(*) FROM employee_inbox WHERE run_id = ?1 AND state = 'claimed')",
        [third_run.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?)).unwrap();
    assert_eq!((run_count, triggers, answered), (2, 2, 2));
    // A message that was merged no longer waits.
    let refused = |m: MessageId| s.service.store().read(move |c| aurelius_storage::conversations::reply_state(c, s_chat(c, m), m)).unwrap();
    assert_eq!(refused(second), Some(aurelius_storage::conversations::ReplyState::Started));
}

fn s_chat(c: &rusqlite::Connection, m: MessageId) -> ConversationId {
    let id: String = c.query_row("SELECT conversation_id FROM messages WHERE id = ?1", [m.to_string()], |r| r.get(0)).unwrap();
    ConversationId::parse(&id).unwrap()
}

#[test]
fn a_stopped_item_never_becomes_a_run_that_works_and_a_later_one_goes() {
    let s = setup();
    let (_, first) = say(&s, "first");
    let job = claim(&s).unwrap();
    let (_, second) = say(&s, "second");
    let CommandResult::StopRequested { already_finished, .. } = exec(&s, Command::StopRun(aurelius_domain::conversation::StopRun { run_id: second })) else { panic!() };
    assert!(!already_finished);
    let status: Option<String> = s.service.store().read(move |c| aurelius_storage::conversations::run_status(c, second)).unwrap();
    assert_eq!(status.as_deref(), Some("cancelled"), "whoever asks finds it stopped");
    assert_eq!(items(&s, "cancelled"), 1);
    let (epoch, id) = (job.lease_epoch, job.id.clone());
    s.service.store().write(move |tx| { queries::complete_job(tx, &id, epoch)?; aurelius_storage::conversations::set_run_status(tx, first, "succeeded", None) }).unwrap();
    assert!(claim(&s).is_none(), "nothing else waits");
}

#[test]
fn a_run_whose_lease_ran_out_goes_again_and_only_then_the_employee_takes_more() {
    let s = setup();
    let (_, first) = say(&s, "first");
    let job = claim(&s).unwrap();
    let (epoch, id) = (job.lease_epoch, job.id.clone());
    let (_, second) = say(&s, "second");
    assert!(claim(&s).is_none(), "busy");
    // Its driver is gone (a crash) and the lease has run out: it never started, so it goes again.
    let expired = id.clone();
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE jobs SET lease_until_ms = 0 WHERE id = ?1", [expired])?)).unwrap();
    s.service.end_abandoned_chat_runs(aurelius_storage::now_ms(), &Default::default()).unwrap();
    assert!(s.service.store().write({ let id = id.clone(); move |tx| Ok(!queries::holds_lease(tx, &id, epoch)?) }).unwrap(), "the old lease is fenced off");
    // The run itself goes first; the item behind it waits for its thread.
    let again = claim(&s).expect("the same run again");
    assert_eq!((again.id.clone(), again.lease_epoch), (first.to_string(), epoch + 1));
    let (_, third_state): (String, String) = s.service.store().read(move |c| Ok(c.query_row("SELECT run_id, state FROM employee_inbox WHERE run_id = ?1", [second.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap();
    assert_eq!(third_state, "queued");
    assert!(claim(&s).is_none());
}

#[test]
fn an_employee_works_in_another_thread_while_a_run_is_going() {
    let s = setup();
    let (_, first) = say(&s, "first");
    let job = claim(&s).unwrap();
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'running' WHERE id = ?1", [first.to_string()])?)).unwrap();
    let CommandResult::TaskCreated { task_id } = exec(&s, Command::CreateTask(CreateTask { confirm_secret: false, files: vec![], run: Default::default(), project_id: s.project, employee_id: Some(s.alex), title: "Dark".into(), goal: "Dark mode".into(), acceptance_criteria: vec![] })) else { panic!() };
    let task_job = s.service.store().write(|tx| queries::claim_due_job(tx, "t", aurelius_storage::now_ms() + 60_000, 45_000, 4)).unwrap().expect("the task's job");
    assert_eq!(task_job.id, task_id.to_string());
    let task = s.service.task_context(task_id).unwrap().unwrap();
    s.service.start_task_conversation(&task, task_job.lease_epoch).unwrap();
    let next = claim(&s).expect("the task's run goes while the first one is going");
    assert_ne!(next.id, job.id);
    assert_ne!(next.payload["thread_key"], job.payload["thread_key"]);
    // The machine's limit still holds: with room for two, nothing more starts.
    say(&s, "third");
    assert!(s.service.claim_next_work("t", 45_000, 2).unwrap().is_none(), "two go, the limit is two");
}

#[test]
fn a_run_that_waits_for_an_approval_keeps_its_thread() {
    let s = setup();
    let (_, first) = say(&s, "first");
    claim(&s).unwrap();
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'running' WHERE id = ?1", [first.to_string()])?)).unwrap();
    assert!(matches!(s.service.request_runtime_permission(first, Some("c1".into()), "Bash", &serde_json::json!({ "command": "ls" })).unwrap(), RuntimePermission::Pending { .. }));
    say(&s, "second");
    assert!(claim(&s).is_none(), "the same thread waits for the approval");
}

#[test]
fn a_conversation_without_an_employee_is_not_in_the_mailbox() {
    let s = setup();
    let conversation_id = crate::tests::conversation(&s.service);
    let CommandResult::ChatMessageSent { run_id, .. } = exec(&s, Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id, text: "hi".into(), images: vec![], files: vec![], interrupt: false })) else { panic!() };
    assert_eq!((items(&s, "queued"), run_rows(&s)), (0, 1));
    let job = claim(&s).expect("from jobs, as before");
    assert_eq!(job.id, run_id.to_string());
}

#[test]
fn the_background_model_jobs_held_by_load_are_exactly_the_title_summary_and_memory_jobs() {
    use crate::conversation::{session_history::SESSION_SUMMARY_JOB_KIND, session_title::SESSION_TITLE_JOB_KIND};
    use crate::memory::{CONSOLIDATE_JOB_KIND, EXTRACT_JOB_KIND, SUMMARY_JOB_KIND};
    use crate::ticket::title::TICKET_TITLE_JOB_KIND;
    let mut kinds = aurelius_storage::queries::BACKGROUND_MODEL_JOB_KINDS.to_vec();
    kinds.sort_unstable();
    let mut expected = vec![EXTRACT_JOB_KIND, SUMMARY_JOB_KIND, CONSOLIDATE_JOB_KIND, SESSION_SUMMARY_JOB_KIND, SESSION_TITLE_JOB_KIND, TICKET_TITLE_JOB_KIND];
    expected.sort_unstable();
    assert_eq!(kinds, expected);
}

fn queued_for(s: &Setup, message: MessageId) -> (bool, Option<String>) {
    let store = s.service.store().clone();
    let chat = s.chat;
    let list = s.service.store().read(move |c| aurelius_storage::conversations::list_messages(c, chat, &|k| store.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok()))).unwrap();
    let m = list.into_iter().find(|m| m.id == message).expect("the message is listed");
    (m.queued, m.queued_for)
}

#[test]
fn a_message_held_by_the_machine_says_so_and_one_behind_an_answer_does_not() {
    let s = setup();
    let (first, _) = say(&s, "first");
    // Nothing runs yet and the load holds new runs: it waits for the machine.
    assert!(!s.service.note_held_runs("load:cpu").unwrap().is_empty(), "the hold is noted");
    assert_eq!(queued_for(&s, first), (true, Some("machine".to_owned())));

    // Started: the next message waits for that answer, not for the machine.
    assert!(claim(&s).is_some());
    let (second, _) = say(&s, "second");
    s.service.note_held_runs("load:cpu").unwrap();
    assert_eq!(queued_for(&s, second), (true, None));
}
