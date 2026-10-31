//! The discussion engine: the general channel, a project's channel and a
//! ticket's discussion, who answers (B3) and the loop limits over all of them.

use super::*;
use crate::conversation::discussion;
use crate::task::task_runs::EmployeeAction;
use aurelius_domain::company::{UpdateProject};
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::group_chat::{MAX_HOPS, SendGroupMessage, WAKING_POSTS_PER_RUN, WAKING_POSTS_PER_WINDOW};
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{self, ColumnKind, SetDispatcher};
use aurelius_domain::views::{SystemLine, SystemLineKind};
use aurelius_domain::{ConversationId, EmployeeId, RunId, TicketId, UserId};

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
    let create = CreateEmployee { name: name.into(), role: "Dev".into(), title: None, reports_to: boss, instructions: None, folder: None };
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
    match exec(s, Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel }))? {
        CommandResult::GroupMessageSent { to, run_ids, .. } => Ok((to, run_ids)),
        other => panic!("{other:?}"),
    }
}

fn post(s: &Setup, ticket: TicketId, text: &str) -> Result<(Vec<EmployeeId>, Vec<RunId>), AppError> {
    match exec(s, Command::PostTicketMessage(ticket::PostTicketMessage { confirm_secret: false, ticket_id: ticket, text: text.into(), images: vec![], files: vec![] }))? {
        CommandResult::TicketMessagePosted { to, run_ids, .. } => Ok((to, run_ids)),
        other => panic!("{other:?}"),
    }
}

fn new_ticket(s: &Setup, title: &str, owner: Option<EmployeeId>) -> TicketId {
    let create = ticket::CreateTicket {
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

/// The runs queued for `employee` in its main chat, oldest first: (run, discussion that woke it, hop).
fn woken(s: &Setup, employee: EmployeeId) -> Vec<(RunId, ConversationId, u32)> {
    s.service
        .store()
        .read(move |c| {
            let chat = aurelius_storage::employees::main_chat(c, employee)?.unwrap();
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
    // A ticket's discussion is no channel.
    let t = new_ticket(&s, "Login", None);
    let discussion = ticket_view(&s, t).conversation_id.unwrap();
    assert_eq!(send(&s, Some(discussion), "x").unwrap_err().code, ErrorCode::NotFound);
}

fn aurelius_storage_main_chat(s: &Setup, employee: EmployeeId) -> ConversationId {
    s.service.store().read(move |c| aurelius_storage::employees::main_chat(c, employee)).unwrap().unwrap()
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
    assert!(system_lines(&s, d).iter().any(|(k, text)| *k == SystemLineKind::Limit && text.contains("only answers here while the ticket is in Todo, Doing or Plan review")), "{:?}", system_lines(&s, d));
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
    assert_eq!(runs_in(&s).len(), 1, "no extra run while one is queued");
    let note = messages(&s, root_conversation).into_iter().last().unwrap();
    assert_eq!((note.update.as_deref(), note.author.as_deref()), (Some("message"), Some("grace")));
    assert!(note.text.starts_with("Please also check the logout.") && note.text.contains("post_to_discussion"), "{}", note.text);
    // @Ann (the owner) also goes to the main task; @Bo besides wakes Bo.
    let (to, runs) = post(&s, t, "@Ann @Bo one more thing").unwrap();
    assert_eq!(to, vec![s.ann, s.bo]);
    assert_eq!(runs.len(), 1, "only Bo was woken");
    assert_eq!(last_woken(&s, s.bo).0, runs[0]);
    assert!(woken(&s, s.ann).is_empty());
    // In Review the owner no longer answers by itself.
    engine_puts(&s, t, ColumnKind::Review);
    let (to, _) = post(&s, t, "Thanks").unwrap();
    assert!(to.is_empty());
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
        let (ok, text) = post_in_channel(&s, run, None, &format!("@{name} your turn"));
        assert!(ok && text.contains(&format!("{name} will answer")), "hop {hop}: {text}");
        let (next, conversation, at) = last_woken(&s, who);
        assert_eq!((conversation, at), (general, hop));
        run = next;
    }
    let before = (woken(&s, s.ann).len(), woken(&s, s.bo).len());
    // Bo (at hop MAX_HOPS) names Ann and Cy: neither is woken.
    let (ok, text) = post_in_channel(&s, run, None, "@Ann @Cy again");
    assert!(ok && text.contains("Limit reached") && text.contains("will not be woken"), "{text}");
    assert_eq!((woken(&s, s.ann).len(), woken(&s, s.bo).len()), before, "nobody past MAX_HOPS");
    assert!(woken(&s, s.cy).is_empty());
    let lines = system_lines(&s, general);
    assert!(lines.contains(&(SystemLineKind::Limit, "Limit reached: Ann, Cy was not woken.".into())), "{lines:?}");
    // The system line itself names them, and is not part of what employees read.
    let read = act(&s, run, EmployeeAction::ReadGroupChat { last: None, channel: None }).1;
    assert!(!read.contains("Limit reached"), "{read}");
    // A system line written anywhere wakes nobody, even with @Name in it.
    let mut line = SystemLine::new(SystemLineKind::Moved);
    line.by = Some("@Cy".into());
    s.service.store().write(move |tx| discussion::post_system(tx, general, &line)).unwrap();
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
    s.service.store().write(move |tx| discussion::post_system(tx, general, &line)).unwrap();
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
    // Per run: WAKING_POSTS_PER_RUN waking posts, also when they go to different discussions.
    let (_, runs) = send(&s, None, "@Ann hi").unwrap();
    let run = runs[0];
    assert!(post_in_channel(&s, run, None, "@Bo one").1.contains("Bo will answer"));
    assert!(post_in_channel(&s, run, Some("Orbit"), "@Bo two").1.contains("Bo will answer"));
    assert_eq!(WAKING_POSTS_PER_RUN, 2);
    let (ok, text) = post_in_channel(&s, run, Some("General"), "@Bo three");
    assert!(ok && text.contains("Limit reached") && text.contains("in this run"), "{text}");
    // Per hour: Ann's runs from three discussions share one budget of WAKING_POSTS_PER_WINDOW.
    let (_, r_orbit) = send(&s, Some(orbit), "@Ann hi").unwrap();
    let (_, r_ticket) = post(&s, t, "@Ann hi").unwrap();
    let (_, r_general) = send(&s, None, "@Ann again").unwrap();
    assert_eq!(WAKING_POSTS_PER_WINDOW, 4);
    // Two waking posts so far (in `run`); two more from two other discussions...
    assert!(post_in_channel(&s, r_orbit[0], None, "@Cy look").1.contains("Cy will answer"));
    assert!(post_in_discussion(&s, r_ticket[0], "@Cy look").1.contains("Cy will answer"));
    // ...and the fifth, in a third discussion, is refused.
    let (ok, text) = post_in_channel(&s, r_general[0], None, "@Cy and here");
    assert!(ok && text.contains("Limit reached") && text.contains("last hour") && text.contains("all channels and tickets together"), "{text}");
    for conversation in [general, orbit, d] {
        assert!(!messages(&s, conversation).is_empty());
    }
    assert!(system_lines(&s, general).iter().any(|(k, text)| *k == SystemLineKind::Limit && text == "Limit reached: Cy was not woken."));
    // A post without @ still goes through.
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
    assert_eq!(err.code, ErrorCode::Validation);
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
    assert_eq!(send(&s, Some(orbit), "x").unwrap_err().code, ErrorCode::Validation);
    let (ok, text) = post_in_channel(&s, runs[0], Some("Orbit"), "x");
    assert!(!ok && text.contains("archived"), "{text}");
}

#[test]
fn ticket_post_needs_a_board_member_and_an_existing_ticket() {
    let s = setup();
    let t = new_ticket(&s, "Login", None);
    let command = Command::PostTicketMessage(ticket::PostTicketMessage { confirm_secret: false, ticket_id: t, text: "x".into(), images: vec![], files: vec![] });
    let Err(err) = s.service.execute(&ctx(None), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }) else { panic!("posted without a board member") };
    assert_eq!(err.code, ErrorCode::BoardMemberRequired);
    assert_eq!(post(&s, TicketId::new(), "x").unwrap_err().code, ErrorCode::NotFound);
}

/// The message `send` stored in a channel: its id.
fn sent_message(s: &Setup, channel: Option<ConversationId>, text: &str) -> aurelius_domain::MessageId {
    match exec(s, Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel })).unwrap() {
        CommandResult::GroupMessageSent { message_id, .. } => message_id,
        other => panic!("{other:?}"),
    }
}

fn ticket_from(s: &Setup, message: Option<aurelius_domain::MessageId>, user: Option<UserId>) -> Result<(TicketId, u32), AppError> {
    let create = ticket::CreateTicket {
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
    let chat = aurelius_storage_main_chat(&s, s.ann);
    let (ann_ticket, _) = {
        let (_, runs) = send(&s, None, "@Ann what about XML?").unwrap();
        assert!(!runs.is_empty());
        let update = messages(&s, chat).into_iter().rev().find(|m| m.update.is_some()).expect("an update in Ann's chat").id;
        ticket_from(&s, Some(update), Some(s.user)).unwrap()
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
    assert_eq!(post(&s, t, "late").unwrap_err().code, ErrorCode::Validation);
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
    exec(&s, Command::UpdateEmployee(aurelius_domain::employee::UpdateEmployee { employee_id: ann, name: "Ann".into(), role: "Dev".into(), title: None, instructions: None, sequential_tasks: Some(true) })).unwrap();
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
    // Trace m4: a named colleague without a main chat wakes nobody, so the post uses no budget.
    let s = setup();
    let (cy, project) = (s.cy, s.project);
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE conversations SET kind = 'task', project_id = ?2 WHERE employee_id = ?1 AND kind = 'employee'", [cy.to_string(), project.to_string()])?)).unwrap();
    let (_, runs) = send(&s, None, "@Ann hi").unwrap();
    let run = runs[0];
    for _ in 0..WAKING_POSTS_PER_RUN + 1 {
        let (ok, text) = post_in_channel(&s, run, None, "@Cy look");
        assert!(ok && !text.contains("Limit reached"), "{text}");
    }
    assert!(post_in_channel(&s, run, None, "@Bo one").1.contains("Bo will answer"));
    assert!(post_in_channel(&s, run, None, "@Bo two").1.contains("Bo will answer"));
    assert!(post_in_channel(&s, run, None, "@Bo three").1.contains("Limit reached"));
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
