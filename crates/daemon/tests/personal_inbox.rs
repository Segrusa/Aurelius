//! A3 the personal Inbox through the daemon's API (AUR-FA-F2): who gets a row for
//! each event, what it says, how it merges and leaves, what is pushed and to whom,
//! and what an older client on protocol 5 reads (rule R1 of the freeze).
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::Client;
use aurelius_domain::autonomy::AgentMode;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::group_chat::SendGroupMessage;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::{CreateTask, RunSettings, TaskStatus};
use aurelius_domain::ticket::*;
use aurelius_domain::views::NotificationView;
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, EventKind, ProjectId, RunId, TicketId, UserId};
use aurelius_protocol::{EmployeeTaskOp, Method, ResponseBody};
use aurelius_runtime::fake;
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError};
use aurelius_storage::Store;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Words that only a message holds, to prove they never reach a row or a push.
const CANARY: &str = "canary-text-of-the-message";

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

fn describe(ticket_id: TicketId, text: &str) -> UpdateTicket {
    UpdateTicket { design_url: None, ticket_id, title: None, description: Some(text.into()), priority: None, labels: None, deadline_ms: None, clear_deadline: false, run: None, add_files: None, confirm_secret: false }
}

async fn user_id(client: &Client) -> UserId {
    client.user_whoami().await.unwrap().user.expect("signed in").id
}

async fn run(client: &Client, command: Command) -> CommandResult {
    client.execute(aurelius_client::envelope(command, None)).await.unwrap().result
}

async fn setup(client: &Client) -> (EmployeeId, ProjectId, ConversationId) {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Forge".into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = run(client, Command::CreateEmployee(create)).await else { panic!() };
    let CommandResult::ProjectCreated { project_id, .. } = run(client, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })).await else { panic!() };
    let channel = client.channels().await.unwrap().channels.into_iter().find(|c| c.project_id == Some(project_id)).expect("its channel").conversation_id;
    (employee_id, project_id, channel)
}

fn new_ticket(project_id: ProjectId, owner: Option<EmployeeId>, description: Option<&str>, mode: AgentMode, start: bool) -> CreateTicket {
    CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id,
        title: "Login".into(),
        description: description.map(str::to_owned),
        priority: TicketPriority::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: owner,
        run: RunSettings { mode: Some(mode), ..Default::default() },
        files: vec![],
        start,
        from_message: None,
        from_idea: None,
    }
}

async fn create(client: &Client, ticket: CreateTicket) -> TicketId {
    let CommandResult::TicketCreated { ticket_id, .. } = client.create_ticket(ticket).await.unwrap().result else { panic!("not created") };
    ticket_id
}

/// A ticket that is not started.
async fn backlog(client: &Client, project_id: ProjectId) -> TicketId {
    create(client, new_ticket(project_id, None, None, AgentMode::Auto, false)).await
}

async fn until(client: &Client, id: TicketId, column: ColumnKind, status: TaskStatus) -> TicketDetail {
    for _ in 0..1600 {
        let detail = client.ticket(id).await.unwrap();
        if detail.ticket.column == column && detail.root_task.as_ref().map(|t| t.status) == Some(status) {
            return detail;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let detail = client.ticket(id).await.unwrap();
    panic!("never {column:?}/{status:?}: {:?}/{:?}", detail.ticket.column, detail.root_task.map(|t| t.status));
}

async fn live_run(root: &Path, conversation: ConversationId) -> RunId {
    for _ in 0..2000 {
        let found: Option<String> = db(root).query_row("SELECT id FROM runs WHERE conversation_id = ?1 AND status = 'running' LIMIT 1", [conversation.to_string()], |r| r.get(0)).ok();
        if let Some(run) = found {
            return RunId::parse(&run).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    panic!("no live run in {conversation}");
}

async fn inbox(client: &Client) -> Vec<NotificationView> {
    client.notifications().await.unwrap().notifications
}

async fn unread(client: &Client) -> u64 {
    client.notifications().await.unwrap().unread
}

/// The rows of a board member about this ticket.
async fn rows_of(client: &Client, ticket: TicketId) -> Vec<NotificationView> {
    inbox(client).await.into_iter().filter(|n| n.ticket_id == Some(ticket)).collect()
}

async fn set_responsible(client: &Client, ticket_id: TicketId, responsible_user_id: Option<UserId>) {
    run(client, Command::SetTicketResponsible(SetTicketResponsible { ticket_id, responsible_user_id })).await;
}

async fn follow(client: &Client, ticket_id: TicketId) {
    run(client, Command::FollowTicket(FollowTicket { ticket_id, on: true })).await;
}

async fn post(client: &Client, ticket_id: TicketId, text: &str) -> aurelius_domain::MessageId {
    let CommandResult::TicketMessagePosted { message_id, .. } = run(client, Command::PostTicketMessage(PostTicketMessage { confirm_secret: false, ticket_id, text: text.into(), images: vec![], files: vec![], thread_parent: None })).await else { panic!() };
    message_id
}

async fn say_in_channel(client: &Client, channel: Option<ConversationId>, text: &str) {
    run(client, Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel, thread_parent: None })).await;
}

/// Every event's notifications as a push would read them: title, body and data.
fn all_pushes(root: &Path) -> Vec<(aurelius_storage::timeline::FoundNotification, String)> {
    let store = Store::open(root).unwrap();
    let last: i64 = store.read(|c| Ok(c.query_row("SELECT MAX(seq) FROM events", [], |r| r.get(0))?)).unwrap();
    let mut out = Vec::new();
    for seq in 1..=last {
        for (found, _) in store.read(move |c| aurelius_storage::remote::pushes_for_seq(c, seq as u64)).unwrap() {
            let alert = aurelius_remote_gateway::push::Alert::for_notification(&found, None);
            out.push((found, serde_json::json!({ "title": alert.title, "body": alert.body, "data": alert.data }).to_string()));
        }
    }
    out
}

// ---- who gets a row, per event ----

#[tokio::test(flavor = "multi_thread")]
async fn only_the_new_responsible_gets_a_row_and_never_who_did_it() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, bea, cara) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await, connect_as(dir.path(), "cara").await);
    let (alex_id, bea_id) = (user_id(&alex).await, user_id(&bea).await);
    let (_, project_id, _) = setup(&alex).await;
    let ticket = backlog(&alex, project_id).await;
    let number = alex.ticket(ticket).await.unwrap().ticket.number;

    set_responsible(&alex, ticket, Some(bea_id)).await;
    let rows = rows_of(&bea, ticket).await;
    assert_eq!(rows.len(), 1, "{rows:#?}");
    let row = &rows[0];
    assert_eq!((row.reason.as_deref(), row.severity.as_str(), row.title.as_str(), row.needs_you), (Some("responsible"), "info", "alex made you responsible", false));
    assert_eq!((row.ticket_number, row.subject.as_str(), row.read), (Some(number), format!("#{number} Login").as_str(), false));
    assert!(rows_of(&alex, ticket).await.is_empty(), "never to the actor");
    assert!(rows_of(&cara, ticket).await.is_empty(), "only the new responsible");

    // Taking it yourself is your own action; a clear names nobody.
    let own = backlog(&alex, project_id).await;
    set_responsible(&alex, own, Some(alex_id)).await;
    set_responsible(&alex, own, None).await;
    assert!(rows_of(&alex, own).await.is_empty());

    // Created with someone else as Responsible: that is news for them too.
    let mut chosen = new_ticket(project_id, None, None, AgentMode::Auto, false);
    chosen.responsible_user_id = Some(bea_id);
    let chosen = create(&alex, chosen).await;
    assert_eq!(rows_of(&bea, chosen).await[0].reason.as_deref(), Some("responsible"));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mention_reaches_who_it_names_in_a_ticket_and_in_a_channel_and_holds_no_text() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, bea, cara) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await, connect_as(dir.path(), "cara").await);
    let cara_id = user_id(&cara).await;
    let (_, project_id, channel) = setup(&alex).await;
    let ticket = backlog(&alex, project_id).await;
    let discussion = alex.ticket(ticket).await.unwrap().ticket.conversation_id.expect("a discussion");

    alex.disable_user(cara_id).await.unwrap();
    let message = post(&alex, ticket, &format!("@bea @alex @cara {CANARY}")).await;
    let row = rows_of(&bea, ticket).await.pop().expect("a row for who is named");
    assert_eq!((row.reason.as_deref(), row.title.as_str(), row.needs_you), (Some("mention"), "alex mentioned you", false));
    assert_eq!((row.message_id, row.conversation_id), (Some(message), Some(discussion)));
    assert!(rows_of(&alex, ticket).await.is_empty(), "naming yourself is no news");
    assert_eq!(db(dir.path()).query_row("SELECT COUNT(*) FROM notifications WHERE user_id = ?1", [cara_id.to_string()], |r| r.get::<_, i64>(0)).unwrap(), 0, "never to a disabled member");

    // Outside a ticket: one row per channel, never a ticket's fields.
    say_in_channel(&alex, Some(channel), &format!("@bea {CANARY} one")).await;
    say_in_channel(&alex, Some(channel), &format!("@bea {CANARY} two")).await;
    say_in_channel(&alex, None, &format!("@bea {CANARY} three")).await;
    let outside: Vec<_> = inbox(&bea).await.into_iter().filter(|n| n.ticket_id.is_none()).collect();
    assert_eq!(outside.len(), 2, "{outside:#?}");
    let in_project = outside.iter().find(|n| n.conversation_id == Some(channel)).expect("the project's channel");
    assert_eq!((in_project.merged_count, in_project.subject.as_str(), in_project.task_id), (1, "#Atlas", None));
    assert!(outside.iter().any(|n| n.subject == "#general"), "{outside:#?}");

    // Never the text: not in any column of any row, in the view, or in any push.
    let stored: Vec<String> = db(dir.path())
        .prepare("SELECT n.target_json || '|' || n.severity || '|' || COALESCE(n.reason, '') || '|' || e.metadata_json FROM notifications n JOIN events e ON e.id = n.event_id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!stored.is_empty() && stored.iter().all(|s| !s.contains(CANARY)), "{stored:#?}");
    assert!(!serde_json::to_string(&inbox(&bea).await).unwrap().contains(CANARY));
    let pushes = all_pushes(dir.path());
    assert!(pushes.len() >= 3, "{pushes:#?}");
    assert!(pushes.iter().all(|(_, alert)| !alert.contains(CANARY)), "{pushes:#?}");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_employee_that_names_a_board_member_gives_them_a_row() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 40).await;
    let (alex, bea) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await);
    let (owner, project_id, _) = setup(&alex).await;
    let ticket = create(&alex, new_ticket(project_id, Some(owner), Some("Work on it"), AgentMode::Plan, true)).await;
    let doing = until(&alex, ticket, ColumnKind::Doing, TaskStatus::Running).await;
    let run = live_run(dir.path(), doing.root_task.unwrap().conversation_id.expect("a conversation")).await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let (ok, said) = bridge.employee_act(run, EmployeeTaskOp::PostToDiscussion { message: format!("@bea {CANARY}") }).await.unwrap();
    assert!(ok, "{said}");
    let row = rows_of(&bea, ticket).await.pop().expect("a row");
    assert_eq!((row.reason.as_deref(), row.title.as_str()), (Some("mention"), "Forge mentioned you"));
    assert!(row.message_id.is_some());
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_name_in_a_description_gives_a_row_once_and_only_when_it_is_new() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, bea, cara) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await, connect_as(dir.path(), "cara").await);
    let (_, project_id, _) = setup(&alex).await;
    let ticket = create(&alex, new_ticket(project_id, None, Some("Ask @bea"), AgentMode::Auto, false)).await;
    let row = rows_of(&bea, ticket).await.pop().expect("a row");
    assert_eq!((row.reason.as_deref(), row.message_id, row.merged_count), (Some("mention"), None, 0));

    let version = alex.ticket(ticket).await.unwrap().ticket.version;
    let update = describe(ticket, "Ask @bea and @cara");
    alex.ticket_command(Command::UpdateTicket(update), Some(version)).await.unwrap();
    assert_eq!(rows_of(&bea, ticket).await[0].merged_count, 0, "bea was named before");
    assert_eq!(rows_of(&cara, ticket).await.len(), 1);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_goes_to_followers_and_responsible_and_needs_you_follows_a_handover() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, bea, cara, dan) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await, connect_as(dir.path(), "cara").await, connect_as(dir.path(), "dan").await);
    let (bea_id, cara_id) = (user_id(&bea).await, user_id(&cara).await);
    let (owner, project_id, _) = setup(&alex).await;
    let ticket = create(&alex, new_ticket(project_id, Some(owner), Some("Work on it"), AgentMode::Auto, false)).await;
    follow(&bea, ticket).await;
    set_responsible(&alex, ticket, Some(cara_id)).await;
    alex.ticket_command(Command::StartTicket(StartTicket { ticket_id: ticket, employee_id: None }), None).await.unwrap();
    let detail = until(&alex, ticket, ColumnKind::Review, TaskStatus::Review).await;
    let root = detail.root_task.unwrap().id;

    for (client, name, needs) in [(&alex, "alex", false), (&bea, "bea", false), (&cara, "cara", true)] {
        let rows = rows_of(client, ticket).await;
        assert_eq!(rows.len(), 1, "{name}: {rows:#?}");
        let row = &rows[0];
        assert_eq!((row.reason.as_deref(), row.severity.as_str(), row.needs_you, row.task_id), (Some("review"), "info", needs, Some(root)), "{name}");
        assert_eq!(row.title, if needs { "Ready for your review" } else { "Ready for review" }, "{name}");
    }
    assert!(rows_of(&dan, ticket).await.is_empty(), "does not follow");

    // needs_you is worked out when read: it goes with the handover.
    set_responsible(&alex, ticket, Some(bea_id)).await;
    assert!(!rows_of(&cara, ticket).await[0].needs_you);
    let bea_row = rows_of(&bea, ticket).await.pop().unwrap();
    assert_eq!((bea_row.needs_you, bea_row.title.as_str(), bea_row.reason.as_deref()), (true, "Ready for your review", Some("review")), "the head stays the review");
    assert_eq!(bea_row.merged_count, 1);

    // Without an active Responsible it waits for everyone who has a row.
    alex.disable_user(bea_id).await.unwrap();
    assert!(rows_of(&cara, ticket).await[0].needs_you);
    assert!(rows_of(&alex, ticket).await[0].needs_you);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_send_back_replaces_the_review_for_followers_but_not_for_who_did_it() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, bea) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await);
    let (owner, project_id, _) = setup(&alex).await;
    let ticket = create(&alex, new_ticket(project_id, Some(owner), Some("Work on it"), AgentMode::Auto, false)).await;
    follow(&bea, ticket).await;
    alex.ticket_command(Command::StartTicket(StartTicket { ticket_id: ticket, employee_id: None }), None).await.unwrap();
    until(&alex, ticket, ColumnKind::Review, TaskStatus::Review).await;
    assert_eq!(rows_of(&bea, ticket).await[0].reason.as_deref(), Some("review"));

    alex.ticket_command(Command::SendBackTicket(SendBackTicket { confirm_secret: false, ticket_id: ticket, feedback: format!("{CANARY} do better") }), None).await.unwrap();
    let rows = rows_of(&bea, ticket).await;
    assert_eq!(rows.len(), 1, "{rows:#?}");
    assert!(rows[0].title.starts_with("alex sent it back to "), "{}", rows[0].title);
    assert_eq!((rows[0].reason.as_deref(), rows[0].needs_you, rows[0].merged_count), (Some("sent_back"), false, 0), "the review it replaces can no longer be seen");
    assert!(!serde_json::to_string(&rows).unwrap().contains(CANARY));
    let alex_rows: Vec<_> = rows_of(&alex, ticket).await.into_iter().filter(|r| r.reason.as_deref() == Some("sent_back")).collect();
    assert!(alex_rows.is_empty(), "never to the actor");

    // Back in Review, and then Done: every row of the closed ticket goes (B12).
    until(&alex, ticket, ColumnKind::Review, TaskStatus::Review).await;
    alex.ticket_command(Command::AcceptTicket(AcceptTicket { confirm_secret: false, ticket_id: ticket, note: None }), None).await.unwrap();
    assert!(rows_of(&bea, ticket).await.is_empty());
    assert_eq!(unread(&bea).await, 0, "the count agrees with the list");
    // B2: an accepted main task has no "Result ready" for anyone.
    assert!(inbox(&alex).await.iter().all(|n| n.title != "Result ready"), "{:#?}", inbox(&alex).await);
    assert!(inbox(&bea).await.iter().all(|n| n.title != "Result ready"));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plan_review_goes_to_followers_and_leaves_when_the_ticket_does() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 40).await;
    let (alex, bea) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await);
    let bea_id = user_id(&bea).await;
    let (owner, project_id, _) = setup(&alex).await;
    let ticket = create(&alex, new_ticket(project_id, Some(owner), Some("Plan it"), AgentMode::Plan, false)).await;
    set_responsible(&alex, ticket, Some(bea_id)).await;
    alex.ticket_command(Command::StartTicket(StartTicket { ticket_id: ticket, employee_id: None }), None).await.unwrap();
    let doing = until(&alex, ticket, ColumnKind::Doing, TaskStatus::Running).await;
    let run = live_run(dir.path(), doing.root_task.unwrap().conversation_id.expect("a conversation")).await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    bridge.permission_ask(run, Some("plan-1".into()), "ExitPlanMode".into(), serde_json::json!({ "plan": format!("# {CANARY}") })).await.unwrap();
    until(&alex, ticket, ColumnKind::PlanReview, TaskStatus::Waiting).await;

    let row = rows_of(&bea, ticket).await.pop().expect("a row");
    assert_eq!((row.reason.as_deref(), row.title.as_str(), row.needs_you), (Some("plan_review"), "Plan ready for your review", true));
    let row = rows_of(&alex, ticket).await.pop().expect("a row for the creator");
    assert_eq!((row.reason.as_deref(), row.title.as_str(), row.needs_you), (Some("plan_review"), "Plan ready for review", false));
    assert!(inbox(&alex).await.iter().all(|n| n.title != aurelius_storage::timeline::PLAN_TITLE), "no second row in the old words");

    // Decided: the row leaves with the column.
    alex.ticket_command(Command::ApprovePlan(ApprovePlan { ticket_id: ticket, decision: PlanApproval::Approve, feedback: None }), None).await.unwrap();
    assert!(rows_of(&alex, ticket).await.iter().all(|r| r.reason.as_deref() != Some("plan_review")));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_main_task_goes_to_followers_instead_of_everyone_and_leaves_when_started_again() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with(dir.path(), Arc::new(FailMarked)).await;
    let (alex, bea, cara) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await, connect_as(dir.path(), "cara").await);
    let cara_id = user_id(&cara).await;
    let (owner, project_id, _) = setup(&alex).await;
    let ticket = create(&alex, new_ticket(project_id, Some(owner), Some("FAIL-ME"), AgentMode::Auto, false)).await;
    follow(&bea, ticket).await;
    set_responsible(&alex, ticket, Some(cara_id)).await;
    alex.ticket_command(Command::StartTicket(StartTicket { ticket_id: ticket, employee_id: None }), None).await.unwrap();
    let failed = until(&alex, ticket, ColumnKind::Doing, TaskStatus::Failed).await;
    let root = failed.root_task.unwrap().id;

    let row = rows_of(&cara, ticket).await.pop().expect("a row");
    assert_eq!((row.reason.as_deref(), row.severity.as_str(), row.title.as_str(), row.needs_you, row.task_id), (Some("failed"), "warning", "The main task failed", true, Some(root)));
    let row = rows_of(&bea, ticket).await.pop().expect("a row");
    assert_eq!((row.reason.as_deref(), row.needs_you, row.target_type.as_str()), (Some("failed"), false, "task.failed"));
    assert_eq!(db(dir.path()).query_row("SELECT COUNT(*) FROM notifications WHERE user_id IS NULL AND event_id IN (SELECT id FROM events WHERE type = 'task.failed')", [], |r| r.get::<_, i64>(0)).unwrap(), 0, "B2: no shared Task failed");

    // Started again: the row of the failure is gone once it no longer failed.
    let version = alex.ticket(ticket).await.unwrap().ticket.version;
    alex.ticket_command(Command::UpdateTicket(describe(ticket, "Works now")), Some(version)).await.unwrap();
    alex.ticket_command(Command::StartTicket(StartTicket { ticket_id: ticket, employee_id: None }), None).await.unwrap();
    until(&alex, ticket, ColumnKind::Review, TaskStatus::Review).await;
    let rows = rows_of(&bea, ticket).await;
    assert_eq!(rows.len(), 1, "{rows:#?}");
    assert_eq!(rows[0].reason.as_deref(), Some("review"), "the hidden failure holds no new row back");
    stop(dir.path(), daemon).await;
}

async fn plain_task(client: &Client, project_id: ProjectId, owner: EmployeeId, goal: &str) -> aurelius_domain::TaskId {
    let create = CreateTask { confirm_secret: false, files: vec![], run: Default::default(), project_id, employee_id: Some(owner), title: "Plain".into(), goal: goal.into(), acceptance_criteria: vec![] };
    let CommandResult::TaskCreated { task_id, .. } = run(client, Command::CreateTask(create)).await else { panic!() };
    task_id
}

async fn shared_row(client: &Client, task: aurelius_domain::TaskId, title: &str) -> NotificationView {
    for _ in 0..400 {
        if let Some(row) = inbox(client).await.into_iter().find(|n| n.task_id == Some(task) && n.title == title) {
            return row;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no {title:?} for {task}: {:#?}", inbox(client).await);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_without_a_ticket_still_tells_everyone_as_before() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with(dir.path(), Arc::new(FailMarked)).await;
    let (alex, bea) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await);
    let (owner, project_id, _) = setup(&alex).await;
    let done = plain_task(&alex, project_id, owner, "Do it").await;
    let failed = plain_task(&alex, project_id, owner, "FAIL-ME").await;
    for client in [&alex, &bea] {
        let row = shared_row(client, done, "Result ready").await;
        assert_eq!((row.reason.as_deref(), row.severity.as_str(), row.needs_you), (None, "info", false));
        let row = shared_row(client, failed, "Task failed").await;
        assert_eq!((row.reason.as_deref(), row.severity.as_str()), (None, "warning"));
    }
    stop(dir.path(), daemon).await;
}

// ---- merging and push ----

#[tokio::test(flavor = "multi_thread")]
async fn events_on_one_ticket_merge_under_the_most_important_reason_and_a_read_row_starts_anew() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, bea) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await);
    let bea_id = user_id(&bea).await;
    let (owner, project_id, _) = setup(&alex).await;
    let ticket = create(&alex, new_ticket(project_id, Some(owner), Some("Work on it"), AgentMode::Auto, false)).await;
    set_responsible(&alex, ticket, Some(bea_id)).await;
    let first = rows_of(&bea, ticket).await.pop().unwrap();
    assert_eq!((first.reason.as_deref(), first.merged_count), (Some("responsible"), 0));

    post(&alex, ticket, "@bea look").await;
    let merged = rows_of(&bea, ticket).await;
    assert_eq!(merged.len(), 1, "{merged:#?}");
    assert_eq!((merged[0].id.as_str(), merged[0].reason.as_deref(), merged[0].merged_count, merged[0].title.as_str()), (first.id.as_str(), Some("mention"), 1, "alex mentioned you"));
    assert!(merged[0].created_at_ms >= first.created_at_ms);

    alex.ticket_command(Command::StartTicket(StartTicket { ticket_id: ticket, employee_id: None }), None).await.unwrap();
    until(&alex, ticket, ColumnKind::Review, TaskStatus::Review).await;
    let head = rows_of(&bea, ticket).await.pop().unwrap();
    assert_eq!((head.reason.as_deref(), head.merged_count, head.target_type.as_str()), (Some("review"), 2, "ticket.review"));

    // A mention does not take the head from the review, but it is pushed in its own words.
    let review_title = head.title.clone();
    post(&alex, ticket, "@bea and now?").await;
    let kept = rows_of(&bea, ticket).await.pop().unwrap();
    assert_eq!((kept.reason.as_deref(), kept.title, kept.merged_count), (Some("review"), review_title, 3));
    let pushes: Vec<_> = all_pushes(dir.path()).into_iter().filter(|(f, _)| f.user_id.as_deref() == Some(&bea_id.to_string())).collect();
    assert_eq!(pushes.len(), 1, "only the last push of the row is left to find: {pushes:#?}");
    assert_eq!((pushes[0].0.view.title.as_str(), pushes[0].0.view.reason.as_deref(), pushes[0].0.view.id.as_str()), ("alex mentioned you", Some("mention"), kept.id.as_str()));

    // Read: the next event is a new row, not a raised count on the old one.
    bea.mark_notifications_read().await.unwrap();
    post(&alex, ticket, "@bea once more").await;
    let rows = rows_of(&bea, ticket).await;
    assert_eq!(rows.len(), 2, "{rows:#?}");
    let new = rows.iter().find(|r| !r.read).expect("an unread one");
    assert_eq!((new.reason.as_deref(), new.merged_count), (Some("mention"), 0));
    assert_eq!(unread(&bea).await, 1);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_push_goes_per_person_only_to_their_iphones_and_never_to_android() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, bea) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await);
    let (alex_id, bea_id) = (user_id(&alex).await, user_id(&bea).await);
    let (_, project_id, _) = setup(&alex).await;
    let ticket = backlog(&alex, project_id).await;
    follow(&bea, ticket).await;

    let store = Store::open(dir.path()).unwrap();
    let device = |name: &'static str, phone: &'static str, platform: &'static str, user: UserId| {
        let id = aurelius_domain::ClientId::new();
        store
            .write(move |tx| {
                aurelius_storage::remote::create_client(tx, id, aurelius_domain::PrincipalId::new(), name, phone, "v", &aurelius_domain::remote::Capability::DEVICE_DEFAULT)?;
                tx.execute("UPDATE remote_clients SET user_id = ?2 WHERE id = ?1", [id.to_string(), user.to_string()])?;
                aurelius_storage::remote::set_push(tx, id, platform, name, "sandbox")
            })
            .unwrap();
        id
    };
    let bea_phone = device("bea-iphone", "ios", "apns", bea_id);
    let bea_android = device("bea-android", "android", "fcm", bea_id);
    let alex_phone = device("alex-iphone", "ios", "apns", alex_id);

    post(&alex, ticket, "@bea look").await;
    let seq: i64 = db(dir.path()).query_row("SELECT MAX(json_extract(target_json, '$.push.seq')) FROM notifications", [], |r| r.get(0)).unwrap();
    let pushes = store.read(move |c| aurelius_storage::remote::pushes_for_seq(c, seq as u64)).unwrap();
    assert_eq!(pushes.len(), 1, "{pushes:#?}");
    let (found, targets) = &pushes[0];
    assert_eq!(found.user_id.as_deref(), Some(bea_id.to_string().as_str()));
    let targets: Vec<_> = targets.iter().map(|t| t.client_id).collect();
    assert!(targets.contains(&bea_phone), "{targets:?}");
    assert!(!targets.contains(&bea_android), "B5: no push on Android");
    assert!(!targets.contains(&alex_phone), "only their own devices");
    let alert = aurelius_remote_gateway::push::Alert::for_notification(found, None);
    assert_eq!(alert.data["ticket_id"], serde_json::json!(ticket));
    assert_eq!(alert.data["reason"], "mention");
    assert_eq!(alert.title, "alex mentioned you");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_row_outside_a_ticket_names_its_channel_through_the_redactor_and_pushes_no_text() {
    let secret = "sk-test-FAKE-channel-name-0000";
    let owner = "personal-inbox-channel-name";
    aurelius_runtime::redact::add_to_set(owner, aurelius_runtime::redact::Secret { name: "CHANNEL_NAME".into(), label: String::new(), value: zeroize::Zeroizing::new(secret.into()) });
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, bea) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await);
    let store = Store::open(dir.path()).unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = run(&alex, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })).await else { panic!() };
    let channel = alex.channels().await.unwrap().channels.into_iter().find(|c| c.project_id == Some(project_id)).expect("its channel").conversation_id;
    // A name with a secret in it can only come from before that secret was in the vault.
    let name = format!("Atlas {secret}");
    store.write(move |tx| Ok(tx.execute("UPDATE projects SET name = ?1 WHERE id = ?2", rusqlite::params![name, project_id.to_string()])?)).unwrap();

    say_in_channel(&alex, Some(channel), &format!("@bea {CANARY}")).await;
    let row = inbox(&bea).await.into_iter().find(|n| n.conversation_id == Some(channel)).expect("a row");
    assert!(row.subject.starts_with("#Atlas ") && !row.subject.contains(secret), "{}", row.subject);
    assert_eq!(row.ticket_id, None);
    let stored: String = db(dir.path()).query_row("SELECT metadata_json FROM events WHERE json_extract(metadata_json, '$.mentioned_users') IS NOT NULL ORDER BY seq DESC LIMIT 1", [], |r| r.get(0)).unwrap();
    assert!(!stored.contains(secret) && !stored.contains(CANARY), "{stored}");
    let (found, alert) = all_pushes(dir.path()).into_iter().find(|(f, _)| f.view.id == row.id).expect("a push");
    assert_eq!(found.view.subject, row.subject);
    assert!(!alert.contains(secret) && !alert.contains(CANARY), "{alert}");
    aurelius_runtime::redact::install_set(owner, Vec::new());
    stop(dir.path(), daemon).await;
}

// ---- one row read (notification.mark_read) ----

#[tokio::test(flavor = "multi_thread")]
async fn reading_a_row_counts_for_one_person_and_keeps_needs_you_until_a_new_event_makes_it_unread() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, bea, cara) = (connect(dir.path()).await, connect_as(dir.path(), "bea").await, connect_as(dir.path(), "cara").await);
    let bea_id = user_id(&bea).await;
    let (owner, project_id, _) = setup(&alex).await;
    let ticket = create(&alex, new_ticket(project_id, Some(owner), Some("Work on it"), AgentMode::Auto, false)).await;
    follow(&cara, ticket).await;
    set_responsible(&alex, ticket, Some(bea_id)).await;
    alex.ticket_command(Command::StartTicket(StartTicket { ticket_id: ticket, employee_id: None }), None).await.unwrap();
    until(&alex, ticket, ColumnKind::Review, TaskStatus::Review).await;

    // Bea cannot read Cara's row, and reading her own leaves Cara's unread.
    let (bea_row, cara_row) = (rows_of(&bea, ticket).await.pop().unwrap(), rows_of(&cara, ticket).await.pop().unwrap());
    assert_eq!(bea.mark_notification_read(cara_row.id.clone()).await.unwrap(), 0);
    assert!(!rows_of(&cara, ticket).await[0].read);
    assert_eq!(bea.mark_notification_read(bea_row.id.clone()).await.unwrap(), 1);
    assert_eq!(bea.mark_notification_read(bea_row.id.clone()).await.unwrap(), 0, "a repeat is no error");
    assert_eq!(bea.mark_notification_read("no-such-row".into()).await.unwrap(), 0);
    assert!(!rows_of(&cara, ticket).await[0].read);

    // Read, it still waits for her: that comes from the state, not from reading.
    let read = rows_of(&bea, ticket).await.pop().unwrap();
    assert_eq!((read.read, read.needs_you, read.title.as_str()), (true, true, "Ready for your review"));
    assert_eq!(unread(&bea).await, 0);

    // A new event gives a new unread row; the one that was read stays read.
    post(&alex, ticket, "@bea and now?").await;
    let rows = rows_of(&bea, ticket).await;
    assert_eq!(rows.len(), 2, "{rows:#?}");
    assert!(rows.iter().any(|r| r.id == bea_row.id && r.read));
    let new = rows.iter().find(|r| !r.read).expect("an unread one");
    assert_eq!((new.reason.as_deref(), new.merged_count), (Some("mention"), 0));
    assert_eq!(unread(&bea).await, 1);
    stop(dir.path(), daemon).await;
}

// ---- the A0 point ----

#[tokio::test(flavor = "multi_thread")]
async fn a_new_board_member_sees_shared_rows_from_before_as_read_without_a_write() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    let (owner, project_id, _) = setup(&alex).await;
    let task = plain_task(&alex, project_id, owner, "Do it").await;
    let old = shared_row(&alex, task, "Result ready").await;
    assert!(!old.read);
    tokio::time::sleep(Duration::from_millis(5)).await;

    let bea = connect_as(dir.path(), "bea").await;
    let seen = inbox(&bea).await.into_iter().find(|n| n.id == old.id).expect("still in the list");
    assert!(seen.read, "from before they joined");
    assert_eq!(unread(&bea).await, 0);
    assert_eq!(db(dir.path()).query_row("SELECT COUNT(*) FROM notification_reads", [], |r| r.get::<_, i64>(0)).unwrap(), 0, "nothing was written");
    assert!(!inbox(&alex).await.into_iter().find(|n| n.id == old.id).unwrap().read, "alex was there and did not read it");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_prunes_old_rows_at_its_start_and_keeps_their_events() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    let (owner, project_id, _) = setup(&alex).await;
    let task = plain_task(&alex, project_id, owner, "Do it").await;
    let old = shared_row(&alex, task, "Result ready").await;
    stop(dir.path(), daemon).await;

    let long_ago = aurelius_storage::now_ms() - aurelius_storage::timeline::NOTIFICATION_KEEP_MS - 1;
    let writable = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    writable.execute("UPDATE notifications SET created_at_ms = ?1 WHERE id = ?2", rusqlite::params![long_ago, old.id]).unwrap();
    drop(writable);
    let event: String = db(dir.path()).query_row("SELECT event_id FROM notifications WHERE id = ?1", [&old.id], |r| r.get(0)).unwrap();

    let daemon = start(dir.path()).await;
    let mut left = 1;
    for _ in 0..400 {
        left = db(dir.path()).query_row("SELECT COUNT(*) FROM notifications WHERE id = ?1", [&old.id], |r| r.get::<_, i64>(0)).unwrap();
        if left == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(left, 0, "gone at the start");
    assert_eq!(db(dir.path()).query_row("SELECT COUNT(*) FROM events WHERE id = ?1", [&event], |r| r.get::<_, i64>(0)).unwrap(), 1, "the event stays");
    stop(dir.path(), daemon).await;
}

// ---- older clients (R1) ----

/// `severity` as a client on protocol 5 from before phase A may read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OldSeverity {
    Info,
    Warning,
    Action,
}

/// The fields of a notification an older client knows.
#[derive(Debug, Deserialize)]
struct OldNotification {
    severity: OldSeverity,
    title: String,
    subject: String,
    target_type: String,
    task_id: Option<aurelius_domain::TaskId>,
}

/// The tab of an older desktop client (`note_kind`).
fn old_tab(n: &OldNotification) -> &'static str {
    match (n.severity, n.target_type.as_str()) {
        (OldSeverity::Action, _) => "needs_you",
        (_, "task.failed") => "failed",
        _ => "results",
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_older_client_reads_every_new_row_with_its_own_enum_and_gets_no_new_event() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with(dir.path(), Arc::new(FailMarked)).await;
    let alex = connect(dir.path()).await;
    let (old_client, mut events) = connect_events_as(dir.path(), "bea").await;
    old_client.subscribe(0).await.unwrap();
    let bea_id = user_id(&old_client).await;
    let (owner, project_id, _) = setup(&alex).await;

    let review = create(&alex, new_ticket(project_id, Some(owner), Some("Work on it"), AgentMode::Auto, false)).await;
    follow(&old_client, review).await;
    alex.ticket_command(Command::StartTicket(StartTicket { ticket_id: review, employee_id: None }), None).await.unwrap();
    until(&alex, review, ColumnKind::Review, TaskStatus::Review).await;
    let failing = create(&alex, new_ticket(project_id, Some(owner), Some("FAIL-ME"), AgentMode::Auto, false)).await;
    follow(&old_client, failing).await;
    alex.ticket_command(Command::StartTicket(StartTicket { ticket_id: failing, employee_id: None }), None).await.unwrap();
    until(&alex, failing, ColumnKind::Doing, TaskStatus::Failed).await;
    let other = backlog(&alex, project_id).await;
    set_responsible(&alex, other, Some(bea_id)).await;
    post(&alex, other, "@bea look").await;

    let ResponseBody::Notifications(snapshot) = old_client.call(Method::NotificationList { project_id: None }).await.unwrap() else { panic!() };
    let wire = serde_json::to_value(&snapshot).unwrap();
    let rows: Vec<OldNotification> = serde_json::from_value(wire["notifications"].clone()).expect("the old enum reads every row");
    assert!(rows.len() >= 2, "{rows:#?}");
    assert!(rows.iter().all(|r| !r.title.is_empty() && r.subject.starts_with('#') && r.severity != OldSeverity::Action), "{rows:#?}");
    assert_eq!(rows.iter().filter(|r| old_tab(r) == "failed").count(), 1, "{rows:#?}");
    assert!(rows.iter().filter(|r| old_tab(r) == "results").count() >= 2, "{rows:#?}");
    assert!(rows.iter().filter(|r| old_tab(r) == "failed").all(|r| r.task_id.is_some()), "an older client can open it");

    // No new kind on the wire: nothing a client without `accepts` could not read.
    let mut kinds = Vec::new();
    while let Ok(event) = events.try_recv() {
        kinds.push(event.kind);
    }
    assert!(!kinds.is_empty());
    assert!(kinds.iter().all(|k| !matches!(k, EventKind::TicketFollowChanged | EventKind::MessageEdited)), "{kinds:?}");
    stop(dir.path(), daemon).await;
}
