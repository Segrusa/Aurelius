//! Plan review of a ticket (Fase 1, S4, impact analysis §3.6): the main task of
//! a ticket in Plan mode hands in its plan through the real bridge path; it is
//! no inbox approval, Claude is told to stop, and once the run ended the card
//! waits in Plan review. Approve carries the plan out in the same conversation
//! (Auto); revise keeps Plan mode and a new plan follows. Runs use the fake
//! runtime (the real Claude Code's stop after the deny: real_claude_plan.rs).

mod common;

use aurelius_client::{Client, bridge::BridgeClient};
use aurelius_domain::autonomy::AgentMode;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::{RunSettings, TaskStatus};
use aurelius_domain::ticket::*;
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, ErrorCode, ProjectId, RunId, TicketId};
use std::time::Duration;

async fn setup(client: &Client) -> (EmployeeId, ProjectId) {
    client.user_create("grace", "a-long-enough-password").await.unwrap();
    let create = CreateEmployee { name: "Planner".into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    (employee_id, project_id)
}

async fn planned_ticket(client: &Client, project_id: ProjectId, owner: EmployeeId, mode: Option<AgentMode>) -> TicketId {
    let made = client
        .create_ticket(CreateTicket {
            project_id,
            title: "Export".into(),
            description: Some("Export the report as CSV.".into()),
            priority: TicketPriority::default(),
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: Some(owner),
            run: RunSettings { mode, ..Default::default() },
            files: vec![],
            start: true,
            from_message: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!("{:?}", made.result) };
    ticket_id
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

/// The run of the main task's conversation that is going now.
async fn live_run(root: &std::path::Path, conversation: ConversationId) -> RunId {
    for _ in 0..2000 {
        let found: Option<String> = common::db(root)
            .query_row("SELECT id FROM runs WHERE conversation_id = ?1 AND status = 'running' LIMIT 1", [conversation.to_string()], |r| r.get(0))
            .ok();
        if let Some(run) = found {
            return RunId::parse(&run).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let db = common::db(root);
    let mut stmt = db.prepare("SELECT id, status, conversation_id FROM runs").unwrap();
    let all: Vec<(String, String, Option<String>)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().collect::<Result<_, _>>().unwrap();
    panic!("no live run in {conversation}: {all:?}");
}

fn mode_of(root: &std::path::Path, conversation: ConversationId) -> String {
    common::db(root).query_row("SELECT agent_mode FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0)).unwrap()
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
async fn a_tickets_plan_waits_for_the_board_and_is_revised_then_approved() {
    let dir = tempfile::tempdir().unwrap();
    // Slow enough to hand a plan in while the run goes.
    let daemon = common::start_with_delay(dir.path(), 40).await;
    let client = common::connect(dir.path()).await;
    let (planner, atlas) = setup(&client).await;
    let id = planned_ticket(&client, atlas, planner, Some(AgentMode::Plan)).await;

    // The main task works in Plan mode; its ExitPlanMode is answered at once with a stop.
    let doing = until(&client, id, ColumnKind::Doing, TaskStatus::Running).await;
    let conversation = doing.root_task.as_ref().unwrap().conversation_id.expect("a conversation");
    assert_eq!(mode_of(dir.path(), conversation), "plan");
    let run = live_run(dir.path(), conversation).await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let input = serde_json::json!({ "plan": "# Plan\n\n1. Write export.csv" });
    let answer = tokio::time::timeout(Duration::from_secs(5), bridge.permission_ask(run, Some("plan-1".into()), "ExitPlanMode".into(), input)).await.expect("answered at once").unwrap();
    assert!(!answer.allow && answer.updated_permissions.is_none());
    assert!(answer.message.as_deref().unwrap().contains("Stop now"), "{:?}", answer.message);
    assert!(client.approvals().await.unwrap().approvals.iter().all(|a| a.tool != "claude.ExitPlanMode"), "no inbox approval for a ticket's plan");
    let messages = client.messages(conversation).await.unwrap().messages;
    assert!(messages.iter().any(|m| m.update.as_deref() == Some("plan") && m.text == "# Plan\n\n1. Write export.csv"), "the plan is in the conversation");

    // The run ended: Plan review, nothing reported (MU1-13 AC1).
    let review = until(&client, id, ColumnKind::PlanReview, TaskStatus::Waiting).await;
    let root = review.root_task.unwrap();
    assert_eq!(root.waiting_reason.as_deref(), Some("plan_review"));
    assert!(root.result.is_none(), "no report yet");
    // Not a Review: accept and send back do not apply here.
    assert_eq!(code(client.ticket_command(Command::AcceptTicket(AcceptTicket { ticket_id: id, note: None }), None).await), ErrorCode::Validation);

    // Revise (feedback required): Plan mode stays, the feedback goes in, a new plan follows.
    assert_eq!(code(client.ticket_command(Command::RevisePlan(RevisePlan { ticket_id: id, feedback: "".into() }), None).await), ErrorCode::Validation);
    client.ticket_command(Command::RevisePlan(RevisePlan { ticket_id: id, feedback: "Also a JSON export".into() }), None).await.unwrap();
    assert_eq!(mode_of(dir.path(), conversation), "plan");
    let messages = client.messages(conversation).await.unwrap().messages;
    assert!(messages.iter().any(|m| m.author.as_deref() == Some("grace") && m.text.contains("Also a JSON export")), "B2: the feedback is in the conversation");
    // Its run ended without ExitPlanMode: the answer is the plan, so Plan review again.
    until(&client, id, ColumnKind::PlanReview, TaskStatus::Waiting).await;

    // Approve: Auto, the same conversation carries it out and reports (→ Review).
    client.ticket_command(Command::ApprovePlan(ApprovePlan { ticket_id: id, decision: PlanApproval::Approve, feedback: Some("Keep it small".into()) }), None).await.unwrap();
    assert_eq!(mode_of(dir.path(), conversation), "auto");
    let reported = until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    assert_eq!(reported.root_task.unwrap().conversation_id, Some(conversation), "the same conversation and session");
    assert_eq!(code(client.ticket_command(Command::ApprovePlan(ApprovePlan { ticket_id: id, decision: PlanApproval::Approve, feedback: None }), None).await), ErrorCode::Validation);
    // MU2-03: two plans handed in (cards with the main task), sent back once, approved once.
    let root = client.ticket(id).await.unwrap().root_task.map(|t| t.id);
    let lines = discussion_lines(&client, id).await;
    let plans: Vec<_> = lines.iter().filter(|(k, _, _)| *k == aurelius_domain::views::SystemLineKind::Plan).collect();
    assert_eq!(plans.len(), 2, "{lines:#?}");
    assert!(plans.iter().all(|(_, task, text)| *task == root && text == "Planner handed in a plan · waiting for a board member"), "{plans:#?}");
    let texts: Vec<&str> = lines.iter().map(|(_, _, t)| t.as_str()).collect();
    let at = |want: &str| texts.iter().position(|t| *t == want).unwrap_or_else(|| panic!("no “{want}” in {texts:#?}"));
    assert!(at("grace sent the plan back · Plan review → Doing") < at("grace approved the plan · Plan review → Doing"));
    assert!(at("grace approved the plan · Plan review → Doing") < at("Planner handed in the report · Review"));

    drop(client);
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_plan_mode_plan_review_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let client = common::connect(dir.path()).await;
    let (planner, atlas) = setup(&client).await;
    let id = planned_ticket(&client, atlas, planner, Some(AgentMode::Auto)).await;
    until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    let kinds: Vec<String> = {
        let db = common::db(dir.path());
        let mut stmt = db.prepare("SELECT type FROM events WHERE aggregate_type = 'ticket' AND aggregate_id = ?1").unwrap();
        stmt.query_map([id.to_string()], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    };
    assert!(!kinds.iter().any(|k| k == "ticket.plan_review"), "{kinds:?}");
    drop(client);
    common::stop(dir.path(), daemon).await;
}
