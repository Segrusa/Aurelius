//! B09 acceptance: connections, per-field inheritance with origin, run
//! snapshots, no silent fallback, secrets never in the database.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ProviderId,
    connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults},
    conversation::{MessageRole, MessageStatus, SendChatMessage},
    employee::{CreateEmployee, Setting},
    project::CreateProject,
    task::CreateTask,
};
use aurelius_protocol::CommandResult;
use common::*;
use std::path::Path;
use std::time::Duration;

async fn start(root: &Path) -> Daemon {
    start_with_delay(root, 2).await
}

async fn exec(client: &Client, command: Command) -> CommandResult {
    client.execute(envelope(command, None)).await.unwrap().result
}

fn last_run_config(root: &Path) -> serde_json::Value {
    let text: String = db(root).query_row("SELECT resolved_config_json FROM runs ORDER BY rowid DESC LIMIT 1", [], |r| r.get(0)).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// The resolved configuration of the first run in the conversation of the task `title`.
async fn task_run_config(root: &Path, title: &str) -> serde_json::Value {
    for _ in 0..400 {
        let config = db(root)
            .query_row(
                "SELECT r.resolved_config_json FROM runs r JOIN tasks t ON t.conversation_id = r.conversation_id WHERE t.title = ?1 ORDER BY r.rowid LIMIT 1",
                [title],
                |r| r.get::<_, String>(0),
            )
            .ok();
        if let Some(config) = config {
            return serde_json::from_str(&config).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no run in the conversation of task {title}");
}

async fn employee(client: &Client, project_id: aurelius_domain::ProjectId, name: &str) -> aurelius_domain::EmployeeId {
    let create = CreateEmployee { name: name.into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None };
    match exec(client, Command::CreateEmployee(create)).await {
        CommandResult::EmployeeCreated { employee_id, .. } => employee_id,
        other => panic!("{other:?}"),
    }
}

fn new_task(project_id: aurelius_domain::ProjectId, employee_id: aurelius_domain::EmployeeId, title: &str) -> Command {
    Command::CreateTask(CreateTask {
        confirm_secret: false,
        project_id,
        employee_id: Some(employee_id),
        title: title.into(),
        goal: "Doe het".into(),
        acceptance_criteria: vec![],
        depends_on: vec![],
        run: Default::default(),
        files: vec![],
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn builtin_connection_and_inheritance_with_origin() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;

    let snapshot = client.connections().await.unwrap();
    let builtin = snapshot.connections.iter().find(|c| c.provider == "fake").expect("built-in local connection");
    assert_eq!(builtin.status, "ready");
    assert!(snapshot.defaults.iter().any(|d| d.scope == ConfigScope::Global && d.purpose == Purpose::Employee));

    // A project conversation's run: global default.
    exec(&client, Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "hoi".into(), images: Vec::new(), interrupt: false })).await;
    let config = last_run_config(dir.path());
    assert_eq!(config["connection_origin"], "global default");
    assert_eq!(config["provider"], "fake");

    // Project override of the model for employees; the employee's task inherits it.
    let CommandResult::ProjectCreated { project_id, .. } =
        exec(&client, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })).await
    else {
        panic!()
    };
    let settings = RunDefaults { connection: Setting::Inherit, model: Setting::Value("fake-project-model".into()), persona: Setting::Inherit, effort: Setting::Inherit };
    exec(&client, Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Project(project_id), purpose: Purpose::Employee, settings })).await;
    let alex = employee(&client, project_id, "Alex").await;
    exec(&client, new_task(project_id, alex, "Inherit")).await;
    let config = task_run_config(dir.path(), "Inherit").await;
    assert_eq!(config["model"], "fake-project-model");
    assert_eq!(config["model_origin"], "project Atlas");
    assert_eq!(config["connection_origin"], "global default");

    // The run still answers via the snapshot's connection.
    for _ in 0..200 {
        if client.tasks(Some(project_id)).await.unwrap().tasks.iter().any(|t| t.status.is_terminal()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn api_connection_needs_a_key_and_disabled_means_no_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;

    let CommandResult::ProviderAdded { connection_id } = exec(
        &client,
        Command::AddProvider(AddProvider { provider: ProviderKind::AnthropicApi, display_name: "Anthropic".into(), runtime_path: None, default_model: None }),
    )
    .await
    else {
        panic!()
    };
    let mut status = String::new();
    for _ in 0..100 {
        let snap = client.connections().await.unwrap();
        let c = snap.connections.iter().find(|c| c.id == connection_id).unwrap();
        status = c.status.clone();
        assert!(!c.has_secret);
        if status == "needs_auth" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(status, "needs_auth");

    // Explicitly disabled connection for employees: the run fails visibly, no fallback to the built-in runtime.
    let settings = RunDefaults { connection: Setting::Disabled, model: Setting::Inherit, persona: Setting::Inherit, effort: Setting::Inherit };
    exec(&client, Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings })).await;
    let CommandResult::ChatMessageSent { conversation_id, .. } =
        exec(&client, Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "hoi".into(), images: Vec::new(), interrupt: false })).await
    else {
        panic!()
    };
    let config = last_run_config(dir.path());
    assert!(config["connection_id"].is_null());
    tokio::time::sleep(Duration::from_millis(500)).await;
    let messages = client.messages(conversation_id).await.unwrap().messages;
    assert!(!messages.iter().any(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Complete));

    // Unknown connection is rejected.
    let bogus = RunDefaults { connection: Setting::Value(ProviderId::new()), model: Setting::Inherit, persona: Setting::Inherit, effort: Setting::Inherit };
    let err = client.execute(envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings: bogus }), None)).await;
    assert!(err.is_err());

    // The database never contains a secret value, only references.
    let secret_ref: Option<String> = db(dir.path()).query_row("SELECT secret_ref FROM connections WHERE id = ?1", [connection_id.to_string()], |r| r.get(0)).unwrap();
    assert!(secret_ref.unwrap().starts_with("conn-"));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_delegated_task_runs_with_the_model_and_effort_of_the_employee_who_does_it() {
    use aurelius_client::bridge::{BridgeClient, EmployeeTaskOp};
    let dir = tempfile::tempdir().unwrap();
    // Slow answers: Jean's task is still running when he delegates.
    let daemon = start_with_delay(dir.path(), 100).await;
    let client = connect(dir.path()).await;
    free_chart(&client).await;
    let CommandResult::ProjectCreated { project_id, .. } =
        exec(&client, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })).await
    else {
        panic!()
    };
    let (jean, simon) = (employee(&client, project_id, "Jean").await, employee(&client, project_id, "Simon").await);
    let set = |scope: ConfigScope, model: &str, effort: &str| {
        let settings = RunDefaults { connection: Setting::Inherit, model: Setting::Value(model.into()), persona: Setting::Inherit, effort: Setting::Value(effort.into()) };
        exec(&client, Command::SetRunDefaults(SetRunDefaults { scope, purpose: Purpose::Employee, settings }))
    };
    set(ConfigScope::Global, "fake-global-model", "high").await;
    set(ConfigScope::Project(project_id), "fake-project-model", "medium").await;
    set(ConfigScope::Employee(jean), "fake-jean-model", "max").await;
    set(ConfigScope::Employee(simon), "fake-simon-model", "low").await;
    // Clients see the employees' own defaults.
    let snapshot = client.connections().await.unwrap();
    assert!(snapshot.defaults.iter().any(|d| d.scope == ConfigScope::Employee(simon) && d.purpose == Purpose::Employee && d.settings.model == Setting::Value("fake-simon-model".into())));

    // Jean works on a task and gives Simon a part of it.
    exec(&client, new_task(project_id, jean, "Plan")).await;
    let config = task_run_config(dir.path(), "Plan").await;
    assert_eq!((config["model"].as_str(), config["effort"].as_str()), (Some("fake-jean-model"), Some("max")));
    let jean_run: String = db(dir.path())
        .query_row("SELECT r.id FROM runs r JOIN tasks t ON t.conversation_id = r.conversation_id WHERE t.title = 'Plan' ORDER BY r.rowid LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let (ok, text) = bridge
        .employee_act(aurelius_domain::RunId::parse(&jean_run).unwrap(), EmployeeTaskOp::Delegate { employee: "Simon".into(), title: "Server".into(), goal: "Bouw het".into(), criteria: vec![], project: None })
        .await
        .unwrap();
    assert!(ok, "{text}");

    // Simon's task conversation runs with Simon's model and effort, not Jean's.
    let config = task_run_config(dir.path(), "Server").await;
    assert_eq!(config["model"], "fake-simon-model");
    assert_eq!(config["effort"], "low");
    assert_eq!(config["model_origin"], "employee Simon");
    stop(dir.path(), daemon).await;
}
