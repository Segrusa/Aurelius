//! End to end with the real Claude Code CLI (it uses the user's own Claude
//! login and a few tokens, so these tests are ignored by default). An employee
//! starts a task in plan mode; the user switches the mode of the task's
//! conversation while it works; the same answer goes on in the new mode,
//! instead of only the next one.
//!
//! Run with: cargo test -p aurelius-daemon --test real_claude_mode_switch -- --ignored --nocapture
//! (build `aurelius-bridge` first: `cargo build -p aurelius-agent-bridge`, same target directory.)

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, ProjectId, TaskId,
    approval::ApprovalStatus,
    autonomy::{AgentMode, SetAgentMode},
    connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults},
    conversation::{MessageRole, MessageStatus},
    employee::{CreateEmployee, Setting},
    project::CreateProject,
    task::{CreateTask, RunSettings},
};
use aurelius_protocol::{CommandResult, Method, ResponseBody};
use common::*;
use std::path::{Path, PathBuf};
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
    for _ in 0..200 {
        let ResponseBody::Connections(snapshot) = client.call(Method::ProviderList).await.unwrap() else { panic!() };
        let connection = snapshot.connections.iter().find(|c| c.id == connection_id).expect("the connection exists");
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
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Modes".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let folder = PathBuf::from(client.projects().await.unwrap().projects.into_iter().find(|p| p.id == project_id).unwrap().folder.expect("project folder"));
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(
            Command::CreateEmployee(CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Alex".into(), role: "Builder".into(), title: None, reports_to: None, instructions: None, folder: None }),
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

async fn set_mode(client: &Client, conversation_id: ConversationId, mode: AgentMode) {
    client.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id, mode }), None)).await.unwrap();
}

async fn answers(client: &Client, conversation: ConversationId) -> Vec<aurelius_protocol::MessageView> {
    client.messages(conversation).await.unwrap().messages.into_iter().filter(|m| m.role == MessageRole::Assistant && m.update.is_none()).collect()
}

/// The conversation a task is worked on in, once it exists.
async fn task_conversation(client: &Client, task: TaskId) -> ConversationId {
    for _ in 0..600 {
        if let Some(c) = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == task).and_then(|t| t.conversation_id) {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the task never got a conversation");
}

/// A task that starts in plan mode, switch to `mode` as soon as its answer runs, and wait for that one answer.
async fn switch_during(client: &Client, employee: EmployeeId, project: ProjectId, folder: &Path, file: &str, mode: AgentMode) -> String {
    for i in 1..=6 {
        std::fs::write(folder.join(format!("f{i}.txt")), format!("file {i}")).unwrap();
    }
    let goal = format!(
        "Read f1.txt, f2.txt, f3.txt, f4.txt, f5.txt and f6.txt in your working folder, one Read call each, in order. \
         Then, if your current mode allows editing files, create {file} there containing exactly the word hi with the Write tool; \
         if it does not, say so. Do not call ExitPlanMode and do not ask questions."
    );
    let cmd = CreateTask {
        confirm_secret: false,
        files: vec![],
        run: RunSettings { mode: Some(AgentMode::Plan), ..Default::default() },
        project_id: project,
        employee_id: Some(employee),
        title: format!("Switch for {file}"),
        goal,
        acceptance_criteria: vec![],
    };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    let conversation = task_conversation(client, task_id).await;
    // The answer runs (its message is still partial): switch now.
    for _ in 0..600 {
        if answers(client, conversation).await.iter().any(|m| m.status == MessageStatus::Partial) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    set_mode(client, conversation, mode).await;
    for _ in 0..1800 {
        let done = answers(client, conversation).await;
        if let Some(m) = done.first().filter(|m| m.status != MessageStatus::Partial) {
            assert_eq!(done.len(), 1, "one answer, not a second run");
            return m.text.clone();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("no final answer");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real Claude Code CLI"]
async fn a_mode_switch_reaches_the_running_answer() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let client = connect(dir.path()).await;
    let (employee, project, folder) = setup(&client).await;

    // Plan → Accept edits: Claude Code's own mode lets the write through.
    let answer = switch_during(&client, employee, project, &folder, "edits.txt", AgentMode::AcceptEdits).await;
    println!("ACCEPT EDITS: {answer}");
    assert_eq!(std::fs::read_to_string(folder.join("edits.txt")).expect("edits.txt was written in the same answer").trim(), "hi");

    // Plan → Auto: the session leaves plan mode; the bridge adds Auto's rules on the write.
    let answer = switch_during(&client, employee, project, &folder, "auto.txt", AgentMode::Auto).await;
    println!("AUTO: {answer}");
    assert_eq!(std::fs::read_to_string(folder.join("auto.txt")).expect("auto.txt was written in the same answer").trim(), "hi");

    let pending: Vec<_> = client.approvals().await.unwrap().approvals.into_iter().filter(|a| a.status == ApprovalStatus::Pending).collect();
    assert!(pending.is_empty(), "nothing asked after the switch, got {pending:?}");
    stop(dir.path(), daemon).await;
}
