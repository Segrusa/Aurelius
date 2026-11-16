//! A board member's lasting chat with an employee (`conversation::employee_chat`): the
//! mailbox on the owner's login, tickets linked by the server, one update per arrival
//! in Review and `tell_responsible`. The daemon's `tests/board_chat.rs` runs it end to end.

use super::*;
use crate::conversation::employee_chat::{NO_RESPONSIBLE, NO_TICKET_TO_TELL};
use crate::run::inbox::tests_support::use_real_mailbox;
use crate::task::task_runs::EmployeeAction;
use aurelius_domain::conversation::{SendChatMessage, TICKET_NOTE_KIND, TICKET_UPDATE_KIND};
use aurelius_domain::employee::{CreateEmployee, LifecycleStatus, OpenEmployeeChat, SetEmployeeStatus};
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{ColumnKind, StartTicket};
use aurelius_domain::views::MessageView;
use aurelius_domain::{ConversationId, EmployeeId, RunId, TaskId, TicketId, UserId};
use aurelius_storage::employee_chats as chats;

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    grace: UserId,
    robin: UserId,
    /// Head of the org chart with a report, so it may make tickets.
    atlas: EmployeeId,
    /// Reports to Atlas: no management, and no dispatcher either.
    bo: EmployeeId,
    main: ConversationId,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

fn exec(s: &Setup, user: UserId, command: Command) -> Result<CommandResult, AppError> {
    s.service.execute(&ctx(Some(user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).map(|c| c.receipt.result)
}

fn user(service: &CommandService, name: &str) -> UserId {
    let (id, name) = (UserId::new(), name.to_owned());
    service.store().write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &id.to_string(), username: &name, created_by_user: None, now_ms: 1 }, PHC)).unwrap();
    id
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    let (grace, robin) = (user(&service, "grace"), user(&service, "robin"));
    let run = |command| service.execute(&ctx(Some(grace)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).unwrap().receipt.result;
    run(Command::CreateProject(CreateProject { name: "Orbit".into(), description: None, folder: None }));
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Atlas".into(), role: "Lead".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id: atlas, conversation_id: main } = run(Command::CreateEmployee(create)) else { panic!() };
    let CommandResult::EmployeeCreated { employee_id: bo, .. } = run(Command::CreateEmployee(CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Bo".into(), role: "Dev".into(), title: None, reports_to: Some(atlas), instructions: None, folder: None })) else { panic!() };
    Setup { _dir: dir, service, grace, robin, atlas, bo, main }
}

fn open(s: &Setup, user: UserId) -> ConversationId {
    match exec(s, user, Command::OpenEmployeeChat(OpenEmployeeChat { employee_id: s.atlas })).unwrap() {
        CommandResult::EmployeeChatOpened { conversation_id } => conversation_id,
        other => panic!("{other:?}"),
    }
}

fn say(s: &Setup, user: UserId, chat: ConversationId, text: &str) -> Result<CommandResult, AppError> {
    exec(s, user, Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: chat, text: text.into(), images: vec![], files: vec![], interrupt: false }))
}

fn messages(s: &Setup, chat: ConversationId) -> Vec<MessageView> {
    let st = s.service.store().clone();
    let read = move |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    s.service.store().read(move |c| aurelius_storage::conversations::list_messages(c, chat, &read)).unwrap()
}

fn of_kind(s: &Setup, chat: ConversationId, kind: &str) -> Vec<MessageView> {
    messages(s, chat).into_iter().filter(|m| m.update.as_deref() == Some(kind)).collect()
}

/// A run of Atlas in `chat` that is not the mailbox's, for acting with the board tools.
fn run_in(s: &Setup, chat: ConversationId) -> RunId {
    let run = RunId::new();
    let login = s.grace.to_string();
    s.service
        .store()
        .write(move |tx| {
            tx.execute(
                "INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status, started_at_ms, login_user) VALUES (?1, ?2, 1, 'fake', '', '{}', 'running', 1, ?3)",
                rusqlite::params![run.to_string(), chat.to_string(), login],
            )?;
            Ok(())
        })
        .unwrap();
    run
}

fn create_action(title: &str) -> EmployeeAction {
    EmployeeAction::CreateTicket { project: Some("Orbit".into()), title: title.into(), description: Some("Why and what.".into()), priority: None, labels: vec![] }
}

/// A ticket made by a run in `chat`, and what the employee was told.
fn ticket_from(s: &Setup, chat: ConversationId, title: &'static str) -> (TicketId, String) {
    let run = run_in(s, chat);
    let (ok, text, _) = s.service.employee_action(run, create_action(title)).unwrap();
    assert!(ok, "{text}");
    let id: String = s.service.store().read(move |c| Ok(c.query_row("SELECT id FROM tickets WHERE title = ?1", [title], |r| r.get(0))?)).unwrap();
    (TicketId::parse(&id).unwrap(), text)
}

fn in_review(s: &Setup, ticket: TicketId) {
    s.service.store().write(move |tx| chats::reached_review(tx, ticket)).unwrap();
}

/// The ticket is started on Atlas: its main task, with a run going on.
fn started(s: &Setup, ticket: TicketId) -> (TaskId, RunId) {
    exec(s, s.grace, Command::StartTicket(StartTicket { ticket_id: ticket, employee_id: Some(s.atlas) })).unwrap();
    let read = |key: &str| s.service.store().blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let root = s.service.store().read(|c| aurelius_storage::tickets::view(c, ticket, &read)).unwrap().unwrap().root_task_id.expect("a main task");
    let job = s.service.store().write(|tx| aurelius_storage::queries::claim_due_job(tx, "test", aurelius_storage::now_ms() + 60_000, 45_000, 4)).unwrap().expect("the task's job");
    let task = s.service.task_context(root).unwrap().unwrap();
    s.service.start_task_conversation(&task, job.lease_epoch).unwrap();
    let conversation = s.service.store().read(move |c| aurelius_storage::tasks::task_conversation(c, root)).unwrap().map(|(c, _)| c).expect("its conversation");
    let run = s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, conversation)).unwrap()[0].0;
    (root, run)
}

fn column(s: &Setup, ticket: TicketId) -> ColumnKind {
    let read = |key: &str| s.service.store().blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    s.service.store().read(|c| aurelius_storage::tickets::view(c, ticket, &read)).unwrap().unwrap().column
}

fn run_count(s: &Setup) -> i64 {
    s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0))?)).unwrap()
}

fn queued_items(s: &Setup) -> i64 {
    s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM employee_inbox WHERE state = 'queued'", [], |r| r.get(0))?)).unwrap()
}

#[test]
fn one_chat_per_board_member_and_employee_beside_the_main_chat() {
    let s = setup();
    let chat = open(&s, s.grace);
    assert_eq!(open(&s, s.grace), chat, "it lasts");
    let other = open(&s, s.robin);
    assert_ne!(chat, other);
    assert_ne!(chat, s.main);
    let main = s.service.store().read(|c| aurelius_storage::employees::main_chat(c, s.atlas)).unwrap();
    assert_eq!(main, Some(s.main), "the main chat stays the employee's own");
}

#[test]
fn opening_needs_a_board_member_and_an_employee_that_is_not_archived() {
    let s = setup();
    let command = Command::OpenEmployeeChat(OpenEmployeeChat { employee_id: s.atlas });
    let nobody = s.service.execute(&ctx(None), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).map(|_| ()).unwrap_err();
    assert_eq!(nobody.code, ErrorCode::BoardMemberRequired);
    exec(&s, s.grace, Command::SetEmployeeStatus(SetEmployeeStatus { employee_id: s.atlas, status: aurelius_domain::employee::LifecycleStatus::Archived })).unwrap();
    assert!(exec(&s, s.grace, Command::OpenEmployeeChat(OpenEmployeeChat { employee_id: s.atlas })).is_err());
}

#[test]
fn a_message_goes_through_the_mailbox_on_the_login_of_the_chats_board_member() {
    let s = setup();
    use_real_mailbox(s.atlas);
    let chat = open(&s, s.grace);
    let CommandResult::ChatMessageSent { message_id, .. } = say(&s, s.grace, chat, "Shall we rename it?").unwrap() else { panic!() };
    assert_eq!((queued_items(&s), run_count(&s)), (1, 0), "waits in the mailbox");
    let (job, _) = s.service.claim_next_work("t", 45_000, 4).unwrap().expect("the worker takes it");
    assert_eq!(job.payload["employee_id"], s.atlas.to_string());
    assert_eq!(job.payload["thread_key"], format!("chat:{chat}"));
    assert_eq!(job.payload["reply_to"], message_id.to_string());
    let login: Option<String> = s.service.store().read(|c| Ok(c.query_row("SELECT login_user FROM runs WHERE id = ?1", [job.id.clone()], |r| r.get(0))?)).unwrap();
    assert_eq!(login, Some(s.grace.to_string()));
    let plan = s.service.prepare_chat_run(&job.payload).unwrap().expect("a run to prepare");
    let system = plan.system.expect("identity and guidance");
    assert!(system.contains("one-to-one chat with the board member @grace"), "{system}");
    assert!(system.contains("create_ticket"), "{system}");
    assert_eq!(messages(&s, chat)[0].author_user.as_ref().map(|u| u.id), Some(s.grace), "their words carry their name");
}

#[test]
fn only_its_board_member_writes_in_a_chat_and_older_chats_stay_read_only() {
    let s = setup();
    let chat = open(&s, s.grace);
    assert_eq!(say(&s, s.robin, chat, "Mine?").map(|_| ()).unwrap_err().code, ErrorCode::NotFound, "as if it were not there");
    assert!(messages(&s, chat).is_empty());
    let older = ConversationId::new();
    let employee = s.atlas.to_string();
    s.service
        .store()
        .write(move |tx| {
            tx.execute("INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES (?1, 'employee', NULL, ?2, 'Atlas', 1, ?3)", rusqlite::params![older.to_string(), employee, aurelius_storage::now_ms() + 1])?;
            Ok(())
        })
        .unwrap();
    assert!(say(&s, s.grace, older, "Still there?").is_err(), "history takes no message");
    assert!(messages(&s, older).is_empty());
}

#[test]
fn a_paused_employee_takes_no_message() {
    let s = setup();
    let chat = open(&s, s.grace);
    exec(&s, s.grace, Command::SetEmployeeStatus(SetEmployeeStatus { employee_id: s.atlas, status: aurelius_domain::employee::LifecycleStatus::Paused })).unwrap();
    assert!(say(&s, s.grace, chat, "Hello?").is_err());
}

#[test]
fn a_chat_lasts_it_cannot_be_deleted() {
    let s = setup();
    let chat = open(&s, s.grace);
    say(&s, s.grace, chat, "Hello").unwrap();
    let delete = Command::DeleteConversation(aurelius_domain::conversation::DeleteConversation { conversation_id: chat });
    assert!(exec(&s, s.grace, delete).is_err());
}

fn open_with(s: &Setup, user: UserId, employee: EmployeeId) -> ConversationId {
    match exec(s, user, Command::OpenEmployeeChat(OpenEmployeeChat { employee_id: employee })).unwrap() {
        CommandResult::EmployeeChatOpened { conversation_id } => conversation_id,
        other => panic!("{other:?}"),
    }
}

/// A run of `employee` in `conversation`, on the board member's login.
fn run_of_in(s: &Setup, employee: EmployeeId, conversation: ConversationId) -> RunId {
    let run = RunId::new();
    let login = s.grace.to_string();
    s.service
        .store()
        .write(move |tx| {
            tx.execute(
                "INSERT INTO runs(id, conversation_id, employee_id, attempt, runtime_kind, model_id, resolved_config_json, status, started_at_ms, login_user) VALUES (?1, ?2, ?3, 1, 'fake', '', '{}', 'running', 1, ?4)",
                rusqlite::params![run.to_string(), conversation.to_string(), employee.to_string(), login],
            )?;
            Ok(())
        })
        .unwrap();
    run
}

#[test]
fn a_non_manager_makes_a_ticket_in_its_chat_and_the_review_update_comes_back_there() {
    let s = setup();
    let chat = open_with(&s, s.grace, s.bo);
    let (ok, text, _) = s.service.employee_action(run_of_in(&s, s.bo, chat), create_action("From Bo's chat")).unwrap();
    assert!(ok, "{text}");
    assert!(text.contains("aurelius://") && text.contains("Link it in your reply"), "{text}");
    let id: String = s.service.store().read(|c| Ok(c.query_row("SELECT id FROM tickets WHERE title = 'From Bo''s chat'", [], |r| r.get(0))?)).unwrap();
    let ticket = TicketId::parse(&id).unwrap();
    assert_eq!(column(&s, ticket), ColumnKind::Backlog, "it starts nothing");
    in_review(&s, ticket);
    assert!(!s.service.deliver_chat_updates().unwrap().is_empty());
    let told = of_kind(&s, chat, TICKET_UPDATE_KIND);
    assert_eq!(told.len(), 1, "the update comes back to the chat it was made in");
    assert!(told[0].text.contains("From Bo's chat"), "{}", told[0].text);
}

#[test]
fn a_non_manager_cannot_make_a_ticket_from_a_ticket_discussion_or_its_own_workspace() {
    let s = setup();
    let (ticket, _) = ticket_from(&s, open(&s, s.grace), "A ticket to talk about");
    let discussion: String = s.service.store().read(move |c| Ok(c.query_row("SELECT conversation_id FROM tickets WHERE id = ?1", [ticket.to_string()], |r| r.get(0))?)).unwrap();
    let discussion = ConversationId::parse(&discussion).unwrap();
    let workspace: String = s.service.store().read({ let bo = s.bo; move |c| Ok(c.query_row("SELECT id FROM conversations WHERE employee_id = ?1 AND board_member_id IS NULL", [bo.to_string()], |r| r.get(0))?) }).unwrap();
    let workspace = ConversationId::parse(&workspace).unwrap();
    for conversation in [discussion, workspace] {
        let (ok, text, _) = s.service.employee_action(run_of_in(&s, s.bo, conversation), create_action("Not from a chat")).unwrap();
        assert!(!ok && text.contains("Only management creates tickets"), "{text}");
    }
}

#[test]
fn the_chat_of_another_employee_does_not_count() {
    let s = setup();
    let atlas_chat = open(&s, s.grace);
    let (ok, text, _) = s.service.employee_action(run_of_in(&s, s.bo, atlas_chat), create_action("Bo in Atlas' chat")).unwrap();
    assert!(!ok && text.contains("Only management creates tickets"), "{text}");
}

#[test]
fn a_non_manager_in_its_chat_is_offered_create_ticket_and_told_it_may() {
    let s = setup();
    use_real_mailbox(s.bo);
    let chat = open_with(&s, s.grace, s.bo);
    say(&s, s.grace, chat, "Could you pick this up?").unwrap();
    let (job, _) = s.service.claim_next_work("t", 45_000, 4).unwrap().expect("the worker takes it");
    let plan = s.service.prepare_chat_run(&job.payload).unwrap().expect("a run to prepare");
    assert!(plan.tool_policy.allows("mcp__aurelius__create_ticket") && !plan.tool_policy.bridge_tools_off().contains(&"create_ticket"));
    let system = plan.system.expect("identity and guidance");
    assert!(system.contains("make an ordinary ticket with create_ticket") && system.contains("whatever your place in the org chart"), "{system}");
    assert!(!system.contains("name who can") && !system.contains("add_idea"), "{system}");
}

#[test]
fn a_tool_list_without_create_ticket_also_holds_in_a_chat() {
    let s = setup();
    use aurelius_domain::employee_tools::ToolPolicy;
    let policy = ToolPolicy { tools: Some(vec!["Read".into()]), ..Default::default() };
    let hire = CreateEmployee { access_level: None, tool_policy: Some(policy), start_profile: None, description: None, name: "Cid".into(), role: "Dev".into(), title: None, reports_to: Some(s.atlas), instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id: cid, .. } = exec(&s, s.grace, Command::CreateEmployee(hire)).unwrap() else { panic!() };
    use_real_mailbox(cid);
    let chat = open_with(&s, s.grace, cid);
    say(&s, s.grace, chat, "Could you pick this up?").unwrap();
    let (job, _) = s.service.claim_next_work("t", 45_000, 4).unwrap().expect("the worker takes it");
    let plan = s.service.prepare_chat_run(&job.payload).unwrap().expect("a run to prepare");
    assert!(!plan.tool_policy.allows("mcp__aurelius__create_ticket") && plan.tool_policy.bridge_tools_off().contains(&"create_ticket"), "not offered, so not callable");
}

#[test]
fn the_server_links_a_ticket_made_in_a_chat_and_a_run_elsewhere_links_nothing() {
    let s = setup();
    let chat = open(&s, s.grace);
    let (ticket, text) = ticket_from(&s, chat, "Rename the host");
    assert!(text.contains(&format!("/ticket/{ticket})")) && text.contains("aurelius://"), "{text}");
    let linked = |t: TicketId| s.service.store().read(move |c| Ok(c.query_row("SELECT COUNT(*) FROM chat_tickets WHERE ticket_id = ?1", [t.to_string()], |r| r.get::<_, i64>(0))?)).unwrap();
    assert_eq!(linked(ticket), 1);
    let (elsewhere, text) = ticket_from(&s, s.main, "Not from a chat");
    assert_eq!(linked(elsewhere), 0);
    assert!(!text.contains("Link it in your reply"), "{text}");
}

#[test]
fn a_ticket_from_a_chat_counts_as_arrived_when_its_work_really_reaches_review() {
    let s = setup();
    let chat = open(&s, s.grace);
    let (ticket, _) = ticket_from(&s, chat, "Rename the host");
    let (root, run) = started(&s, ticket);
    let st = s.service.store().clone();
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE runs SET status = 'succeeded' WHERE id = ?1", [run.to_string()])?;
            crate::task::task_runs::complete(tx, &st, root, "It is renamed.")
        })
        .unwrap();
    assert_eq!(column(&s, ticket), ColumnKind::Review);
    assert!(s.service.store().read(chats::updates_waiting).unwrap(), "counted with the move, in one transaction");
    assert_eq!(of_kind(&s, chat, TICKET_UPDATE_KIND).len(), 0, "written by the pass, not by the move");
    s.service.deliver_chat_updates().unwrap();
    let told = of_kind(&s, chat, TICKET_UPDATE_KIND);
    assert_eq!(told.len(), 1);
    assert!(told[0].text.contains("Rename the host") && told[0].text.contains(&format!("/ticket/{ticket})")), "{}", told[0].text);
}

#[test]
fn an_arrival_in_review_is_told_once_and_a_later_arrival_again() {
    let s = setup();
    use_real_mailbox(s.atlas);
    let chat = open(&s, s.grace);
    let (ticket, _) = ticket_from(&s, chat, "Rename the host");
    assert!(s.service.deliver_chat_updates().unwrap().is_empty(), "nothing has arrived");
    in_review(&s, ticket);
    assert!(!s.service.deliver_chat_updates().unwrap().is_empty());
    assert_eq!((of_kind(&s, chat, TICKET_UPDATE_KIND).len(), queued_items(&s)), (1, 1));
    assert!(s.service.deliver_chat_updates().unwrap().is_empty(), "no second delivery");
    assert_eq!((of_kind(&s, chat, TICKET_UPDATE_KIND).len(), queued_items(&s)), (1, 1));
    let (job, _) = s.service.claim_next_work("t", 45_000, 4).unwrap().expect("the update wakes the employee");
    assert_eq!(job.payload["thread_key"], format!("chat:{chat}"));
    let login: Option<String> = s.service.store().read(|c| Ok(c.query_row("SELECT login_user FROM runs WHERE id = ?1", [job.id.clone()], |r| r.get(0))?)).unwrap();
    assert_eq!(login, Some(s.grace.to_string()), "on the login of the chat's board member");
    // Sent back and done again: that is news.
    in_review(&s, ticket);
    s.service.deliver_chat_updates().unwrap();
    assert_eq!(of_kind(&s, chat, TICKET_UPDATE_KIND).len(), 2);
}

#[test]
fn what_arrives_together_is_one_update_and_chats_are_apart() {
    let s = setup();
    use_real_mailbox(s.atlas);
    let chat = open(&s, s.grace);
    let other = open(&s, s.robin);
    let (one, _) = ticket_from(&s, chat, "One");
    let (two, _) = ticket_from(&s, chat, "Two");
    let (three, _) = ticket_from(&s, other, "Three");
    for t in [one, two, three] {
        in_review(&s, t);
    }
    s.service.deliver_chat_updates().unwrap();
    let told = of_kind(&s, chat, TICKET_UPDATE_KIND);
    assert_eq!(told.len(), 1);
    assert!(told[0].text.contains("One") && told[0].text.contains("Two") && !told[0].text.contains("Three"), "{}", told[0].text);
    assert_eq!(of_kind(&s, other, TICKET_UPDATE_KIND).len(), 1);
}

#[test]
fn a_paused_employees_update_waits_until_it_is_back() {
    let s = setup();
    use_real_mailbox(s.atlas);
    let chat = open(&s, s.grace);
    let (ticket, _) = ticket_from(&s, chat, "Rename the host");
    in_review(&s, ticket);
    let status = |status| Command::SetEmployeeStatus(SetEmployeeStatus { employee_id: s.atlas, status });
    exec(&s, s.grace, status(aurelius_domain::employee::LifecycleStatus::Paused)).unwrap();
    s.service.deliver_chat_updates().unwrap();
    assert_eq!(of_kind(&s, chat, TICKET_UPDATE_KIND).len(), 0);
    exec(&s, s.grace, status(aurelius_domain::employee::LifecycleStatus::Active)).unwrap();
    s.service.deliver_chat_updates().unwrap();
    assert_eq!(of_kind(&s, chat, TICKET_UPDATE_KIND).len(), 1);
}

#[test]
fn tell_responsible_is_an_unread_line_with_a_link_and_starts_nothing() {
    let s = setup();
    use_real_mailbox(s.atlas);
    let (ticket, _) = ticket_from(&s, s.main, "Rename the host");
    started(&s, ticket);
    let (job, _) = s.service.claim_next_work("t", 45_000, 4).unwrap().expect("the task's run");
    let run = RunId::parse(&job.id).unwrap();
    let before = run_count(&s);
    let inbox = queued_items(&s);
    let (ok, text, _) = s.service.employee_action(run, EmployeeAction::TellResponsible { message: "Which name do you want?".into() }).unwrap();
    assert!(ok, "{text}");
    assert_eq!((run_count(&s), queued_items(&s)), (before, inbox), "no run, nothing queued");
    assert_eq!(column(&s, ticket), ColumnKind::Doing, "the ticket stays where it is");
    assert!(!aurelius_storage::timeline::notifies("message.assistant_committed", &serde_json::json!({ "kind": TICKET_NOTE_KIND })), "no notification (P-82)");

    let list = s.service.store().read(|c| chats::list(c, s.grace, |_| None)).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].employee_id, list[0].unread), (s.atlas, 1));
    let notes = of_kind(&s, list[0].conversation_id, TICKET_NOTE_KIND);
    assert_eq!(notes.len(), 1);
    assert!(notes[0].text.starts_with("Which name do you want?") && notes[0].text.contains(&format!("/ticket/{ticket})")), "{}", notes[0].text);
}

#[test]
fn tell_responsible_needs_a_ticket_with_a_responsible() {
    let s = setup();
    let run = run_in(&s, s.main);
    let (ok, text, _) = s.service.employee_action(run, EmployeeAction::TellResponsible { message: "Hello".into() }).unwrap();
    assert_eq!((ok, text.as_str()), (false, NO_TICKET_TO_TELL));
    let (ticket, _) = ticket_from(&s, s.main, "Nobody's");
    let (_, run) = started(&s, ticket);
    s.service.store().write(move |tx| { tx.execute("UPDATE tickets SET responsible_user_id = NULL WHERE id = ?1", [ticket.to_string()])?; Ok(()) }).unwrap();
    let (ok, text, _) = s.service.employee_action(run, EmployeeAction::TellResponsible { message: "Hello".into() }).unwrap();
    assert_eq!((ok, text.as_str()), (false, NO_RESPONSIBLE));
}

// ---- Ticket #50: paused employees and the overview ----

fn set_status(s: &Setup, employee: EmployeeId, status: LifecycleStatus) {
    exec(s, s.grace, Command::SetEmployeeStatus(SetEmployeeStatus { employee_id: employee, status })).unwrap();
}

fn overview(s: &Setup, reader: UserId) -> aurelius_domain::views::ActivityOverview {
    s.service.store().read(move |c| aurelius_storage::activity::overview(c, Some(reader), aurelius_storage::now_ms())).unwrap()
}

fn ticket_state(s: &Setup, ticket: TicketId) -> (String, Option<String>) {
    let found = overview(s, s.grace).tickets.into_iter().find(|t| t.ticket_id == ticket).expect("the ticket is in the overview");
    (found.state, found.waits_for)
}

#[test]
fn a_paused_employee_starts_nothing_new_and_starts_again_once_resumed() {
    let s = setup();
    use_real_mailbox(s.atlas);
    let (ticket, _) = ticket_from(&s, s.main, "Rename the host");
    started(&s, ticket);
    set_status(&s, s.atlas, LifecycleStatus::Paused);

    assert!(s.service.claim_next_work("t", 45_000, 4).unwrap().is_none(), "the item stays in the mailbox");
    assert_eq!(ticket_state(&s, ticket), ("paused".to_owned(), Some("paused".to_owned())));
    let waits: Vec<String> = overview(&s, s.grace).waiting.into_iter().map(|w| w.waits_for).collect();
    assert_eq!(waits, ["paused"], "paused work is not waiting for a slot");

    set_status(&s, s.atlas, LifecycleStatus::Active);
    assert!(s.service.claim_next_work("t", 45_000, 4).unwrap().is_some(), "resumed: it starts");
}

#[test]
fn a_running_run_goes_on_when_its_employee_is_paused() {
    let s = setup();
    use_real_mailbox(s.atlas);
    let (ticket, _) = ticket_from(&s, s.main, "Rename the host");
    started(&s, ticket);
    let (job, _) = s.service.claim_next_work("t", 45_000, 4).unwrap().expect("the task's run");
    let run = job.id.clone();
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'running', started_at_ms = 5 WHERE id = ?1", [run])?)).unwrap();
    set_status(&s, s.atlas, LifecycleStatus::Paused);

    let now = overview(&s, s.grace);
    assert_eq!((now.busy.len(), now.busy[0].employee_paused, now.busy[0].since_ms), (1, true, 5));
    assert_eq!(ticket_state(&s, ticket).0, "running");
    let id = job.id.clone();
    let state: String = s.service.store().read(move |c| Ok(c.query_row("SELECT state FROM jobs WHERE id = ?1", [id], |r| r.get(0))?)).unwrap();
    assert_eq!(state, "leased", "pausing leaves the lease alone");
}

#[test]
fn a_ticket_behind_a_busy_employee_waits_for_a_slot_and_the_running_one_does_not() {
    let s = setup();
    use_real_mailbox(s.atlas);
    let (first, _) = ticket_from(&s, s.main, "One");
    started(&s, first);
    let (job, _) = s.service.claim_next_work("t", 45_000, 4).unwrap().expect("the first run");
    let run = job.id.clone();
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'running' WHERE id = ?1", [run])?)).unwrap();
    let (second, _) = ticket_from(&s, s.main, "Two");
    started(&s, second);

    assert_eq!(ticket_state(&s, first).0, "running");
    assert_eq!(ticket_state(&s, second), ("waiting_slot".to_owned(), Some("slot".to_owned())));
}

#[test]
fn a_ticket_whose_owner_asked_a_question_waits_for_something_else() {
    let s = setup();
    use_real_mailbox(s.atlas);
    let (ticket, _) = ticket_from(&s, s.main, "One");
    let (root, _) = started(&s, ticket);
    s.service.store().write(move |tx| {
        tx.execute("DELETE FROM employee_inbox", [])?;
        Ok(tx.execute("UPDATE tasks SET status = 'waiting', waiting_reason = 'input' WHERE id = ?1", [root.to_string()])?)
    }).unwrap();
    assert_eq!(ticket_state(&s, ticket), ("waiting_other".to_owned(), Some("board_member".to_owned())));
}
