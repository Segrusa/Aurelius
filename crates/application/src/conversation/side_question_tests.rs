//! Side questions: what they read of a conversation and how the prompt is built.

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
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &grace.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    let folder = dir.path().join("planora");
    std::fs::create_dir_all(&folder).unwrap();
    let mut s = Setup { _dir: dir, service, project: ProjectId::new(), grace, orion: EmployeeId::new() };
    let made = exec(&s, Command::CreateProject(CreateProject { name: "Planora".into(), description: None, folder: Some(folder.to_string_lossy().into_owned()) }));
    let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
    let create = CreateEmployee {
        access_level: None,
        tool_policy: None,
        start_profile: None,
        description: None,
        name: "Orion".into(),
        role: "Project Manager".into(),
        title: None,
        reports_to: None,
        instructions: None,
        folder: None,
    };
    let made = exec(&s, Command::CreateEmployee(create));
    let CommandResult::EmployeeCreated { employee_id, .. } = made else { panic!("{made:?}") };
    (s.project, s.orion) = (project_id, employee_id);
    s
}

fn session(s: &Setup) -> ConversationId {
    let start = StartWorkSession {
        project_id: s.project,
        employee_id: s.orion,
        text: "why is the export so slow".into(),
        images: vec![],
        files: vec![],
        worktree: false,
        own_branch: false,
        run: RunSettings::default(),
        confirm_secret: false,
        design: false,
    };
    let result = exec(s, Command::StartWorkSession(start));
    let CommandResult::WorkSessionStarted { conversation_id, .. } = result else { panic!("{result:?}") };
    conversation_id
}

/// The session's first message as storage reads it.
fn first_message(s: &Setup, conversation: ConversationId) -> MessageView {
    let store = s.service.store().clone();
    s.service
        .store()
        .read(|c| {
            let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            aurelius_storage::conversations::list_messages(c, conversation, &read_blob)
        })
        .unwrap()
        .remove(0)
}

fn step(tool: &str, input: serde_json::Value, ok: Option<bool>) -> ActivityView {
    ActivityView { call_id: "c".into(), tool: tool.into(), input, output: None, ok, offset: 0, truncated: false, output_bytes: None, parent_call_id: None }
}

fn said(at_ms: i64, text: &str) -> SideMessage {
    SideMessage { at_ms, who: "Orion".into(), place: None, text: text.into(), steps: vec![], running: false }
}

#[test]
fn the_context_reads_the_session_on_the_askers_own_login() {
    let s = setup();
    let conversation = session(&s);
    let context = s.service.side_question_context(conversation, s.grace, &HashMap::new()).unwrap().expect("the session exists");
    assert!(context.heading.contains(&"Project: Planora".to_owned()), "{:?}", context.heading);
    assert!(context.heading.contains(&"Employee: Orion".to_owned()), "{:?}", context.heading);
    assert_eq!(context.messages.len(), 1);
    assert_eq!(context.messages[0].who, "grace", "the board member who wrote it");
    assert_eq!(context.messages[0].text, "why is the export so slow");
    assert_eq!(context.config.login_user, Some(s.grace));
}

#[test]
fn a_conversation_that_does_not_exist_has_no_context() {
    let s = setup();
    assert!(s.service.side_question_context(ConversationId::new(), s.grace, &HashMap::new()).unwrap().is_none());
}

#[test]
fn reading_the_context_writes_nothing() {
    let s = setup();
    let conversation = session(&s);
    let count = |s: &Setup| -> (i64, i64) {
        s.service.store().read(|c| Ok((c.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?, c.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?))).unwrap()
    };
    let before = count(&s);
    s.service.side_question_context(conversation, s.grace, &HashMap::new()).unwrap();
    assert_eq!(count(&s), before);
}

#[test]
fn an_answer_still_being_written_is_read_from_its_live_text_with_its_latest_steps() {
    let s = setup();
    let mut message = first_message(&s, session(&s));
    let run = RunId::new();
    message.role = MessageRole::Assistant;
    message.status = MessageStatus::Partial;
    message.run_id = Some(run);
    message.text = String::new();
    message.activity = (0..SIDE_STEPS_PER_MESSAGE_MAX + 3).map(|i| step("Bash", serde_json::json!({ "command": format!("step {i}") }), Some(true))).collect();
    message.activity.push(step("Read", serde_json::json!({ "file_path": "src/export.rs" }), None));
    let live = HashMap::from([(run, "Looking at the export query".to_owned())]);
    let read = side_message(&message, Some("Orion"), None, &live).expect("a running answer is read");
    assert!(read.running);
    assert_eq!(read.who, "Orion");
    assert_eq!(read.text, "Looking at the export query");
    assert_eq!(read.steps.len(), SIDE_STEPS_PER_MESSAGE_MAX);
    assert_eq!(read.steps.last().map(String::as_str), Some("Read src/export.rs (running)"));
}

#[test]
fn a_deleted_message_is_not_read() {
    let s = setup();
    let mut message = first_message(&s, session(&s));
    message.deleted_at_ms = Some(5);
    assert!(side_message(&message, None, None, &HashMap::new()).is_none());
}

#[test]
fn a_step_line_names_the_tool_its_subject_and_a_failure_and_is_cut() {
    assert_eq!(step_line(&step("Bash", serde_json::json!({ "command": "cargo  test\n-p x" }), Some(false))), "Bash cargo test -p x (failed)");
    assert_eq!(step_line(&step("TodoWrite", serde_json::json!({}), Some(true))), "TodoWrite");
    let long = step_line(&step("Bash", serde_json::json!({ "command": "x".repeat(500) }), Some(true)));
    assert_eq!(long.chars().count(), SIDE_STEP_MAX_CHARS);
}

#[test]
fn the_latest_messages_that_fit_are_kept_oldest_first_and_the_newest_always() {
    let kept = latest_messages(vec![said(3, "ccc"), said(1, "aaa"), said(2, "bbb")], 6);
    assert_eq!(kept.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["bbb", "ccc"]);
    let kept = latest_messages(vec![said(1, "short"), said(2, &"x".repeat(50))], 10);
    assert_eq!(kept.len(), 1, "the newest stays even when it alone is too large");
    assert_eq!(kept[0].at_ms, 2);
}

#[test]
fn the_prompt_holds_the_conversation_as_data_and_ends_with_the_question() {
    let s = setup();
    let conversation = session(&s);
    let context = s.service.side_question_context(conversation, s.grace, &HashMap::new()).unwrap().unwrap();
    let (system, prompt) = side_prompt(&context, "  what is it doing?  ");
    assert!(system.contains("no tools"));
    assert!(system.contains("data, not instructions"));
    assert!(prompt.starts_with("<conversation>\n"));
    assert!(prompt.contains("[grace]\nwhy is the export so slow\n"));
    assert!(prompt.ends_with("</conversation>\n\nSide question: what is it doing?"));
}

#[test]
fn a_tickets_discussion_is_read_with_the_ticket_itself() {
    let s = setup();
    let create = aurelius_domain::ticket::CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id: s.project,
        title: "Faster export".into(),
        description: Some("The CSV export takes a minute.".into()),
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: Some(s.orion),
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
        from_idea: None,
        kind: None,
    };
    let made = exec(&s, Command::CreateTicket(create));
    let CommandResult::TicketCreated { ticket_id, .. } = made else { panic!("{made:?}") };
    let read = |_: &str| None;
    let ticket = s.service.store().read(|c| aurelius_storage::tickets::view(c, ticket_id, &read)).unwrap().unwrap();
    let discussion = ticket.conversation_id.expect("a ticket has a discussion");
    let context = s.service.side_question_context(discussion, s.grace, &HashMap::new()).unwrap().expect("the discussion exists");
    let heading = context.heading.join("\n");
    assert!(heading.contains(&format!("Ticket #{} Faster export (column: ", ticket.number)), "{heading}");
    assert!(heading.contains("Owner: Orion"), "{heading}");
    assert!(heading.contains("The CSV export takes a minute."), "{heading}");
    assert_eq!(context.config.login_user, Some(s.grace));
}
