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
    backlog_ticket(&client, project_id, "Shared work").await;
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
        .employee_act(aurelius_domain::RunId::parse(&jean_run).unwrap(), EmployeeTaskOp::Delegate { employee: "Simon".into(), title: "Server".into(), goal: "Bouw het".into(), criteria: vec![], project: None, ticket: Some("#1".into()) })
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

/// Ticket #49: what Settings shows is what the next run uses, it survives a
/// restart, Default brings the company's back and a level the model lacks is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_chosen_effort_is_shown_kept_used_by_the_next_run_and_can_be_reset() {
    use aurelius_domain::views::RunValueSource;
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let CommandResult::ProjectCreated { project_id, .. } = exec(&client, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })).await else { panic!() };
    let alex = employee(&client, project_id, "Alex").await;
    let line = |runs: &aurelius_domain::views::EmployeeRunsView| runs.employees.iter().find(|e| e.employee_id == alex).unwrap().clone();
    let set = |scope, effort: Setting<String>| Command::SetRunDefaults(SetRunDefaults { scope, purpose: Purpose::Employee, settings: RunDefaults { effort, ..RunDefaults::default() } });

    // Nothing chosen: no effort goes to the run and the overview says so.
    let before = line(&client.employee_runs().await.unwrap());
    assert_eq!((before.effort.as_deref(), before.effort_source), (None, RunValueSource::Runtime));
    exec(&client, new_task(project_id, alex, "Nothing")).await;
    assert_eq!(task_run_config(dir.path(), "Nothing").await["effort"], serde_json::Value::Null);

    // The employee's own choice, kept over a restart and used by its next run.
    exec(&client, set(ConfigScope::Employee(alex), Setting::Value("max".into()))).await;
    stop(dir.path(), daemon).await;
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let own = line(&client.employee_runs().await.unwrap());
    assert_eq!((own.effort.as_deref(), own.effort_source), (Some("max"), RunValueSource::Employee));
    exec(&client, new_task(project_id, alex, "Own")).await;
    assert_eq!(task_run_config(dir.path(), "Own").await["effort"], "max");

    // Default: the company's level again.
    exec(&client, set(ConfigScope::Global, Setting::Value("low".into()))).await;
    exec(&client, set(ConfigScope::Employee(alex), Setting::Inherit)).await;
    let reset = line(&client.employee_runs().await.unwrap());
    assert_eq!((reset.effort.as_deref(), reset.effort_source), (Some("low"), RunValueSource::Company));
    exec(&client, new_task(project_id, alex, "Reset")).await;
    assert_eq!(task_run_config(dir.path(), "Reset").await["effort"], "low");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_effort_the_model_lacks_is_refused_and_a_stored_one_is_left_out_of_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let CommandResult::ProjectCreated { project_id, .. } = exec(&client, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })).await else { panic!() };
    let alex = employee(&client, project_id, "Alex").await;
    let settings = |model: &str, effort: &str| RunDefaults { model: Setting::Value(model.into()), effort: Setting::Value(effort.into()), ..RunDefaults::default() };
    let write = |settings| Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Employee(alex), purpose: Purpose::Employee, settings });

    let refused = client.execute(envelope(write(settings("claude-haiku-4-5", "high")), None)).await.unwrap_err();
    assert!(refused.to_string().contains("claude-haiku-4-5 has no effort setting."), "{refused}");
    let unknown = client.execute(envelope(write(settings("claude-opus-5-5", "ludicrous")), None)).await.unwrap_err();
    assert!(unknown.to_string().contains("Effort must be one of"), "{unknown}");

    // Moving to a model without it is allowed; the run and the overview drop the level.
    exec(&client, write(settings("claude-opus-5-5", "high"))).await;
    exec(&client, write(settings("claude-haiku-4-5", "high"))).await;
    let runs = client.employee_runs().await.unwrap();
    let line = runs.employees.iter().find(|e| e.employee_id == alex).unwrap();
    assert_eq!((line.effort.as_deref(), line.unsupported_effort.as_deref()), (None, Some("high")));
    assert!(line.supported_efforts.is_empty());
    exec(&client, new_task(project_id, alex, "Dropped")).await;
    assert_eq!(task_run_config(dir.path(), "Dropped").await["effort"], serde_json::Value::Null);
    stop(dir.path(), daemon).await;
}
