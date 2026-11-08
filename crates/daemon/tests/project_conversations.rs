//! Project conversations: made, listed and continued through the protocol; the
//! assistant of a project conversation sees the project's employees and tasks,
//! gives work to an employee (the result comes back in the conversation), cannot
//! hand over, and uses ideas and the project's memory.

mod common;
use common::*;

use aurelius_client::{Client, bridge::BridgeClient, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, ProjectId,
    conversation::{MessageRole, MessageStatus, ChatStart},
    employee::CreateEmployee,
    memory::EmployeeMemoryOp,
    project::CreateProject,
    project_conversation::{SendProjectMessage, StartProjectConversation},
};
use aurelius_protocol::{CommandResult, EmployeeTaskOp, MessageView};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, fake};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn fast() -> fake::FakeRuntime {
    fake::FakeRuntime { delay: Duration::from_millis(1), ..Default::default() }
}

/// The fake, keeping the system text of every run.
#[derive(Default)]
struct Recording {
    systems: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl AgentRuntime for Recording {
    async fn probe(&self) -> RuntimeCapabilities {
        fast().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        self.systems.lock().unwrap().push(request.system.clone().unwrap_or_default());
        fast().start(request).await
    }
}

async fn employee(client: &Client, project_id: ProjectId, name: &str) -> EmployeeId {
    match client.execute(envelope(Command::CreateEmployee(CreateEmployee { description: None, name: name.into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None }), None)).await.unwrap().result {
        CommandResult::EmployeeCreated { employee_id, .. } => employee_id,
        other => panic!("{other:?}"),
    }
}

fn send(project_id: ProjectId, conversation_id: Option<ConversationId>, text: &str) -> SendProjectMessage {
    SendProjectMessage { confirm_secret: false, project_id, text: text.into(), conversation_id, images: Vec::new(), files: Vec::new(), interrupt: false, start: ChatStart::default() }
}

/// Wait until `count` answers in the conversation are complete.
async fn answers(client: &Client, conversation: ConversationId, count: usize) -> Vec<MessageView> {
    for _ in 0..600 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if messages.iter().filter(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Complete).count() >= count {
            return messages;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no {count} answers in {conversation}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_conversation_is_made_listed_and_continued() {
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
    assert!(client.project_conversations(project_id).await.unwrap().conversations.is_empty());

    // An empty conversation exists at once; its first message names it.
    let CommandResult::ProjectConversationStarted { project_id: started_in, conversation_id } = client
        .start_project_conversation(StartProjectConversation { project_id, start: ChatStart::default() })
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    assert_eq!(started_in, project_id);
    let listed = client.project_conversations(project_id).await.unwrap();
    assert_eq!(listed.project_id, project_id);
    assert_eq!(listed.conversations.len(), 1);
    let first = &listed.conversations[0];
    assert!((first.id, first.message_count, first.working) == (conversation_id, 0, false), "{first:?}");

    let CommandResult::ProjectMessageSent { conversation_id: same, .. } = client.send_project_message(send(project_id, Some(conversation_id), "Wat staat er open?")).await.unwrap().result else { panic!() };
    assert_eq!(same, conversation_id);
    answers(&client, conversation_id, 1).await;
    let CommandResult::ProjectMessageSent { .. } = client.send_project_message(send(project_id, Some(conversation_id), "En wie werkt waaraan?")).await.unwrap().result else { panic!() };
    let messages = answers(&client, conversation_id, 2).await;
    assert_eq!(messages.len(), 4, "two questions, two answers");

    // Without a conversation a new one is made; the list is most recent first.
    let CommandResult::ProjectMessageSent { conversation_id: second, .. } = client.send_project_message(send(project_id, None, "Een ander onderwerp")).await.unwrap().result else { panic!() };
    assert_ne!(second, conversation_id);
    answers(&client, second, 1).await;
    let listed = client.project_conversations(project_id).await.unwrap().conversations;
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, second);
    assert_eq!(listed[1].title, "Wat staat er open?");
    assert_eq!(listed[1].message_count, 4);

    // Other projects do not see them.
    let CommandResult::ProjectCreated { project_id: other, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Other".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    assert!(client.project_conversations(other).await.unwrap().conversations.is_empty());
    assert!(client.send_project_message(send(other, Some(conversation_id), "Verkeerd project")).await.is_err());
    assert!(client.send_project_message(send(project_id, Some(ConversationId::new()), "Onbekend")).await.is_err());

    // The assistant is told who it is, and who it can give work to.
    let system = runtime.systems.lock().unwrap().iter().find(|s| s.contains("assistant of the project")).cloned().expect("the assistant answered");
    assert!(system.starts_with("You are the assistant of the project “Atlas”."), "{system}");
    assert!(system.contains("- @Alex (Developer)") && system.contains("delegate_task"), "{system}");
    assert!(!system.starts_with("You are Alex"), "{system}");

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_assistant_gives_work_to_an_employee_and_the_result_comes_back() {
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
    backlog_ticket(&client, project_id, "Dark mode").await;
    let CommandResult::ProjectMessageSent { conversation_id, run_id, .. } = client.send_project_message(send(project_id, None, "Laat Alex dark mode bouwen")).await.unwrap().result else { panic!() };
    answers(&client, conversation_id, 1).await;

    // The assistant's tools (the bridge calls these with the run).
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let (ok, text) = bridge.employee_act(run_id, EmployeeTaskOp::Delegate { employee: "Alex".into(), title: "Dark mode".into(), goal: "Maak het donkere thema".into(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(ok && text.contains("Gave “Dark mode” to Alex"), "{text}");
    let (ok, _) = bridge.employee_act(run_id, EmployeeTaskOp::Delegate { employee: "Nobody".into(), title: "x".into(), goal: String::new(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(!ok, "no such employee");
    let (ok, text) = bridge.employee_act(run_id, EmployeeTaskOp::HandOver { employee: "Alex".into(), reason: "x".into() }).await.unwrap();
    assert!(!ok && text.contains("delegate_task"), "the assistant cannot hand over: {text}");

    // Alex does it as a task of his own, given by the project assistant.
    let mut task = None;
    for _ in 0..600 {
        task = client.tasks(Some(project_id)).await.unwrap().tasks.into_iter().find(|t| t.title == "Dark mode" && t.status.is_terminal());
        if task.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let task = task.expect("Alex' task ends");
    assert_eq!(task.employee_name.as_deref(), Some("Alex"));
    assert_eq!(task.delegated_by.as_deref(), Some("Project assistant"));
    assert_ne!(task.conversation_id, Some(conversation_id));

    // Its result is in the project conversation, and the assistant answers it.
    let mut messages = Vec::new();
    for _ in 0..600 {
        messages = client.messages(conversation_id).await.unwrap().messages;
        let found = messages.iter().position(|m| m.update.as_deref() == Some("finding"));
        if found.is_some_and(|f| messages[f + 1..].iter().any(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Complete)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let found = messages.iter().position(|m| m.update.as_deref() == Some("finding")).unwrap_or_else(|| panic!("{messages:#?}"));
    assert!(messages[found].text.starts_with("Finished “Dark mode”") && messages[found].author.as_deref() == Some("Alex"), "{:?}", messages[found]);
    assert!(messages[found + 1..].iter().any(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Complete), "the assistant answers the result: {messages:#?}");

    // It reads the project: tasks (and the task of its own delegation) and its conversations.
    let (ok, text) = bridge.employee_act(run_id, EmployeeTaskOp::ListTasks { status: "ended".into(), employee: "alex".into(), query: String::new(), project: None }).await.unwrap();
    assert!(ok && text.contains("Dark mode"), "{text}");
    let CommandResult::ProjectMessageSent { conversation_id: second, .. } = client.send_project_message(send(project_id, None, "Tweede gesprek over lanceren")).await.unwrap().result else { panic!() };
    answers(&client, second, 1).await;
    let (ok, text) = bridge.employee_act(run_id, EmployeeTaskOp::ListConversations { query: String::new() }).await.unwrap();
    assert!(ok && text.contains("“Tweede gesprek over lanceren”") && !text.contains(&conversation_id.to_string()[28..]), "{text}");
    let (_, text) = bridge.employee_act(run_id, EmployeeTaskOp::ReadConversation { conversation: "lanceren".into() }).await.unwrap();
    assert!(text.contains("[user · ") && text.contains("Tweede gesprek over lanceren"), "{text}");

    // Ideas and the project's memory.
    let (ok, text) = bridge.employee_act(run_id, EmployeeTaskOp::AddIdea { title: "Thema per gebruiker".into(), note: "Kwam op tijdens dark mode".into(), kind: Default::default(), project: None }).await.unwrap();
    assert!(ok, "{text}");
    let ideas = client.ideas(project_id).await.unwrap().ideas;
    assert!(ideas.iter().any(|i| i.title == "Thema per gebruiker" && i.employee_id.is_none()), "{ideas:?}");
    let (ok, text) = bridge.memory_run(run_id, EmployeeMemoryOp::Remember { kind: "decision".into(), scope: "employee".into(), content: "We bewaren alle data lokaal in SQLite".into() }).await.unwrap();
    assert!(ok && text == "Remembered.", "{text}");
    let (ok, text) = bridge.memory_run(run_id, EmployeeMemoryOp::Recall { query: "sqlite opslag data".into() }).await.unwrap();
    assert!(ok && text.contains("SQLite") && text.contains("project"), "it went to the project's memory: {text}");

    stop(dir.path(), daemon).await;
}
