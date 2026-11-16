//! End to end with the real Claude Code CLI (it uses the user's own Claude
//! login and some tokens, so these tests are ignored by default): plain chats
//! work with tools in a folder of their own; a task is worked on in a
//! conversation, with subagents, ending with a report; an employee delegates a
//! part to a colleague and gets it back; the project assistant lets an employee
//! do something; a question while working is asked and answered in the task's
//! conversation. Files the agents name come along with their messages.
//!
//! Run with: cargo test -p aurelius-daemon --test real_claude_tasks -- --ignored --nocapture --test-threads 1
//! (build `aurelius-bridge` first: `cargo build -p aurelius-agent-bridge`, same target directory.)

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, ProjectId, TaskId,
    approval::{ApprovalStatus, DecideApproval},
    autonomy::{AgentMode, SetAgentMode},
    connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults},
    conversation::{MessageRole, MessageStatus, ChatStart, SendChatMessage},
    employee::{CreateEmployee, Setting},
    project::CreateProject,
    project_conversation::SendProjectMessage,
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, Method, ResponseBody, TaskView};
use common::*;
use std::time::Duration;

/// Claude Code (haiku) for everything.
async fn claude(client: &Client) {
    let path = aurelius_runtime::claude_code::locate().expect("Claude Code is installed");
    let CommandResult::ProviderAdded { connection_id } = client
        .execute(envelope(
            Command::AddProvider(AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(path.display().to_string()), default_model: None }),
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
        let c = snapshot.connections.iter().find(|c| c.id == connection_id).expect("the connection exists");
        match (c.status.as_str(), c.last_verified_ms) {
            ("ready", _) => break,
            ("needs_auth" | "unavailable", Some(_)) => panic!("Claude Code is not usable: {c:?}"),
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    for purpose in [Purpose::Employee, Purpose::Memory] {
        let settings = RunDefaults { connection: Setting::Value(connection_id), model: Setting::Value("haiku".into()), persona: Setting::Inherit, effort: Setting::Inherit };
        client.execute(envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose, settings }), None)).await.unwrap();
    }
}

async fn project(client: &Client) -> ProjectId {
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Real".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    project_id
}

async fn employee(client: &Client, project_id: ProjectId, name: &str, role: &str, mode: AgentMode) -> EmployeeId {
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client
        .execute(envelope(
            Command::CreateEmployee(CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: role.into(), title: None, reports_to: None, instructions: None, folder: None }),
            None,
        ))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    client.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id, mode }), None)).await.unwrap();
    employee_id
}

async fn task(client: &Client, project_id: ProjectId, employee_id: EmployeeId, title: &str, goal: &str) -> TaskId {
    let cmd = CreateTask { confirm_secret: false, files: vec![], project_id, employee_id: Some(employee_id), title: title.into(), goal: goal.into(), acceptance_criteria: vec![], run: Default::default() };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    task_id
}

async fn wait_task(client: &Client, id: TaskId, done: impl Fn(&TaskView) -> bool) -> TaskView {
    for _ in 0..3000 {
        if let Some(t) = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == id) {
            if done(&t) {
                return t;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("task did not reach the expected state: {:?}", client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == id));
}

fn print_conversation(messages: &[aurelius_protocol::MessageView]) {
    for m in messages {
        let who = m.author.clone().unwrap_or_else(|| format!("{:?}", m.role));
        println!("--- {who} {:?} {:?} files={:?}\n{}", m.status, m.update, m.files.iter().map(|f| &f.name).collect::<Vec<_>>(), m.text.chars().take(1200).collect::<String>());
    }
}

/// A subagent asked to run in the background still answers within the same
/// answer: the run ends at the first `result`, so background tasks are off.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real Claude Code CLI"]
async fn a_background_subagent_answers_before_the_answer_ends() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let client = connect(dir.path()).await;
    claude(&client).await;
    let send = |conversation_id, text: &str| {
        Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id, text: text.into(), images: Vec::new(), files: vec![], interrupt: false })
    };
    let CommandResult::ChatMessageSent { conversation_id, .. } = client.execute(envelope(send(common::conversation(&client).await, "Hallo"), None)).await.unwrap().result else { panic!() };
    wait_answers(&client, conversation_id, 1).await;
    client.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id, mode: AgentMode::Auto }), None)).await.unwrap();
    let ask = "Start exactly one subagent with the Agent tool, in the background (run_in_background true) if you can, and ask it: what is 17 times 23? \
               Then reply with only the number it gives back.";
    client.execute(envelope(send(conversation_id, ask), None)).await.unwrap();
    let messages = wait_answers(&client, conversation_id, 2).await;
    print_conversation(&messages);
    let answer = messages.iter().rev().find(|m| m.role == MessageRole::Assistant && m.update.is_none()).unwrap();
    assert!(answer.activity.iter().any(|a| a.tool == "Task" || a.tool == "Agent"), "a subagent was used: {:?}", answer.activity.iter().map(|a| &a.tool).collect::<Vec<_>>());
    assert!(answer.text.contains("391"), "the subagent's answer came back in the same answer: {}", answer.text);
    stop(dir.path(), daemon).await;
}

async fn wait_answers(client: &Client, conversation: ConversationId, n: usize) -> Vec<aurelius_protocol::MessageView> {
    for _ in 0..1200 {
        let messages = client.messages(conversation).await.unwrap().messages;
        let done = messages.iter().filter(|m| m.role == MessageRole::Assistant && m.update.is_none() && m.status != MessageStatus::Partial).count();
        if done >= n {
            return messages;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    print_conversation(&client.messages(conversation).await.unwrap().messages);
    for a in client.approvals().await.unwrap().approvals {
        println!("APPROVAL {:?} {} {}", a.status, a.tool, a.arguments);
    }
    panic!("no answer");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real Claude Code CLI"]
async fn a_task_is_done_in_its_conversation_with_a_subagent_and_a_report() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let client = connect(dir.path()).await;
    claude(&client).await;
    let project = project(&client).await;
    let alex = employee(&client, project, "Alex", "Researcher", AgentMode::Auto).await;
    let id = task(
        &client,
        project,
        alex,
        "Facts",
        "Use the Task tool to start exactly one subagent that writes a file facts.md with three short facts about Belgium in your working folder. \
         When it is done, finish with your report and name the file's full path.",
    )
    .await;
    let t = wait_task(&client, id, |t| t.status.is_terminal()).await;
    let conversation = t.conversation_id.expect("the task's conversation");
    let messages = client.messages(conversation).await.unwrap().messages;
    print_conversation(&messages);
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    let report = t.result.clone().unwrap();
    assert!(report.starts_with("# Facts"), "{report}");
    let answer = messages.iter().rev().find(|m| m.role == MessageRole::Assistant && m.update.is_none()).unwrap();
    assert!(answer.activity.iter().any(|a| a.tool == "Task" || a.tool == "Agent"), "a subagent was used: {:?}", answer.activity.iter().map(|a| &a.tool).collect::<Vec<_>>());
    assert!(answer.files.iter().any(|f| f.name == "facts.md"), "the report's file comes along: {:?}", answer.files);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real Claude Code CLI"]
async fn a_employee_delegates_to_a_colleague_and_reports_when_it_comes_back() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let client = connect(dir.path()).await;
    claude(&client).await;
    let project = project(&client).await;
    let alex = employee(&client, project, "Alex", "Backend developer", AgentMode::Auto).await;
    let iris = employee(&client, project, "Iris", "Frontend designer", AgentMode::Auto).await;
    let id = task(
        &client,
        project,
        alex,
        "Site",
        "Delegate one part to your colleague Iris with the delegate_task tool: title 'Colors', goal 'Write colors.css with one CSS rule that makes the body background white, then report'. \
         Write README.md with one line yourself. When Iris's result has come back, finish with your report.",
    )
    .await;
    let t = wait_task(&client, id, |t| t.status.is_terminal()).await;
    let messages = client.messages(t.conversation_id.unwrap()).await.unwrap().messages;
    print_conversation(&messages);
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    assert_eq!(t.children.len(), 1, "Alex delegated one part");
    let child = client.tasks(None).await.unwrap().tasks.into_iter().find(|c| c.id == t.children[0]).unwrap();
    assert_eq!((child.employee_id, child.status), (Some(iris), TaskStatus::Succeeded), "{child:?}");
    assert!(messages.iter().any(|m| m.update.as_deref() == Some("finding") && m.author.as_deref() == Some("Iris")), "Iris's result came back");
    let last = messages.last().unwrap();
    assert!(last.role == MessageRole::Assistant && last.update.is_none(), "Alex finished after Iris");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real Claude Code CLI"]
async fn from_a_project_conversation_the_assistant_lets_an_employee_do_it() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let client = connect(dir.path()).await;
    claude(&client).await;
    let project = project(&client).await;
    employee(&client, project, "Alex", "Backend developer", AgentMode::Auto).await;
    let iris = employee(&client, project, "Iris", "Frontend designer", AgentMode::Auto).await;
    // Not in a task, and the tool is not named: the assistant decides how to let Iris do it.
    let send = Command::SendProjectMessage(SendProjectMessage {
        confirm_secret: false,
        project_id: project,
        text: "I need colors.css with one CSS rule that makes the body background white. That is frontend work: let Iris do it, don't write it yourself. Tell me when she is done.".into(),
        conversation_id: None,
        images: Vec::new(),
        files: Vec::new(),
        interrupt: false,
        start: ChatStart { agent_mode: Some(AgentMode::Auto), ..Default::default() },
    });
    let CommandResult::ProjectMessageSent { conversation_id, .. } = client.execute(envelope(send, None)).await.unwrap().result else { panic!() };
    let mut given = None;
    for _ in 0..1800 {
        given = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.employee_id == Some(iris));
        if given.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if given.is_none() {
        let messages = client.messages(conversation_id).await.unwrap().messages;
        print_conversation(&messages);
        for m in &messages {
            println!("ACTIVITY {:?}", m.activity.iter().map(|a| (&a.tool, a.ok, &a.output)).collect::<Vec<_>>());
        }
    }
    let given = given.expect("the assistant gave Iris a task");
    let given = wait_task(&client, given.id, |t| t.status.is_terminal()).await;
    assert_eq!((given.status, given.parent_task_id), (TaskStatus::Succeeded, None), "{given:?}");
    let mut messages = Vec::new();
    for _ in 0..1800 {
        messages = client.messages(conversation_id).await.unwrap().messages;
        let found = messages.iter().position(|m| m.update.as_deref() == Some("finding") && m.author.as_deref() == Some("Iris"));
        if found.is_some_and(|i| messages[i + 1..].iter().any(|m| m.role == MessageRole::Assistant && m.update.is_none() && m.status == MessageStatus::Complete)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    print_conversation(&messages);
    let found = messages.iter().position(|m| m.update.as_deref() == Some("finding") && m.author.as_deref() == Some("Iris")).expect("Iris's result came back in the conversation");
    assert!(messages[found + 1..].iter().any(|m| m.role == MessageRole::Assistant && m.update.is_none()), "the assistant answered on it");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real Claude Code CLI"]
async fn a_question_while_working_is_decided_in_the_task_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let client = connect(dir.path()).await;
    claude(&client).await;
    let project = project(&client).await;
    let alex = employee(&client, project, "Alex", "Operator", AgentMode::Ask).await;
    let id = task(&client, project, alex, "Echo", "Create the file ok.txt containing exactly aurelius-ok in your working folder with the Write tool, then report.").await;
    let t = wait_task(&client, id, |t| t.pending_approvals > 0 || t.status.is_terminal()).await;
    assert!(t.pending_approvals > 0, "Manual mode asks first: {t:?}");
    let approval = client
        .approvals()
        .await
        .unwrap()
        .approvals
        .into_iter()
        .find(|a| a.status == ApprovalStatus::Pending && a.conversation_id == t.conversation_id)
        .expect("asked in the task's conversation");
    println!("ASKED: {} {}", approval.tool, approval.arguments);
    client
        .execute(envelope(Command::DecideApproval(DecideApproval { approval_id: approval.id, action_digest: approval.action_digest.clone(), approve: true, always_allow: false }), None))
        .await
        .unwrap();
    let t = wait_task(&client, id, |t| t.status.is_terminal()).await;
    let messages = client.messages(t.conversation_id.unwrap()).await.unwrap().messages;
    print_conversation(&messages);
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    assert!(t.result.unwrap().contains("ok.txt"));
    assert!(approval.tool.contains("Write"), "{}", approval.tool);
    stop(dir.path(), daemon).await;
}

