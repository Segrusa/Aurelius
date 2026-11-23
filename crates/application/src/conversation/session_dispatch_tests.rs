//! The dispatcher of a work session (feature `session_dispatch`): `work_session.dispatch`,
//! the dispatcher's runs, its `assign_session` and the board member's `work_session.assign`.

use crate::task::task_runs::EmployeeAction;
use crate::*;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::work_session::{AssignWorkSession, DispatchWorkSession, SessionDispatch};
use aurelius_domain::{ConversationId, EmployeeId, ProjectId, RunId, UserId};
use aurelius_storage::Store;
use std::sync::Arc;

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
const TEXT: &str = "The login page shows a blank screen after the last deploy.";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    project: ProjectId,
    user: UserId,
    other: UserId,
    orion: EmployeeId,
    flux: EmployeeId,
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

fn employee(service: &CommandService, user: UserId, name: &str, role: &str) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: role.into(), title: None, reports_to: None, instructions: None, folder: None };
    let made = service.execute(&ctx(Some(user)), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command: Command::CreateEmployee(create) }).unwrap().receipt.result;
    let CommandResult::EmployeeCreated { employee_id, .. } = made else { panic!("{made:?}") };
    employee_id
}

/// Grace and Ada, project Planora, Orion (the company's dispatcher) and Flux.
fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(&dir.path().join("data")).unwrap()));
    let user = add_user(&service, "grace");
    let other = add_user(&service, "ada");
    let folder = dir.path().join("planora");
    std::fs::create_dir_all(&folder).unwrap();
    let command = Command::CreateProject(CreateProject { name: "Planora".into(), description: None, folder: Some(folder.to_string_lossy().into_owned()) });
    let made = service.execute(&ctx(Some(user)), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command }).unwrap().receipt.result;
    let CommandResult::ProjectCreated { project_id: project, .. } = made else { panic!("{made:?}") };
    let orion = employee(&service, user, "Orion", "Project Manager");
    let flux = employee(&service, user, "Flux", "Frontend Developer");
    set_dispatcher(&service, Some(orion));
    Setup { _dir: dir, service, project, user, other, orion, flux }
}

fn set_dispatcher(service: &CommandService, dispatcher: Option<EmployeeId>) {
    let id = dispatcher.map(|d| d.to_string());
    service.store().write(move |tx| Ok(tx.execute("UPDATE company SET dispatcher_employee_id = ?1", [id])?)).unwrap();
}

fn dispatch(s: &Setup) -> ConversationId {
    let command = Command::DispatchWorkSession(DispatchWorkSession { project_id: s.project, text: TEXT.into(), images: vec![], files: vec![], worktree: false, own_branch: false, run: Default::default(), confirm_secret: false, design: false });
    let result = exec_as(s, Some(s.user), command).unwrap();
    let CommandResult::WorkSessionDispatching { conversation_id, .. } = result else { panic!("{result:?}") };
    conversation_id
}

fn session(s: &Setup, conversation: ConversationId) -> aurelius_storage::work_sessions::Session {
    s.service.store().read(|c| aurelius_storage::work_sessions::of_conversation(c, conversation)).unwrap().unwrap()
}

fn employee_of(s: &Setup, conversation: ConversationId) -> String {
    s.service.store().read(|c| Ok(c.query_row("SELECT employee_id FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0))?)).unwrap()
}

/// Runs that answer a message of the session (the chosen employee's), not the dispatcher's.
fn answering_runs(s: &Setup, conversation: ConversationId) -> Vec<(String, Option<String>)> {
    s.service
        .store()
        .read(|c| {
            let mut stmt = c.prepare("SELECT id, employee_id FROM runs WHERE conversation_id = ?1 AND id NOT IN (SELECT run_id FROM session_dispatch_runs)")?;
            let rows = stmt.query_map([conversation.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

/// The session's own lines (from Aurelius), in order.
fn lines(s: &Setup, conversation: ConversationId) -> Vec<String> {
    let read = |key: &str| s.service.store().blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let messages = s.service.store().read(|c| aurelius_storage::conversations::list_messages(c, conversation, &read)).unwrap();
    messages.into_iter().filter(|m| m.update.as_deref() == Some("system")).map(|m| m.text).collect()
}

fn dispatcher_run(s: &Setup, conversation: ConversationId) -> crate::conversation::session_dispatch::SessionDispatchRun {
    s.service.session_dispatch_run(conversation).unwrap().0.expect("the dispatcher runs")
}

fn assign(s: &Setup, run: RunId, name: &str) -> (bool, String) {
    let (ok, text, _) = s.service.employee_action(run, EmployeeAction::AssignSession { employee: name.into(), reason: "The login page is frontend work.".into() }).unwrap();
    (ok, text)
}

#[test]
fn a_dispatched_session_waits_for_the_dispatcher_and_answers_nothing_yet() {
    let s = setup();
    let conversation = dispatch(&s);
    assert_eq!(session(&s, conversation).dispatch, Some(SessionDispatch::Choosing));
    assert_eq!(employee_of(&s, conversation), s.orion.to_string(), "the dispatcher holds it until it has chosen");
    assert!(answering_runs(&s, conversation).is_empty(), "nobody answers before the choice");
    let job: (String, String) = s
        .service
        .store()
        .read(|c| Ok(c.query_row("SELECT kind, json_extract(payload_json, '$.conversation_id') FROM jobs WHERE kind = 'session.dispatch'", [], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap();
    assert_eq!(job, ("session.dispatch".to_owned(), conversation.to_string()));
    assert_eq!(lines(&s, conversation), ["Orion is choosing who takes this…"]);
    let listed = s.service.store().read(|c| aurelius_storage::work_sessions::list(c, s.user, None)).unwrap();
    assert_eq!((listed[0].dispatch, listed[0].running), (Some(SessionDispatch::Choosing), true));
}

#[test]
fn without_an_active_dispatcher_a_dispatched_session_is_refused_and_nothing_is_made() {
    let s = setup();
    set_dispatcher(&s.service, None);
    let command = Command::DispatchWorkSession(DispatchWorkSession { project_id: s.project, text: TEXT.into(), images: vec![], files: vec![], worktree: false, own_branch: false, run: Default::default(), confirm_secret: false, design: false });
    let refused = exec_as(&s, Some(s.user), command).unwrap_err();
    assert!(refused.message.contains("no active dispatcher"), "{}", refused.message);
    let sessions: i64 = s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM conversations WHERE kind = 'session'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(sessions, 0);
}

#[test]
fn nothing_more_can_be_written_in_a_session_until_somebody_takes_it() {
    let s = setup();
    let conversation = dispatch(&s);
    let send = Command::SendChatMessage(aurelius_domain::conversation::SendChatMessage { conversation_id: conversation, text: "And the API?".into(), images: vec![], files: vec![], interrupt: false, confirm_secret: false });
    let refused = exec_as(&s, Some(s.user), send).unwrap_err();
    assert!(refused.message.contains("Nobody works in this session yet"), "{}", refused.message);
}

#[test]
fn the_dispatchers_choice_hands_the_session_over_and_the_first_message_is_answered() {
    let s = setup();
    let conversation = dispatch(&s);
    let run = dispatcher_run(&s, conversation);
    assert_eq!(run.dispatcher, s.orion);
    assert_eq!(run.config.login_user, Some(s.user), "the dispatcher runs on the login of who started the session");
    assert!(run.prompt.contains(TEXT) && run.prompt.contains("Flux") && run.prompt.contains("Planora"), "{}", run.prompt);
    assert!(run.system.contains("assign_session"));

    let (ok, text) = assign(&s, run.run_id, "flux");
    assert!(ok, "{text}");
    assert_eq!(session(&s, conversation).dispatch, None);
    assert_eq!(employee_of(&s, conversation), s.flux.to_string());
    let answering = answering_runs(&s, conversation);
    assert_eq!(answering.len(), 1, "Flux answers the first message");
    assert_eq!(lines(&s, conversation), ["Orion is choosing who takes this…", "Orion chose Flux: The login page is frontend work."]);
    let (again, _) = s.service.session_dispatch_ended(conversation, run.run_id, None).unwrap();
    assert!(!again, "chosen: the job is done");
    // Its choice is final: a second one is refused.
    assert!(!assign(&s, run.run_id, "Orion").0);
}

#[test]
fn the_dispatcher_chooses_a_colleague_not_itself_and_only_its_own_run_can_assign() {
    let s = setup();
    let conversation = dispatch(&s);
    let run = dispatcher_run(&s, conversation);
    let (ok, text) = assign(&s, run.run_id, "Orion");
    assert!(!ok && text.contains("not yourself"), "{text}");
    let (ok, text) = assign(&s, run.run_id, "Nobody");
    assert!(!ok && text.contains("Flux"), "names who it can choose: {text}");
    // A run that is no session dispatcher's cannot assign one.
    let (ok, _) = assign(&s, RunId::new(), "Flux");
    assert!(!ok);
    assert_eq!(session(&s, conversation).dispatch, Some(SessionDispatch::Choosing));
}

#[test]
fn a_dispatcher_that_chooses_nobody_twice_leaves_the_choice_to_the_board_member() {
    let s = setup();
    let conversation = dispatch(&s);
    let first = dispatcher_run(&s, conversation);
    assert!(s.service.session_dispatch_ended(conversation, first.run_id, None).unwrap().0, "one more run");
    let second = dispatcher_run(&s, conversation);
    assert!(second.prompt.contains("You ended without choosing"));
    assert!(!s.service.session_dispatch_ended(conversation, second.run_id, None).unwrap().0, "no third (RULE 08)");
    assert_eq!(session(&s, conversation).dispatch, Some(SessionDispatch::Nobody));
    assert_eq!(lines(&s, conversation).last().map(String::as_str), Some("The dispatcher chose nobody. Choose who takes this session."));
    assert!(s.service.session_dispatch_run(conversation).unwrap().0.is_none(), "nothing to choose any more");
}

#[test]
fn the_board_member_who_started_the_session_chooses_who_takes_it() {
    let s = setup();
    let conversation = dispatch(&s);
    let choose = |user: UserId| exec_as(&s, Some(user), Command::AssignWorkSession(AssignWorkSession { conversation_id: conversation, employee_id: s.flux }));
    let refused = choose(s.other).unwrap_err();
    assert_eq!(refused.code, aurelius_domain::ErrorCode::NotFound, "another board member's session");
    // While the dispatcher still chooses, the board member may choose first.
    let chosen = choose(s.user).unwrap();
    let CommandResult::WorkSessionStarted { run_id, .. } = chosen else { panic!("{chosen:?}") };
    assert_eq!(answering_runs(&s, conversation).iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(), [run_id.to_string()]);
    assert_eq!(employee_of(&s, conversation), s.flux.to_string(), "Flux answers");
    assert_eq!(lines(&s, conversation).last().map(String::as_str), Some("grace chose Flux."));
    assert!(choose(s.user).unwrap_err().message.contains("already"), "once taken, it stays taken");
    // The dispatcher's run that comes after finds nothing to choose.
    assert!(s.service.session_dispatch_run(conversation).unwrap().0.is_none());
}

#[test]
fn the_dispatchers_run_is_left_to_its_own_job_when_the_daemon_restarts() {
    let s = setup();
    let conversation = dispatch(&s);
    let run = dispatcher_run(&s, conversation);
    s.service.reconcile_chat_runs().unwrap();
    let status = |s: &Setup| -> String { s.service.store().read(|c| Ok(c.query_row("SELECT status FROM runs WHERE id = ?1", [run.run_id.to_string()], |r| r.get(0))?)).unwrap() };
    assert_eq!(status(&s), "running", "no chat run: the chat cleanup leaves it");
    s.service.session_dispatch_claimed(conversation).unwrap();
    assert_eq!(status(&s), "failed", "its job, claimed again, ends it");
    assert!(dispatcher_run(&s, conversation).prompt.contains("You ended without choosing"), "it counted as a try");
}
