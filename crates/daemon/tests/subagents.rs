//! While an employee's answer runs subagents, the employee shows them with what
//! they were asked and their latest step (the chip above the composer).

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, ProjectId, TaskId,
    conversation::SendChatMessage,
    employee::CreateEmployee,
    project::CreateProject,
    task::CreateTask,
};
use aurelius_protocol::{AgentView, CommandResult};
use aurelius_runtime::{Activity, AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use common::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Starts one subagent that reads a file, then waits until released.
struct WithSubagent {
    release: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl AgentRuntime for WithSubagent {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let release = self.release.clone();
        let answer = fake::answer_for(&request);
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            let agent = serde_json::json!({ "description": "Find the chip", "subagent_type": "Explore", "prompt": "Find where the branch chip is drawn.\nReport the file." });
            let _ = tx.send(RuntimeEvent::Activity(Activity::Started { call_id: "agent1".into(), tool: "Agent".into(), input: agent, parent: None })).await;
            for (call, tool, input) in [("s1", "Glob", serde_json::json!({ "pattern": "**/*.rs" })), ("s2", "Grep", serde_json::json!({ "pattern": "git_chip", "path": "C:/work/clients" }))] {
                let _ = tx.send(RuntimeEvent::Activity(Activity::Started { call_id: call.into(), tool: tool.into(), input, parent: Some("agent1".into()) })).await;
            }
            release.notified().await;
            let _ = tx.send(RuntimeEvent::Activity(Activity::Finished { call_id: "agent1".into(), ok: true, output: "chat.rs".into() })).await;
            let _ = tx.send(RuntimeEvent::TextDelta(answer)).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

async fn setup(client: &Client) -> (ProjectId, EmployeeId) {
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(
            Command::CreateEmployee(CreateEmployee { name: "Alex".into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None }),
            None,
        ))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    (project_id, employee_id)
}

/// A task for the employee; the conversation it is worked on in, once it exists.
async fn start_task(client: &Client, project_id: ProjectId, employee_id: EmployeeId) -> ConversationId {
    let cmd = CreateTask { confirm_secret: false, files: vec![], run: Default::default(), project_id, employee_id: Some(employee_id), title: "Find the chip".into(), goal: "Where is the chip?".into(), acceptance_criteria: vec![], depends_on: vec![] };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    task_conversation(client, task_id).await
}

async fn task_conversation(client: &Client, task: TaskId) -> ConversationId {
    for _ in 0..400 {
        if let Some(c) = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == task).and_then(|t| t.conversation_id) {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the task never got a conversation");
}

async fn subagents(client: &Client, project: ProjectId, employee: EmployeeId) -> Vec<AgentView> {
    let m = client.employees().await.unwrap().employees.into_iter().find(|m| m.id == employee).unwrap();
    m.agents.into_iter().filter(|a| a.kind == "subagent").collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_subagent_shows_what_it_was_asked_and_its_latest_step() {
    let dir = tempfile::tempdir().unwrap();
    let release = Arc::new(tokio::sync::Notify::new());
    let _daemon = start_with(dir.path(), Arc::new(WithSubagent { release: release.clone() })).await;
    let client = connect(dir.path()).await;
    let (project_id, alex) = setup(&client).await;
    let thread = start_task(&client, project_id, alex).await;

    let mut seen = Vec::new();
    for _ in 0..400 {
        seen = subagents(&client, project_id, alex).await;
        if seen.first().is_some_and(|a| a.steps == 2) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let [agent] = &seen[..] else { panic!("one subagent: {seen:?}") };
    assert_eq!((agent.name.as_str(), agent.detail.as_str(), agent.conversation_id), ("Find the chip", "Explore", Some(thread)));
    assert_eq!(agent.prompt.as_deref(), Some("Find where the branch chip is drawn.\nReport the file."));
    assert_eq!((agent.doing.as_deref(), agent.steps), (Some("Grep · git_chip"), 2));

    // Once it reports back, it no longer shows.
    release.notify_one();
    for _ in 0..400 {
        if subagents(&client, project_id, alex).await.is_empty() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the subagent still shows after it finished");
}

async fn own(client: &Client, conversation: ConversationId) -> Vec<AgentView> {
    client.subagents(conversation).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn each_conversation_sees_only_its_own_subagents() {
    let dir = tempfile::tempdir().unwrap();
    let release = Arc::new(tokio::sync::Notify::new());
    let _daemon = start_with(dir.path(), Arc::new(WithSubagent { release: release.clone() })).await;
    let client = connect(dir.path()).await;
    let (project_id, alex) = setup(&client).await;
    let idle = match client
        .execute(envelope(Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "Hello".into(), images: Vec::new(), interrupt: false }), None))
        .await
        .unwrap()
        .result
    {
        CommandResult::ChatMessageSent { conversation_id, .. } => conversation_id,
        other => panic!("{other:?}"),
    };
    // Nothing runs in the plain chat besides its own subagent; the task starts later.
    for _ in 0..400 {
        if own(&client, idle).await.first().is_some_and(|a| a.steps == 2) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let [chat_agent] = &own(&client, idle).await[..] else { panic!("one in the plain chat") };
    assert_eq!((chat_agent.name.as_str(), chat_agent.conversation_id), ("Find the chip", Some(idle)));

    let thread = start_task(&client, project_id, alex).await;
    for _ in 0..400 {
        if own(&client, thread).await.first().is_some_and(|a| a.steps == 2) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let mine = own(&client, thread).await;
    assert_eq!(mine.len(), 1, "{mine:?}");
    assert_eq!(mine[0].conversation_id, Some(thread));
    let theirs = own(&client, idle).await;
    assert_eq!(theirs.len(), 1, "{theirs:?}");
    assert_eq!(theirs[0].conversation_id, Some(idle));
    // An unknown conversation has none.
    assert!(own(&client, ConversationId(uuid::Uuid::from_u128(99))).await.is_empty());
    release.notify_waiters();
}
