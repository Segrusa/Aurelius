//! The discussion engine: the general channel, a project's channel and a
//! ticket's discussion, who answers (B3) and the loop limits over all of them.

use super::*;
use crate::conversation::discussion;
use crate::run::approvals::RuntimePermission;
use crate::task::task_runs::EmployeeAction;
use aurelius_domain::company::{UpdateProject};
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::group_chat::{MAX_HOPS, SendGroupMessage, WAKING_POSTS_PER_RUN, WAKING_POSTS_PER_WINDOW};
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{self, ColumnKind, SetDispatcher};
use aurelius_domain::views::{SystemLine, SystemLineKind};
use aurelius_domain::{ConversationId, EmployeeId, RunId, TaskId, TicketId, UserId};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    project: ProjectId,
    user: UserId,
    ceo: EmployeeId,
    ann: EmployeeId,
    bo: EmployeeId,
    cy: EmployeeId,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: user }
}

fn exec(s: &Setup, command: Command) -> Result<CommandResult, AppError> {
    s.service.execute(&ctx(Some(s.user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).map(|c| c.receipt.result)
}

fn hire(service: &CommandService, user: UserId, name: &str, boss: Option<EmployeeId>) -> EmployeeId {
    let create = CreateEmployee { access_level: None, description: None, name: name.into(), role: "Dev".into(), title: None, reports_to: boss, instructions: None, folder: None };
    let made = service.execute(&ctx(Some(user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateEmployee(create) }).unwrap().receipt.result;
    let CommandResult::EmployeeCreated { employee_id, .. } = made else { panic!("{made:?}") };
    employee_id
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    let user = UserId::new();
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    let made = service
        .execute(&ctx(Some(user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateProject(CreateProject { name: "Orbit".into(), description: None, folder: None }) })
        .unwrap()
        .receipt
        .result;
    let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
    let ceo = hire(&service, user, "Atlas", None);
    let ann = hire(&service, user, "Ann", Some(ceo));
    let bo = hire(&service, user, "Bo", Some(ceo));
    let cy = hire(&service, user, "Cy", Some(ceo));
    Setup { _dir: dir, service, project: project_id, user, ceo, ann, bo, cy }
}

fn general(s: &Setup) -> ConversationId {
    s.service.store().read(aurelius_storage::conversations::group_conversation).unwrap().unwrap()
}

fn channel(s: &Setup, project: ProjectId) -> ConversationId {
    s.service.store().read(|c| aurelius_storage::discussions::channel(c, Some(project))).unwrap().expect("a project channel")
}

fn send(s: &Setup, channel: Option<ConversationId>, text: &str) -> Result<(Vec<EmployeeId>, Vec<RunId>), AppError> {
    match exec(s, Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel, thread_parent: None }))? {
        CommandResult::GroupMessageSent { to, run_ids, .. } => Ok((to, run_ids)),
        other => panic!("{other:?}"),
    }
}

fn post(s: &Setup, ticket: TicketId, text: &str) -> Result<(Vec<EmployeeId>, Vec<RunId>), AppError> {
    match exec(s, Command::PostTicketMessage(ticket::PostTicketMessage { confirm_secret: false, ticket_id: ticket, text: text.into(), images: vec![], files: vec![], thread_parent: None }))? {
        CommandResult::TicketMessagePosted { to, run_ids, .. } => Ok((to, run_ids)),
        other => panic!("{other:?}"),
    }
}

fn new_ticket(s: &Setup, title: &str, owner: Option<EmployeeId>) -> TicketId {
    let create = ticket::CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id: s.project,
        title: title.into(),
        description: None,
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: owner,
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
        from_idea: None,
    };
    let CommandResult::TicketCreated { ticket_id, .. } = exec(s, Command::CreateTicket(create)).unwrap() else { panic!() };
    ticket_id
}

fn ticket_view(s: &Setup, id: TicketId) -> aurelius_domain::ticket::TicketView {
    let read = |_: &str| None;
    s.service.store().read(|c| aurelius_storage::tickets::view(c, id, &read)).unwrap().unwrap()
}

fn engine_puts(s: &Setup, id: TicketId, kind: ColumnKind) {
    let target = s.service.store().read(|c| aurelius_storage::company::column_of_kind(c, kind)).unwrap().id;
    s.service.store().write(move |tx| aurelius_storage::tickets::move_to(tx, id, target, None)).unwrap();
}

fn act(s: &Setup, run: RunId, action: EmployeeAction) -> (bool, String) {
    let (ok, text, _) = s.service.employee_action(run, action).unwrap();
    (ok, text)
}

fn post_in_channel(s: &Setup, run: RunId, channel: Option<&str>, message: &str) -> (bool, String) {
    act(s, run, EmployeeAction::PostToGroupChat { message: message.into(), channel: channel.map(str::to_owned) })
}

fn post_in_discussion(s: &Setup, run: RunId, message: &str) -> (bool, String) {
    act(s, run, EmployeeAction::PostToDiscussion { message: message.into() })
}

fn hand_off(s: &Setup, run: RunId, to: &str, message: &str) -> (bool, String) {
    act(s, run, EmployeeAction::HandOff { to: to.into(), message: message.into() })
}

/// The runs queued for `employee` in its main chat, oldest first: (run, discussion that woke it, hop).
fn woken(s: &Setup, employee: EmployeeId) -> Vec<(RunId, ConversationId, u32)> {
    s.service
        .store()
        .read(move |c| {
            let chat = last_chat(c, employee);
            let mut stmt = c.prepare("SELECT r.id FROM runs r WHERE r.conversation_id = ?1 ORDER BY r.rowid")?;
            let ids: Vec<String> = stmt.query_map([chat.to_string()], |r| r.get(0))?.collect::<Result<_, _>>()?;
            let mut out = Vec::new();
            for id in ids {
                let run = RunId::parse(&id).unwrap();
                if let Some(g) = aurelius_storage::conversations::group_run(c, run)? {
                    out.push((run, g.conversation_id, g.hop));
                }
            }
            Ok(out)
        })
        .unwrap()
}

fn messages(s: &Setup, conversation: ConversationId) -> Vec<aurelius_domain::views::MessageView> {
    let st = s.service.store().clone();
    s.service.store().read(move |c| aurelius_storage::conversations::list_messages(c, conversation, &|k| st.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok()))).unwrap()
}

fn system_lines(s: &Setup, conversation: ConversationId) -> Vec<(SystemLineKind, String)> {
    messages(s, conversation).into_iter().filter_map(|m| m.system.map(|l| (l.kind, m.text))).collect()
}

/// Let `employee`'s woken runs end without an answer (so they no longer count as running).
fn last_woken(s: &Setup, employee: EmployeeId) -> (RunId, ConversationId, u32) {
    *woken(s, employee).last().expect("woken")
}

/// The update that woke `employee` last, as its main chat has it.
fn wake_text(s: &Setup, employee: EmployeeId) -> String {
    messages(s, aurelius_storage_main_chat(s, employee)).into_iter().rev().find(|m| m.update.as_deref() == Some("group")).expect("an update").text
}

/// What the run of `run` is given: its system text and its turns.
fn plan_of(s: &Setup, run: RunId) -> crate::run::plan::ChatRunPlan {
    let payload: String = s.service.store().read(|c| Ok(c.query_row("SELECT payload_json FROM jobs WHERE id = ?1", [run.to_string()], |r| r.get(0))?)).unwrap();
    s.service.prepare_chat_run(&serde_json::from_str(&payload).unwrap()).unwrap().expect("a plan")
}

#[test]
fn a_non_lead_is_woken_with_a_scoped_view_and_the_lead_with_the_whole_thread() {
    let s = setup();
    send(&s, None, "@Ann please look at the logs").unwrap();
    send(&s, None, "@Bo what about the release date").unwrap();
    send(&s, None, "@Ann and @Cy, one more thing").unwrap();

    // Ann is no lead: what named her, and how much she misses.
    let ann = wake_text(&s, s.ann);
    assert!(ann.contains("please look at the logs"), "{ann}");
    assert!(!ann.contains("release date"), "not addressed to her: {ann}");
    assert!(ann.contains("Other messages in this thread, not shown here: 1. read_group_chat reads them."), "{ann}");
    assert!(ann.contains("The lead of this thread is @Atlas."), "{ann}");

    // The CEO leads the general channel: everything, with no note.
    send(&s, None, "And what does the boss think?").unwrap();
    let ceo = wake_text(&s, s.ceo);
    assert!(ceo.contains(r#""text":"@Ann please look at the logs"}"#) && ceo.contains(r#""board_member":true"#) && ceo.contains("release date"), "{ceo}");
    assert!(!ceo.contains("Scoped view") && ceo.contains("You are the lead of this thread"), "{ceo}");
}

#[test]
fn the_owner_of_a_ticket_without_a_responsible_leads_and_sees_all_of_it() {
    let s = setup();
    let ticket = new_ticket(&s, "Alone", Some(s.ann));
    post(&s, ticket, "@Ann first question").unwrap();
    post(&s, ticket, "A thought in passing").unwrap();
    post(&s, ticket, "@Ann second question").unwrap();
    let ann = wake_text(&s, s.ann);
    assert!(ann.contains("first question") && ann.contains("in passing") && !ann.contains("Scoped view"), "{ann}");
    assert!(ann.contains("You are the lead of this thread"), "{ann}");
}

#[test]
fn the_full_thread_stays_readable_with_the_tool() {
    let s = setup();
    send(&s, None, "@Bo private to Bo").unwrap();
    let (_, runs) = send(&s, None, "@Ann your turn").unwrap();
    assert!(!wake_text(&s, s.ann).contains("private to Bo"));
    let (ok, text) = act(&s, runs[0], EmployeeAction::ReadGroupChat { last: None, channel: None });
    assert!(ok && text.contains("private to Bo") && text.contains("your turn"), "{text}");
}

#[test]
fn the_platform_rules_and_the_roster_are_in_the_system_text() {
    let s = setup();
    let (_, runs) = send(&s, None, "@Ann hello").unwrap();
    exec(&s, Command::UpdateEmployee(aurelius_domain::employee::UpdateEmployee { access_level: None, description: Some("Reviews Rust.".into()), employee_id: s.bo, name: "Bo".into(), role: "Dev".into(), title: None, instructions: None, sequential_tasks: None })).unwrap();
    let system = plan_of(&s, runs[0]).system.unwrap();
    assert!(system.starts_with(crate::run::plan::PLATFORM_RULES), "{system}");
    assert!(system.contains("- @Bo (Dev): Reviews Rust."), "the description: {system}");
    assert!(system.contains("- @Cy (Dev)"), "{system}");
}

#[test]
fn a_description_falls_back_to_the_first_instruction_line_and_can_be_cleared() {
    let s = setup();
    let update = |description: Option<&str>, instructions: Option<&str>| {
        exec(&s, Command::UpdateEmployee(aurelius_domain::employee::UpdateEmployee { access_level: None, description: description.map(str::to_owned), employee_id: s.bo, name: "Bo".into(), role: "Dev".into(), title: None, instructions: instructions.map(str::to_owned), sequential_tasks: None })).unwrap();
    };
    update(None, Some("\nGuards the release.\nMore detail."));
    let roster = |s: &Setup| {
        let (_, runs) = send(s, None, "@Ann hello").unwrap();
        let system = plan_of(s, *runs.last().unwrap()).system.unwrap();
        system.lines().find(|l| l.starts_with("- @Bo")).unwrap().to_owned()
    };
    assert_eq!(roster(&s), "- @Bo (Dev): Guards the release.");
    update(Some("  Reviews   Rust. "), Some("\nGuards the release."));
    assert!(roster(&s).ends_with(": Reviews Rust."), "trimmed on one line");
    update(Some(""), Some("\nGuards the release."));
    assert!(roster(&s).ends_with(": Guards the release."), "cleared");
    let too_long = "x".repeat(161);
    assert!(exec(&s, Command::UpdateEmployee(aurelius_domain::employee::UpdateEmployee { access_level: None, description: Some(too_long), employee_id: s.bo, name: "Bo".into(), role: "Dev".into(), title: None, instructions: None, sequential_tasks: None })).is_err());
}

#[test]
fn a_message_that_woke_the_run_is_marked_new() {
    let s = setup();
    let (_, runs) = send(&s, None, "@Ann start").unwrap();
    let turn = plan_of(&s, runs[0]).turns.last().unwrap().text.clone();
    assert!(turn.starts_with("[Group chat · from Owner] (new)\n"), "{turn}");
}

#[test]
fn the_general_channel_wakes_the_ceo_and_a_project_channel_its_dispatcher() {
    let s = setup();
    // General, no @: the CEO, at hop 0, answering in the general channel.
    let (to, runs) = send(&s, None, "How are we doing?").unwrap();
    assert_eq!((to, runs.len()), (vec![s.ceo], 1));
    assert_eq!(woken(&s, s.ceo), vec![(runs[0], general(&s), 0)]);

    // The project's channel, no @, no dispatcher: the CEO, answering in that channel.
    let orbit = channel(&s, s.project);
    let (to, runs) = send(&s, Some(orbit), "Status of Orbit?").unwrap();
    assert_eq!(to, vec![s.ceo]);
    assert_eq!(last_woken(&s, s.ceo), (runs[0], orbit, 0));
    // With a dispatcher for the project: that one.
    exec(&s, Command::UpdateProject(UpdateProject { project_id: s.project, dispatcher: Some(SetDispatcher { employee_id: Some(s.bo) }) })).unwrap();
    let (to, _) = send(&s, Some(orbit), "And now?").unwrap();
    assert_eq!(to, vec![s.bo]);
    let update = messages(&s, aurelius_storage_main_chat(&s, s.bo)).into_iter().last().unwrap();
    assert!(update.text.starts_with("In the #Orbit channel of the Team chat"), "{}", update.text);
    assert!(update.text.contains("From the board member grace:\nAnd now?"), "{}", update.text);
    // @Name wins over the dispatcher, and the answer is copied to the same channel.
    let (to, runs) = send(&s, Some(orbit), "@Ann can you look?").unwrap();
    assert_eq!(to, vec![s.ann]);
    s.service
        .store()
        .write({
            let st = s.service.store().clone();
            let chat = aurelius_storage_main_chat(&s, s.ann);
            let run = runs[0];
            move |tx| discussion::mirror_answer(tx, &st, run, chat, "On it.", None, &[], None)
        })
        .unwrap();
    let answer = messages(&s, orbit).into_iter().find(|m| m.text == "On it.").expect("copied to the channel");
    assert_eq!(answer.author.as_deref(), Some("Ann"));
    // The board member's messages carry their author.
    assert!(messages(&s, orbit).iter().filter(|m| m.role == aurelius_domain::conversation::MessageRole::User).all(|m| m.author_user.as_ref().map(|u| u.username.as_str()) == Some("grace")));
    // A ticket's discussion is its channel too (feature `ticket_channels`).
    let t = new_ticket(&s, "Login", None);
    let discussion = ticket_view(&s, t).conversation_id.unwrap();
    send(&s, Some(discussion), "x").unwrap();
    assert!(messages(&s, discussion).iter().any(|m| m.text == "x"));
}

/// The conversation `employee` was last woken in: its own for that thread (R6), else its main chat.
fn aurelius_storage_main_chat(s: &Setup, employee: EmployeeId) -> ConversationId {
    s.service.store().read(move |c| Ok(last_chat(c, employee))).unwrap()
}

fn last_chat(c: &rusqlite::Connection, employee: EmployeeId) -> ConversationId {
    let id: String = c
        .query_row(
            "SELECT id FROM conversations WHERE employee_id = ?1 AND kind = 'employee' AND quick_answer = 0 ORDER BY thread_key IS NOT NULL DESC, created_at_ms DESC, rowid DESC LIMIT 1",
            [employee.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    ConversationId::parse(&id).unwrap()
}

#[test]
fn a_mention_in_a_ticket_wakes_and_the_answer_comes_back_there() {
    let s = setup();
    let t = new_ticket(&s, "Login", Some(s.ann));
    let d = ticket_view(&s, t).conversation_id.expect("a discussion");
    // Backlog without @: nobody, with a hint.
    let (to, runs) = post(&s, t, "Some thoughts first.").unwrap();
    assert!(to.is_empty() && runs.is_empty());
    // A system line (not an employee's message), which clients show as one.
    assert!(system_lines(&s, d).iter().any(|(k, text)| *k == SystemLineKind::Limit && text.contains("only answers here while the ticket is in Todo, Doing, Plan review, Testing or Review")), "{:?}", system_lines(&s, d));
    assert!(!messages(&s, d).iter().any(|m| m.system.is_none() && m.text.starts_with("Nobody was woken")));
    // @Bo: Bo is woken at hop 0, in its own main chat, with the ticket's context; it is no owner.
    let (to, runs) = post(&s, t, "@Bo what do you think?").unwrap();
    assert_eq!(to, vec![s.bo]);
    assert_eq!(last_woken(&s, s.bo), (runs[0], d, 0));
    let update = messages(&s, aurelius_storage_main_chat(&s, s.bo)).into_iter().last().unwrap();
    assert!(update.text.starts_with("In the discussion of ticket #1 “Login”:"), "{}", update.text);
    let quoted = update.text.lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim_end_matches(',')).ok()).collect::<Vec<_>>();
    assert!(quoted.contains(&serde_json::json!({ "from": "grace", "board_member": true, "text": "Some thoughts first." })), "the earlier messages as JSON: {}", update.text);
    assert!(update.text.contains("You are not the owner of ticket #1"), "{}", update.text);
    // Its answer is copied to the ticket's discussion, under its name.
    s.service
        .store()
        .write({
            let st = s.service.store().clone();
            let chat = aurelius_storage_main_chat(&s, s.bo);
            let run = runs[0];
            move |tx| discussion::mirror_answer(tx, &st, run, chat, "I would split it.", None, &[], None)
        })
        .unwrap();
    assert!(messages(&s, d).iter().any(|m| m.text == "I would split it." && m.author.as_deref() == Some("Bo")));
    // Its run has the discussion tools; it posts there.
    assert!(s.service.store().read(|c| discussion::run_discussion(c, runs[0])).unwrap().is_some_and(|x| x.id == d));
    let (ok, text) = post_in_discussion(&s, runs[0], "Looks fine to me.");
    assert!(ok, "{text}");
    assert!(messages(&s, d).iter().any(|m| m.text == "Looks fine to me." && m.author.as_deref() == Some("Bo")));
    // Woken in the discussion, its group chat default is not the ticket: the project's channel.
    let (ok, text) = post_in_channel(&s, runs[0], None, "FYI");
    assert!(ok && text.starts_with("Posted in the #Orbit channel."), "{text}");
    // The non-owner did not touch the ticket.
    let view = ticket_view(&s, t);
    assert_eq!((view.column, view.root_task_id), (ColumnKind::Backlog, None));
    // A run of the general channel has no ticket discussion.
    let (_, runs) = send(&s, None, "@Cy hi").unwrap();
    let (ok, text) = post_in_discussion(&s, runs[0], "x");
    assert!(!ok && text.contains("no ticket discussion"), "{text}");
    // Ticket discussions are not among the channels.
    let channels = s.service.store().read(aurelius_storage::discussions::channels).unwrap();
    assert!(channels.iter().all(|c| c.conversation_id != d));
}

#[test]
fn without_a_mention_a_ticket_goes_to_its_owner_in_the_main_task() {
    let s = setup();
    let t = new_ticket(&s, "Login", Some(s.ann));
    exec(&s, Command::StartTicket(ticket::StartTicket { ticket_id: t, employee_id: None })).unwrap();
    let view = ticket_view(&s, t);
    assert_eq!(view.column, ColumnKind::Todo);
    let root = view.root_task_id.expect("a main task");
    // Todo, the main task not started yet: it is the owner's, without a run; the
    // main task reads it in the discussion when it starts.
    let (to, runs) = post(&s, t, "Start with the login form.").unwrap();
    assert_eq!((to, runs), (vec![s.ann], vec![]));
    assert!(woken(&s, s.ann).is_empty(), "the owner is not woken in its main chat");
    // The main task starts: its conversation, with a queued run.
    let job = s.service.store().write(|tx| aurelius_storage::queries::claim_due_job(tx, "test", aurelius_storage::now_ms() + 60_000, 45_000, 4)).unwrap().expect("the task's job");
    let task = s.service.task_context(root).unwrap().unwrap();
    s.service.start_task_conversation(&task, job.lease_epoch).unwrap();
    let root_conversation = s.service.store().read(move |c| aurelius_storage::tasks::task_conversation(c, root)).unwrap().map(|(c, _)| c).expect("its conversation");
    let runs_in = |s: &Setup| -> Vec<(RunId, String)> { s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, root_conversation)).unwrap() };
    let before = runs_in(&s);
    assert_eq!(before.len(), 1, "{before:?}");
    // Its run starts with the discussion as context and has the discussion tools.
    assert!(s.service.store().read(|c| discussion::conversation_discussion(c, root_conversation)).unwrap().is_some());
    assert!(s.service.store().read(|c| discussion::run_discussion(c, before[0].0)).unwrap().is_some());
    // Now a message goes into the main task, and the queued run answers it: no extra run.
    // The owner has no entry in `run_ids` (that run is the task's): `to` is longer.
    let (to, runs) = post(&s, t, "Please also check the logout.").unwrap();
    assert_eq!((to, runs), (vec![s.ann], vec![]));
    assert_eq!(runs_in(&s).len(), 2, "queued behind it: the worker takes them together");
    let note = messages(&s, root_conversation).into_iter().last().unwrap();
    assert_eq!((note.update.as_deref(), note.author.as_deref()), (Some("message"), Some("grace")));
    assert!(note.text.starts_with("Please also check the logout.") && note.text.contains("post_to_discussion"), "{}", note.text);
    // @Ann (the owner) also goes to the main task; @Bo besides wakes Bo.
    let (to, runs) = post(&s, t, "@Ann @Bo one more thing").unwrap();
    assert_eq!(to, vec![s.ann, s.bo]);
    assert_eq!(runs.len(), 1, "only Bo was woken");
    assert_eq!(last_woken(&s, s.bo).0, runs[0]);
    assert!(woken(&s, s.ann).is_empty());
    // In Backlog the owner no longer answers by itself.
    engine_puts(&s, t, ColumnKind::Backlog);
    let (to, _) = post(&s, t, "Thanks").unwrap();
    assert!(to.is_empty());
}

#[test]
fn an_answer_in_the_discussion_reaches_an_owner_whose_report_asks_a_question() {
    // AUR-BUG-Q: the owner reports with a question (Review); the board member answers
    // in the discussion without @, as in the main task's own conversation: a run of the
    // main task answers it, the ticket stays in Review and reports again.
    use aurelius_domain::task::TaskStatus;
    let s = setup();
    let t = new_ticket(&s, "Server name", Some(s.ann));
    exec(&s, Command::StartTicket(ticket::StartTicket { ticket_id: t, employee_id: None })).unwrap();
    let root = ticket_view(&s, t).root_task_id.expect("a main task");
    let job = s.service.store().write(|tx| aurelius_storage::queries::claim_due_job(tx, "test", aurelius_storage::now_ms() + 60_000, 45_000, 4)).unwrap().expect("the task's job");
    let task = s.service.task_context(root).unwrap().unwrap();
    s.service.start_task_conversation(&task, job.lease_epoch).unwrap();
    let root_conversation = s.service.store().read(move |c| aurelius_storage::tasks::task_conversation(c, root)).unwrap().map(|(c, _)| c).expect("its conversation");
    let runs_in = |s: &Setup| -> Vec<(RunId, String)> { s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, root_conversation)).unwrap() };
    // The owner's run ends with a question: the main task reports, the ticket goes to Review.
    let first = runs_in(&s)[0].0;
    let st = s.service.store().clone();
    s.service.store().write(move |tx| {
        tx.execute("UPDATE runs SET status = 'succeeded' WHERE id = ?1", [first.to_string()])?;
        crate::task::task_runs::complete(tx, &st, root, "Which server name do you mean: the host or the label?")
    })
    .unwrap();
    assert_eq!(task_status(&s, root), TaskStatus::Review);
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Review);
    assert!(runs_in(&s).is_empty());
    // The answer without @ goes to the owner in the main task, and a run answers it.
    let (to, runs) = post(&s, t, "The label all clients show.").unwrap();
    assert_eq!((to, runs), (vec![s.ann], vec![]), "the owner took it, nobody else was woken");
    let note = messages(&s, root_conversation).into_iter().last().unwrap();
    assert!(note.text.starts_with("The label all clients show."), "{}", note.text);
    assert_eq!(runs_in(&s).len(), 1, "a run of the main task answers it");
    assert!(woken(&s, s.ann).is_empty(), "not in the owner's main chat");
    let d = ticket_view(&s, t).conversation_id.unwrap();
    assert!(system_lines(&s, d).iter().all(|(k, _)| *k != SystemLineKind::Limit), "no 'nobody' line: {:?}", system_lines(&s, d));
    // The task stays in Review until that run reports again.
    assert_eq!(task_status(&s, root), TaskStatus::Review);
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Review);
    // A second answer while that run is queued waits too; the worker takes them together.
    post(&s, t, "And show it on mobile too.").unwrap();
    assert_eq!(runs_in(&s).len(), 2);
}

/// A started ticket whose owner works in Auto: its main task, that task's conversation and its run.
fn started_ticket(s: &Setup) -> (TicketId, TaskId, ConversationId, RunId) {
    let t = new_ticket(s, "Server name", Some(s.ann));
    exec(s, Command::StartTicket(ticket::StartTicket { ticket_id: t, employee_id: None })).unwrap();
    let root = ticket_view(s, t).root_task_id.expect("a main task");
    let job = s.service.store().write(|tx| aurelius_storage::queries::claim_due_job(tx, "test", aurelius_storage::now_ms() + 60_000, 45_000, 4)).unwrap().expect("the task's job");
    let task = s.service.task_context(root).unwrap().unwrap();
    s.service.start_task_conversation(&task, job.lease_epoch).unwrap();
    let conversation = s.service.store().read(move |c| aurelius_storage::tasks::task_conversation(c, root)).unwrap().map(|(c, _)| c).expect("its conversation");
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE conversations SET agent_mode = 'auto' WHERE id = ?1", [conversation.to_string()])?)).unwrap();
    let run = s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, conversation)).unwrap()[0].0;
    (t, root, conversation, run)
}

/// `started_ticket`, whose owner reported (`report`): the ticket is in Review.
fn reported_ticket(s: &Setup, report: &str) -> (TicketId, TaskId, ConversationId) {
    let (t, root, conversation, run) = started_ticket(s);
    end_run(s, run, report);
    assert_eq!(ticket_view(s, t).column, ColumnKind::Review);
    (t, root, conversation)
}

/// The run ends with `answer`, as a dispatcher's end does for the task.
fn end_run(s: &Setup, run: RunId, answer: &str) {
    let st = s.service.store().clone();
    let answer = answer.to_owned();
    s.service.store().write(move |tx| {
        tx.execute("UPDATE runs SET status = 'succeeded' WHERE id = ?1", [run.to_string()])?;
        crate::task::task_runs::on_run_finished(tx, &st, run, true, None, &answer)
    })
    .unwrap();
}

fn result_of(s: &Setup, task: TaskId) -> Option<String> {
    s.service.store().read(move |c| {
        let key: Option<String> = c.query_row("SELECT b.storage_key FROM tasks t JOIN blobs b ON b.id = t.result_blob_id WHERE t.id = ?1", [task.to_string()], |r| r.get(0)).ok();
        Ok(key)
    }).unwrap().and_then(|k| s.service.store().blobs().read(&k).ok()).and_then(|b| String::from_utf8(b).ok())
}

#[test]
fn an_answer_in_review_is_a_message_of_the_owner_and_no_second_report() {
    let s = setup();
    let (t, root, conversation) = reported_ticket(&s, "The server name is done. Tell me which label you want.");
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let reviews = |s: &Setup| system_lines(s, d).iter().filter(|(k, _)| *k == SystemLineKind::Review).count();
    assert_eq!(reviews(&s), 1);
    let report = result_of(&s, root).unwrap();
    post(&s, t, "The label all clients show.").unwrap();
    let run = s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, conversation)).unwrap()[0].0;
    end_run(&s, run, "Then I keep that one.");
    assert_eq!(reviews(&s), 1, "an answer does not say again that the work is ready");
    assert_eq!(result_of(&s, root).unwrap(), report, "the report stays what the board member decides on");
    assert_eq!(task_status(&s, root), aurelius_domain::task::TaskStatus::Review);
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Review);
    let last = messages(&s, d).into_iter().last().unwrap();
    assert!(last.text.ends_with("Then I keep that one."), "{}", last.text);
    assert_eq!(last.author.as_deref(), Some("Ann"), "under the owner's name");
}

#[test]
fn a_run_that_ends_on_a_question_keeps_the_ticket_in_doing_until_it_is_answered() {
    use aurelius_domain::task::TaskStatus;
    let s = setup();
    let (t, root, conversation, run) = started_ticket(&s);
    let d = ticket_view(&s, t).conversation_id.unwrap();
    end_run(&s, run, "I can do either.\n\nWhich label do you want?");
    assert_eq!(task_status(&s, root), TaskStatus::Waiting);
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Doing, "a question is no report");
    assert_eq!(system_lines(&s, d).iter().filter(|(k, _)| *k == SystemLineKind::Review).count(), 0);
    assert!(messages(&s, d).into_iter().last().unwrap().text.ends_with("Which label do you want?"), "the question reaches the board member");
    // The answer wakes the owner, and the task works again.
    post(&s, t, "The label all clients show.").unwrap();
    assert_eq!(task_status(&s, root), TaskStatus::Running);
    let next = s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, conversation)).unwrap();
    assert_eq!(next.len(), 1, "one run answers it");
    end_run(&s, next[0].0, "Done with the label.");
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Review);
}

#[test]
fn a_run_that_asked_in_the_discussion_and_signs_off_keeps_the_ticket_in_doing() {
    use aurelius_domain::task::TaskStatus;
    let s = setup();
    let (t, root, _, run) = started_ticket(&s);
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let st = s.service.store().clone();
    let discussion = s.service.store().read(move |c| crate::conversation::discussion::load(c, d)).unwrap().unwrap();
    s.service.store().write(move |tx| crate::conversation::discussion::post_from_run(tx, &st, run, &discussion, "Which label do you want?").map(|_| ())).unwrap();
    end_run(&s, run, "I asked in the discussion.");
    assert_eq!(task_status(&s, root), TaskStatus::Waiting);
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Doing);
}

#[test]
fn a_run_that_ends_on_a_statement_still_reports() {
    let s = setup();
    let (t, _, _) = reported_ticket(&s, "All done. Anything else?\n\nThe label is changed.");
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Review);
}

#[test]
fn a_run_that_posted_in_the_discussion_itself_is_not_posted_twice() {
    let s = setup();
    let (t, _, conversation) = reported_ticket(&s, "The server name is done.");
    let d = ticket_view(&s, t).conversation_id.unwrap();
    post(&s, t, "Push it.").unwrap();
    let run = s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, conversation)).unwrap()[0].0;
    let st = s.service.store().clone();
    let discussion = s.service.store().read(move |c| crate::conversation::discussion::load(c, d)).unwrap().unwrap();
    s.service.store().write(move |tx| crate::conversation::discussion::post_from_run(tx, &st, run, &discussion, "Pushed.").map(|_| ())).unwrap();
    let before = messages(&s, d).len();
    end_run(&s, run, "Pushed, and I said so in the discussion.");
    assert_eq!(messages(&s, d).len(), before);
}

/// The ticket's report is in, and a board member's message started a run that answers it.
fn answering_ticket(s: &Setup) -> (TicketId, TaskId, ConversationId, RunId) {
    let (t, root, conversation) = reported_ticket(s, "The server name is done.");
    post(s, t, "The label all clients show.").unwrap();
    let run = s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, conversation)).unwrap()[0].0;
    (t, root, ticket_view(s, t).conversation_id.unwrap(), run)
}

/// The run ends as `failed` or stopped; nothing but the discussion may hear of it.
fn answer_breaks(s: &Setup, t: TicketId, root: TaskId, d: ConversationId, run: RunId, failed: Option<&'static str>) -> String {
    let moved = |s: &Setup| system_lines(s, d).iter().filter(|(k, _)| *k == SystemLineKind::Moved).count();
    let (report, moved_before) = (result_of(s, root).unwrap(), moved(s));
    let st = s.service.store().clone();
    s.service.store().write(move |tx| {
        tx.execute("UPDATE runs SET status = 'failed' WHERE id = ?1", [run.to_string()])?;
        crate::task::task_runs::on_run_finished(tx, &st, run, false, failed, "half a sent")
    }).unwrap();
    assert_eq!(task_status(s, root), aurelius_domain::task::TaskStatus::Review);
    assert_eq!(ticket_view(s, t).column, ColumnKind::Review);
    assert_eq!(result_of(s, root).unwrap(), report);
    assert_eq!(moved(s), moved_before, "the card did not move");
    messages(s, d).into_iter().last().unwrap().text
}

#[test]
fn a_failed_answer_in_review_keeps_the_ticket_in_review_and_says_so_in_the_discussion() {
    let s = setup();
    let (t, root, d, run) = answering_ticket(&s);
    let said = answer_breaks(&s, t, root, d, run, Some("boom"));
    assert!(said.contains("could not finish my answer (boom)"), "{said}");
}

#[test]
fn a_stopped_answer_in_review_keeps_the_ticket_in_review_and_says_so_in_the_discussion() {
    let s = setup();
    let (t, root, d, run) = answering_ticket(&s);
    let said = answer_breaks(&s, t, root, d, run, None);
    assert!(said.contains("was stopped") && !said.contains("half a sent"), "{said}");
}

#[test]
fn an_answer_that_cannot_start_keeps_the_ticket_in_review_and_says_so_in_the_discussion() {
    let s = setup();
    let (t, root, d, run) = answering_ticket(&s);
    let st = s.service.store().clone();
    s.service.store().write(move |tx| {
        tx.execute("UPDATE runs SET status = 'failed' WHERE id = ?1", [run.to_string()])?;
        crate::task::task_runs::on_run_not_started(tx, &st, run)
    }).unwrap();
    assert_eq!(task_status(&s, root), aurelius_domain::task::TaskStatus::Review);
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Review);
    let said = messages(&s, d).into_iter().last().unwrap().text;
    assert!(said.contains("could not start"), "{said}");
}

#[test]
fn an_answer_with_another_run_behind_it_still_reaches_the_discussion() {
    let s = setup();
    let (t, root, d, run) = answering_ticket(&s);
    let conversation = s.service.store().read(move |c| aurelius_storage::tasks::task_conversation(c, root)).unwrap().unwrap().0;
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'running' WHERE id = ?1", [run.to_string()])?)).unwrap();
    post(&s, t, "And on the phone too.").unwrap();
    let behind = s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, conversation)).unwrap();
    assert_eq!(behind.len(), 2, "one more run behind the live one");
    end_run(&s, run, "The first answer.");
    let said = messages(&s, d).into_iter().last().unwrap().text;
    assert!(said.ends_with("The first answer."), "{said}");
    assert_eq!(task_status(&s, root), aurelius_domain::task::TaskStatus::Review);
}

#[test]
fn a_run_that_works_on_after_its_post_in_the_discussion_still_answers_there() {
    let s = setup();
    let (_, _, d, run) = answering_ticket(&s);
    let st = s.service.store().clone();
    let discussion = s.service.store().read(move |c| crate::conversation::discussion::load(c, d)).unwrap().unwrap();
    s.service.store().write(move |tx| crate::conversation::discussion::post_from_run(tx, &st, run, &discussion, "Looking at it.").map(|_| ())).unwrap();
    let tool_later = |tool: &'static str, call: &'static str| s.service.store().write(move |tx| {
        aurelius_storage::conversations::activity_started(tx, run, aurelius_domain::MessageId::new(), call, tool, &serde_json::json!({}), None, 0)?;
        Ok(tx.execute("UPDATE run_activity SET started_at_ms = started_at_ms + 1000 WHERE call_id = ?1", [call])?)
    }).unwrap();
    // Another post is not more work: the run's last word is still in the discussion.
    tool_later("mcp__aurelius__post_to_discussion", "call-1");
    assert!(s.service.store().read(move |c| aurelius_storage::discussions::run_said_last_in(c, d, run)).unwrap());
    tool_later("Read", "call-2");
    end_run(&s, run, "Here is the real answer.");
    let said = messages(&s, d).into_iter().last().unwrap().text;
    assert!(said.ends_with("Here is the real answer."), "{said}");
}

#[test]
fn a_report_is_the_closing_text_without_the_notes_before_the_last_tool_use() {
    let s = setup();
    let (_, root, _, run) = started_ticket(&s);
    let notes = "I'll start by looking at the screenshot.\n\n";
    let closing = "The server name now shows in every client; I tested the desktop and the phone, and nothing else changed.";
    let offset = notes.len() as u32;
    s.service.store().write(move |tx| aurelius_storage::conversations::activity_started(tx, run, aurelius_domain::MessageId::new(), "call-1", "Read", &serde_json::json!({}), None, offset)).unwrap();
    end_run(&s, run, &format!("{notes}{closing}"));
    let report = result_of(&s, root).unwrap();
    assert!(report.contains(closing) && !report.contains("I'll start"), "{report}");
}

#[test]
fn hops_stop_at_max_and_leave_a_system_line_that_wakes_nobody() {
    let s = setup();
    let general = general(&s);
    let (_, runs) = send(&s, None, "@Ann start").unwrap();
    let mut run = runs[0];
    let names = [("Bo", s.bo), ("Ann", s.ann)];
    for hop in 1..=MAX_HOPS {
        let (name, who) = names[(hop as usize + 1) % 2];
        let (ok, text) = hand_off(&s, run, name, "Your turn: check the release notes and report back.");
        assert!(ok && text.contains(&format!("{name} will answer")), "hop {hop}: {text}");
        let (next, conversation, at) = last_woken(&s, who);
        assert_eq!((conversation, at), (general, hop));
        run = next;
    }
    let before = (woken(&s, s.ann).len(), woken(&s, s.bo).len());
    // Bo (at hop MAX_HOPS) cannot hand over any more.
    let (ok, text) = hand_off(&s, run, "Ann", "One more.");
    assert!(!ok && text.contains("Limit reached") && text.contains("will not be woken"), "{text}");
    assert_eq!((woken(&s, s.ann).len(), woken(&s, s.bo).len()), before, "nobody past MAX_HOPS");
    let lines = system_lines(&s, general);
    assert!(lines.contains(&(SystemLineKind::Limit, "Limit reached: Ann was not woken.".into())), "{lines:?}");
    // The system line itself names them, and is not part of what employees read.
    let read = act(&s, run, EmployeeAction::ReadGroupChat { last: None, channel: None }).1;
    assert!(!read.contains("Limit reached"), "{read}");
    // A system line written anywhere wakes nobody, even with @Name in it.
    let mut line = SystemLine::new(SystemLineKind::Moved);
    line.by = Some("@Cy".into());
    s.service.store().write(move |tx| discussion::system_line(tx, general, &line, None)).unwrap();
    assert!(woken(&s, s.cy).is_empty());
}

#[test]
fn system_lines_are_written_in_words_with_the_ticket_number() {
    let s = setup();
    let general = general(&s);
    let t = new_ticket(&s, "Login", None);
    let number = ticket_view(&s, t).number;
    let mut line = SystemLine::new(SystemLineKind::TicketCreated);
    line.by = Some("alex".into());
    line.ticket_id = Some(t);
    s.service.store().write(move |tx| discussion::system_line(tx, general, &line, None)).unwrap();
    let lines = system_lines(&s, general);
    assert!(lines.contains(&(SystemLineKind::TicketCreated, format!("alex created #{number} from a message"))), "{lines:?}");
}

#[test]
fn waking_posts_are_capped_per_run_and_per_hour_over_three_discussions_together() {
    let s = setup();
    let general = general(&s);
    let orbit = channel(&s, s.project);
    let t = new_ticket(&s, "Login", None);
    let d = ticket_view(&s, t).conversation_id.unwrap();
    // Per run: WAKING_POSTS_PER_RUN handoffs.
    let (_, runs) = send(&s, None, "@Ann hi").unwrap();
    let run = runs[0];
    assert!(hand_off(&s, run, "Bo", "One.").1.contains("Bo will answer"));
    assert!(hand_off(&s, run, "Bo", "Two.").1.contains("Bo will answer"));
    assert_eq!(WAKING_POSTS_PER_RUN, 2);
    let (ok, text) = hand_off(&s, run, "Bo", "Three.");
    assert!(!ok && text.contains("Limit reached") && text.contains("in this run"), "{text}");
    // Per hour: Ann's runs from three discussions share one budget of WAKING_POSTS_PER_WINDOW.
    let (_, r_orbit) = send(&s, Some(orbit), "@Ann hi").unwrap();
    let (_, r_ticket) = post(&s, t, "@Ann hi").unwrap();
    let (_, r_general) = send(&s, None, "@Ann again").unwrap();
    assert_eq!(WAKING_POSTS_PER_WINDOW, 4);
    // Two waking posts so far (in `run`); two more from two other discussions...
    assert!(hand_off(&s, r_orbit[0], "Cy", "Look.").1.contains("Cy will answer"));
    assert!(hand_off(&s, r_ticket[0], "Cy", "Look.").1.contains("Cy will answer"));
    // ...and the fifth, in a third discussion, is refused.
    let (ok, text) = hand_off(&s, r_general[0], "Cy", "And here.");
    assert!(!ok && text.contains("Limit reached") && text.contains("last hour") && text.contains("all channels and tickets together"), "{text}");
    for conversation in [general, orbit, d] {
        assert!(!messages(&s, conversation).is_empty());
    }
    assert!(system_lines(&s, general).iter().any(|(k, text)| *k == SystemLineKind::Limit && text == "Limit reached: Cy was not woken."));
    // A post still goes through: it wakes nobody.
    let (ok, text) = post_in_channel(&s, r_general[0], None, "No mention.");
    assert_eq!((ok, text.as_str()), (true, "Posted in the group chat."));
}

#[test]
fn a_ticket_on_done_and_an_archived_projects_channel_are_read_only() {
    let s = setup();
    let t = new_ticket(&s, "Login", Some(s.ann));
    let (_, runs) = post(&s, t, "@Bo before").unwrap();
    engine_puts(&s, t, ColumnKind::Done);
    let err = post(&s, t, "after").unwrap_err();
    // A closed channel (the daemon sends `validation` to a client without `ticket_channels`).
    assert_eq!(err.code, ErrorCode::ChannelArchived);
    assert!(err.message.contains("can only be read"), "{}", err.message);
    let (ok, text) = post_in_discussion(&s, runs[0], "after");
    assert!(!ok && text.contains("can only be read"), "{text}");
    // Reading still works, and says so.
    let (ok, read) = act(&s, runs[0], EmployeeAction::ReadDiscussion { last: None });
    assert!(ok && read.contains("before") && read.contains("can only be read"), "{read}");
    // Unknown channel: a clear refusal naming the channels.
    let (ok, text) = post_in_channel(&s, runs[0], Some("Nowhere"), "x");
    assert!(!ok && text.contains("There is no channel “Nowhere”") && text.contains("Orbit"), "{text}");
    // Archived project: its channel can only be read.
    exec(&s, Command::SetProjectStatus(aurelius_domain::employee::SetProjectStatus { project_id: s.project, status: aurelius_domain::employee::LifecycleStatus::Archived })).unwrap();
    let orbit = channel(&s, s.project);
    assert_eq!(send(&s, Some(orbit), "x").unwrap_err().code, ErrorCode::ChannelArchived);
    let (ok, text) = post_in_channel(&s, runs[0], Some("Orbit"), "x");
    assert!(!ok && text.contains("archived"), "{text}");
}

#[test]
fn ticket_post_needs_a_board_member_and_an_existing_ticket() {
    let s = setup();
    let t = new_ticket(&s, "Login", None);
    let command = Command::PostTicketMessage(ticket::PostTicketMessage { confirm_secret: false, ticket_id: t, text: "x".into(), images: vec![], files: vec![], thread_parent: None });
    let Err(err) = s.service.execute(&ctx(None), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }) else { panic!("posted without a board member") };
    assert_eq!(err.code, ErrorCode::BoardMemberRequired);
    assert_eq!(post(&s, TicketId::new(), "x").unwrap_err().code, ErrorCode::NotFound);
}

/// The message `send` stored in a channel: its id.
fn sent_message(s: &Setup, channel: Option<ConversationId>, text: &str) -> aurelius_domain::MessageId {
    match exec(s, Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel, thread_parent: None })).unwrap() {
        CommandResult::GroupMessageSent { message_id, .. } => message_id,
        other => panic!("{other:?}"),
    }
}

fn ticket_from(s: &Setup, message: Option<aurelius_domain::MessageId>, user: Option<UserId>) -> Result<(TicketId, u32), AppError> {
    let create = ticket::CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id: s.project,
        title: "From chat".into(),
        description: None,
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: None,
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: message,
        from_idea: None,
    };
    let envelope = CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateTicket(create) };
    match s.service.execute(&ctx(user), envelope)?.receipt.result {
        CommandResult::TicketCreated { ticket_id, number } => Ok((ticket_id, number)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_ticket_made_from_a_message_links_both_ways_and_wakes_nobody() {
    // MU2-10 AC3: the chat gets "→ #N" at the message (ticket_created with its id), the
    // discussion starts with created_from pointing at it; only a board member (B6).
    let s = setup();
    let orbit = channel(&s, s.project);
    let message = sent_message(&s, Some(orbit), "The export should also do CSV");
    let before: usize = woken(&s, s.ceo).len() + woken(&s, s.ann).len();
    assert_eq!(ticket_from(&s, Some(message), None).unwrap_err().code, ErrorCode::BoardMemberRequired);
    assert_eq!(ticket_from(&s, Some(aurelius_domain::MessageId::new()), Some(s.user)).unwrap_err().code, ErrorCode::Validation, "an unknown message");

    let (t, number) = ticket_from(&s, Some(message), Some(s.user)).unwrap();
    let back: Vec<_> = messages(&s, orbit).into_iter().filter(|m| m.system.as_ref().is_some_and(|l| l.kind == SystemLineKind::TicketCreated)).collect();
    assert_eq!(back.len(), 1);
    let line = back[0].system.clone().unwrap();
    assert_eq!((line.ticket_id, line.message_id, line.by.as_deref()), (Some(t), Some(message), Some("grace")));
    assert_eq!(back[0].text, format!("grace created #{number} from a message"));
    let discussion = ticket_view(&s, t).conversation_id.unwrap();
    let first = messages(&s, discussion).into_iter().next().expect("a first line");
    let line = first.system.clone().expect("a system line first");
    assert_eq!((line.kind, line.conversation_id, line.message_id), (SystemLineKind::CreatedFrom, Some(orbit), Some(message)));
    assert_eq!(first.text, format!("#{number} created from a message in the #Orbit channel · grace"));
    assert_eq!(woken(&s, s.ceo).len() + woken(&s, s.ann).len(), before, "neither line wakes anyone");

    // From the general channel and from an employee's chat as well.
    let general = general(&s);
    let hello = sent_message(&s, None, "And a PDF export?");
    let (_, n) = ticket_from(&s, Some(hello), Some(s.user)).unwrap();
    assert!(system_lines(&s, general).contains(&(SystemLineKind::TicketCreated, format!("grace created #{n} from a message"))));
    let (ann_ticket, chat, _) = {
        let (_, runs) = send(&s, None, "@Ann what about XML?").unwrap();
        assert!(!runs.is_empty());
        let chat = aurelius_storage_main_chat(&s, s.ann);
        let update = messages(&s, chat).into_iter().rev().find(|m| m.update.is_some()).expect("an update in Ann's chat").id;
        let (ticket, number) = ticket_from(&s, Some(update), Some(s.user)).unwrap();
        (ticket, chat, number)
    };
    let first = messages(&s, ticket_view(&s, ann_ticket).conversation_id.unwrap()).into_iter().next().unwrap();
    assert!(first.text.contains("in Ann's chat"), "{}", first.text);
    assert!(messages(&s, chat).iter().any(|m| m.system.as_ref().is_some_and(|l| l.kind == SystemLineKind::TicketCreated && l.ticket_id == Some(ann_ticket))));
}

#[test]
fn a_run_woken_in_a_projects_channel_works_in_that_projects_context() {
    // MU2-08 AC4: the project's memory (and folder) for a run a project's channel woke;
    // the company's context for one the General channel woke.
    let s = setup();
    let orbit = channel(&s, s.project);
    let project = s.project;
    let scope = s.service.store().read(move |c| aurelius_storage::memory::scope_id(c, aurelius_domain::memory::MemoryScope::Project(project))).unwrap().expect("the project's scope");
    let plan_of = |run: RunId| {
        let payload: String = s.service.store().read(move |c| Ok(c.query_row("SELECT payload_json FROM jobs WHERE id = ?1", [run.to_string()], |r| r.get(0))?)).unwrap();
        s.service.prepare_chat_run(&serde_json::from_str(&payload).unwrap()).unwrap().expect("a plan")
    };
    let (_, runs) = send(&s, Some(orbit), "@Ann how is the export going?").unwrap();
    let in_project = plan_of(runs[0]);
    assert!(in_project.system.as_deref().unwrap().contains("The message you answer now comes from the project Orbit: answer in its context, with its memory"), "{:?}", in_project.system);
    assert!(in_project.manifest.as_ref().unwrap().scopes.contains(&scope), "the project's memory: {:?}", in_project.manifest);

    let (_, runs) = send(&s, None, "@Bo how is the export going?").unwrap();
    let in_general = plan_of(runs[0]);
    assert!(!in_general.system.as_deref().unwrap().contains("comes from the project"), "{:?}", in_general.system);
    assert!(!in_general.manifest.as_ref().unwrap().scopes.contains(&scope), "the company's context: {:?}", in_general.manifest);
}

#[test]
fn retry_summary_needs_a_board_member_and_a_ticket_on_done() {
    let s = setup();
    let t = new_ticket(&s, "Login", None);
    let command = Command::RetryTicketSummary(ticket::RetryTicketSummary { ticket_id: t });
    let Err(err) = s.service.execute(&ctx(None), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: command.clone() }) else { panic!("retried without a board member") };
    assert_eq!(err.code, ErrorCode::BoardMemberRequired);
    assert_eq!(exec(&s, command).unwrap_err().code, ErrorCode::Validation, "in Backlog");
    assert_eq!(exec(&s, Command::RetryTicketSummary(ticket::RetryTicketSummary { ticket_id: TicketId::new() })).unwrap_err().code, ErrorCode::NotFound);
}

#[test]
fn a_system_line_goes_into_a_discussion_on_done_but_people_cannot() {
    // B7 for people and tools; the summary's lines still get in (MU2-12 AC2/AC4).
    let s = setup();
    let t = new_ticket(&s, "Closed", None);
    engine_puts(&s, t, ColumnKind::Done);
    assert_eq!(post(&s, t, "late").unwrap_err().code, ErrorCode::ChannelArchived);
    s.service.store().write(move |tx| discussion::ticket_line(tx, t, SystemLine::new(SystemLineKind::SummaryFailed), "The summary for project memory failed: quota. A board member can try again.")).unwrap();
    let d = ticket_view(&s, t).conversation_id.unwrap();
    assert_eq!(system_lines(&s, d).last().map(|(k, _)| *k), Some(SystemLineKind::SummaryFailed));
}

/// Lease the job of `task` (a test stands in for the scheduler): its epoch.
fn lease(s: &Setup, task: aurelius_domain::TaskId) -> i64 {
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE jobs SET state = 'leased', lease_owner = 'test', lease_until_ms = ?2, lease_epoch = lease_epoch + 1 WHERE id = ?1", rusqlite::params![task.to_string(), aurelius_storage::now_ms() + 60_000])?;
            Ok(tx.query_row("SELECT lease_epoch FROM jobs WHERE id = ?1", [task.to_string()], |r| r.get::<_, i64>(0))?)
        })
        .unwrap()
}

/// Start `ticket` and its main task's conversation (Running, one queued run): (main task, its conversation).
fn started(s: &Setup, ticket: TicketId) -> (aurelius_domain::TaskId, ConversationId) {
    exec(s, Command::StartTicket(ticket::StartTicket { ticket_id: ticket, employee_id: None })).unwrap();
    let root = ticket_view(s, ticket).root_task_id.expect("a main task");
    let epoch = lease(s, root);
    let task = s.service.task_context(root).unwrap().unwrap();
    s.service.start_task_conversation(&task, epoch).unwrap();
    let conversation = s.service.store().read(move |c| aurelius_storage::tasks::task_conversation(c, root)).unwrap().map(|(c, _)| c).expect("its conversation");
    (root, conversation)
}

fn task_status(s: &Setup, task: aurelius_domain::TaskId) -> aurelius_domain::task::TaskStatus {
    s.service.store().read(move |c| aurelius_storage::tasks::load_task(c, task)).unwrap().unwrap().status
}

fn runs_in(s: &Setup, conversation: ConversationId) -> Vec<(RunId, String)> {
    s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, conversation)).unwrap()
}

#[test]
fn a_message_for_an_owner_whose_main_task_waits_its_turn_runs_nothing_and_comes_along_at_the_turn() {
    // Trace M1: a main task that waits for its turn (Queued, its conversation there)
    // gets no run of its own from the discussion: nothing jumps the line, and no
    // answer to a chat message ends the task (Review). The owner reads it at its turn.
    use aurelius_domain::task::TaskStatus;
    let s = setup();
    let a = new_ticket(&s, "Login", Some(s.ann));
    let b = new_ticket(&s, "Logout", Some(s.ann));
    let (root_a, conv_a) = started(&s, a);
    let (root_b, _) = started(&s, b);
    assert_eq!((task_status(&s, root_a), task_status(&s, root_b)), (TaskStatus::Running, TaskStatus::Running));
    // A's run ended while a colleague still worked on a part; then Ann works on one task at a time.
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE runs SET status = 'succeeded' WHERE conversation_id = ?1", [conv_a.to_string()])?;
            let _ = crate::task::task_line::set_status(tx, root_a, TaskStatus::Waiting, Some("dependency"))?;
            Ok(())
        })
        .unwrap();
    let ann = ticket_view(&s, a).owner_employee_id.unwrap();
    exec(&s, Command::UpdateEmployee(aurelius_domain::employee::UpdateEmployee { access_level: None, description: None, employee_id: ann, name: "Ann".into(), role: "Dev".into(), title: None, instructions: None, sequential_tasks: Some(true) })).unwrap();
    // The colleague is done: A waits for its turn behind B (as after Send back with a busy owner).
    let st = s.service.store().clone();
    s.service.store().write(move |tx| crate::task::task_runs::continue_when_done(tx, &st, root_a)).unwrap();
    assert_eq!(task_status(&s, root_a), TaskStatus::Queued);
    assert!(runs_in(&s, conv_a).is_empty());
    let column = ticket_view(&s, a).column;

    // A message without @: it goes to the owner in the main task, and nothing runs.
    let (to, runs) = post(&s, a, "Also check the logout link.").unwrap();
    assert_eq!((to, runs), (vec![s.ann], vec![]), "the owner gets it in the main task, nobody was woken");
    assert!(runs_in(&s, conv_a).is_empty(), "no run jumps the line");
    assert!(woken(&s, s.ann).is_empty());
    assert_eq!(task_status(&s, root_a), TaskStatus::Queued);
    let view = ticket_view(&s, a);
    assert_eq!(view.column, column);
    assert_ne!(view.column, ColumnKind::Review);
    assert!(system_lines(&s, ticket_view(&s, a).conversation_id.unwrap()).iter().all(|(k, _)| *k != SystemLineKind::Limit), "no 'nobody' line");

    // B ends: A's turn comes, and its run has the message in what it reads.
    s.service.store().write(move |tx| crate::task::task_line::set_status(tx, root_b, TaskStatus::Review, None).map(|_| ())).unwrap();
    let epoch = lease(&s, root_a);
    let task = s.service.task_context(root_a).unwrap().unwrap();
    s.service.start_task_conversation(&task, epoch).unwrap();
    assert_eq!(task_status(&s, root_a), TaskStatus::Running);
    let queued = runs_in(&s, conv_a);
    assert_eq!(queued.len(), 1, "{queued:?}");
    let payload: String = s.service.store().read(move |c| Ok(c.query_row("SELECT payload_json FROM jobs WHERE id = ?1", [queued[0].0.to_string()], |r| r.get(0))?)).unwrap();
    let plan = s.service.prepare_chat_run(&serde_json::from_str(&payload).unwrap()).unwrap().expect("a plan");
    assert!(plan.turns.iter().any(|t| t.text.contains("Also check the logout link.")), "the message comes along at the turn: {:?}", plan.turns.iter().map(|t| &t.text).collect::<Vec<_>>());
}

#[test]
fn a_message_for_an_owner_whose_main_task_failed_reaches_nobody_and_says_so() {
    // Trace m1/c4: the main task failed in Doing (B6): the owner is not in `to`, and
    // the discussion gets a system line "The main task failed · Start makes a new one".
    use aurelius_domain::task::TaskStatus;
    let s = setup();
    let t = new_ticket(&s, "Login", Some(s.ann));
    let (root, conversation) = started(&s, t);
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE runs SET status = 'failed' WHERE conversation_id = ?1", [conversation.to_string()])?;
            crate::task::task_line::set_status(tx, root, TaskStatus::Failed, None).map(|_| ())
        })
        .unwrap();
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let (to, runs) = post(&s, t, "Any news?").unwrap();
    assert!(to.is_empty() && runs.is_empty(), "{to:?} {runs:?}");
    assert!(runs_in(&s, conversation).is_empty());
    let lines = system_lines(&s, d);
    assert!(lines.iter().any(|(k, text)| *k == SystemLineKind::Limit && text.starts_with("The main task failed · Start makes a new one")), "{lines:?}");
    // With @Bo besides: Bo is woken, the owner still not, and the hint is there again.
    let (to, runs) = post(&s, t, "@Bo can you look?").unwrap();
    assert_eq!((to, runs.len()), (vec![s.bo], 1));
    assert!(woken(&s, s.ann).is_empty());
}

#[test]
fn a_waiting_main_task_takes_the_message_without_resuming() {
    // Trace m2: Waiting (e.g. after a stop): the message is a note in the main task, nothing runs.
    use aurelius_domain::task::TaskStatus;
    let s = setup();
    let t = new_ticket(&s, "Login", Some(s.ann));
    let (root, conversation) = started(&s, t);
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE runs SET status = 'cancelled' WHERE conversation_id = ?1", [conversation.to_string()])?;
            crate::task::task_line::set_status(tx, root, TaskStatus::Waiting, Some("input")).map(|_| ())
        })
        .unwrap();
    let (to, runs) = post(&s, t, "Go on with the form.").unwrap();
    assert_eq!((to, runs), (vec![s.ann], vec![]));
    assert!(runs_in(&s, conversation).is_empty());
    assert_eq!(task_status(&s, root), TaskStatus::Waiting);
    assert!(messages(&s, conversation).iter().any(|m| m.text.starts_with("Go on with the form.")));
}

#[test]
fn only_colleagues_that_can_be_woken_use_the_waking_budget() {
    // Trace m4: a colleague without a main chat cannot be handed to, and that uses no budget.
    let s = setup();
    let (cy, project) = (s.cy, s.project);
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE conversations SET kind = 'task', project_id = ?2 WHERE employee_id = ?1 AND kind = 'employee'", [cy.to_string(), project.to_string()])?)).unwrap();
    let (_, runs) = send(&s, None, "@Ann hi").unwrap();
    let run = runs[0];
    for _ in 0..WAKING_POSTS_PER_RUN + 1 {
        let (ok, text) = hand_off(&s, run, "Cy", "Look.");
        assert!(!ok && !text.contains("Limit reached"), "{text}");
    }
    assert!(hand_off(&s, run, "Bo", "One.").1.contains("Bo will answer"));
    assert!(hand_off(&s, run, "Bo", "Two.").1.contains("Bo will answer"));
    assert!(hand_off(&s, run, "Bo", "Three.").1.contains("Limit reached"));
}

#[test]
fn a_ticket_from_a_message_of_another_projects_channel_or_discussion_is_refused() {
    // Sentinel n1: a message of a project (its channel, a ticket's discussion, a task)
    // only makes a ticket in that project; the General channel and an employee's chat in any.
    let s = setup();
    let made = exec(&s, Command::CreateProject(CreateProject { name: "Nova".into(), description: None, folder: None })).unwrap();
    let CommandResult::ProjectCreated { project_id: nova, .. } = made else { panic!("{made:?}") };
    let elsewhere = sent_message(&s, Some(channel(&s, nova)), "Nova needs a logo");
    let err = ticket_from(&s, Some(elsewhere), Some(s.user)).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    assert!(err.message.contains("another project"), "{}", err.message);
    // The same project's channel, a ticket's discussion and a task: fine.
    let here = sent_message(&s, Some(channel(&s, s.project)), "Orbit needs a logo");
    assert!(ticket_from(&s, Some(here), Some(s.user)).is_ok());
    let t = new_ticket(&s, "Login", Some(s.ann));
    post(&s, t, "Split off the reset flow").unwrap();
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let in_discussion = messages(&s, d).into_iter().find(|m| m.text == "Split off the reset flow").unwrap().id;
    assert!(ticket_from(&s, Some(in_discussion), Some(s.user)).is_ok());
    let (_, conversation) = started(&s, t);
    let in_task = messages(&s, conversation).into_iter().next().expect("the brief").id;
    assert!(ticket_from(&s, Some(in_task), Some(s.user)).is_ok());
    // The General channel: any project.
    let general = sent_message(&s, None, "Everyone needs a logo");
    assert!(ticket_from(&s, Some(general), Some(s.user)).is_ok());
}

#[test]
fn a_ticket_channel_cannot_be_deleted_and_keeps_its_messages() {
    // Trace R1 M1: for kind 'ticket' archived_at_ms means "closed", so a delete would erase
    // the history and leave a channel that sync later "reopens".
    let s = setup();
    let t = new_ticket(&s, "Login", None);
    let d = ticket_view(&s, t).conversation_id.unwrap();
    post(&s, t, "keep me").unwrap();
    let delete = || exec(&s, Command::DeleteConversation(aurelius_domain::conversation::DeleteConversation { conversation_id: d }));
    let err = delete().unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    assert_eq!(err.message, "A ticket's channel cannot be deleted; archive the ticket instead.");
    assert_eq!(s.service.store().read(move |c| aurelius_storage::discussions::archived_at(c, d)).unwrap(), None);
    assert!(messages(&s, d).iter().any(|m| m.text == "keep me"), "the message text stays");
    let listed = s.service.store().read(aurelius_storage::discussions::ticket_channels).unwrap();
    assert!(!listed[0].archived, "listed as open, not as archived");
    // A closed channel is refused with the same words, not as "not found".
    s.service.store().write(move |tx| aurelius_storage::discussions::set_ticket_channel_archived(tx, d, Some(1))).unwrap();
    assert_eq!(delete().unwrap_err().message, "A ticket's channel cannot be deleted; archive the ticket instead.");
    assert!(messages(&s, d).iter().any(|m| m.text == "keep me"));
}

#[test]
fn a_post_in_a_ticket_channel_through_group_chat_send_needs_a_board_member() {
    // Trace R1 m2: group_chat.send reaches a ticket's channel too, behind the same gate as ticket.post.
    let s = setup();
    let t = new_ticket(&s, "Login", None);
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let command = Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: "no one signed in".into(), images: vec![], files: vec![], channel: Some(d), thread_parent: None });
    let Err(err) = s.service.execute(&ctx(None), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }) else { panic!("posted in a ticket's channel without a board member") };
    assert_eq!(err.code, ErrorCode::BoardMemberRequired);
    assert!(!messages(&s, d).iter().any(|m| m.text == "no one signed in"), "nothing was stored");
    // A board member may.
    send(&s, Some(d), "signed in").unwrap();
    assert!(messages(&s, d).iter().any(|m| m.text == "signed in"));
}

/// Who follows a ticket, as (username, reason).
fn followers(s: &Setup, ticket: TicketId) -> Vec<(String, String)> {
    s.service.store().read(|c| aurelius_storage::followers::list(c, ticket)).unwrap().into_iter().map(|f| (f.username, f.reason)).collect()
}

#[test]
fn what_an_employee_writes_or_a_decisions_feedback_names_makes_that_board_member_follow() {
    let s = setup();
    let dana = UserId::new();
    s.service.store().write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &dana.to_string(), username: "dana", created_by_user: None, now_ms: 2 }, PHC)).unwrap();
    let (grace, named) = (("grace".to_owned(), "created".to_owned()), ("dana".to_owned(), "mentioned".to_owned()));
    let leave = |t| s.service.execute(&ctx(Some(dana)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::FollowTicket(ticket::FollowTicket { ticket_id: t, on: false }) }).unwrap();
    let t = new_ticket(&s, "Login", Some(s.ann));
    let (_, runs) = post(&s, t, "@Bo what do you think?").unwrap();
    assert_eq!(followers(&s, t), vec![grace.clone()]);

    // The answer of a woken run, copied to the discussion.
    let st = s.service.store().clone();
    let chat = aurelius_storage_main_chat(&s, s.bo);
    let run = runs[0];
    s.service.store().write(move |tx| discussion::mirror_answer(tx, &st, run, chat, "Ask @dana.", None, &[], None)).unwrap();
    assert_eq!(followers(&s, t), vec![grace.clone(), named.clone()]);
    leave(t);

    // A post of a run in the discussion.
    let (ok, text) = post_in_discussion(&s, runs[0], "@dana have a look");
    assert!(ok, "{text}");
    assert_eq!(followers(&s, t), vec![grace.clone(), named.clone()]);
    leave(t);

    // The feedback of a decision, a board member's message in the discussion.
    let (st, user) = (s.service.store().clone(), s.user);
    s.service.store().write(move |tx| discussion::decision_feedback(tx, &st, t, user, &serde_json::json!({}), "cmd", "@dana knows this")).unwrap();
    assert_eq!(followers(&s, t), vec![grace, named]);
}

#[path = "discussion_thread_tests.rs"]
mod threads;

#[path = "discussion_edit_tests.rs"]
mod edits;

/// Say that `employee`'s oldest woken run in its main chat is going, since 14 minutes.
fn go_running(s: &Setup, employee: EmployeeId) {
    let chat = aurelius_storage_main_chat(s, employee);
    s.service
        .store()
        .write(move |tx| {
            Ok(tx.execute("UPDATE runs SET status = 'running', started_at_ms = ?2 WHERE conversation_id = ?1", rusqlite::params![chat.to_string(), aurelius_storage::now_ms() - 14 * 60_000])?)
        })
        .unwrap();
}

/// The hidden conversation of `employee`'s quick answers, if there is one, with its runs (run, discussion that woke it).
fn quick_runs(s: &Setup, employee: EmployeeId) -> Option<(ConversationId, Vec<(RunId, ConversationId)>)> {
    s.service
        .store()
        .read(move |c| {
            let Some(chat) = c.query_row("SELECT id FROM conversations WHERE employee_id = ?1 AND quick_answer = 1", [employee.to_string()], |r| r.get::<_, String>(0)).ok() else { return Ok(None) };
            let chat = ConversationId::parse(&chat).unwrap();
            let mut stmt = c.prepare("SELECT id FROM runs WHERE conversation_id = ?1 ORDER BY rowid")?;
            let ids: Vec<String> = stmt.query_map([chat.to_string()], |r| r.get(0))?.collect::<Result<_, _>>()?;
            let runs = ids
                .iter()
                .filter_map(|id| RunId::parse(id).ok())
                .filter_map(|run| aurelius_storage::conversations::group_run(c, run).ok().flatten().map(|g| (run, g.conversation_id)))
                .collect();
            Ok(Some((chat, runs)))
        })
        .unwrap()
}

#[test]
fn a_mention_of_a_busy_employee_also_gets_a_quick_answer_that_wakes_nobody() {
    let s = setup();
    let general = general(&s);
    send(&s, None, "@Bo first question").unwrap();
    assert!(quick_runs(&s, s.bo).is_none(), "an idle employee gets no quick answer");
    go_running(&s, s.bo);
    // Bo is mid-run: the message waits behind it, and a quick run answers at once.
    let (to, runs) = send(&s, None, "@Bo status?").unwrap();
    assert_eq!(to, vec![s.bo]);
    assert_eq!(runs.len(), 1);
    let (quick_chat, quick) = quick_runs(&s, s.bo).expect("a quick-answer conversation");
    assert_eq!(quick.len(), 1);
    assert_eq!(quick[0].1, general, "its answer goes to the channel that asked");
    assert_ne!(quick[0].0, runs[0], "it is not the run behind the long one");
    let lines = system_lines(&s, general);
    assert!(lines.iter().any(|(k, t)| *k == SystemLineKind::Limit && t.contains("Bo is busy with a run (14m): a quick answer follows now")), "{lines:?}");
    // It is hidden: not among Bo's chats, never its main chat.
    let chats = s.service.store().read(move |c| aurelius_storage::conversations::employee_conversations(c, s.bo)).unwrap();
    assert!(chats.iter().all(|c| c.id != quick_chat), "{chats:?}");
    assert_ne!(quick_chat, aurelius_storage_main_chat(&s, s.bo));
    assert!(s.service.store().read(move |c| aurelius_storage::conversations::is_quick_answer(c, quick_chat)).unwrap());
    // It has no tools: no folder, so a text-only profile.
    let first_run = quick[0].0;
    let job = s.service.store().read(move |c| Ok(c.query_row("SELECT payload_json FROM jobs WHERE json_extract(payload_json, '$.run_id') = ?1", [first_run.to_string()], |r| r.get::<_, String>(0))?)).unwrap();
    let plan = s.service.prepare_chat_run(&serde_json::from_str(&job).unwrap()).unwrap().expect("a plan");
    assert!(plan.work.is_none(), "a quick answer works without tools");
    // Its answer lands in the channel under Bo's name and wakes the colleague it names: nobody.
    let st = s.service.store().clone();
    let run = quick[0].0;
    s.service.store().write(move |tx| discussion::mirror_answer(tx, &st, run, quick_chat, "I am mid-run; @Cy has the details.", None, &[], None)).unwrap();
    assert!(messages(&s, general).iter().any(|m| m.author.as_deref() == Some("Bo") && m.text.starts_with("I am mid-run")));
    assert!(woken(&s, s.cy).is_empty(), "a quick answer wakes nobody");
}

#[test]
fn a_quick_answer_that_fails_posts_nothing_and_only_one_runs_at_a_time() {
    let s = setup();
    let general = general(&s);
    send(&s, None, "@Bo first").unwrap();
    go_running(&s, s.bo);
    send(&s, None, "@Bo second").unwrap();
    // The first quick run is still going: the next message gets the busy line only.
    send(&s, None, "@Bo third").unwrap();
    let (quick_chat, quick) = quick_runs(&s, s.bo).unwrap();
    assert_eq!(quick.len(), 1, "one quick run per employee at a time");
    assert!(system_lines(&s, general).iter().any(|(_, t)| t.contains("Bo is busy with a run (14m), will answer when it ends.")));
    // It fails: nothing is posted, so the busy line stays the only word.
    let (st, run) = (s.service.store().clone(), quick[0].0);
    let posted = s.service.store().write(move |tx| discussion::mirror_answer(tx, &st, run, quick_chat, "half an answ", Some("failed_start"), &[], None)).unwrap();
    assert!(posted.is_empty());
    assert!(!messages(&s, general).iter().any(|m| m.author.as_deref() == Some("Bo") && m.system.is_none() && m.text.contains("half an answ")));
}

#[test]
fn a_quick_answer_that_arrives_after_the_full_answer_is_dropped() {
    let s = setup();
    let general = general(&s);
    send(&s, None, "@Bo first").unwrap();
    go_running(&s, s.bo);
    let (_, runs) = send(&s, None, "@Bo status?").unwrap();
    let (quick_chat, quick) = quick_runs(&s, s.bo).unwrap();
    let main = aurelius_storage_main_chat(&s, s.bo);
    // The long run ends first and answers.
    let (st, long) = (s.service.store().clone(), runs[0]);
    s.service.store().write(move |tx| discussion::mirror_answer(tx, &st, long, main, "All done.", None, &[], None)).unwrap();
    // The quick run only now finishes: its answer would be a second, stale reply.
    let (st, run) = (s.service.store().clone(), quick[0].0);
    let posted = s.service.store().write(move |tx| discussion::mirror_answer(tx, &st, run, quick_chat, "I am mid-run.", None, &[], None)).unwrap();
    assert!(posted.is_empty());
    let from_bo: Vec<_> = messages(&s, general).into_iter().filter(|m| m.author.as_deref() == Some("Bo") && m.system.is_none()).collect();
    assert!(from_bo.iter().any(|m| m.text.starts_with("All done")), "{from_bo:?}");
    assert!(!from_bo.iter().any(|m| m.text.starts_with("I am mid-run")), "{from_bo:?}");
}

#[test]
fn quick_answers_stop_at_the_hourly_cost_cap_and_the_busy_line_stays() {
    let s = setup();
    let general = general(&s);
    send(&s, None, "@Bo first").unwrap();
    go_running(&s, s.bo);
    send(&s, None, "@Bo second").unwrap();
    let (quick_chat, quick) = quick_runs(&s, s.bo).unwrap();
    // The quick run ended and cost the whole hour's share.
    let run = quick[0].0;
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE runs SET status = 'succeeded' WHERE id = ?1", [run.to_string()])?;
            let event: String = tx.query_row("SELECT id FROM events LIMIT 1", [], |r| r.get(0))?;
            let usage = aurelius_storage::usage::NewUsage { input_tokens: 1, output_tokens: 1, cached_tokens: 0, cache_creation_tokens: 0, cost_micro_usd: Some(aurelius_domain::group_chat::QUICK_ANSWER_COST_PER_HOUR_MICRO), estimate: true, model_calls: 1 };
            aurelius_storage::usage::insert(tx, run, aurelius_domain::EventId::parse(&event).unwrap(), &usage)
        })
        .unwrap();
    send(&s, None, "@Bo third").unwrap();
    assert_eq!(quick_runs(&s, s.bo).unwrap().1.len(), 1, "capped: no new quick run");
    assert_eq!(quick_runs(&s, s.bo).unwrap().0, quick_chat);
    assert!(system_lines(&s, general).iter().filter(|(_, t)| t.contains("will answer when it ends.")).count() >= 1);
}

#[test]
fn a_message_to_a_busy_owner_gets_a_quick_answer_in_the_ticket() {
    let s = setup();
    let (t, _root, conversation, run) = started_ticket(&s);
    let d = ticket_view(&s, t).conversation_id.expect("a discussion");
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = 'running', started_at_ms = ?2 WHERE id = ?1", rusqlite::params![run.to_string(), aurelius_storage::now_ms() - 14 * 60_000])?)).unwrap();
    let before = s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, conversation)).unwrap().len();
    let (to, _) = post(&s, t, "status?").unwrap();
    assert_eq!(to, vec![s.ann]);
    let (_, quick) = quick_runs(&s, s.ann).expect("a quick answer for the owner");
    assert_eq!(quick.len(), 1);
    assert_eq!(quick[0].1, d, "its answer goes to the ticket's discussion");
    assert!(system_lines(&s, d).iter().any(|(_, t)| t.contains("Ann is busy with a run (14m): a quick answer follows now")), "{:?}", system_lines(&s, d));
    // The owner's own run in the task goes on as before: the message still reaches it.
    let after = s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, conversation)).unwrap().len();
    assert!(after >= before);
}

// ---- run cards, interrupt cards and the odometer (U1, U2, U4) ----

fn set_running(s: &Setup, run: RunId, started_at_ms: i64) {
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE runs SET status = 'running', started_at_ms = ?2 WHERE id = ?1", rusqlite::params![run.to_string(), started_at_ms])?;
            Ok(tx.execute("UPDATE jobs SET state = 'leased', lease_owner = 'test', lease_until_ms = 0 WHERE id = ?1", [run.to_string()])?)
        })
        .unwrap();
}

fn record_usage(s: &Setup, run: RunId, input: u64, login: Option<UserId>) {
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE runs SET login_user = ?2 WHERE id = ?1", rusqlite::params![run.to_string(), login.map(|u| u.to_string())])?;
            let event: String = tx.query_row("SELECT id FROM events LIMIT 1", [], |r| r.get(0))?;
            let usage = aurelius_storage::usage::NewUsage { input_tokens: input, output_tokens: 1, cached_tokens: 0, cache_creation_tokens: 0, cost_micro_usd: Some(10), estimate: false, model_calls: 2 };
            aurelius_storage::usage::insert(tx, run, aurelius_domain::EventId::parse(&event).unwrap(), &usage)
        })
        .unwrap();
}

#[test]
fn a_post_names_its_run_and_only_the_last_post_of_a_run_has_its_card() {
    let s = setup();
    let (t, _, _, _) = started_ticket(&s);
    let d = ticket_view(&s, t).conversation_id.expect("a discussion");
    let (_, runs) = post(&s, t, "@Bo please look").unwrap();
    let run = runs[0];
    set_running(&s, run, 1);
    assert!(post_in_discussion(&s, run, "First finding.").0);
    assert!(post_in_discussion(&s, run, "Second finding.").0);
    record_usage(&s, run, 40, None);

    let all = messages(&s, d);
    let of = |start: &str| all.iter().find(|m| m.text.starts_with(start)).unwrap().clone();
    let (first, second) = (of("First finding."), of("Second finding."));
    assert_eq!((first.run_id, second.run_id), (Some(run), Some(run)), "the run is read from the post's own meta");
    assert_eq!(first.run, None, "one card per run: on its last post");
    assert_eq!(first.usage, None);
    let card = second.run.expect("a card on the last post");
    assert_eq!((card.status.as_str(), card.tool_calls, card.failed_calls), ("running", 0, 0));
    assert_eq!(second.usage.map(|u| (u.input_tokens, u.model_calls)), Some((40, 2)));
    assert!(!second.awaiting_user);
    assert_eq!(s.service.store().read(move |c| aurelius_storage::conversations::activity_for_run(c, run)).unwrap().calls.len(), 0);
}

#[test]
fn an_open_question_of_a_woken_run_is_on_its_ticket_and_marks_its_post() {
    let s = setup();
    let (t, _, _, _) = started_ticket(&s);
    let d = ticket_view(&s, t).conversation_id.expect("a discussion");
    let (_, runs) = post(&s, t, "@Bo which format?").unwrap();
    let run = runs[0];
    set_running(&s, run, 1);
    assert!(post_in_discussion(&s, run, "Looking into it.").0);
    assert!(!messages(&s, d).iter().any(|m| m.awaiting_user));

    let input = serde_json::json!({ "questions": [{ "question": "Which format?", "header": "Format", "multiSelect": false, "options": [] }] });
    let RuntimePermission::Pending { approval_id, .. } = s.service.request_runtime_permission(run, Some("q1".into()), "AskUserQuestion", &input).unwrap() else { panic!("a question waits") };

    assert!(messages(&s, d).iter().find(|m| m.text == "Looking into it.").unwrap().awaiting_user, "the post stands for the waiting run");
    let st = s.service.store().clone();
    let listed = s.service.store().read(move |c| aurelius_storage::approvals::list_approvals(c, &|k| st.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok()))).unwrap();
    let a = listed.iter().find(|a| a.id == approval_id).expect("listed");
    assert_eq!((a.ticket_id, a.discussion_id), (Some(t), Some(d)));
}

#[test]
fn a_reported_line_finds_its_run_and_a_post_that_ended_on_a_question_waits() {
    let s = setup();
    let (t, root, _, run) = started_ticket(&s);
    let d = ticket_view(&s, t).conversation_id.expect("a discussion");
    set_running(&s, run, 1);
    assert!(post_in_discussion(&s, run, "Shall I go on?").0);
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE tasks SET status = 'waiting', waiting_reason = 'input' WHERE id = ?1", [root.to_string()])?)).unwrap();
    assert!(messages(&s, d).iter().find(|m| m.text == "Shall I go on?").unwrap().awaiting_user);

    s.service
        .store()
        .write(move |tx| {
            let mut line = SystemLine::new(SystemLineKind::Reported);
            line.task_id = Some(root);
            discussion::ticket_line(tx, t, line, "Ann reported back")?;
            Ok(())
        })
        .unwrap();
    let all = messages(&s, d);
    let line = all.iter().rfind(|m| m.system.as_ref().is_some_and(|l| l.kind == SystemLineKind::Reported)).unwrap();
    assert_eq!(line.run_id, Some(run));
    assert_eq!(line.run.as_ref().map(|c| c.status.as_str()), Some("running"), "the card goes to the line, the run's last word");
    assert_eq!(all.iter().find(|m| m.text == "Shall I go on?").unwrap().run, None);
}

#[test]
fn the_odometer_counts_a_tickets_tasks_and_its_discussion_and_each_login() {
    let s = setup();
    let (t, _, _, main) = started_ticket(&s);
    let (_, woken) = post(&s, t, "@Bo please look").unwrap();
    record_usage(&s, main, 100, Some(s.user));
    record_usage(&s, woken[0], 30, None);

    let totals = s.service.store().read(move |c| aurelius_storage::usage::ticket_totals(c, t)).unwrap();
    assert_eq!((totals.total.runs, totals.total.input_tokens, totals.total.model_calls, totals.total.cost_micro_usd), (2, 130, 4, Some(20)));
    let empty = new_ticket(&s, "Idle", None);
    assert_eq!(s.service.store().read(move |c| aurelius_storage::usage::ticket_totals(c, empty)).unwrap().total.runs, 0);

    let snapshot = s.service.store().read(|c| aurelius_storage::usage::snapshot(c, |_| 0)).unwrap();
    let grace = snapshot.by_user.iter().find(|r| r.username.as_deref() == Some("grace")).expect("grace's login");
    assert_eq!((grace.runs, grace.input_tokens), (1, 100));
    assert!(snapshot.by_user.iter().any(|r| r.login_user.is_none() && r.input_tokens == 30), "no one's login is a row too");

    // A channel counts the runs it woke, whose conversation is the employee's own.
    let general = general(&s);
    let (_, runs) = send(&s, None, "@Bo a quick one").unwrap();
    record_usage(&s, runs[0], 7, None);
    assert_eq!(s.service.store().read(move |c| aurelius_storage::usage::conversation_totals(c, general)).unwrap().total.input_tokens, 7);
}

#[path = "handoff_tests.rs"]
mod handoff_tests;

#[path = "scheduled_tests.rs"]
mod scheduled_tests;
