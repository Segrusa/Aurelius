//! The engine under the board (Fase 1, S4): a started ticket gets a main task,
//! the card follows that task's status, the report stops in Review for a board
//! member, who accepts it (Done) or sends it back with feedback (B2). A failed
//! main task keeps the card in Doing and can start again (B6). Runs use the
//! fake runtime.

mod common;

use aurelius_client::{Client, bridge::BridgeClient};
use aurelius_domain::employee::{CreateEmployee, LifecycleStatus, SetEmployeeStatus, SendEmployeeMessage};
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::TaskStatus;
use aurelius_domain::ticket::*;
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, ErrorCode, EventKind, ProjectId, RunId, TicketId};
use aurelius_protocol::EmployeeTaskOp;
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use std::sync::Arc;
use std::time::Duration;

/// The fake runtime, failing the work of a ticket whose description says FAIL-ME.
struct FailMarked;

#[async_trait::async_trait]
impl AgentRuntime for FailMarked {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let marked = request.turns.iter().any(|t| t.text.contains("FAIL-ME"));
        let behavior = if marked { fake::FakeBehavior::FailAfterDeltas } else { fake::FakeBehavior::Normal };
        fake::FakeRuntime { delay: Duration::from_millis(1), behavior }.start(request).await
    }
}

/// The fake runtime, holding a run live while its request says HOLD-MAIN (or
/// HOLD-PART) until that gate opens; then it fails when it says FAIL-ME.
struct Held {
    main: tokio::sync::watch::Sender<bool>,
    part: tokio::sync::watch::Sender<bool>,
}

impl Held {
    fn new() -> Arc<Self> {
        Arc::new(Self { main: tokio::sync::watch::channel(false).0, part: tokio::sync::watch::channel(false).0 })
    }
}

#[async_trait::async_trait]
impl AgentRuntime for Held {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let said = |word: &str| request.turns.iter().any(|t| t.text.contains(word));
        let fails = said("FAIL-ME");
        let gate = if said("HOLD-PART") { Some(self.part.subscribe()) } else if said("HOLD-MAIN") { Some(self.main.subscribe()) } else { None };
        let Some(mut gate) = gate.filter(|g| !*g.borrow()) else {
            let behavior = if fails { fake::FakeBehavior::FailAfterDeltas } else { fake::FakeBehavior::Normal };
            return fake::FakeRuntime { delay: Duration::from_millis(1), behavior }.start(request).await;
        };
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let (input_tx, _input_rx) = tokio::sync::mpsc::channel(8);
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            while !*gate.borrow() {
                tokio::select! {
                    changed = gate.changed() => if changed.is_err() { break },
                    _ = cancel_rx.changed() => {
                        let _ = tx.send(RuntimeEvent::Interrupted).await;
                        return;
                    }
                }
            }
            let _ = tx.send(RuntimeEvent::TextDelta("Report: done.".into())).await;
            let end = if fails { RuntimeEvent::Failed { code: "fake_failure".into(), message: "failed on purpose".into() } } else { RuntimeEvent::Completed };
            let _ = tx.send(end).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

/// The run of `conversation` that is live now (waits for one).
async fn live_run(root: &std::path::Path, conversation: ConversationId) -> RunId {
    for _ in 0..400 {
        let found: Option<String> = common::db(root).query_row("SELECT id FROM runs WHERE conversation_id = ?1 AND status = 'running'", [conversation.to_string()], |r| r.get(0)).ok();
        if let Some(run) = found {
            return RunId::parse(&run).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no live run in {conversation}");
}

fn message<T: std::fmt::Debug>(result: Result<T, aurelius_client::ClientError>) -> String {
    match result {
        Err(aurelius_client::ClientError::Protocol(err)) => err.message,
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

async fn project(client: &Client, name: &str) -> ProjectId {
    let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!("{:?}", created.result) };
    project_id
}

async fn employee(client: &Client, name: &str) -> EmployeeId {
    let create = CreateEmployee { description: None, name: name.into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let created = client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap();
    let CommandResult::EmployeeCreated { employee_id, .. } = created.result else { panic!("{:?}", created.result) };
    employee_id
}

/// In Auto: a lone employee is management and would choose its own mode (Plan, then Plan review).
async fn ticket(client: &Client, project_id: ProjectId, title: &str, description: &str, owner: Option<EmployeeId>, start: bool) -> TicketId {
    let made = client
        .create_ticket(CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id,
            title: title.into(),
            description: Some(description.into()),
            priority: TicketPriority::default(),
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: owner,
            run: aurelius_domain::task::RunSettings { mode: Some(aurelius_domain::autonomy::AgentMode::Auto), ..Default::default() },
            files: vec![],
            start,
            from_message: None,
            from_idea: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!("{:?}", made.result) };
    ticket_id
}

/// Wait until the card is in `column` with its main task in `status`.
async fn until(client: &Client, id: TicketId, column: ColumnKind, status: TaskStatus) -> TicketDetail {
    for _ in 0..400 {
        let detail = client.ticket(id).await.unwrap();
        if detail.ticket.column == column && detail.root_task.as_ref().map(|t| t.status) == Some(status) {
            return detail;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let detail = client.ticket(id).await.unwrap();
    panic!("never {column:?}/{status:?}: {:?}/{:?}", detail.ticket.column, detail.root_task.map(|t| t.status));
}

fn ticket_events(root: &std::path::Path, id: TicketId) -> Vec<(String, Option<String>)> {
    let db = common::db(root);
    let mut stmt = db
        .prepare("SELECT type, json_extract(metadata_json, '$.to') FROM events WHERE aggregate_type = 'ticket' AND aggregate_id = ?1 ORDER BY aggregate_version")
        .unwrap();
    stmt.query_map([id.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap()
}

fn code<T: std::fmt::Debug>(result: Result<T, aurelius_client::ClientError>) -> ErrorCode {
    match result {
        Err(aurelius_client::ClientError::Protocol(err)) => err.code,
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

/// The system lines of a ticket's discussion, in words (MU2-03).
async fn discussion_lines(client: &Client, id: TicketId) -> Vec<(aurelius_domain::views::SystemLineKind, Option<aurelius_domain::TaskId>, String)> {
    let conversation = client.ticket(id).await.unwrap().ticket.conversation_id.expect("a discussion");
    client.messages(conversation).await.unwrap().messages.into_iter().filter_map(|m| m.system.map(|s| (s.kind, s.task_id, m.text))).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_is_worked_reported_sent_back_and_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let client = common::connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder").await;
    let atlas = project(&client, "Atlas").await;

    // Created with an owner and started: the main task goes to that employee (MU1-12 AC1).
    let id = ticket(&client, atlas, "Login page", "A login page with a password field.", Some(builder), true).await;
    let detail = until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    let root = detail.root_task.clone().unwrap();
    assert_eq!((root.employee_id, root.ticket_id, root.parent_task_id), (Some(builder), Some(id), None));
    assert_eq!(detail.tasks.len(), 1);
    // The report stopped there: no success, no result to anyone (§3.3).
    assert!(root.result.is_some(), "the report is kept");
    let kinds: Vec<String> = ticket_events(dir.path(), id).into_iter().map(|(k, _)| k).collect();
    assert!(kinds.iter().any(|k| k == "ticket.review"), "{kinds:?}");
    let moves: Vec<Option<String>> = ticket_events(dir.path(), id).into_iter().filter(|(k, _)| k == "ticket.moved").map(|(_, to)| to).collect();
    assert_eq!(moves, [Some("todo".into()), Some("doing".into()), Some("review".into())], "Backlog → Todo → Doing → Review");

    // Send back: the feedback is required and goes to the main task's conversation (B2).
    assert_eq!(code(client.ticket_command(Command::SendBackTicket(SendBackTicket { confirm_secret: false, ticket_id: id, feedback: " ".into() }), None).await), ErrorCode::Validation);
    client.ticket_command(Command::SendBackTicket(SendBackTicket { confirm_secret: false, ticket_id: id, feedback: "Add a forgot-password link".into() }), None).await.unwrap();
    let again = until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    assert_eq!(again.ticket.review_feedback.as_deref(), Some("Add a forgot-password link"));
    let conversation = again.root_task.as_ref().unwrap().conversation_id.expect("the main task's conversation");
    let messages = client.messages(conversation).await.unwrap().messages;
    let feedback = messages.iter().find(|m| m.text.contains("Add a forgot-password link")).expect("the feedback is in the conversation");
    assert_eq!((feedback.update.as_deref(), feedback.author.as_deref()), (Some("message"), Some("grace")), "said by the board member");
    // The same session answered it: the last message is the employee's answer to the feedback.
    assert!(messages.last().unwrap().text.contains("forgot-password"), "{:?}", messages.last());

    // Accept: Done, and the main task succeeds, accepted by the board member.
    client.ticket_command(Command::AcceptTicket(AcceptTicket { confirm_secret: false, ticket_id: id, note: Some("Good".into()) }), None).await.unwrap();
    let done = until(&client, id, ColumnKind::Done, TaskStatus::Succeeded).await;
    assert!(done.ticket.done_at_ms.is_some());
    let reason: String = common::db(dir.path())
        .query_row(
            "SELECT json_extract(metadata_json, '$.reason') FROM events WHERE aggregate_type = 'task' AND aggregate_id = ?1 AND type = 'task.succeeded'",
            [root.id.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reason, "accepted");
    // MU2-03 / MU2-06 AC2: sent back to Doing and put on Done, by the board member, once each.
    let texts: Vec<String> = discussion_lines(&client, id).await.into_iter().map(|(_, _, t)| t).collect();
    assert_eq!(texts.iter().filter(|t| *t == "grace sent the ticket back for more work — the ticket moved from Review to Doing").count(), 1, "{texts:#?}");
    assert_eq!(texts.iter().filter(|t| *t == "grace accepted the report and moved the ticket to Done").count(), 1, "{texts:#?}");
    assert_eq!(texts.iter().filter(|t| t.ends_with("handed in the report. It is ready for your review")).count(), 2, "{texts:#?}");

    drop(client);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_main_task_stays_in_doing_and_starts_again() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start_with(dir.path(), Arc::new(FailMarked)).await;
    let client = common::connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder").await;
    let atlas = project(&client, "Atlas").await;

    // B6: failed, the card stays in Doing ("Needs you", "Start again"); never Review or Done.
    let id = ticket(&client, atlas, "Flaky", "FAIL-ME please", Some(builder), true).await;
    let failed = until(&client, id, ColumnKind::Doing, TaskStatus::Failed).await;
    let first = failed.root_task.unwrap().id;
    assert_eq!(code(client.ticket_command(Command::AcceptTicket(AcceptTicket { confirm_secret: false, ticket_id: id, note: None }), None).await), ErrorCode::Validation);
    // Start again: a new main task, the old one stays linked to the ticket.
    client.ticket_command(Command::StartTicket(StartTicket { ticket_id: id, employee_id: None }), None).await.unwrap();
    let detail = until(&client, id, ColumnKind::Doing, TaskStatus::Failed).await;
    let second = detail.root_task.as_ref().unwrap().id;
    assert_ne!(first, second, "a new main task");
    assert_eq!(detail.tasks.iter().map(|t| t.id).collect::<Vec<_>>(), [first, second]);

    // Cancel: back to Backlog, the main task ends.
    let other = ticket(&client, atlas, "Later", "nothing special", None, true).await;
    let waiting = until(&client, other, ColumnKind::Todo, TaskStatus::Draft).await;
    client.ticket_command(Command::CancelTicket(CancelTicket { ticket_id: other }), None).await.unwrap();
    let cancelled = client.ticket(other).await.unwrap();
    assert_eq!((cancelled.ticket.column, cancelled.root_task.map(|t| t.status)), (ColumnKind::Backlog, Some(TaskStatus::Cancelled)));
    assert_eq!(waiting.tasks.len(), 1);
    // MU2-03: a failed main task says so where the card stays, and a cancel says who.
    let texts: Vec<String> = discussion_lines(&client, id).await.into_iter().map(|(_, _, t)| t).collect();
    assert_eq!(texts.iter().filter(|t| *t == "Builder could not finish the ticket's main task. Press Start to begin a new attempt.").count(), 2, "{texts:#?}");
    assert!(discussion_lines(&client, other).await.iter().any(|(_, _, t)| t == "grace cancelled the ticket — the ticket moved from Todo to Backlog"));

    drop(client);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_ticket_a_task_still_succeeds_on_its_report() {
    // B7: a task given outside the board keeps the old path: report, success, no review gate.
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let client = common::connect(dir.path()).await;
    let builder = employee(&client, "Builder").await;
    let atlas = project(&client, "Atlas").await;
    let create = aurelius_domain::task::CreateTask {
        confirm_secret: false,
        project_id: atlas,
        employee_id: Some(builder),
        title: "Plain".into(),
        goal: "No ticket".into(),
        acceptance_criteria: vec![],
        depends_on: vec![],
        run: Default::default(),
        files: vec![],
    };
    let CommandResult::TaskCreated { task_id } = client.execute(aurelius_client::envelope(Command::CreateTask(create), None)).await.unwrap().result else { panic!() };
    for _ in 0..400 {
        if client.tasks(Some(atlas)).await.unwrap().tasks.iter().any(|t| t.id == task_id && t.status == TaskStatus::Succeeded) {
            drop(client);
            common::stop(dir.path(), daemon).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the task did not succeed");
}

#[tokio::test(flavor = "multi_thread")]
async fn accept_waits_for_a_run_in_the_main_tasks_conversation() {
    // B6: Done only when nothing runs any more, also a question asked in Review (§3.3).
    let dir = tempfile::tempdir().unwrap();
    let held = Held::new();
    let daemon = common::start_with(dir.path(), held.clone()).await;
    let client = common::connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder").await;
    let atlas = project(&client, "Atlas").await;
    let id = ticket(&client, atlas, "Report page", "A page with the monthly report.", Some(builder), true).await;
    let detail = until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    let conversation = detail.root_task.unwrap().conversation_id.unwrap();

    let ask = SendEmployeeMessage { confirm_secret: false, employee_id: builder, text: "HOLD-MAIN one more question".into(), conversation_id: conversation, images: vec![], files: vec![], interrupt: false };
    client.execute(aurelius_client::envelope(Command::SendEmployeeMessage(ask), None)).await.unwrap();
    live_run(dir.path(), conversation).await;
    let refused = client.ticket_command(Command::AcceptTicket(AcceptTicket { confirm_secret: false, ticket_id: id, note: None }), None).await;
    assert!(message(refused).contains("still running"));
    let still = client.ticket(id).await.unwrap();
    assert_eq!((still.ticket.column, still.root_task.map(|t| t.status)), (ColumnKind::Review, Some(TaskStatus::Review)));

    // Answered: the main task reported again, and accept goes.
    held.main.send_replace(true);
    for _ in 0..400 {
        let running: bool = common::db(dir.path())
            .query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE conversation_id = ?1 AND status IN ('created','starting','running','waiting','cancelling'))", [conversation.to_string()], |r| r.get(0))
            .unwrap();
        if !running {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    client.ticket_command(Command::AcceptTicket(AcceptTicket { confirm_secret: false, ticket_id: id, note: None }), None).await.unwrap();
    until(&client, id, ColumnKind::Done, TaskStatus::Succeeded).await;

    drop(client);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_engines_card_moves_reach_the_event_stream() {
    // The task job moves the card too (its owner is paused: Queued → Waiting, Todo → Doing); clients see it live.
    // (The subscription fills a gap in the seqs from the log, so this holds as long as a later event follows;
    // that the job publishes the move itself is `ticket_tests::the_task_job_returns_the_cards_move_with_its_own`.)
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let (client, mut events) = common::connect_events_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder").await;
    let atlas = project(&client, "Atlas").await;
    client.execute(aurelius_client::envelope(Command::SetEmployeeStatus(SetEmployeeStatus { employee_id: builder, status: LifecycleStatus::Paused }), None)).await.unwrap();
    let base = client.health().await.unwrap().last_seq;
    client.subscribe(base).await.unwrap();

    let id = ticket(&client, atlas, "Paused owner", "Waits for its owner.", Some(builder), true).await;
    let detail = until(&client, id, ColumnKind::Doing, TaskStatus::Waiting).await;
    let moved_in_db = ticket_events(dir.path(), id).into_iter().filter(|(k, _)| k == "ticket.moved").count();
    assert_eq!(moved_in_db, 2, "Backlog → Todo by the board member, Todo → Doing by the engine");
    let mut moved_live = 0;
    while moved_live < moved_in_db {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv()).await.expect("every move is published").expect("events stay open");
        if event.kind == EventKind::TicketMoved && event.aggregate_id == id.to_string() {
            moved_live += 1;
        }
    }
    assert_eq!(detail.root_task.unwrap().employee_id, Some(builder));

    drop(client);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_ends_the_delegations_of_failed_and_earlier_main_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let held = Held::new();
    let daemon = common::start_with(dir.path(), held.clone()).await;
    let client = common::connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder").await;
    let helper = CreateEmployee { description: None, name: "Helper".into(), role: "Engineer".into(), title: None, reports_to: Some(builder), instructions: None, folder: None };
    client.execute(aurelius_client::envelope(Command::CreateEmployee(helper), None)).await.unwrap();
    let atlas = project(&client, "Atlas").await;

    // The main task gives a part to a colleague during its run, then fails; the part goes on.
    let id = ticket(&client, atlas, "Flaky", "HOLD-MAIN FAIL-ME the work", Some(builder), true).await;
    let doing = until(&client, id, ColumnKind::Doing, TaskStatus::Running).await;
    let first = doing.root_task.unwrap();
    let run = live_run(dir.path(), first.conversation_id.unwrap()).await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let (ok, text) = bridge
        .employee_act(run, EmployeeTaskOp::Delegate { employee: "Helper".into(), title: "Part".into(), goal: "HOLD-PART a part".into(), criteria: vec![], project: None, ticket: None })
        .await
        .unwrap();
    assert!(ok, "{text}");
    held.main.send_replace(true);
    until(&client, id, ColumnKind::Doing, TaskStatus::Failed).await;
    // Start again: a new main task that fails too; the part of the first one still runs.
    client.ticket_command(Command::StartTicket(StartTicket { ticket_id: id, employee_id: None }), None).await.unwrap();
    let again = until(&client, id, ColumnKind::Doing, TaskStatus::Failed).await;
    assert_ne!(again.root_task.as_ref().unwrap().id, first.id);
    let part = again.tasks.iter().find(|t| t.parent_task_id == Some(first.id)).expect("the part").clone();
    assert_eq!(part.status, TaskStatus::Running);

    // Cancel: the card goes to Backlog and the part of the earlier, failed main task ends too.
    client.ticket_command(Command::CancelTicket(CancelTicket { ticket_id: id }), None).await.unwrap();
    let cancelled = client.ticket(id).await.unwrap();
    assert_eq!(cancelled.ticket.column, ColumnKind::Backlog);
    let status = |t: aurelius_domain::TaskId| cancelled.tasks.iter().find(|x| x.id == t).map(|x| x.status);
    assert_eq!(status(part.id), Some(TaskStatus::Cancelled));
    assert_eq!((status(first.id), status(again.root_task.unwrap().id)), (Some(TaskStatus::Failed), Some(TaskStatus::Failed)), "ended tasks stay as they are");
    // Its run is stopped as well.
    let part_conversation = part.conversation_id.unwrap();
    for _ in 0..400 {
        let live: bool = common::db(dir.path()).query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE conversation_id = ?1 AND status = 'running')", [part_conversation.to_string()], |r| r.get(0)).unwrap();
        if !live {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let live: bool = common::db(dir.path()).query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE conversation_id = ?1 AND status = 'running')", [part_conversation.to_string()], |r| r.get(0)).unwrap();
    assert!(!live, "the part's run was stopped");

    drop(client);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tickets_main_task_is_assigned_and_cancelled_on_the_board() {
    // Trace M3 m4: task.assign and task.cancel refuse the main task; the ticket's commands do it.
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let client = common::connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder").await;
    let atlas = project(&client, "Atlas").await;
    let id = ticket(&client, atlas, "On the board", "nothing special", None, true).await;
    let waiting = until(&client, id, ColumnKind::Todo, TaskStatus::Draft).await;
    let root = waiting.root_task.unwrap().id;

    let assign = client.execute(aurelius_client::envelope(Command::AssignTask(aurelius_domain::task::AssignTask { task_id: root, employee_id: Some(builder) }), None)).await;
    assert!(message(assign).contains("use the board"));
    let cancel = client.execute(aurelius_client::envelope(Command::CancelTask(aurelius_domain::task::CancelTask { task_id: root }), None)).await;
    assert!(message(cancel).contains("use the board"));
    let same = client.ticket(id).await.unwrap();
    assert_eq!((same.ticket.column, same.ticket.owner_employee_id, same.root_task.map(|t| t.status)), (ColumnKind::Todo, None, Some(TaskStatus::Draft)));

    client.ticket_command(Command::CancelTicket(CancelTicket { ticket_id: id }), None).await.unwrap();
    let cancelled = client.ticket(id).await.unwrap();
    assert_eq!((cancelled.ticket.column, cancelled.root_task.map(|t| t.status)), (ColumnKind::Backlog, Some(TaskStatus::Cancelled)));

    drop(client);
    common::stop(dir.path(), daemon).await;
}

/// The project's memory that names a ticket ("Ticket #n …").
async fn ticket_summaries(client: &Client, project_id: ProjectId) -> Vec<aurelius_protocol::MemoryView> {
    client.memory_of(Some(project_id), None).await.unwrap().items.into_iter().filter(|m| m.content.starts_with("Ticket #")).collect()
}

/// Whether every memory extraction job has ended.
fn extractions_ended(root: &std::path::Path) -> bool {
    common::db(root).query_row("SELECT NOT EXISTS(SELECT 1 FROM jobs WHERE kind = 'memory.extract' AND state IN ('ready','leased'))", [], |r| r.get(0)).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_in_done_leaves_one_summary_in_its_projects_memory() {
    // MU1-18: Done writes exactly one summary to the ticket's project memory, naming the ticket;
    // a ticket that never reached Done, and a task without a ticket (B7), write none.
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let client = common::connect_as(dir.path(), "grace").await;
    let builder = employee(&client, "Builder").await;
    let atlas = project(&client, "Atlas").await;
    let other = project(&client, "Other").await;

    let id = ticket(&client, atlas, "Login page", "A login page with a password field.", Some(builder), true).await;
    let waits = ticket(&client, atlas, "Signup page", "A signup page.", Some(builder), true).await;
    until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    until(&client, waits, ColumnKind::Review, TaskStatus::Review).await;
    let create = aurelius_domain::task::CreateTask {
        confirm_secret: false,
        project_id: atlas,
        employee_id: Some(builder),
        title: "Plain".into(),
        goal: "No ticket".into(),
        acceptance_criteria: vec![],
        depends_on: vec![],
        run: Default::default(),
        files: vec![],
    };
    client.execute(aurelius_client::envelope(Command::CreateTask(create), None)).await.unwrap();
    for _ in 0..400 {
        if client.tasks(Some(atlas)).await.unwrap().tasks.iter().any(|t| t.title == "Plain" && t.status == TaskStatus::Succeeded) && extractions_ended(dir.path()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(ticket_summaries(&client, atlas).await.is_empty(), "no summary before Done, none for a task without a ticket");

    client.ticket_command(Command::AcceptTicket(AcceptTicket { confirm_secret: false, ticket_id: id, note: None }), None).await.unwrap();
    let done = until(&client, id, ColumnKind::Done, TaskStatus::Succeeded).await;
    let mut summaries = Vec::new();
    for _ in 0..400 {
        summaries = ticket_summaries(&client, atlas).await;
        if !summaries.is_empty() && extractions_ended(dir.path()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(summaries.len(), 1, "exactly one: {summaries:#?}");
    let summary = &summaries[0];
    let head = format!("Ticket #{} “Login page” is done: ", done.ticket.number);
    assert!(summary.content.starts_with(&head), "{}", summary.content);
    assert!(summary.content.contains("delivered"), "the extraction's summary: {}", summary.content);
    assert_eq!((summary.scope.as_str(), summary.employee_id, summary.status), ("project", None, aurelius_domain::memory::MemoryStatus::Active));
    assert!(!summary.sources.is_empty(), "it points at the main task's success");
    // The per-task extraction is still there next to it.
    let all = client.memory_of(Some(atlas), Some(builder)).await.unwrap().items;
    assert!(all.iter().any(|m| m.scope == "employee" && m.content.contains("Login page")), "{all:#?}");
    // The same extraction once more (a retried job): still one.
    let db = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    let again = db
        .execute(
            "UPDATE jobs SET state = 'ready', not_before_ms = 0, lease_owner = NULL, lease_until_ms = NULL WHERE kind = 'memory.extract' AND json_extract(payload_json, '$.ticket_id') = ?1",
            [id.to_string()],
        )
        .unwrap();
    assert_eq!(again, 1, "one extraction carries the ticket");
    for _ in 0..400 {
        if extractions_ended(dir.path()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(extractions_ended(dir.path()));
    assert_eq!(ticket_summaries(&client, atlas).await.len(), 1, "still exactly one");
    // The ticket still in Review, and the other project: nothing.
    assert!(summaries.iter().all(|m| !m.content.contains("Signup page")));
    assert!(ticket_summaries(&client, other).await.is_empty());
    assert_eq!(client.ticket(id).await.unwrap().ticket.column, ColumnKind::Done);

    drop(client);
    common::stop(dir.path(), daemon).await;
}
