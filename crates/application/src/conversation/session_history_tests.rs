//! A work session's summary for the History (feature `session_history`): when it
//! is asked for, what it reads, how it is stored and who may ask for it again.

use super::*;
use crate::*;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::RunSettings;
use aurelius_domain::work_session::StartWorkSession;
use aurelius_domain::{ConversationId, EmployeeId, ProjectId, UserId};
use aurelius_storage::Store;
use std::sync::Arc;

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    project: ProjectId,
    grace: UserId,
    ada: UserId,
    orion: EmployeeId,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: aurelius_domain::PrincipalId::new(), client_id: aurelius_domain::ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

fn exec_as(s: &Setup, user: Option<UserId>, command: Command) -> Result<CommandResult, AppError> {
    s.service.execute(&ctx(user), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command }).map(|c| c.receipt.result)
}

fn add_user(service: &CommandService, name: &str) -> UserId {
    let user = UserId::new();
    let name = name.to_owned();
    service.store().write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &user.to_string(), username: &name, created_by_user: None, now_ms: 1 }, PHC)).unwrap();
    user
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(&dir.path().join("data")).unwrap()));
    let grace = add_user(&service, "grace");
    let ada = add_user(&service, "ada");
    let folder = dir.path().join("planora");
    std::fs::create_dir_all(&folder).unwrap();
    let command = Command::CreateProject(CreateProject { name: "Planora".into(), description: None, folder: Some(folder.to_string_lossy().into_owned()) });
    let made = service.execute(&ctx(Some(grace)), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command }).unwrap().receipt.result;
    let CommandResult::ProjectCreated { project_id: project, .. } = made else { panic!("{made:?}") };
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Orion".into(), role: "Project Manager".into(), title: None, reports_to: None, instructions: None, folder: None };
    let made = service.execute(&ctx(Some(grace)), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command: Command::CreateEmployee(create) }).unwrap().receipt.result;
    let CommandResult::EmployeeCreated { employee_id: orion, .. } = made else { panic!("{made:?}") };
    Setup { _dir: dir, service, project, grace, ada, orion }
}

fn session(s: &Setup) -> (ConversationId, aurelius_domain::RunId) {
    let start = Command::StartWorkSession(StartWorkSession { project_id: s.project, employee_id: s.orion, text: "Maak de export sneller.".into(), images: vec![], files: vec![], worktree: false, run: RunSettings::default(), confirm_secret: false, design: false });
    let result = exec_as(s, Some(s.grace), start).unwrap();
    let CommandResult::WorkSessionStarted { conversation_id, run_id, .. } = result else { panic!("{result:?}") };
    (conversation_id, run_id)
}

/// (id, not_before_ms, state) of the session's summary jobs.
fn jobs(s: &Setup, conversation: ConversationId) -> Vec<(String, i64, String)> {
    s.service
        .store()
        .read(move |c| {
            let mut stmt = c.prepare("SELECT id, not_before_ms, state FROM jobs WHERE kind = ?1 AND json_extract(payload_json, '$.conversation_id') = ?2 ORDER BY id")?;
            let rows = stmt.query_map([SESSION_SUMMARY_JOB_KIND, &conversation.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap()
}

fn state(s: &Setup, conversation: ConversationId) -> (Option<String>, Option<String>) {
    s.service.store().read(move |c| Ok(c.query_row("SELECT summary_state, summary_text FROM work_sessions WHERE conversation_id = ?1", [conversation.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap()
}

/// Run `schedule_session_summary` as the end of an answer does, on the session's first message event.
fn answered(s: &Setup, conversation: ConversationId, run: aurelius_domain::RunId) {
    s.service
        .store()
        .write(move |tx| {
            let source: String = tx.query_row("SELECT source_event_id FROM messages WHERE conversation_id = ?1 ORDER BY ordinal LIMIT 1", [conversation.to_string()], |r| r.get(0))?;
            schedule_session_summary(tx, conversation, EventId::parse(&source).unwrap(), run)
        })
        .unwrap();
}

/// Lease the session's ready job now, as the dispatcher would; (job id, epoch).
fn lease(s: &Setup, conversation: ConversationId) -> (String, i64) {
    s.service
        .store()
        .write(move |tx| {
            let id: String = tx.query_row(
                "SELECT id FROM jobs WHERE kind = ?1 AND state = 'ready' AND json_extract(payload_json, '$.conversation_id') = ?2",
                [SESSION_SUMMARY_JOB_KIND, &conversation.to_string()],
                |r| r.get(0),
            )?;
            tx.execute("UPDATE jobs SET state = 'leased', lease_owner = 'test', lease_until_ms = ?2, lease_epoch = lease_epoch + 1 WHERE id = ?1", rusqlite::params![id, aurelius_storage::now_ms() + 60_000])?;
            let epoch: i64 = tx.query_row("SELECT lease_epoch FROM jobs WHERE id = ?1", [&id], |r| r.get(0))?;
            Ok((id, epoch))
        })
        .unwrap()
}

#[test]
fn an_answer_in_a_session_asks_for_its_summary_once_it_has_gone_quiet() {
    let s = setup();
    let (conversation, run) = session(&s);
    assert!(jobs(&s, conversation).is_empty(), "nothing before an answer");
    let before = aurelius_storage::now_ms();
    answered(&s, conversation, run);
    let first = jobs(&s, conversation);
    assert_eq!(first.len(), 1);
    assert!(first[0].1 >= before + SESSION_SUMMARY_QUIET_MS, "waits for the quiet time");
    assert_eq!(state(&s, conversation), (Some("pending".into()), None));

    // The next answer moves the same job on: one model call per stretch of work.
    std::thread::sleep(std::time::Duration::from_millis(5));
    answered(&s, conversation, run);
    let second = jobs(&s, conversation);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].0, first[0].0);
    assert!(second[0].1 > first[0].1);
}

#[test]
fn a_conversation_that_is_no_session_gets_no_session_summary() {
    let s = setup();
    let (conversation, run) = session(&s);
    let other = ConversationId::new();
    s.service
        .store()
        .write(move |tx| {
            let source: String = tx.query_row("SELECT source_event_id FROM messages WHERE conversation_id = ?1 LIMIT 1", [conversation.to_string()], |r| r.get(0))?;
            schedule_session_summary(tx, other, EventId::parse(&source).unwrap(), run)
        })
        .unwrap();
    let n: i64 = s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM jobs WHERE kind = ?1", [SESSION_SUMMARY_JOB_KIND], |r| r.get(0))?)).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn the_input_reads_the_sessions_messages_on_the_login_of_its_run() {
    let s = setup();
    let (conversation, run) = session(&s);
    let input = s.service.session_summary_input(conversation, Some(run)).unwrap().expect("one message to read");
    assert_eq!(input.title, "Maak de export sneller.");
    assert_eq!(input.project_name, "Planora");
    assert_eq!(input.employee_name, "Orion");
    assert_eq!(input.turns, vec![("Board member".to_owned(), "Maak de export sneller.".to_owned())]);
    assert_eq!(input.previous, None);
    assert_eq!(input.config.login_user, Some(s.grace));
    // Without a run (a retry): the login of who started it.
    assert_eq!(s.service.session_summary_input(conversation, None).unwrap().unwrap().config.login_user, Some(s.grace));
}

#[test]
fn a_summary_is_redacted_flattened_and_stored_and_the_next_reads_only_what_came_after() {
    let s = setup();
    let (conversation, run) = session(&s);
    answered(&s, conversation, run);
    let input = s.service.session_summary_input(conversation, Some(run)).unwrap().unwrap();
    let (job, epoch) = lease(&s, conversation);
    let value = format!("ghp_{}", aurelius_domain::CommandId::new().0.simple());
    let owner = format!("session-summary-test-{}", aurelius_domain::CommandId::new());
    let secret = aurelius_runtime::redact::Secret { name: "GH_TOKEN".into(), label: "GitHub token".into(), value: zeroize::Zeroizing::new(value.clone()) };
    aurelius_runtime::redact::install_set(&owner, vec![secret]);
    let notices = s.service.record_session_summary(&job, epoch, &input, &format!("  Orion made\n the export   faster with {value}. "));
    aurelius_runtime::redact::install_set(&owner, Vec::new());
    assert_eq!(notices.unwrap().len(), 1, "an open History hears of it");
    assert_eq!(state(&s, conversation), (Some("ready".into()), Some("Orion made the export faster with «secret:GH_TOKEN».".into())));
    assert_eq!(jobs(&s, conversation)[0].2, "done");
    // Nothing new since: no input, so no model call.
    assert!(s.service.session_summary_input(conversation, Some(run)).unwrap().is_none());
}

#[test]
fn a_result_after_the_lease_was_lost_writes_nothing() {
    let s = setup();
    let (conversation, run) = session(&s);
    answered(&s, conversation, run);
    let input = s.service.session_summary_input(conversation, Some(run)).unwrap().unwrap();
    let (job, epoch) = lease(&s, conversation);
    assert!(s.service.record_session_summary(&job, epoch + 1, &input, "Too late.").unwrap().is_empty());
    assert_eq!(state(&s, conversation), (Some("pending".into()), None));
}

#[test]
fn a_failed_summary_shows_as_failed_and_any_board_member_asks_again() {
    let s = setup();
    let (conversation, run) = session(&s);
    answered(&s, conversation, run);
    let (job, epoch) = lease(&s, conversation);
    assert_eq!(s.service.session_summary_failed(&job, epoch, conversation).unwrap().len(), 1);
    assert_eq!(state(&s, conversation), (Some("failed".into()), None));

    // Ada did not start it, but sees it in the History and may ask again; it runs at once.
    let asked = exec_as(&s, Some(s.ada), Command::RetrySessionSummary(RetrySessionSummary { conversation_id: conversation })).unwrap();
    assert_eq!(asked, CommandResult::SessionSummaryAsked { conversation_id: conversation });
    assert_eq!(state(&s, conversation), (Some("pending".into()), None));
    let ready: Vec<_> = jobs(&s, conversation).into_iter().filter(|j| j.2 == "ready").collect();
    assert_eq!(ready.len(), 1);
    assert!(ready[0].1 <= aurelius_storage::now_ms());

    // While it is being made, a second ask is refused.
    lease(&s, conversation);
    let again = exec_as(&s, Some(s.ada), Command::RetrySessionSummary(RetrySessionSummary { conversation_id: conversation }));
    assert!(again.unwrap_err().to_string().contains(SUMMARY_UNDER_WAY));
}

#[test]
fn asking_needs_a_board_member_and_a_session_that_was_not_deleted() {
    let s = setup();
    let (conversation, _) = session(&s);
    let ask = || Command::RetrySessionSummary(RetrySessionSummary { conversation_id: conversation });
    assert!(exec_as(&s, None, ask()).is_err());
    let unknown = exec_as(&s, Some(s.grace), Command::RetrySessionSummary(RetrySessionSummary { conversation_id: ConversationId::new() }));
    assert!(unknown.is_err());
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE conversations SET archived_at_ms = 1 WHERE id = ?1", [conversation.to_string()])?)).unwrap();
    assert!(exec_as(&s, Some(s.grace), ask()).is_err());
    assert!(s.service.session_summary_input(conversation, None).unwrap().is_none(), "a deleted session is not summarized");
}

#[test]
fn the_newest_messages_win_when_a_session_is_long() {
    let turns: Vec<(String, String)> = (0..10).map(|i| ("Orion".to_owned(), format!("{i}{}", "x".repeat(99)))).collect();
    let kept = fit_turns(turns, 350);
    assert_eq!(kept.iter().map(|t| t.1.chars().next().unwrap()).collect::<String>(), "789");
    let one = fit_turns(vec![("Orion".into(), "y".repeat(SESSION_SUMMARY_MESSAGE_MAX_CHARS + 50)), ("Orion".into(), "  ".into())], 10);
    assert_eq!(one.len(), 1, "the newest message is always read, an empty one never");
    assert_eq!(one[0].1.chars().count(), SESSION_SUMMARY_MESSAGE_MAX_CHARS);
}
