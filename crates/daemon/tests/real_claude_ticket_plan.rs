//! End to end with the real Claude Code CLI (it uses the user's own Claude
//! login and a few tokens, so this test is ignored by default): the main task
//! of a ticket in Plan mode hands in its plan through the real bridge; it is
//! no inbox approval, the deny tells Claude to stop, and the run must really
//! end there (nothing is carried out) with the card in Plan review. Approved,
//! the same session carries the plan out and reports (Review).
//!
//! Run with: cargo test -p aurelius-daemon --test real_claude_ticket_plan -- --ignored --nocapture
//! (build `aurelius-bridge` first: `cargo build -p aurelius-agent-bridge`, same target directory.)

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, EmployeeId, ProjectId, TicketId,
    approval::ApprovalStatus,
    autonomy::AgentMode,
    connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults},
    conversation::MessageRole,
    employee::{CreateEmployee, Setting},
    project::CreateProject,
    task::{RunSettings, TaskStatus},
    ticket::{ApprovePlan, ColumnKind, CreateTicket, PlanApproval, TicketDetail, TicketPriority},
};
use aurelius_protocol::{CommandResult, Method, ResponseBody};
use common::*;
use std::path::PathBuf;
use std::time::Duration;

/// A project folder, an employee, and Claude Code as the connection for everything.
async fn setup(client: &Client) -> (EmployeeId, ProjectId, PathBuf) {
    let claude = aurelius_runtime::claude_code::locate().expect("Claude Code is installed");
    let CommandResult::ProviderAdded { connection_id } = client
        .execute(envelope(
            Command::AddProvider(AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(claude.display().to_string()), default_model: None }),
            None,
        ))
        .await
        .unwrap()
        .result
    else {
        panic!("connection not added")
    };
    // Wait for the probe: ready means installed and signed in.
    for _ in 0..200 {
        let ResponseBody::Connections(snapshot) = client.call(Method::ProviderList).await.unwrap() else { panic!() };
        let connection = snapshot.connections.iter().find(|c| c.id == connection_id).expect("the connection exists");
        // "needs_auth" is also the status before the probe has run: judge it only once verified.
        match (connection.status.as_str(), connection.last_verified_ms) {
            ("ready", _) => break,
            ("needs_auth" | "unavailable", Some(_)) => panic!("Claude Code is not usable: {connection:?}"),
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    for purpose in [Purpose::Employee, Purpose::Memory] {
        let settings = RunDefaults { connection: Setting::Value(connection_id), model: Setting::Value("haiku".into()), persona: Setting::Inherit, effort: Setting::Inherit };
        client.execute(envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose, settings }), None)).await.unwrap();
    }
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Plans".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let folder = PathBuf::from(client.projects().await.unwrap().projects.into_iter().find(|p| p.id == project_id).unwrap().folder.expect("project folder"));
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(
            Command::CreateEmployee(CreateEmployee { access_level: None, description: None, name: "Alex".into(), role: "Builder".into(), title: None, reports_to: None, instructions: None, folder: None }),
            None,
        ))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    (employee_id, project_id, folder)
}

async fn until(client: &Client, id: TicketId, column: ColumnKind, status: TaskStatus) -> TicketDetail {
    for _ in 0..1800 {
        let detail = client.ticket(id).await.unwrap();
        if detail.ticket.column == column && detail.root_task.as_ref().map(|t| t.status) == Some(status) {
            return detail;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the ticket never reached {column:?}/{status:?}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real Claude Code CLI"]
async fn a_real_ticket_plan_stops_after_the_deny_and_goes_on_once_approved() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let client = connect_as(dir.path(), "grace").await;
    let (employee, project, folder) = setup(&client).await;
    let made = client
        .create_ticket(CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id: project,
            title: "Hello file".into(),
            description: Some(
                "Plan how to create a file named hello.txt containing exactly the word hi in your working folder. \
                 Keep the plan to three lines. Present it with ExitPlanMode. After it is approved, create the file, then reply DONE."
                    .into(),
            ),
            priority: TicketPriority::default(),
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: Some(employee),
            run: RunSettings { mode: Some(AgentMode::Plan), ..Default::default() },
            files: vec![],
            start: true,
            from_message: None,
            from_idea: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id: id, .. } = made.result else { panic!() };

    // The run hands in its plan, is told to stop, and ends: Plan review, nothing done.
    let review = until(&client, id, ColumnKind::PlanReview, TaskStatus::Waiting).await;
    let root = review.root_task.unwrap();
    let conversation = root.conversation_id.unwrap();
    let messages = client.messages(conversation).await.unwrap().messages;
    for m in &messages {
        println!("[{:?} {:?} {:?}] {}", m.role, m.update, m.author, m.text.chars().take(300).collect::<String>());
    }
    let handed_in = messages.iter().any(|m| m.update.as_deref() == Some("plan"));
    println!("PLAN HANDED IN WITH ExitPlanMode: {handed_in}");
    let runs: Vec<(String, String)> = {
        let db = db(dir.path());
        let mut stmt = db.prepare("SELECT id, status FROM runs WHERE conversation_id = ?1").unwrap();
        stmt.query_map([conversation.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap()
    };
    println!("RUNS AFTER THE DENY: {runs:?}");
    assert_eq!(runs.len(), 1, "one run, and it ended");
    assert_eq!(runs[0].1, "succeeded", "it ended on its own after the deny");
    assert!(!folder.join("hello.txt").exists(), "nothing was carried out before the decision");
    assert!(client.approvals().await.unwrap().approvals.iter().all(|a| !(a.status == ApprovalStatus::Pending && a.tool == "claude.ExitPlanMode")), "no inbox approval");
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(!folder.join("hello.txt").exists() && client.ticket(id).await.unwrap().ticket.column == ColumnKind::PlanReview, "it stays stopped");

    // Approved: the same session carries it out and reports.
    client.ticket_command(Command::ApprovePlan(ApprovePlan { ticket_id: id, decision: PlanApproval::Approve, feedback: None }), None).await.unwrap();
    until(&client, id, ColumnKind::Review, TaskStatus::Review).await;
    let content = std::fs::read_to_string(folder.join("hello.txt")).expect("hello.txt was created after approval");
    assert_eq!(content.trim(), "hi");
    let last = client.messages(conversation).await.unwrap().messages.into_iter().rev().find(|m| m.role == MessageRole::Assistant && m.update.is_none()).unwrap();
    println!("ANSWER AFTER APPROVAL: {}", last.text);

    stop(dir.path(), daemon).await;
}
