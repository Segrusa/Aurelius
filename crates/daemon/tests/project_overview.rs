//! The assistant of a project conversation knows the project's tasks: an overview
//! in every answer, and `list_tasks` / `read_task` for the details. It reads the
//! project's other conversations with `list_conversations` / `read_conversation`.

mod common;
use common::*;

use aurelius_client::{Client, bridge::BridgeClient, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, ProjectId, RunId, TaskId,
    conversation::{MessageRole, MessageStatus, ChatStart},
    employee::CreateEmployee,
    project::CreateProject,
    project_conversation::SendProjectMessage,
    task::CreateTask,
};
use aurelius_protocol::{CommandResult, EmployeeTaskOp};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, fake};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn fast() -> fake::FakeRuntime {
    fake::FakeRuntime { delay: Duration::from_millis(1), ..Default::default() }
}

/// The fake, keeping the system text and the turns of every run.
#[derive(Default)]
struct Recording {
    systems: Mutex<Vec<String>>,
    prompts: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl AgentRuntime for Recording {
    async fn probe(&self) -> RuntimeCapabilities {
        fast().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        self.systems.lock().unwrap().push(request.system.clone().unwrap_or_default());
        self.prompts.lock().unwrap().push(request.turns.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join("\n"));
        fast().start(request).await
    }
}

async fn employee(client: &Client, project_id: ProjectId, name: &str) -> EmployeeId {
    let create = CreateEmployee { access_level: None, description: None, name: name.into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None };
    match client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result {
        CommandResult::EmployeeCreated { employee_id, .. } => employee_id,
        other => panic!("{other:?}"),
    }
}

async fn task(client: &Client, project_id: ProjectId, employee_id: EmployeeId, title: &str) -> TaskId {
    let cmd = CreateTask { confirm_secret: false, files: vec![], run: Default::default(), project_id, employee_id: Some(employee_id), title: title.into(), goal: format!("Werk aan {title}"), acceptance_criteria: vec![], depends_on: vec![] };
    match client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result {
        CommandResult::TaskCreated { task_id } => task_id,
        other => panic!("{other:?}"),
    }
}

/// A message in a project conversation (a new one without `conversation_id`): its conversation and run.
async fn say(client: &Client, project_id: ProjectId, conversation_id: Option<ConversationId>, text: &str) -> (ConversationId, RunId) {
    let send = SendProjectMessage { confirm_secret: false, project_id, text: text.into(), conversation_id, images: Vec::new(), files: Vec::new(), interrupt: false, start: ChatStart::default() };
    match client.send_project_message(send).await.unwrap().result {
        CommandResult::ProjectMessageSent { conversation_id, run_id, .. } => (conversation_id, run_id),
        other => panic!("{other:?}"),
    }
}

async fn answered(client: &Client, conversation: ConversationId) {
    for _ in 0..400 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if messages.iter().any(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Complete) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no answer in {conversation}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_project_assistant_sees_and_reads_the_projects_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Arc::new(Recording::default());
    let daemon = start_with(dir.path(), runtime.clone()).await;
    let client = connect(dir.path()).await;
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    employee(&client, project_id, "Alex").await;
    let iris = employee(&client, project_id, "Iris").await;

    // Iris finishes a task in a conversation of her own.
    let dark = task(&client, project_id, iris, "Dark mode").await;
    let mut done = None;
    for _ in 0..600 {
        done = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == dark && t.status.is_terminal());
        if done.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let done = done.expect("the task ends");
    let report = done.result.clone().expect("a report");
    let first_line = report.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("# ")).unwrap().to_owned();

    // What is remembered of the task is drawn from its report too.
    let mut extracted = false;
    for _ in 0..400 {
        extracted = runtime.prompts.lock().unwrap().iter().any(|p| p.contains("Task completed: Dark mode\nReport (JSON-quoted):\n\"") && p.contains(&first_line));
        if extracted {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(extracted, "the extraction prompt carries the report: {:?}", runtime.prompts.lock().unwrap());

    // The assistant of a project conversation gets the overview with it.
    let (conversation_id, run) = say(&client, project_id, None, "Hoe staat het met dark mode?").await;
    answered(&client, conversation_id).await;
    let system = runtime.systems.lock().unwrap().iter().rev().find(|s| s.starts_with("You are the assistant of the project")).cloned().expect("the assistant answered");
    assert!(system.contains("## Tasks in this project"), "{system}");
    assert!(system.contains("“Dark mode” · succeeded · Iris"), "{system}");
    assert!(system.contains("Result:"), "the result's gist is there: {system}");
    // The task's own conversation has its task as context, not the overview.
    let iris_system = runtime.systems.lock().unwrap().iter().find(|s| s.contains("You are Iris")).cloned().unwrap();
    assert!(!iris_system.contains("## Tasks in this project"), "{iris_system}");

    // From that run, the assistant can list and read the task.
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let list = |status: &str, employee: &str, query: &str| EmployeeTaskOp::ListTasks { status: status.into(), employee: employee.into(), query: query.into(), project: None };
    let (ok, text) = bridge.employee_act(run, list("ended", "iris", "dark")).await.unwrap();
    assert!(ok && text.contains("Dark mode") && text.contains(&dark.to_string()[28..]), "{text}");
    let (_, text) = bridge.employee_act(run, list("open", "", "")).await.unwrap();
    assert_eq!(text, "No tasks match.");
    let (_, text) = bridge.employee_act(run, list("busy", "", "")).await.unwrap();
    assert!(text.starts_with("Unknown status"), "{text}");

    let (ok, text) = bridge.employee_act(run, EmployeeTaskOp::ReadTask { task: dark.to_string()[28..].into() }).await.unwrap();
    assert!(ok, "{text}");
    assert!(text.contains("Werk aan Dark mode") && text.contains("## Result"), "{text}");
    assert!(text.contains(&first_line), "the report is in it: {text}");
    assert!(text.contains("## Latest in the task's conversation"), "{text}");
    let (_, by_title) = bridge.employee_act(run, EmployeeTaskOp::ReadTask { task: "dark mode".into() }).await.unwrap();
    assert!(by_title.contains("Werk aan Dark mode"), "{by_title}");
    let (_, missing) = bridge.employee_act(run, EmployeeTaskOp::ReadTask { task: "Light mode".into() }).await.unwrap();
    assert!(missing.starts_with("No task in this project matches"), "{missing}");

    // It reads the project's other conversations, not the one it answers in.
    let (thread, thread_run) = say(&client, project_id, None, "Launch plan for March").await;
    answered(&client, thread).await;
    let (ok, text) = bridge.employee_act(run, EmployeeTaskOp::ListConversations { query: String::new() }).await.unwrap();
    assert!(ok && text.contains("“Launch plan for March”") && text.contains(&thread.to_string()[28..]), "{text}");
    assert!(!text.contains(&conversation_id.to_string()[28..]), "not its own current conversation: {text}");
    let (_, text) = bridge.employee_act(run, EmployeeTaskOp::ReadConversation { conversation: "launch plan".into() }).await.unwrap();
    assert!(text.contains("## Latest messages") && text.contains("[user · just now] Launch plan for March"), "{text}");
    let (_, text) = bridge.employee_act(run, EmployeeTaskOp::ReadConversation { conversation: conversation_id.to_string()[28..].into() }).await.unwrap();
    assert_eq!(text, "That is this conversation.");
    // From the other conversation, the first one is read.
    let (_, text) = bridge.employee_act(thread_run, EmployeeTaskOp::ReadConversation { conversation: conversation_id.to_string()[28..].into() }).await.unwrap();
    assert!(text.starts_with("# “Hoe staat het met dark mode?”") && text.contains("Hoe staat het met dark mode?"), "{text}");

    stop(dir.path(), daemon).await;
}
