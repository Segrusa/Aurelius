//! App keys end to end (feature `app_keys`): outside apps through
//! `aurelius-mcp`. Keys are made by an owner; a session that authenticates
//! with one has that app's scopes only, over local IPC, the gateway's
//! WebSocket and `/v1/mcp`; limits hold; revoking stops open sessions at
//! once; tasks keep the app that created them.

mod common;

use aurelius_client::{Client, ClientError, envelope};
use aurelius_domain::{
    Command, ConversationId, ErrorCode, EmployeeId, ProjectId,
    app_key::{AppScope, CreateAppKey},
    autonomy::{AgentMode, SetAgentMode},
    employee::CreateEmployee,
    project::CreateProject,
    task::{CreateTask, RunSettings},
};
use aurelius_protocol::{CommandResult, HostWrite, Method, ResponseBody};
use common::*;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Remote access without Tailscale on a free port; the same in every test of this binary.
fn remote_env() {
    // SAFETY (test): every test in this binary sets the same values before its daemon starts.
    unsafe {
        std::env::set_var("AURELIUS_REMOTE_PORT", "0");
        std::env::set_var("AURELIUS_ENV", "dev");
    }
}

async fn boot(root: &Path) -> (Daemon, Client) {
    remote_env();
    let daemon = start(root).await;
    let owner = connect(root).await;
    (daemon, owner)
}

async fn setup(client: &Client) -> (ProjectId, EmployeeId, ConversationId) {
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let employee = CreateEmployee { name: "Simon".into(), role: "Server".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(employee), None)).await.unwrap().result else { panic!() };
    (project_id, employee_id, conversation_id)
}

async fn new_key(owner: &Client, name: &str, scopes: Vec<AppScope>, per_hour: Option<u32>) -> (aurelius_domain::AppKeyId, String) {
    let created = owner.create_app_key(CreateAppKey { name: name.into(), scopes, limit_per_hour: per_hour, limit_concurrent: None, audience: Default::default(), webhook_url: None }).await.unwrap();
    assert!(created.secret.starts_with("amk_"));
    (created.key.id, created.secret)
}

async fn app(root: &Path, secret: &str) -> Client {
    Client::connect_app(&aurelius_host::endpoint(root), secret, "test app").await.expect("app session")
}

fn task(project_id: ProjectId, employee_id: Option<EmployeeId>, mode: Option<AgentMode>) -> Command {
    Command::CreateTask(CreateTask {
        confirm_secret: false,
        project_id,
        employee_id,
        title: "From the app".into(),
        goal: "Check it".into(),
        acceptance_criteria: vec![],
        depends_on: vec![],
        run: RunSettings { mode, ..Default::default() },
        files: vec![],
    })
}

fn code(result: Result<impl std::fmt::Debug, ClientError>) -> ErrorCode {
    match result {
        Err(ClientError::Protocol(e)) => e.code,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn keys_are_made_by_owners_and_the_secret_is_shown_once() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, owner) = boot(dir.path()).await;
    assert!(owner.supports(aurelius_protocol::feature::APP_KEYS));
    let (id, secret) = new_key(&owner, "Assistant", vec![AppScope::Read, AppScope::Run], Some(5)).await;
    let keys = owner.app_keys().await.unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!((keys[0].id, keys[0].scopes.clone(), keys[0].limit_per_hour), (id, vec![AppScope::Read, AppScope::Run], Some(5)));
    // The database keeps the hash only; the receipt keeps no secret.
    let conn = db(dir.path());
    let stored: String = conn.query_row("SELECT key_verifier FROM app_keys", [], |r| r.get(0)).unwrap();
    assert_ne!(stored, secret);
    let kept: Vec<String> = conn.prepare("SELECT result_json FROM command_receipts WHERE method = 'app_key.create'").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(kept.len(), 1);
    assert!(!kept[0].contains(&secret), "{}", kept[0]);

    // An app cannot manage keys, not even with every scope.
    let (_, all) = new_key(&owner, "Everything", AppScope::ALL.to_vec(), None).await;
    let everything = app(dir.path(), &all).await;
    assert_eq!(code(everything.app_keys().await), ErrorCode::Forbidden);
    assert_eq!(code(everything.create_app_key(CreateAppKey { name: "x".into(), scopes: vec![AppScope::Read], limit_per_hour: None, limit_concurrent: None, audience: Default::default(), webhook_url: None }).await), ErrorCode::Forbidden);
    // A wrong key is refused at the hello.
    assert!(Client::connect_app(&aurelius_host::endpoint(dir.path()), "amk_wrong", "test").await.is_err());
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn scopes_decide_what_an_app_may_do() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, owner) = boot(dir.path()).await;
    let (project, employee, main) = setup(&owner).await;
    let (_, read) = new_key(&owner, "Reader", vec![AppScope::Read], None).await;
    let reader = app(dir.path(), &read).await;
    let session = reader.hello.app.clone().expect("the hello names the app");
    assert_eq!((session.name.as_str(), session.scopes.clone()), ("Reader", vec![AppScope::Read]));
    assert!(reader.hello.capabilities.is_empty(), "an app has scopes, no device rights");
    assert_eq!(reader.projects().await.unwrap().projects.len(), 1);
    assert_eq!(reader.employees().await.unwrap().employees.len(), 1);
    assert_eq!(code(reader.messages(main).await), ErrorCode::Forbidden, "an employee's chat is outside projects");
    assert_eq!(code(reader.execute(envelope(task(project, None, None), None)).await), ErrorCode::Forbidden, "read cannot write");

    let (_, draft) = new_key(&owner, "Drafter", vec![AppScope::Draft], None).await;
    let drafter = app(dir.path(), &draft).await;
    drafter.execute(envelope(task(project, None, None), None)).await.expect("a draft");
    assert_eq!(code(drafter.execute(envelope(task(project, Some(employee), None), None)).await), ErrorCode::Forbidden, "starting needs run");
    assert_eq!(code(drafter.projects().await), ErrorCode::Forbidden, "draft does not read");

    let (_, run) = new_key(&owner, "Runner", vec![AppScope::Read, AppScope::Run], None).await;
    let runner = app(dir.path(), &run).await;
    let CommandResult::TaskCreated { task_id } = runner.execute(envelope(task(project, Some(employee), None), None)).await.expect("run starts tasks").result else { panic!() };
    assert_eq!(code(runner.execute(envelope(task(project, Some(employee), Some(AgentMode::Auto)), None)).await), ErrorCode::Forbidden, "Auto needs approve");
    // The task's conversation is in the project, so apps reach it (the employee's chat is not).
    let mut in_task = None;
    for _ in 0..400 {
        in_task = owner.tasks(Some(project)).await.unwrap().tasks.iter().find(|t| t.id == task_id).and_then(|t| t.conversation_id);
        if in_task.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let to_auto = Command::SetAgentMode(SetAgentMode { conversation_id: in_task.expect("a task conversation"), mode: AgentMode::Auto });
    assert_eq!(code(runner.execute(envelope(to_auto.clone(), None)).await), ErrorCode::Forbidden);
    assert_eq!(code(runner.call(Method::FsListDir { path: None }).await), ErrorCode::Forbidden);

    let (_, approve) = new_key(&owner, "Approver", vec![AppScope::Run, AppScope::Approve], None).await;
    let approver = app(dir.path(), &approve).await;
    approver.execute(envelope(task(project, Some(employee), Some(AgentMode::Auto)), None)).await.expect("approve allows Auto");
    approver.execute(envelope(to_auto, None)).await.expect("approve switches to Auto");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn tasks_show_the_app_and_limits_hold() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, owner) = boot(dir.path()).await;
    let (project, employee, _) = setup(&owner).await;
    let (key, secret) = new_key(&owner, "Assistant", vec![AppScope::Run], Some(2)).await;
    let assistant = app(dir.path(), &secret).await;
    let CommandResult::TaskCreated { task_id } = assistant.execute(envelope(task(project, Some(employee), None), None)).await.unwrap().result else { panic!() };
    assistant.execute(envelope(task(project, None, None), None)).await.unwrap();
    let err = assistant.execute(envelope(task(project, None, None), None)).await.unwrap_err();
    assert!(err.to_string().contains("per hour"), "{err}");
    // The user's own tasks are not the app's and do not count.
    owner.execute(envelope(task(project, Some(employee), None), None)).await.unwrap();

    let tasks = owner.tasks(Some(project)).await.unwrap().tasks;
    let made = tasks.iter().find(|t| t.id == task_id).unwrap();
    assert_eq!(made.created_by_app.as_deref(), Some("Assistant"));
    assert_eq!(made.created_by_app_key_id, Some(key));
    assert_eq!(tasks.iter().filter(|t| t.created_by_app.is_some()).count(), 2);
    assert_eq!(tasks.iter().filter(|t| t.created_by_app.is_none()).count(), 1);
    let listed = owner.app_keys().await.unwrap().into_iter().find(|k| k.id == key).unwrap();
    assert_eq!(listed.tasks_last_hour, 2);
    assert!(listed.last_used_ms.is_some());
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_approve_an_employees_auto_becomes_ask() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, owner) = boot(dir.path()).await;
    let (project, employee, main) = setup(&owner).await;
    owner.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id: main, mode: AgentMode::Auto }), None)).await.unwrap();
    let (_, run) = new_key(&owner, "Runner", vec![AppScope::Run], None).await;
    let (_, approve) = new_key(&owner, "Approver", vec![AppScope::Run, AppScope::Approve], None).await;
    let CommandResult::TaskCreated { task_id: plain } = app(dir.path(), &run).await.execute(envelope(task(project, Some(employee), None), None)).await.unwrap().result else { panic!() };
    let CommandResult::TaskCreated { task_id: trusted } = app(dir.path(), &approve).await.execute(envelope(task(project, Some(employee), None), None)).await.unwrap().result else { panic!() };
    let tasks = owner.tasks(Some(project)).await.unwrap().tasks;
    assert_eq!(tasks.iter().find(|t| t.id == plain).unwrap().run.mode, Some(AgentMode::Ask));
    assert_eq!(tasks.iter().find(|t| t.id == trusted).unwrap().run.mode, None, "the employee's own mode");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn revoking_stops_open_sessions_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, owner) = boot(dir.path()).await;
    let (project, _, _) = setup(&owner).await;
    let (key, secret) = new_key(&owner, "Assistant", vec![AppScope::Read, AppScope::Draft], None).await;
    let local = app(dir.path(), &secret).await;
    local.projects().await.unwrap();

    let status = owner.remote(HostWrite::RemoteEnable).await.unwrap();
    let base = format!("http://127.0.0.1:{}", status.port);
    let remote = Client::connect_app_remote(&base, &secret, "test app").await.expect("app over the gateway");
    assert_eq!(remote.hello.app.as_ref().map(|a| a.key_id), Some(key));
    assert!(remote.hello.remote);
    remote.projects().await.unwrap();

    let revoked = owner.revoke_app_key(key).await.unwrap();
    assert!(revoked.revoked_at_ms.is_some());
    assert_eq!(code(local.projects().await), ErrorCode::Forbidden, "the open local session");
    assert_eq!(code(local.execute(envelope(task(project, None, None), None)).await), ErrorCode::Forbidden);
    // The WebSocket is closed (or its next request refused).
    let mut refused = false;
    for _ in 0..40 {
        if remote.projects().await.is_err() {
            refused = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(refused, "the gateway session still works after revoke");
    assert!(Client::connect_app(&aurelius_host::endpoint(dir.path()), &secret, "again").await.is_err());
    assert!(Client::connect_app_remote(&base, &secret, "again").await.is_err());
    stop(dir.path(), daemon).await;
}

/// One JSON-RPC call to `/v1/mcp`.
async fn mcp(http: &reqwest::Client, base: &str, secret: Option<&str>, body: Value) -> (u16, Value) {
    let mut request = http.post(format!("{base}/v1/mcp")).json(&body);
    if let Some(secret) = secret {
        request = request.header("authorization", format!("Bearer {secret}"));
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

fn tool_names(reply: &Value) -> Vec<String> {
    reply["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect()
}

fn tool_text(reply: &Value) -> (String, bool) {
    (reply["result"]["content"][0]["text"].as_str().unwrap_or("").to_owned(), reply["result"]["isError"].as_bool().unwrap_or(true))
}

#[tokio::test(flavor = "multi_thread")]
async fn the_gateway_speaks_mcp_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, owner) = boot(dir.path()).await;
    let (project, _, _) = setup(&owner).await;
    let status = owner.remote(HostWrite::RemoteEnable).await.unwrap();
    let base = format!("http://127.0.0.1:{}", status.port);
    let http = reqwest::Client::new();
    let (key, secret) = new_key(&owner, "Assistant", vec![AppScope::Read, AppScope::Draft], None).await;

    let (code, init) = mcp(&http, &base, Some(&secret), json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } } })).await;
    assert_eq!(code, 200, "{init}");
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "aurelius");
    let (code, _) = mcp(&http, &base, Some(&secret), json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
    assert_eq!(code, 202);

    let (_, tools) = mcp(&http, &base, Some(&secret), json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).await;
    let names = tool_names(&tools);
    assert!(names.contains(&"list_projects".to_owned()) && names.contains(&"create_task".to_owned()) && names.contains(&"recall".to_owned()));
    assert!(!names.contains(&"message_task".to_owned()) && !names.contains(&"answer_approval".to_owned()), "no run or approve tools: {names:?}");

    let (_, listed) = mcp(&http, &base, Some(&secret), json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "list_projects", "arguments": {} } })).await;
    let (text, failed) = tool_text(&listed);
    assert!(!failed && text.contains("Atlas"), "{listed}");
    let (_, created) = mcp(&http, &base, Some(&secret), json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "create_task", "arguments": { "project": "atlas", "title": "Over HTTP", "goal": "Check the gateway" } } })).await;
    let (text, failed) = tool_text(&created);
    assert!(!failed && text.contains("draft"), "{created}");
    let made = owner.tasks(Some(project)).await.unwrap().tasks.into_iter().find(|t| t.title == "Over HTTP").expect("the task");
    assert_eq!((made.created_by_app.as_deref(), made.employee_id), (Some("Assistant"), None));
    // A tool outside the scopes is refused, also when called by name.
    let (_, refused) = mcp(&http, &base, Some(&secret), json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "stop_task", "arguments": { "task": "Over HTTP" } } })).await;
    let (text, failed) = tool_text(&refused);
    assert!(failed && text.contains("run"), "{refused}");

    // No key, a device token, a browser, a revoked key: refused.
    assert_eq!(mcp(&http, &base, None, json!({ "jsonrpc": "2.0", "id": 6, "method": "ping" })).await.0, 401);
    assert_eq!(mcp(&http, &base, Some("not-a-key"), json!({ "jsonrpc": "2.0", "id": 6, "method": "ping" })).await.0, 401);
    let from_page = http.post(format!("{base}/v1/mcp")).header("authorization", format!("Bearer {secret}")).header("origin", "https://evil.example").json(&json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" })).send().await.unwrap();
    assert_eq!(from_page.status().as_u16(), 403);
    assert_eq!(http.get(format!("{base}/v1/mcp")).send().await.unwrap().status().as_u16(), 405);
    owner.revoke_app_key(key).await.unwrap();
    assert_eq!(mcp(&http, &base, Some(&secret), json!({ "jsonrpc": "2.0", "id": 8, "method": "ping" })).await.0, 401);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_stdio_server_runs_its_tools_through_the_client() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, owner) = boot(dir.path()).await;
    let (project, _, _) = setup(&owner).await;
    let (_, secret) = new_key(&owner, "Claude Code", vec![AppScope::Read, AppScope::Run, AppScope::Approve], None).await;
    let server = aurelius_mcp::Server::new(Arc::new(app(dir.path(), &secret).await));
    let call = |name: &str, arguments: Value| json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } });

    let tools = server.handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).await.unwrap();
    let names = tool_names(&tools);
    assert!(names.contains(&"answer_approval".to_owned()) && names.contains(&"set_task_mode".to_owned()), "approve tools with approve: {names:?}");

    let reply = server.handle(call("list_employees", json!({ "project": "Atlas" }))).await.unwrap();
    assert!(tool_text(&reply).0.contains("Simon"), "{reply}");
    let reply = server.handle(call("create_task", json!({ "project": "Atlas", "title": "Started by Claude Code", "goal": "Run it", "employee": "simon" }))).await.unwrap();
    let (text, failed) = tool_text(&reply);
    assert!(!failed && text.contains("for Simon"), "{reply}");
    let made = owner.tasks(Some(project)).await.unwrap().tasks.into_iter().find(|t| t.title == "Started by Claude Code").unwrap();
    assert_eq!(made.created_by_app.as_deref(), Some("Claude Code"));
    assert!(made.employee_id.is_some());

    let reply = server.handle(call("read_task", json!({ "task": "Started by Claude Code" }))).await.unwrap();
    let (text, failed) = tool_text(&reply);
    assert!(!failed && text.contains("Created by app: Claude Code"), "{reply}");
    let reply = server.handle(call("list_tasks", json!({ "created_by_this_app": true }))).await.unwrap();
    assert!(tool_text(&reply).0.contains("Started by Claude Code"));
    let reply = server.handle(call("add_idea", json!({ "project": "Atlas", "title": "Follow up" }))).await.unwrap();
    assert!(!tool_text(&reply).1, "{reply}");
    let reply = server.handle(call("remember", json!({ "project": "Atlas", "content": "The app key test ran.", "kind": "episodic" }))).await.unwrap();
    assert!(!tool_text(&reply).1, "{reply}");
    let reply = server.handle(call("recall", json!({ "project": "Atlas", "query": "app key test" }))).await.unwrap();
    assert!(tool_text(&reply).0.contains("The app key test ran."), "{reply}");
    let reply = server.handle(call("read_inbox", json!({}))).await.unwrap();
    assert!(!tool_text(&reply).1, "{reply}");
    let reply = server.handle(call("ask_project_assistant", json!({ "project": "Atlas", "message": "Hello", "wait_seconds": 20 }))).await.unwrap();
    let (text, failed) = tool_text(&reply);
    assert!(!failed && text.contains("conversation"), "{reply}");
    let reply = server.handle(call("no_such_tool", json!({}))).await.unwrap();
    assert_eq!(reply["error"]["code"], -32602);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_only_app_sees_reads_only_and_old_style_hellos_keep_full_access() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, owner) = boot(dir.path()).await;
    // The user's own clients are unchanged: no app, every right.
    assert!(owner.hello.app.is_none());
    assert!(!owner.hello.capabilities.is_empty());
    let (_, read) = new_key(&owner, "Reader", vec![AppScope::Read], None).await;
    let server = aurelius_mcp::Server::new(Arc::new(app(dir.path(), &read).await));
    let names = tool_names(&server.handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).await.unwrap());
    for hidden in ["create_task", "message_task", "stop_task", "ask_project_assistant", "add_idea", "remember", "set_task_mode", "answer_approval"] {
        assert!(!names.iter().any(|n| n == hidden), "{hidden} in {names:?}");
    }
    assert!(matches!(owner.call(Method::AppKeyList).await, Ok(ResponseBody::AppKeys(keys)) if keys.len() == 1));
    stop(dir.path(), daemon).await;
}
