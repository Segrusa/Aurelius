//! The management channel (features `management_channel`): an app key with the
//! audience `management` reaches the CEO and the CEO's direct reports only, and
//! the server enforces it. `ask_management`, `list_open_questions` and
//! `answer_question` work through the stdio server and through `/v1/mcp`.

mod common;

use aurelius_client::{Client, ClientError, envelope};
use aurelius_domain::{
    AppKeyId, Command, ConversationId, EmployeeId, ErrorCode, ProjectId,
    app_key::{AppAudience, AppScope, CreateAppKey},
    employee::{CreateEmployee, SendEmployeeMessage},
    project::CreateProject,
    project_conversation::SendProjectMessage,
    task::{CancelTask, CreateTask, RunSettings},
};
use aurelius_protocol::{CommandResult, HostWrite};
use common::*;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

fn remote_env() {
    // SAFETY (test): every test in this binary sets the same values before its daemon starts.
    unsafe {
        std::env::set_var("AURELIUS_REMOTE_PORT", "0");
        std::env::set_var("AURELIUS_NO_TAILSCALE", "1");
        std::env::set_var("AURELIUS_ENV", "dev");
    }
}

struct Company {
    project: ProjectId,
    jean: (EmployeeId, ConversationId),
    cora: (EmployeeId, ConversationId),
    dev: (EmployeeId, ConversationId),
}

/// Jean (CEO) -> Cora (CTO) -> Dean (developer), and a project.
async fn company(owner: &Client) -> Company {
    let CommandResult::ProjectCreated { project_id, .. } =
        owner.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let hire = |name: &str, role: &str, title: Option<&str>, boss: Option<EmployeeId>| {
        Command::CreateEmployee(CreateEmployee { name: name.into(), role: role.into(), title: title.map(str::to_owned), reports_to: boss, instructions: None, folder: None })
    };
    let mut made = Vec::new();
    let mut boss = None;
    for (name, role, title) in [("Jean", "Management", Some("CEO")), ("Cora", "Technology", Some("CTO")), ("Dean", "Developer", None)] {
        let CommandResult::EmployeeCreated { employee_id, conversation_id } = owner.execute(envelope(hire(name, role, title, boss), None)).await.unwrap().result else { panic!() };
        boss = Some(employee_id);
        made.push((employee_id, conversation_id));
    }
    Company { project: project_id, jean: made[0], cora: made[1], dev: made[2] }
}

async fn key(owner: &Client, name: &str, audience: AppAudience, scopes: Vec<AppScope>) -> (AppKeyId, String) {
    let created = owner.create_app_key(CreateAppKey { name: name.into(), scopes, limit_per_hour: None, limit_concurrent: None, audience, webhook_url: None }).await.unwrap();
    (created.key.id, created.secret)
}

async fn app(root: &Path, secret: &str) -> Client {
    Client::connect_app(&aurelius_host::endpoint(root), secret, "marcus").await.expect("app session")
}

fn task(project: ProjectId, employee: Option<EmployeeId>, title: &str) -> Command {
    Command::CreateTask(CreateTask { project_id: project, employee_id: employee, title: title.into(), goal: "Do it".into(), acceptance_criteria: vec![], depends_on: vec![], run: RunSettings::default(), files: vec![] })
}

fn message(employee: EmployeeId, conversation: ConversationId) -> Command {
    Command::SendEmployeeMessage(SendEmployeeMessage { employee_id: employee, text: "Status?".into(), conversation_id: conversation, images: vec![], files: vec![], interrupt: false })
}

fn refusal(result: Result<impl std::fmt::Debug, ClientError>) -> String {
    match result {
        Err(ClientError::Protocol(e)) => {
            assert_eq!(e.code, ErrorCode::Forbidden, "{}", e.message);
            e.message
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_management_key_reaches_the_ceo_and_the_ceos_reports_only() {
    remote_env();
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let owner = connect(dir.path()).await;
    assert!(owner.supports(aurelius_protocol::feature::MANAGEMENT_CHANNEL) && owner.supports(aurelius_protocol::feature::WEBHOOKS));
    let c = company(&owner).await;
    let (_, secret) = key(&owner, "Marcus", AppAudience::Management, vec![AppScope::Read, AppScope::Run]).await;
    let marcus = app(dir.path(), &secret).await;
    assert_eq!(marcus.hello.app.as_ref().map(|a| a.audience), Some(AppAudience::Management));

    // The list: the CEO and the CEO's direct report, not the developer.
    let mut names: Vec<String> = marcus.employees().await.unwrap().employees.into_iter().map(|e| e.name).collect();
    names.sort();
    assert_eq!(names, vec!["Cora", "Jean"]);

    // Work and messages for the developer are refused by the server, with a clear reason.
    let why = refusal(marcus.execute(envelope(task(c.project, Some(c.dev.0), "For Dean"), None)).await);
    assert!(why.contains("management"), "{why}");
    refusal(marcus.execute(envelope(message(c.dev.0, c.dev.1), None)).await);
    refusal(marcus.messages(c.dev.1).await);
    let assistant = Command::SendProjectMessage(SendProjectMessage { project_id: c.project, text: "hi".into(), conversation_id: None, images: vec![], files: vec![], interrupt: false, start: Default::default() });
    let why = refusal(marcus.execute(envelope(assistant, None)).await);
    assert!(why.contains("management"), "{why}");

    // The management is fine: a task for the CTO, a message in the CTO's chat, whose conversation it then reads.
    marcus.execute(envelope(task(c.project, Some(c.cora.0), "For Cora"), None)).await.expect("a task for the CTO");
    refusal(marcus.messages(c.cora.1).await);
    marcus.execute(envelope(message(c.cora.0, c.cora.1), None)).await.expect("a message to the CTO");
    marcus.messages(c.cora.1).await.expect("it reads the conversation it started");
    refusal(marcus.messages(c.jean.1).await);
    // A message in a conversation of another employee than the one named is refused too.
    refusal(marcus.execute(envelope(message(c.cora.0, c.jean.1), None)).await);

    // Tasks of the developer stay hidden, and cannot be stopped.
    let CommandResult::TaskCreated { task_id: dev_task } = owner.execute(envelope(task(c.project, Some(c.dev.0), "Dean's own"), None)).await.unwrap().result else { panic!() };
    let titles: Vec<String> = marcus.tasks(None).await.unwrap().tasks.into_iter().map(|t| t.title).collect();
    assert!(titles.contains(&"For Cora".to_owned()) && !titles.contains(&"Dean's own".to_owned()), "{titles:?}");
    refusal(marcus.execute(envelope(Command::CancelTask(CancelTask { task_id: dev_task }), None)).await);

    // An app can never be a management app with approve.
    let err = owner.create_app_key(CreateAppKey { name: "Bad".into(), scopes: vec![AppScope::Read, AppScope::Approve], limit_per_hour: None, limit_concurrent: None, audience: AppAudience::Management, webhook_url: None }).await;
    assert!(err.is_err());

    // Existing behaviour: a key without audience (all) sees everyone and may work with the developer.
    let (_, all_secret) = key(&owner, "Assistant", AppAudience::All, vec![AppScope::Read, AppScope::Run]).await;
    let all = app(dir.path(), &all_secret).await;
    assert_eq!(all.hello.app.as_ref().map(|a| a.audience), Some(AppAudience::All));
    assert_eq!(all.employees().await.unwrap().employees.len(), 3);
    all.execute(envelope(task(c.project, Some(c.dev.0), "From the assistant"), None)).await.expect("audience all is unchanged");
    refusal(all.execute(envelope(message(c.dev.0, c.dev.1), None)).await);
    assert_eq!(owner.app_keys().await.unwrap().iter().filter(|k| k.audience == AppAudience::All).count(), 1);
    stop(dir.path(), daemon).await;
}

async fn call(server: &aurelius_mcp::Server, name: &str, arguments: Value) -> (String, bool) {
    let reply = server.handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } })).await.unwrap();
    (reply["result"]["content"][0]["text"].as_str().unwrap_or("").to_owned(), reply["result"]["isError"].as_bool().unwrap_or(true))
}

fn names(reply: &Value) -> Vec<String> {
    reply["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_management_tools_work_over_stdio() {
    remote_env();
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let owner = connect(dir.path()).await;
    let c = company(&owner).await;
    let (_, secret) = key(&owner, "Marcus", AppAudience::Management, vec![AppScope::Read, AppScope::Run]).await;
    let server = aurelius_mcp::Server::new(Arc::new(app(dir.path(), &secret).await));

    let tools = names(&server.handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).await.unwrap());
    for wanted in ["ask_management", "list_open_questions", "answer_question", "list_employees", "create_task", "message_task"] {
        assert!(tools.contains(&wanted.to_owned()), "{wanted} in {tools:?}");
    }
    for hidden in ["ask_project_assistant", "answer_approval", "set_task_mode"] {
        assert!(!tools.contains(&hidden.to_owned()), "{hidden} in {tools:?}");
    }

    // list_managers (the old name) and list_employees: the management only.
    for tool in ["list_managers", "list_employees"] {
        let (text, failed) = call(&server, tool, json!({})).await;
        assert!(!failed && text.contains("Jean") && text.contains("Cora") && !text.contains("Dean"), "{tool}: {text}");
    }
    // Work for a developer is refused (by name it cannot even be found).
    let (text, failed) = call(&server, "create_task", json!({ "project": "Atlas", "title": "X", "goal": "Y", "employee": "Dean" })).await;
    assert!(failed && text.contains("management"), "{text}");
    let (text, failed) = call(&server, "create_task", json!({ "project": "Atlas", "title": "For the CTO", "goal": "Y", "employee": "Cora" })).await;
    assert!(!failed, "{text}");

    // ask_management: the CEO by default, and the answer comes back (waiting for it here).
    let (text, failed) = call(&server, "ask_management", json!({ "message": "What is the plan?", "wait_seconds": 30 })).await;
    assert!(!failed, "{text}");
    let sent: Value = serde_json::from_str(&text).unwrap();
    assert_eq!((sent["status"].as_str(), sent["to"]["name"].as_str(), sent["kind"].as_str()), (Some("answered"), Some("Jean"), Some("question")), "{text}");
    assert!(sent["reply"]["text"].as_str().is_some_and(|t| !t.is_empty()), "{text}");
    assert_eq!(sent["conversation_id"].as_str(), Some(c.jean.1.to_string().as_str()));
    // Not waiting: sent now, the reply is read later; "CTO" finds Cora by title; a decision is a decision.
    let (text, failed) = call(&server, "ask_management", json!({ "to": "CTO", "kind": "decision", "project": "Atlas", "message": "Ship it?" })).await;
    assert!(!failed, "{text}");
    let sent: Value = serde_json::from_str(&text).unwrap();
    assert_eq!((sent["status"].as_str(), sent["to"]["name"].as_str(), sent["kind"].as_str()), (Some("sent"), Some("Cora"), Some("decision")), "{text}");
    let conversation = sent["conversation_id"].as_str().unwrap().to_owned();
    let (read, failed) = call(&server, "read_conversation", json!({ "conversation": conversation, "last": 10 })).await;
    assert!(!failed && read.contains("Decision requested") && read.contains("Ship it?") && read.contains("Atlas"), "{read}");
    // The developer cannot be asked, an unknown kind is refused.
    let (text, failed) = call(&server, "ask_management", json!({ "to": "Dean", "message": "Hi" })).await;
    assert!(failed, "{text}");
    let (text, failed) = call(&server, "ask_management", json!({ "kind": "gossip", "message": "Hi" })).await;
    assert!(failed && text.contains("kind"), "{text}");

    // Nothing waits for the user yet; answering something that does not exist fails clearly.
    let (text, failed) = call(&server, "list_open_questions", json!({})).await;
    assert!(!failed, "{text}");
    let open: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(open["questions"], json!([]));
    assert!(open["waiting_tasks"].is_array() && open["needs_you"].is_array());
    let (text, failed) = call(&server, "answer_question", json!({ "id": "does-not-exist", "answer": "yes" })).await;
    assert!(failed && text.contains("list_open_questions"), "{text}");
    let (_, failed) = call(&server, "answer_question", json!({ "id": "x" })).await;
    assert!(failed, "an answer is required");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_management_tools_work_over_http() {
    remote_env();
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let owner = connect(dir.path()).await;
    company(&owner).await;
    let status = owner.remote(HostWrite::RemoteEnable).await.unwrap();
    let base = format!("http://127.0.0.1:{}/v1/mcp", status.port);
    let (_, secret) = key(&owner, "Marcus", AppAudience::Management, vec![AppScope::Read, AppScope::Run]).await;
    let http = reqwest::Client::new();
    let rpc = |id: u64, method: &str, params: Value| {
        let request = http.post(&base).header("authorization", format!("Bearer {secret}")).json(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        async move { request.send().await.unwrap().json::<Value>().await.unwrap() }
    };
    let tools = names(&rpc(1, "tools/list", json!({})).await);
    assert!(tools.contains(&"ask_management".to_owned()) && tools.contains(&"answer_question".to_owned()) && !tools.contains(&"ask_project_assistant".to_owned()), "{tools:?}");
    let text = |reply: &Value| (reply["result"]["content"][0]["text"].as_str().unwrap_or("").to_owned(), reply["result"]["isError"].as_bool().unwrap_or(true));

    let (listed, failed) = text(&rpc(2, "tools/call", json!({ "name": "list_employees", "arguments": {} })).await);
    assert!(!failed && listed.contains("Jean") && !listed.contains("Dean"), "{listed}");
    let (asked, failed) = text(&rpc(3, "tools/call", json!({ "name": "ask_management", "arguments": { "to": "CEO", "message": "Over HTTP?", "wait_seconds": 30 } })).await);
    assert!(!failed, "{asked}");
    let asked: Value = serde_json::from_str(&asked).unwrap();
    assert_eq!((asked["status"].as_str(), asked["to"]["name"].as_str()), (Some("answered"), Some("Jean")), "{asked}");
    let (open, failed) = text(&rpc(4, "tools/call", json!({ "name": "list_open_questions", "arguments": {} })).await);
    assert!(!failed && serde_json::from_str::<Value>(&open).unwrap()["questions"].is_array(), "{open}");
    let (refused, failed) = text(&rpc(5, "tools/call", json!({ "name": "create_task", "arguments": { "project": "Atlas", "title": "X", "goal": "Y", "employee": "Dean" } })).await);
    assert!(failed && refused.contains("management"), "{refused}");
    stop(dir.path(), daemon).await;
}
