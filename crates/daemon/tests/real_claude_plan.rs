//! End to end with the real Claude Code CLI (it uses the user's own Claude
//! login and a few tokens, so this test is ignored by default). An employee
//! works on a task in plan mode and hands in a plan; it reaches the inbox
//! through the real approval bridge (`aurelius-bridge`); the user decides; the
//! work follows, or stops.
//!
//! Run with: cargo test -p aurelius-daemon --test real_claude_plan -- --ignored --nocapture
//! (build `aurelius-bridge` first: `cargo build -p aurelius-agent-bridge`, same target directory.)

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    ApprovalId, Command, ConversationId, EmployeeId, ProjectId, TaskId,
    approval::{ApprovalStatus, DecidePlan, PlanDecision},
    autonomy::AgentMode,
    connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults},
    conversation::{MessageRole, MessageStatus},
    employee::{CreateEmployee, Setting},
    project::CreateProject,
    task::{CreateTask, RunSettings},
};
use aurelius_protocol::{ApprovalView, CommandResult, Method, ResponseBody};
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
            Command::CreateEmployee(CreateEmployee { name: "Alex".into(), role: "Builder".into(), title: None, reports_to: None, instructions: None, folder: None }),
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

/// A task that starts in plan mode.
async fn plan_task(client: &Client, employee: EmployeeId, project: ProjectId, goal: &str) -> TaskId {
    let cmd = CreateTask {
        confirm_secret: false,
        files: vec![],
        run: RunSettings { mode: Some(AgentMode::Plan), ..Default::default() },
        project_id: project,
        employee_id: Some(employee),
        title: "Hello file".into(),
        goal: goal.into(),
        acceptance_criteria: vec![],
        depends_on: vec![],
    };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    task_id
}

async fn pending_plan(client: &Client, not: &[ApprovalId]) -> ApprovalView {
    for _ in 0..1200 {
        if let Some(a) = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.status == ApprovalStatus::Pending && a.tool == "claude.ExitPlanMode" && !not.contains(&a.id)) {
            return a;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("Claude never handed in a plan");
}

async fn decide(client: &Client, id: ApprovalId, decision: PlanDecision, feedback: Option<&str>) {
    client.execute(envelope(Command::DecidePlan(DecidePlan { confirm_secret: false, approval_id: id, decision, feedback: feedback.map(str::to_owned) }), None)).await.unwrap();
}

/// The task's conversation and the text of its last finished answer, once the task has ended.
async fn final_answer(client: &Client, task: TaskId) -> (ConversationId, String) {
    for _ in 0..1800 {
        let found = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == task).filter(|t| t.status.is_terminal());
        if let Some(conversation) = found.and_then(|t| t.conversation_id) {
            let messages = client.messages(conversation).await.unwrap().messages;
            if let Some(m) = messages.iter().rev().find(|m| m.role == MessageRole::Assistant && m.update.is_none() && m.status != MessageStatus::Partial) {
                return (conversation, m.text.clone());
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("no final answer");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real Claude Code CLI"]
async fn a_real_plan_is_approved_after_a_revision_and_then_carried_out() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let client = connect(dir.path()).await;
    let (employee, project, folder) = setup(&client).await;

    let task = plan_task(
        &client,
        employee,
        project,
        "Plan how to create a file named hello.txt containing exactly the word hi in your working folder. \
         Keep the plan to three lines. Present it with ExitPlanMode. After it is approved, create the file, then reply DONE.",
    )
    .await;

    // The plan reaches the inbox as text, through the real bridge.
    let first = pending_plan(&client, &[]).await;
    let text = first.arguments["plan"].as_str().expect("the plan text");
    println!("PLAN 1:\n{text}");
    assert!(!text.trim().is_empty());
    assert!(!folder.join("hello.txt").exists(), "nothing runs before approval");

    // Ask for changes: Claude gets the feedback and hands in a revised plan.
    decide(&client, first.id, PlanDecision::Revise, Some("Also mention in the plan that the file ends with a newline.")).await;
    let second = pending_plan(&client, &[first.id]).await;
    let revised = second.arguments["plan"].as_str().expect("the revised plan");
    println!("PLAN 2:\n{revised}");
    assert!(!folder.join("hello.txt").exists(), "still nothing before approval");

    // Approve: the employee carries it out (in auto mode, so without asking again).
    decide(&client, second.id, PlanDecision::Approve, None).await;
    let (_, answer) = final_answer(&client, task).await;
    println!("ANSWER: {answer}");
    let content = std::fs::read_to_string(folder.join("hello.txt")).expect("hello.txt was created after approval");
    assert!(content.trim() == "hi", "{content:?}");
    let asked_again: Vec<_> = client.approvals().await.unwrap().approvals.into_iter().filter(|a| a.status == ApprovalStatus::Pending).collect();
    assert!(asked_again.is_empty(), "auto mode after approval: nothing else asked, got {asked_again:?}");

    stop(dir.path(), daemon).await;
}
