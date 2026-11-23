//! A work session's title made from its content: when it is asked for, what it
//! reads and how it is stored.

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
    orion: EmployeeId,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: aurelius_domain::PrincipalId::new(), client_id: aurelius_domain::ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

fn exec(s: &Setup, command: Command) -> CommandResult {
    s.service.execute(&ctx(Some(s.grace)), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command }).unwrap().receipt.result
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(&dir.path().join("data")).unwrap()));
    let grace = UserId::new();
    service.store().write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &grace.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC)).unwrap();
    let folder = dir.path().join("planora");
    std::fs::create_dir_all(&folder).unwrap();
    let mut s = Setup { _dir: dir, service, project: ProjectId::new(), grace, orion: EmployeeId::new() };
    let made = exec(&s, Command::CreateProject(CreateProject { name: "Planora".into(), description: None, folder: Some(folder.to_string_lossy().into_owned()) }));
    let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Orion".into(), role: "Project Manager".into(), title: None, reports_to: None, instructions: None, folder: None };
    let made = exec(&s, Command::CreateEmployee(create));
    let CommandResult::EmployeeCreated { employee_id, .. } = made else { panic!("{made:?}") };
    (s.project, s.orion) = (project_id, employee_id);
    s
}

fn session(s: &Setup) -> (ConversationId, aurelius_domain::RunId) {
    let start = StartWorkSession { project_id: s.project, employee_id: s.orion, text: "kan je eens kijken waarom de export zo traag is".into(), images: vec![], files: vec![], worktree: false, run: RunSettings::default(), confirm_secret: false, design: false };
    let result = exec(s, Command::StartWorkSession(start));
    let CommandResult::WorkSessionStarted { conversation_id, run_id, .. } = result else { panic!("{result:?}") };
    (conversation_id, run_id)
}

/// (id, not_before_ms, state) of the session's title jobs.
fn jobs(s: &Setup, conversation: ConversationId) -> Vec<(String, i64, String)> {
    s.service
        .store()
        .read(move |c| {
            let mut stmt = c.prepare("SELECT id, not_before_ms, state FROM jobs WHERE kind = ?1 AND json_extract(payload_json, '$.conversation_id') = ?2 ORDER BY id")?;
            let rows = stmt.query_map([SESSION_TITLE_JOB_KIND, &conversation.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap()
}

fn title(s: &Setup, conversation: ConversationId) -> String {
    s.service.store().read(move |c| Ok(c.query_row("SELECT title FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0))?)).unwrap()
}

/// Run `schedule_session_title` as the end of an answer does, on the session's first message event.
fn answered(s: &Setup, conversation: ConversationId, run: aurelius_domain::RunId) {
    s.service
        .store()
        .write(move |tx| {
            let source: String = tx.query_row("SELECT source_event_id FROM messages WHERE conversation_id = ?1 ORDER BY ordinal LIMIT 1", [conversation.to_string()], |r| r.get(0))?;
            schedule_session_title(tx, conversation, EventId::parse(&source).unwrap(), run)
        })
        .unwrap();
}

/// Lease the session's ready title job now, as the dispatcher would; (job id, epoch).
fn lease(s: &Setup, conversation: ConversationId) -> (String, i64) {
    s.service
        .store()
        .write(move |tx| {
            let id: String = tx.query_row(
                "SELECT id FROM jobs WHERE kind = ?1 AND state = 'ready' AND json_extract(payload_json, '$.conversation_id') = ?2",
                [SESSION_TITLE_JOB_KIND, &conversation.to_string()],
                |r| r.get(0),
            )?;
            tx.execute("UPDATE jobs SET state = 'leased', lease_owner = 'test', lease_until_ms = ?2, lease_epoch = lease_epoch + 1 WHERE id = ?1", rusqlite::params![id, aurelius_storage::now_ms() + 60_000])?;
            let epoch: i64 = tx.query_row("SELECT lease_epoch FROM jobs WHERE id = ?1", [&id], |r| r.get(0))?;
            Ok((id, epoch))
        })
        .unwrap()
}

#[test]
fn the_first_answer_in_a_session_asks_for_its_title_at_once_and_later_answers_do_not() {
    let s = setup();
    let (conversation, run) = session(&s);
    assert!(jobs(&s, conversation).is_empty(), "nothing before an answer");
    answered(&s, conversation, run);
    let first = jobs(&s, conversation);
    assert_eq!(first.len(), 1);
    assert!(first[0].1 <= aurelius_storage::now_ms(), "runs at once, not after a quiet time");
    assert_eq!(first[0].2, "ready");
    answered(&s, conversation, run);
    assert_eq!(jobs(&s, conversation), first, "one title per session");
}

#[test]
fn a_conversation_that_is_no_session_gets_no_title_job() {
    let s = setup();
    let (conversation, run) = session(&s);
    let other = ConversationId::new();
    s.service
        .store()
        .write(move |tx| {
            let source: String = tx.query_row("SELECT source_event_id FROM messages WHERE conversation_id = ?1 LIMIT 1", [conversation.to_string()], |r| r.get(0))?;
            schedule_session_title(tx, other, EventId::parse(&source).unwrap(), run)
        })
        .unwrap();
    let n: i64 = s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM jobs WHERE kind = ?1", [SESSION_TITLE_JOB_KIND], |r| r.get(0))?)).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn the_input_reads_the_opening_of_the_session_on_the_login_of_its_run() {
    let s = setup();
    let (conversation, run) = session(&s);
    let input = s.service.session_title_input(conversation, Some(run)).unwrap().expect("one message to read");
    assert_eq!(input.project_name, "Planora");
    assert_eq!(input.employee_name, "Orion");
    assert_eq!(input.turns, vec![("Board member".to_owned(), "kan je eens kijken waarom de export zo traag is".to_owned())]);
    assert_eq!(input.config.login_user, Some(s.grace));
}

#[test]
fn a_made_title_is_cleaned_redacted_and_put_on_the_session() {
    let s = setup();
    let (conversation, run) = session(&s);
    answered(&s, conversation, run);
    let (job, epoch) = lease(&s, conversation);
    let value = format!("ghp_{}", aurelius_domain::CommandId::new().0.simple());
    let owner = format!("session-title-test-{}", aurelius_domain::CommandId::new());
    let secret = aurelius_runtime::redact::Secret { name: "GH_TOKEN".into(), label: "GitHub token".into(), value: zeroize::Zeroizing::new(value.clone()) };
    aurelius_runtime::redact::install_set(&owner, vec![secret]);
    let notices = s.service.record_session_title(&job, epoch, conversation, &format!("Title: \"Trage export met {value}.\"\nBecause the export is slow."));
    aurelius_runtime::redact::install_set(&owner, Vec::new());
    assert_eq!(notices.unwrap().len(), 1, "open lists hear of it");
    assert_eq!(title(&s, conversation), "Trage export met «secret:GH_TOKEN»");
    assert_eq!(jobs(&s, conversation)[0].2, "done");
    let listed = s.service.store().read(move |c| aurelius_storage::work_sessions::list(c, s.grace, None)).unwrap();
    assert_eq!(listed[0].title, "Trage export met «secret:GH_TOKEN»");
}

#[test]
fn an_empty_answer_or_a_lost_lease_keeps_the_first_line_title() {
    let s = setup();
    let (conversation, run) = session(&s);
    answered(&s, conversation, run);
    let (job, epoch) = lease(&s, conversation);
    assert!(s.service.record_session_title(&job, epoch + 1, conversation, "Too late").unwrap().is_empty());
    assert_eq!(title(&s, conversation), "kan je eens kijken waarom de export zo traag is");
    assert!(s.service.record_session_title(&job, epoch, conversation, "  \n\"\"").unwrap().is_empty());
    assert_eq!(title(&s, conversation), "kan je eens kijken waarom de export zo traag is");
    assert_eq!(jobs(&s, conversation)[0].2, "done");
}

#[test]
fn a_deleted_session_gets_no_title() {
    let s = setup();
    let (conversation, run) = session(&s);
    answered(&s, conversation, run);
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE conversations SET archived_at_ms = 1, title = '' WHERE id = ?1", [conversation.to_string()])?)).unwrap();
    assert!(s.service.session_title_input(conversation, Some(run)).unwrap().is_none());
    let (job, epoch) = lease(&s, conversation);
    assert!(s.service.record_session_title(&job, epoch, conversation, "Trage export").unwrap().is_empty());
    assert_eq!(title(&s, conversation), "");
}

#[test]
fn the_oldest_messages_win_when_a_session_opens_long() {
    let turns: Vec<(String, String)> = (0..10).map(|i| ("Orion".to_owned(), format!("{i}{}", "x".repeat(99)))).collect();
    let kept = opening_turns(turns, 350);
    assert_eq!(kept.iter().map(|t| t.1.chars().next().unwrap()).collect::<String>(), "012");
    let one = opening_turns(vec![("Orion".into(), "  ".into()), ("Orion".into(), "y".repeat(SESSION_TITLE_MESSAGE_MAX_CHARS + 50))], 10);
    assert_eq!(one.len(), 1, "the first message is always read, an empty one never");
    assert_eq!(one[0].1.chars().count(), SESSION_TITLE_MESSAGE_MAX_CHARS);
}
