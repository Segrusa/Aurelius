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
    user::UserRunDefaults,
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
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None };
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
    exec(&client, Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "hoi".into(), images: Vec::new(), files: vec![], interrupt: false })).await;
    let config = last_run_config(dir.path());
    assert_eq!(config["connection_origin"], "global default");
    assert_eq!(config["provider"], "fake");

    // A project's model no longer counts (migration 52): the board member's own
    // default does, while the connection still comes from the scopes.
    let CommandResult::ProjectCreated { project_id, .. } =
        exec(&client, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })).await
    else {
        panic!()
    };
    let settings = RunDefaults { connection: Setting::Inherit, model: Setting::Value("fake-project-model".into()), persona: Setting::Inherit, effort: Setting::Inherit };
    exec(&client, Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Project(project_id), purpose: Purpose::Employee, settings })).await;
    client.set_user_run_defaults(UserRunDefaults { model: Some("fake-own-model".into()), ..Default::default() }).await.unwrap();
    let alex = employee(&client, project_id, "Alex").await;
    exec(&client, new_task(project_id, alex, "Inherit")).await;
    let config = task_run_config(dir.path(), "Inherit").await;
    assert_eq!(config["model"], "fake-own-model");
    assert_eq!(config["model_origin"], "the board member's default");
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
    // Without the runtime override: an override answers whatever the connection says, and
    // whether a run without a connection fails is what this test is here for.
    let daemon = common::start_real(dir.path()).await;
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
        exec(&client, Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "hoi".into(), images: Vec::new(), files: vec![], interrupt: false })).await
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
async fn a_delegated_task_runs_with_the_model_and_effort_of_the_board_member_whose_work_it_is() {
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
    // The company's, the project's and the employees' models and efforts no longer count.
    let set = |scope: ConfigScope, model: &str, effort: &str| {
        let settings = RunDefaults { connection: Setting::Inherit, model: Setting::Value(model.into()), persona: Setting::Inherit, effort: Setting::Value(effort.into()) };
        exec(&client, Command::SetRunDefaults(SetRunDefaults { scope, purpose: Purpose::Employee, settings }))
    };
    set(ConfigScope::Global, "fake-global-model", "high").await;
    set(ConfigScope::Project(project_id), "fake-project-model", "medium").await;
    set(ConfigScope::Employee(jean), "fake-jean-model", "max").await;
    set(ConfigScope::Employee(simon), "fake-simon-model", "low").await;
    client.set_user_run_defaults(UserRunDefaults { model: Some("fake-mine".into()), effort: Some("max".into()), ..Default::default() }).await.unwrap();
    let robin = connect_as(dir.path(), "robin").await;
    robin.set_user_run_defaults(UserRunDefaults { model: Some("fake-robins".into()), effort: Some("low".into()), ..Default::default() }).await.unwrap();
    // Robin's ticket, for the part Jean gives away.
    backlog_ticket(&robin, project_id, "Robin's work").await;

    // Robin gives Jean a task; Jean gives Simon a part of it.
    exec(&robin, new_task(project_id, jean, "Plan")).await;
    let config = task_run_config(dir.path(), "Plan").await;
    assert_eq!((config["model"].as_str(), config["effort"].as_str()), (Some("fake-robins"), Some("low")));
    let jean_run: String = db(dir.path())
        .query_row("SELECT r.id FROM runs r JOIN tasks t ON t.conversation_id = r.conversation_id WHERE t.title = 'Plan' ORDER BY r.rowid LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let (ok, text) = bridge
        .employee_act(aurelius_domain::RunId::parse(&jean_run).unwrap(), EmployeeTaskOp::Delegate { employee: "Simon".into(), title: "Server".into(), goal: "Bouw het".into(), criteria: vec![], project: None, ticket: Some("#1".into()) })
        .await
        .unwrap();
    assert!(ok, "{text}");

    // Simon's part runs with Robin's model and effort too: neither Simon's nor the company's, nor mine.
    let config = task_run_config(dir.path(), "Server").await;
    assert_eq!((config["model"].as_str(), config["effort"].as_str()), (Some("fake-robins"), Some("low")));
    assert_eq!(config["model_origin"], "the board member's default");
    stop(dir.path(), daemon).await;
}

/// What My defaults shows is what the next run uses, it survives a restart and
/// clearing it leaves the model's own effort; the overview shows the reader's.
#[tokio::test(flavor = "multi_thread")]
async fn my_chosen_effort_is_shown_kept_used_by_the_next_run_and_can_be_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let CommandResult::ProjectCreated { project_id, .. } = exec(&client, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })).await else { panic!() };
    let alex = employee(&client, project_id, "Alex").await;
    let line = |runs: &aurelius_domain::views::EmployeeRunsView| runs.employees.iter().find(|e| e.employee_id == alex).unwrap().clone();

    // Nothing chosen: no effort goes to the run.
    let before = client.user_run_defaults().await.unwrap();
    assert_eq!(before.defaults, UserRunDefaults::default());
    exec(&client, new_task(project_id, alex, "Nothing")).await;
    assert_eq!(task_run_config(dir.path(), "Nothing").await["effort"], serde_json::Value::Null);

    // My own choice, kept over a restart and used by the next run, also in the overview.
    client.set_user_run_defaults(UserRunDefaults { effort: Some("max".into()), ..Default::default() }).await.unwrap();
    stop(dir.path(), daemon).await;
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let mine = client.user_run_defaults().await.unwrap();
    assert_eq!(mine.defaults.effort.as_deref(), Some("max"));
    assert_eq!(line(&client.employee_runs().await.unwrap()).effort.as_deref(), Some("max"));
    exec(&client, new_task(project_id, alex, "Own")).await;
    assert_eq!(task_run_config(dir.path(), "Own").await["effort"], "max");
    // Another board member sees their own, not mine.
    let robin = connect_as(dir.path(), "robin").await;
    assert_eq!(robin.user_run_defaults().await.unwrap().defaults, UserRunDefaults::default());
    assert_eq!(line(&robin.employee_runs().await.unwrap()).effort, None);

    // Cleared: the model's own effort again.
    client.set_user_run_defaults(UserRunDefaults::default()).await.unwrap();
    exec(&client, new_task(project_id, alex, "Cleared")).await;
    assert_eq!(task_run_config(dir.path(), "Cleared").await["effort"], serde_json::Value::Null);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_effort_my_model_lacks_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let own = |model: &str, effort: &str| UserRunDefaults { model: Some(model.into()), effort: Some(effort.into()), ..Default::default() };
    let refused = client.set_user_run_defaults(own("claude-haiku-4-5", "high")).await.unwrap_err();
    assert!(refused.to_string().contains("claude-haiku-4-5 has no effort setting."), "{refused}");
    let unknown = client.set_user_run_defaults(own("claude-opus-5-5", "ludicrous")).await.unwrap_err();
    assert!(unknown.to_string().contains("effort"), "{unknown}");
    client.set_user_run_defaults(own("claude-opus-5-5", "high")).await.unwrap();
    assert_eq!(client.user_run_defaults().await.unwrap().defaults.effort.as_deref(), Some("high"));
    stop(dir.path(), daemon).await;
}
