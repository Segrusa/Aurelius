//! Per-agent working modes: Plan / Ask / Accept edits / Auto.

mod common;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, RunId,
    approval::{ApprovalStatus, DecideApproval},
    autonomy::{AgentMode, SetAgentMode},
    conversation::{MessageRole, MessageStatus, SendChatMessage},
    employee::CreateEmployee,
    project::CreateProject,
    task::CreateTask,
};
use aurelius_protocol::{ApprovalView, CommandResult, MessageView};
use common::*;
use std::time::Duration;

async fn chat(client: &Client, id: Option<ConversationId>, text: &str) -> (ConversationId, RunId) {
    match client.execute(envelope(Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: match id { Some(id) => id, None => common::conversation(client).await }, text: text.into(), images: Vec::new(), interrupt: false }), None)).await.unwrap().result {
        CommandResult::ChatMessageSent { conversation_id, run_id, .. } => (conversation_id, run_id),
        other => panic!("{other:?}"),
    }
}

async fn answer(client: &Client, id: ConversationId, n: usize) -> MessageView {
    for _ in 0..400 {
        let done: Vec<MessageView> =
            client.messages(id).await.unwrap().messages.into_iter().filter(|m| m.role == MessageRole::Assistant && m.status != MessageStatus::Partial).collect();
        if done.len() >= n {
            return done[n - 1].clone();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no answer");
}

async fn pending(client: &Client) -> Option<ApprovalView> {
    for _ in 0..40 {
        if let Some(a) = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.status == ApprovalStatus::Pending) {
            return Some(a);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    None
}

async fn set_mode(client: &Client, conversation_id: ConversationId, mode: AgentMode) {
    client.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id, mode }), None)).await.unwrap();
}

/// A chat run that stays alive while the test asks permissions (slow fake runtime).
async fn live_chat(client: &Client, root: &std::path::Path) -> (ConversationId, RunId) {
    let (id, run) = chat(client, None, "hallo").await;
    for _ in 0..200 {
        let status: String = db(root).query_row("SELECT status FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).unwrap();
        if matches!(status.as_str(), "starting" | "running" | "waiting") {
            return (id, run);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the run never started");
}

#[tokio::test(flavor = "multi_thread")]
async fn modes_decide_what_needs_asking() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 150).await;
    let client = connect(dir.path()).await;
    let (id, run) = live_chat(&client, dir.path()).await;

    // Default is Ask: a command waits for the user, who can refuse it.
    assert_eq!(client.messages(id).await.unwrap().agent_mode, AgentMode::Ask);
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let asked = tokio::spawn(async move { bridge.permission_ask(run, Some("c1".into()), "Bash".into(), serde_json::json!({ "command": "dir" })).await.unwrap() });
    let approval = pending(&client).await.expect("ask mode asks");
    assert_eq!(approval.tool, "claude.Bash");
    let reject = DecideApproval { approval_id: approval.id, action_digest: approval.action_digest.clone(), approve: false, always_allow: false };
    client.execute(envelope(Command::DecideApproval(reject), None)).await.unwrap();
    assert!(!asked.await.unwrap().allow);

    // Auto: the work goes ahead without a question.
    set_mode(&client, id, AgentMode::Auto).await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let auto = tokio::time::timeout(Duration::from_secs(5), bridge.permission_ask(run, Some("c2".into()), "Bash".into(), serde_json::json!({ "command": "dir" }))).await.unwrap().unwrap();
    assert!(auto.allow, "{auto:?}");
    assert!(pending(&client).await.is_none(), "Auto asked nothing");

    // Back to Ask: it holds at once.
    set_mode(&client, id, AgentMode::Ask).await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let asked = tokio::spawn(async move { bridge.permission_ask(run, Some("c3".into()), "Bash".into(), serde_json::json!({ "command": "dir" })).await.unwrap() });
    let approval = pending(&client).await.expect("ask mode asks again");
    let approve = DecideApproval { approval_id: approval.id, action_digest: approval.action_digest.clone(), approve: true, always_allow: false };
    client.execute(envelope(Command::DecideApproval(approve), None)).await.unwrap();
    assert!(asked.await.unwrap().allow);

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_conversation_works_in_the_employees_mode() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Alex".into(), role: "Onderzoeker".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    set_mode(&client, conversation_id, AgentMode::Plan).await;
    let cmd = CreateTask {
        confirm_secret: false,
        project_id,
        employee_id: Some(employee_id),
        title: "Onboarding".into(),
        goal: "Vergelijk ideeën".into(),
        acceptance_criteria: vec![],
        depends_on: vec![],
        run: Default::default(),
        files: vec![],
    };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    let mut conversation = None;
    for _ in 0..400 {
        conversation = task_view(&client, task_id).await.conversation_id;
        if conversation.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let conversation = conversation.expect("the task's conversation");
    assert_eq!(client.messages(conversation).await.unwrap().agent_mode, AgentMode::Plan, "the task's conversation has the employee's mode");
    stop(dir.path(), daemon).await;
}

async fn task_view(client: &Client, id: aurelius_domain::TaskId) -> aurelius_protocol::TaskView {
    client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == id).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_is_recorded_per_run() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (id, _) = chat(&client, None, "Hoeveel tokens kost dit?").await;
    answer(&client, id, 1).await;
    let mut usage = client.usage().await.unwrap();
    for _ in 0..40 {
        if usage.windows[0].runs > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        usage = client.usage().await.unwrap();
    }
    let today = &usage.windows[0];
    assert_eq!(today.label, "Today");
    assert_eq!(today.runs, 1);
    assert!(today.input_tokens > 0 && today.output_tokens > 0);
    assert_eq!(today.cost_micro_usd, Some(0));
    assert_eq!(usage.by_source[0].connection, "Local test runtime");
    assert_eq!(usage.by_day.len(), 14);
    assert!(usage.by_day.last().unwrap().tokens > 0);
    stop(dir.path(), daemon).await;
}

/// The browser (Claude in Chrome) shares the user's logins: off until the user
/// turns it on, and the setting is part of what every client reads.
#[tokio::test(flavor = "multi_thread")]
async fn the_browser_is_off_until_the_user_allows_it() {
    use aurelius_domain::autonomy::SetBrowserAllowed;
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (id, _) = chat(&client, None, "hallo").await;
    assert!(!client.messages(id).await.unwrap().browser_allowed, "off by default");
    let set = |allowed| client.execute(envelope(Command::SetBrowserAllowed(SetBrowserAllowed { conversation_id: id, allowed }), None));
    assert!(matches!(set(true).await.unwrap().result, CommandResult::BrowserAllowedSet { allowed: true, .. }));
    assert!(client.messages(id).await.unwrap().browser_allowed);
    assert!(matches!(set(false).await.unwrap().result, CommandResult::BrowserAllowedSet { allowed: false, .. }));
    assert!(!client.messages(id).await.unwrap().browser_allowed);
    // An unknown conversation is an error, not a silent no-op.
    let missing = client.execute(envelope(Command::SetBrowserAllowed(SetBrowserAllowed { conversation_id: ConversationId::new(), allowed: true }), None)).await;
    assert!(missing.is_err());
    stop(dir.path(), daemon).await;
}

