//! A budget per ticket (feature `ticket_budget`): what employees set in motion on a
//! ticket since a board member last acted on it. Used up, an employee can hand nothing
//! more on and the ticket does not go on by itself: it waits for a board member, whose
//! message in the discussion lets it go on and counts from zero again. Runs use a fake runtime.

mod common;

use aurelius_client::{Client, bridge::BridgeClient};
use aurelius_domain::company::UpdateProject;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::TaskStatus;
use aurelius_domain::ticket::*;
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, RunId, TicketId};
use aurelius_protocol::EmployeeTaskOp;
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use std::sync::Arc;
use std::time::Duration;

/// The fake runtime with two gates: a run whose turns name HOLD-MAIN or HOLD-ADA is
/// held until its gate opens, then completes.
struct Gates {
    main: tokio::sync::watch::Sender<bool>,
    ada: tokio::sync::watch::Sender<bool>,
}

fn held(request: &RunRequest, mut gate: tokio::sync::watch::Receiver<bool>) -> RunHandle {
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let (input_tx, _input_rx) = tokio::sync::mpsc::channel(8);
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let _ = tx.send(RuntimeEvent::Started).await;
        while !*gate.borrow() {
            tokio::select! {
                changed = gate.changed() => if changed.is_err() { return },
                _ = cancel_rx.changed() => {
                    let _ = tx.send(RuntimeEvent::Interrupted).await;
                    return;
                }
            }
        }
        let _ = tx.send(RuntimeEvent::TextDelta("Report: it is done.".into())).await;
        let _ = tx.send(RuntimeEvent::Completed).await;
    });
    RunHandle::new(request.run_id, rx, input_tx, cancel_tx)
}

#[async_trait::async_trait]
impl AgentRuntime for Gates {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let turns: String = request.turns.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join("\n");
        if turns.contains("HOLD-ADA") && !*self.ada.borrow() {
            return Ok(held(&request, self.ada.subscribe()));
        }
        if turns.contains("HOLD-MAIN") && !*self.main.borrow() {
            return Ok(held(&request, self.main.subscribe()));
        }
        fake::FakeRuntime { delay: Duration::from_millis(1), behavior: fake::FakeBehavior::Normal }.start(request).await
    }
}

async fn employee(client: &Client, name: &str) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    employee_id
}

fn live_run(root: &std::path::Path, conversation: ConversationId) -> Option<RunId> {
    let found: Option<String> = common::db(root).query_row("SELECT id FROM runs WHERE conversation_id = ?1 AND status = 'running'", [conversation.to_string()], |r| r.get(0)).ok();
    found.map(|r| RunId::parse(&r).unwrap())
}

async fn until(client: &Client, id: TicketId, what: &str, done: impl Fn(&TicketDetail) -> bool) -> TicketDetail {
    for _ in 0..800 {
        let detail = client.ticket(id).await.unwrap();
        if done(&detail) {
            return detail;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let detail = client.ticket(id).await.unwrap();
    panic!("never {what}: {:?} / {:?} / {:?} / {:?}", detail.ticket.column, detail.root_task.as_ref().map(|t| (t.status, t.waiting_reason.clone())), detail.ticket.attention, detail.ticket.budget);
}

async fn post(client: &Client, ticket_id: TicketId, text: &str) {
    let post = PostTicketMessage { confirm_secret: false, ticket_id, text: text.into(), images: vec![], files: vec![], thread_parent: None };
    client.ticket_command(Command::PostTicketMessage(post), None).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_whose_budget_is_used_up_waits_for_a_board_member() {
    let dir = tempfile::tempdir().unwrap();
    let gates = Arc::new(Gates { main: tokio::sync::watch::channel(false).0, ada: tokio::sync::watch::channel(false).0 });
    let daemon = common::start_with(dir.path(), gates.clone()).await;
    let client = common::connect(dir.path()).await;
    assert!(client.supports(aurelius_protocol::feature::TICKET_BUDGET));
    common::free_chart(&client).await;
    let CommandResult::ProjectCreated { project_id, .. } = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result else { panic!() };
    // Two wakes per ticket between two actions of a board member.
    let small = UpdateProject { project_id, dispatcher: None, styleguide_url: None, ticket_worktrees: None, ticket_budget: Some(2) };
    client.execute(aurelius_client::envelope(Command::UpdateProject(small), None)).await.unwrap();
    assert_eq!(client.projects().await.unwrap().projects.iter().find(|p| p.id == project_id).unwrap().ticket_budget, Some(2));
    let forge = employee(&client, "Forge").await;
    employee(&client, "Ada").await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();

    let made = client
        .create_ticket(CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id,
            title: "Login page".into(),
            description: Some("HOLD-MAIN A login page.".into()),
            priority: TicketPriority::default(),
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: Some(forge),
            run: aurelius_domain::task::RunSettings { mode: Some(aurelius_domain::autonomy::AgentMode::Auto), ..Default::default() },
            files: vec![],
            start: true,
            from_message: None,
            from_idea: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id: id, .. } = made.result else { panic!("{:?}", made.result) };
    let running = until(&client, id, "running", |d| d.root_task.as_ref().is_some_and(|t| t.status == TaskStatus::Running && t.conversation_id.is_some())).await;
    let conversation = running.root_task.unwrap().conversation_id.unwrap();
    let mut main_run = None;
    for _ in 0..800 {
        main_run = live_run(dir.path(), conversation);
        if main_run.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let main_run = main_run.expect("the main task runs");
    // The owner's own run is the first wake.
    assert_eq!(client.ticket(id).await.unwrap().ticket.budget, Some(TicketBudget { spent: 1, limit: 2 }));

    // A handoff is the second: the budget is used up, and a third thing is refused with the reason.
    let (ok, said) = bridge.employee_act(main_run, EmployeeTaskOp::HandOff { to: "Ada".into(), message: "HOLD-ADA have a look at the form".into() }).await.unwrap();
    assert!(ok, "{said}");
    assert_eq!(client.ticket(id).await.unwrap().ticket.budget, Some(TicketBudget { spent: 2, limit: 2 }));
    let (ok, said) = bridge.employee_act(main_run, EmployeeTaskOp::HandOff { to: "Ada".into(), message: "and once more".into() }).await.unwrap();
    assert!(!ok && said.contains("budget is used up") && said.contains("2 wakes"), "{said}");

    // The owner ends and waits for Ada; when she is done it would go on by itself, but does not.
    gates.main.send_replace(true);
    until(&client, id, "waiting for the colleague", |d| d.root_task.as_ref().is_some_and(|t| t.status == TaskStatus::Waiting)).await;
    gates.ada.send_replace(true);
    let held = until(&client, id, "held for its budget", |d| d.ticket.attention.as_ref().is_some_and(|a| a.kind == "budget")).await;
    assert_eq!((held.ticket.column, held.root_task.as_ref().map(|t| t.status)), (ColumnKind::Doing, Some(TaskStatus::Waiting)));
    assert!(held.ticket.busy.is_empty(), "nothing works on it: {:?}", held.ticket.busy);
    let lines: Vec<String> = client.messages(held.ticket.conversation_id.unwrap()).await.unwrap().messages.into_iter().filter(|m| m.system.is_some()).map(|m| m.text).collect();
    assert!(lines.iter().any(|l| l.contains("budget is used up") && l.contains("Write here to let the work go on")), "{lines:?}");

    // A board member writes: the count starts again and the owner goes on, to its report.
    post(&client, id, "Go on, and finish it.").await;
    let done = until(&client, id, "in review", |d| d.ticket.column == ColumnKind::Review).await;
    assert!(done.ticket.budget.is_some_and(|b| b.spent <= 1 && b.limit == 2), "{:?}", done.ticket.budget);
    assert_eq!(done.ticket.attention.map(|a| a.kind).as_deref(), Some("review"));

    // No limit: nothing is counted against the ticket.
    let none = UpdateProject { project_id, dispatcher: None, styleguide_url: None, ticket_worktrees: None, ticket_budget: Some(0) };
    client.execute(aurelius_client::envelope(Command::UpdateProject(none), None)).await.unwrap();
    assert_eq!(client.ticket(id).await.unwrap().ticket.budget, None);
    common::stop(dir.path(), daemon).await;
}
