//! Employees work on tasks in a conversation: they delegate parts to
//! colleagues and get the results back there, hand a task over, keep their
//! own memory, ask the user only when needed, and files they name open in
//! the app.

mod common;
use common::*;

use aurelius_client::{Client, bridge::BridgeClient, bridge::PermissionAnswer, envelope};
use aurelius_domain::{
    Command, EmployeeId, ProjectId, RunId, TaskId,
    conversation::MessageRole,
    employee::CreateEmployee,
    project::CreateProject,
    task::{AssignTask, CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, EmployeeTaskOp, TaskView};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// An employee's action during `run`, over the bridge channel (as the agent bridge does).
async fn act(root: &Path, run: RunId, op: EmployeeTaskOp) -> Result<(bool, String), aurelius_client::ClientError> {
    BridgeClient::connect(root).await?.employee_act(run, op).await
}

/// An employee's memory action during `run`.
async fn memory(root: &Path, run: RunId, op: aurelius_domain::memory::EmployeeMemoryOp) -> Result<(bool, String), aurelius_client::ClientError> {
    BridgeClient::connect(root).await?.memory_run(run, op).await
}

/// Claude Code asks permission for a tool during `run`.
async fn ask(root: &Path, run: RunId, call: &str, tool: &str, input: serde_json::Value) -> Result<PermissionAnswer, aurelius_client::ClientError> {
    BridgeClient::connect(root).await?.permission_ask(run, Some(call.into()), tool.into(), input).await
}

async fn employee(client: &Client, project_id: ProjectId, name: &str, role: &str) -> EmployeeId {
    match client
        .execute(envelope(Command::CreateEmployee(CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: role.into(), title: None, reports_to: None, instructions: None, folder: None }), None))
        .await
        .unwrap()
        .result
    {
        CommandResult::EmployeeCreated { employee_id, .. } => employee_id,
        other => panic!("{other:?}"),
    }
}

/// A project with Alex (backend) and Iris (frontend), peers who may give each
/// other work (the org chart is not followed).
async fn setup(client: &Client) -> (ProjectId, EmployeeId, EmployeeId) {
    free_chart(client).await;
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let alex = employee(client, project_id, "Alex", "Backend developer").await;
    let iris = employee(client, project_id, "Iris", "Frontend designer").await;
    backlog_ticket(client, project_id, "Shared work").await;
    (project_id, alex, iris)
}

async fn create(client: &Client, project_id: ProjectId, employee_id: Option<EmployeeId>, title: &str) -> TaskId {
    let cmd = CreateTask { confirm_secret: false, files: vec![], run: Default::default(), project_id, employee_id, title: title.into(), goal: format!("Werk aan {title}"), acceptance_criteria: vec![], depends_on: vec![] };
    match client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result {
        CommandResult::TaskCreated { task_id } => task_id,
        other => panic!("{other:?}"),
    }
}

async fn wait_task(client: &Client, id: TaskId, done: impl Fn(&TaskView) -> bool) -> TaskView {
    for _ in 0..600 {
        if let Some(t) = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == id) {
            if done(&t) {
                return t;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("task did not reach the expected state: {:?}", client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == id));
}

/// The running run of a task's conversation.
async fn running_run(root: &Path, task: TaskId) -> RunId {
    let conn = db(root);
    for _ in 0..400 {
        if let Ok(run) = conn.query_row(
            "SELECT id FROM runs WHERE task_id = ?1 AND conversation_id IS NOT NULL AND status IN ('starting','running','waiting')",
            [task.to_string()],
            |r| r.get::<_, String>(0),
        ) {
            return RunId::parse(&run).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no running run for the task");
}

/// The fake, but an employee's first run on a task waits until released (so a
/// test can act during it, like Claude Code through the bridge), and may
/// write a file in its folder and name it.
struct Holding {
    release: Arc<tokio::sync::Notify>,
    hold: &'static str,
    /// Write a screenshot here and name it in the answer.
    file: Option<std::path::PathBuf>,
}

#[async_trait::async_trait]
impl AgentRuntime for Holding {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime { delay: Duration::from_millis(1), ..Default::default() }.probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let system = request.system.clone().unwrap_or_default();
        let first = self.hold.split('|').any(|h| system.contains(&format!("You are {h}"))) &&system.contains("This conversation is a task you took on") && request.turns.len() == 1 && !request.turns.iter().any(|t| t.text.contains("[Task message"));
        let mut text = fake::answer_for(&request);
        if let (true, Some(folder)) = (first, &self.file) {
            let shot = folder.join("scherm 1.png");
            std::fs::write(&shot, [0x89, b'P', b'N', b'G', 1, 2, 3]).unwrap();
            text = format!("## Result\n\nThe screenshot is {}.", shot.display());
        }
        let release = self.release.clone();
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            if first {
                tokio::select! {
                    _ = release.notified() => {}
                    _ = cancel_rx.changed() => {
                        let _ = tx.send(RuntimeEvent::Interrupted).await;
                        return;
                    }
                }
            }
            let _ = tx.send(RuntimeEvent::TextDelta(text)).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

fn holding(hold: &'static str, file: Option<std::path::PathBuf>) -> (Arc<Holding>, Arc<tokio::sync::Notify>) {
    let release = Arc::new(tokio::sync::Notify::new());
    (Arc::new(Holding { release: release.clone(), hold, file }), release)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_employee_delegates_a_part_and_finishes_with_everything() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, iris) = setup(&client).await;
    let parent = create(&client, project, Some(alex), "Dark mode").await;

    // While Alex works, he gives the UI to Iris.
    let run = running_run(dir.path(), parent).await;
    let at_work = client.employees().await.unwrap().employees.into_iter().find(|m| m.id == alex).unwrap();
    assert!(at_work.agents.iter().any(|a| a.kind == "conversation"), "{:?}", at_work.agents);
    let (ok, text) = act(dir.path(), run, EmployeeTaskOp::Delegate { employee: "iris".into(), title: "Dark mode UI".into(), goal: "Maak het donkere thema".into(), criteria: vec![], project: None, ticket: Some("#1".into()) })
        .await
        .unwrap();
    assert!(ok, "{text}");
    let (ok, _) = act(dir.path(), run, EmployeeTaskOp::Delegate { employee: "Nobody".into(), title: "x".into(), goal: String::new(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(!ok, "only colleagues of the project");
    release.notify_one();

    // Iris does her part in a conversation of her own; Alex gets it back and finishes.
    let t = wait_task(&client, parent, |t| t.status.is_terminal()).await;
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    assert_eq!(t.children.len(), 1);
    let child = client.tasks(None).await.unwrap().tasks.into_iter().find(|c| c.id == t.children[0]).unwrap();
    assert_eq!((child.status, child.employee_id, child.parent_task_id, child.delegated_by.as_deref()), (TaskStatus::Succeeded, Some(iris), Some(parent), Some("Alex")));
    let child_messages = client.messages(child.conversation_id.unwrap()).await.unwrap().messages;
    assert!(child_messages[0].text.contains("Alex delegated this to you as part of “Dark mode”"), "{}", child_messages[0].text);

    let messages = client.messages(t.conversation_id.unwrap()).await.unwrap().messages;
    let all: Vec<String> = messages.iter().map(|m| format!("{:?} {:?} {}", m.update, m.author, m.text)).collect();
    assert!(messages.iter().any(|m| m.update.as_deref() == Some("finding") && m.author.as_deref() == Some("Iris") && m.text.contains("Finished “Dark mode UI”")), "{all:#?}");
    assert!(messages.iter().any(|m| m.update.as_deref() == Some("status") && m.text.contains("Your colleagues finished")), "{all:#?}");
    let last = messages.last().unwrap();
    assert!(last.role == MessageRole::Assistant && last.update.is_none(), "Alex finishes with his report: {all:#?}");
    assert!(t.result.unwrap().starts_with("# Dark mode"));

    // Alex followed up, not the user: no approvals; only the user's own task notifies.
    assert!(client.approvals().await.unwrap().approvals.iter().all(|a| a.status != aurelius_domain::approval::ApprovalStatus::Pending));
    let notes = client.notifications().await.unwrap().notifications;
    assert!(notes.iter().all(|n| n.task_id != Some(child.id)), "{notes:?}");
    assert!(notes.iter().any(|n| n.task_id == Some(parent) && n.title == "Result ready"));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_employee_hands_a_task_over_and_the_user_can_assign() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, iris) = setup(&client).await;
    let id = create(&client, project, Some(alex), "Overdracht").await;
    let run = running_run(dir.path(), id).await;
    let (ok, text) = act(dir.path(), run, EmployeeTaskOp::HandOver { employee: "Iris".into(), reason: "Dit is UI-werk".into() }).await.unwrap();
    assert!(ok, "{text}");
    release.notify_one();
    let t = wait_task(&client, id, |t| t.status.is_terminal()).await;
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    assert_eq!(t.employee_id, Some(iris));
    assert_eq!(t.handoffs.len(), 1);
    assert_eq!((t.handoffs[0].from.as_str(), t.handoffs[0].to.as_str(), t.handoffs[0].reason.as_str()), ("Alex", "Iris", "Dit is UI-werk"));
    // Iris worked on it in the task's conversation, now hers.
    let in_task = client.messages(t.conversation_id.expect("a task conversation")).await.unwrap();
    assert_eq!((in_task.employee_id, in_task.project_id), (Some(iris), Some(project)));

    // A draft gets an employee from the user and starts.
    let draft = create(&client, project, None, "Later").await;
    client.execute(envelope(Command::AssignTask(AssignTask { task_id: draft, employee_id: Some(iris) }), None)).await.unwrap();
    let t = wait_task(&client, draft, |t| t.status.is_terminal()).await;
    assert_eq!((t.status, t.employee_id), (TaskStatus::Succeeded, Some(iris)));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_employee_that_may_not_delegate_does_it_itself() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, _) = setup(&client).await;
    let id = create(&client, project, Some(alex), "Eigen werk").await;
    let run = running_run(dir.path(), id).await;
    let (ok, _) = act(dir.path(), run, EmployeeTaskOp::Delegate { employee: "Iris".into(), title: "Een".into(), goal: String::new(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(ok, "delegation is allowed by default");

    rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite"))
        .unwrap()
        .execute("UPDATE employees SET config_json = json_set(config_json, '$.task_policy.delegation_allowed', json('false')) WHERE id = ?1", [alex.to_string()])
        .unwrap();
    let (ok, _) = act(dir.path(), run, EmployeeTaskOp::Delegate { employee: "Iris".into(), title: "Nog een".into(), goal: String::new(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(!ok, "delegation is not allowed for Alex");
    assert!(client.tasks(None).await.unwrap().tasks.iter().all(|t| t.title != "Nog een"));
    release.notify_one();
    wait_task(&client, id, |t| t.status.is_terminal()).await;
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_tasks_given_from_one_conversation_are_answered_in_one_run() {
    use aurelius_domain::{conversation::ChatStart, project_conversation::SendProjectMessage};
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Iris|Bob", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, _, _) = setup(&client).await;
    employee(&client, project, "Bob", "Illustrator").await;

    // The user talks with the assistant of the project, which gives work to two employees.
    let send = SendProjectMessage { confirm_secret: false, project_id: project, text: "Logo en icoon graag".into(), conversation_id: None, images: Vec::new(), files: Vec::new(), interrupt: false, start: ChatStart::default() };
    let CommandResult::ProjectMessageSent { conversation_id, run_id: first_run, .. } = client.send_project_message(send).await.unwrap().result else { panic!() };
    let conn = db(dir.path());
    let runs = || conn.query_row("SELECT COUNT(*) FROM runs WHERE conversation_id = ?1", [conversation_id.to_string()], |r| r.get::<_, i64>(0)).unwrap();
    for (to, title) in [("Iris", "Logo"), ("Bob", "Icoon")] {
        let (ok, text) = act(dir.path(), first_run, EmployeeTaskOp::Delegate { employee: to.into(), title: title.into(), goal: title.into(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
        assert!(ok, "{text}");
    }
    let named = |title: &'static str| {
        let client = &client;
        async move {
            loop {
                if let Some(t) = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.title == title) {
                    break t.id;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    };
    let (logo, icon) = (named("Logo").await, named("Icoon").await);
    running_run(dir.path(), logo).await;
    running_run(dir.path(), icon).await;
    // The assistant's own answer is done before any result comes in.
    for _ in 0..400 {
        let live: bool = conn
            .query_row("SELECT status IN ('created','starting','running','waiting','cancelling') FROM runs WHERE id = ?1", [first_run.to_string()], |r| r.get(0))
            .unwrap();
        if !live {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // The first result comes in: the assistant waits for the other one.
    release.notify_one();
    let first_done = loop {
        let tasks = client.tasks(None).await.unwrap().tasks;
        if let Some(t) = tasks.iter().find(|t| (t.id == logo || t.id == icon) && t.status.is_terminal()) {
            break t.id;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(runs(), 1, "no answer on one result while the other task is open");

    // The second one too: one run answers both.
    release.notify_one();
    let other = if first_done == logo { icon } else { logo };
    wait_task(&client, other, |t| t.status.is_terminal()).await;
    for _ in 0..400 {
        if runs() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(runs(), 2, "exactly one run for both results");
    let messages = client.messages(conversation_id).await.unwrap().messages;
    let all: Vec<String> = messages.iter().map(|m| format!("{:?} {:?} {:?} {}", m.role, m.update, m.author, m.text)).collect();
    assert_eq!(messages.iter().filter(|m| m.update.as_deref() == Some("finding")).count(), 2, "{all:#?}");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn delegated_work_starts_in_the_mode_of_the_one_who_gave_it() {
    use aurelius_domain::autonomy::{AgentMode, SetAgentMode};
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex|Iris|Bob", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    // Alex, Iris and Bob each hold a live run: Dana's task must still be allowed to start.
    client.set_load_limits(aurelius_protocol::LoadLimitsUpdate { max_runs: Some(8), ..Default::default() }).await.unwrap();
    let (project, alex, _) = setup(&client).await;
    employee(&client, project, "Bob", "Illustrator").await;
    employee(&client, project, "Dana", "Copywriter").await;
    let set_mode = |conversation_id: aurelius_domain::ConversationId, mode: AgentMode| client.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id, mode }), None));
    let task_named = |title: &'static str| {
        let client = &client;
        async move {
            loop {
                if let Some(t) = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.title == title) {
                    break t.id;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    };
    let mode_of = |id: TaskId| {
        let client = &client;
        async move {
            let t = wait_task(client, id, |t| t.conversation_id.is_some()).await;
            client.messages(t.conversation_id.unwrap()).await.unwrap().agent_mode
        }
    };

    // The user's task for Alex runs in Auto.
    let parent = create(&client, project, Some(alex), "Hoofdtaak").await;
    let alex_run = running_run(dir.path(), parent).await;
    let alex_conv = wait_task(&client, parent, |t| t.conversation_id.is_some()).await.conversation_id.unwrap();
    set_mode(alex_conv, AgentMode::Auto).await.unwrap();

    // Alex gives the logo to Iris: her task starts in Auto and her commands go ahead unasked.
    let (ok, text) = act(dir.path(), alex_run, EmployeeTaskOp::Delegate { employee: "Iris".into(), title: "Logo".into(), goal: "Maak een logo".into(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(ok, "{text}");
    let logo = task_named("Logo").await;
    assert_eq!(mode_of(logo).await, AgentMode::Auto);
    let iris_run = running_run(dir.path(), logo).await;
    let answer = ask(dir.path(), iris_run, "call-1", "Bash", serde_json::json!({ "command": "cargo test" })).await.unwrap();
    assert!(answer.allow, "Auto allows commands");
    assert!(client.approvals().await.unwrap().approvals.iter().all(|a| a.status != aurelius_domain::approval::ApprovalStatus::Pending));

    // And on down the chain: Iris gives a part to Bob, also in Auto; the user can still change it.
    let (ok, text) = act(dir.path(), iris_run, EmployeeTaskOp::Delegate { employee: "Bob".into(), title: "Icoon".into(), goal: "Teken het icoon".into(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(ok, "{text}");
    let icon = task_named("Icoon").await;
    assert_eq!(mode_of(icon).await, AgentMode::Auto);
    let icon_conv = wait_task(&client, icon, |t| t.conversation_id.is_some()).await.conversation_id.unwrap();
    set_mode(icon_conv, AgentMode::Ask).await.unwrap();
    assert_eq!(client.messages(icon_conv).await.unwrap().agent_mode, AgentMode::Ask);
    let bob_run = running_run(dir.path(), icon).await;
    set_mode(icon_conv, AgentMode::AcceptEdits).await.unwrap();

    // Plan is not passed on: the colleague starts as usual, in Ask.
    set_mode(alex_conv, AgentMode::Plan).await.unwrap();
    let (ok, text) = act(dir.path(), alex_run, EmployeeTaskOp::Delegate { employee: "Dana".into(), title: "Huisstijl".into(), goal: "Kies kleuren".into(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(ok, "{text}");
    assert_eq!(mode_of(task_named("Huisstijl").await).await, AgentMode::Ask);

    // A hand-over to management (Alex is the CEO) passes nothing on: the task starts in Plan and Alex chooses.
    let (ok, text) = act(dir.path(), bob_run, EmployeeTaskOp::HandOver { employee: "Alex".into(), reason: "Hij kent het merk".into() }).await.unwrap();
    assert!(ok, "{text}");
    tokio::time::sleep(Duration::from_millis(100)).await;
    release.notify_waiters();
    let handed = wait_task(&client, icon, |t| t.employee_id == Some(alex) && t.conversation_id.is_some_and(|c| c != icon_conv)).await;
    assert_eq!(client.messages(handed.conversation_id.unwrap()).await.unwrap().agent_mode, AgentMode::Plan);

    // Let every held first run go (Alex' run on the handed-over task is held too) until the logo is done.
    for _ in 0..400 {
        release.notify_waiters();
        if client.tasks(None).await.unwrap().tasks.iter().any(|t| t.id == logo && t.status.is_terminal()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    wait_task(&client, logo, |t| t.status.is_terminal()).await;
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn delegated_tasks_and_subagents_run_on_their_own_defaults() {
    use aurelius_domain::connection::{ConfigScope, Purpose, RunDefaults, SetRunDefaults};
    use aurelius_domain::employee::Setting;
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex|Bob", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, iris) = setup(&client).await;
    let bob = employee(&client, project, "Bob", "Illustrator").await;
    let set = |scope, purpose, model: &str, effort: Option<&str>| {
        let settings = RunDefaults { model: Setting::Value(model.into()), effort: effort.map(|e| Setting::Value(e.into())).unwrap_or_default(), ..Default::default() };
        client.execute(envelope(Command::SetRunDefaults(SetRunDefaults { scope, purpose, settings }), None))
    };
    let task_named = |title: &'static str| {
        let client = &client;
        async move {
            loop {
                if let Some(t) = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.title == title) {
                    break t.id;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    };
    // The configuration of the first run an employee did on a task.
    let conn = db(dir.path());
    let config = |task: TaskId, employee: EmployeeId| {
        let conn = &conn;
        async move {
            for _ in 0..400 {
                if let Ok(json) = conn.query_row(
                    "SELECT r.resolved_config_json FROM runs r JOIN conversations c ON c.id = r.conversation_id WHERE r.task_id = ?1 AND c.employee_id = ?2 ORDER BY r.rowid LIMIT 1",
                    [task.to_string(), employee.to_string()],
                    |r| r.get::<_, String>(0),
                ) {
                    return serde_json::from_str::<serde_json::Value>(&json).unwrap();
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            panic!("no run of that employee on the task");
        }
    };
    set(ConfigScope::Employee(iris), Purpose::Employee, "fake-iris-own", Some("high")).await.unwrap();
    set(ConfigScope::Employee(alex), Purpose::Subagent, "opus", None).await.unwrap();
    set(ConfigScope::Global, Purpose::Subagent, "haiku", None).await.unwrap();

    // The user's own task for Alex: the employee defaults, Alex's subagent model.
    let own = create(&client, project, Some(alex), "Eigen werk").await;
    let alex_config = config(own, alex).await;
    assert_eq!(alex_config["subagent_model"], "opus");
    let alex_run = running_run(dir.path(), own).await;

    // A delegated task runs on the colleague's own model, effort and subagent model.
    let (ok, text) = act(dir.path(), alex_run, EmployeeTaskOp::Delegate { employee: "Iris".into(), title: "Gedeeld".into(), goal: "Deel een".into(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(ok, "{text}");
    let delegated = config(task_named("Gedeeld").await, iris).await;
    assert_eq!((delegated["model"].as_str(), delegated["effort"].as_str()), (Some("fake-iris-own"), Some("high")), "{delegated}");
    assert_eq!(delegated["subagent_model"], "haiku", "the global subagent model");

    // A task the user gives Iris keeps her own model.
    let direct = create(&client, project, Some(iris), "Direct").await;
    assert_eq!(config(direct, iris).await["model"], "fake-iris-own");

    // A hand-over too (Iris starts once Bob's run has ended).
    let bobs = create(&client, project, Some(bob), "Overdracht").await;
    let bob_run = running_run(dir.path(), bobs).await;
    let (ok, text) = act(dir.path(), bob_run, EmployeeTaskOp::HandOver { employee: "Iris".into(), reason: "UI".into() }).await.unwrap();
    assert!(ok, "{text}");
    release.notify_waiters();
    assert_eq!(config(bobs, iris).await["model"], "fake-iris-own");

    wait_task(&client, own, |t| t.status.is_terminal()).await;
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_result_that_comes_in_while_the_employee_works_is_not_lost() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, _) = setup(&client).await;
    let parent = create(&client, project, Some(alex), "Dark mode").await;
    let run = running_run(dir.path(), parent).await;
    let (ok, text) = act(dir.path(), run, EmployeeTaskOp::Delegate { employee: "Iris".into(), title: "Dark mode UI".into(), goal: "Thema".into(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(ok, "{text}");
    // Iris finishes while Alex' first run still goes.
    let child = wait_task(&client, parent, |t| t.children.len() == 1).await.children[0];
    wait_task(&client, child, |t| t.status.is_terminal()).await;
    release.notify_one();

    // Alex' first answer never saw her result: he goes on once more before the report.
    let t = wait_task(&client, parent, |t| t.status.is_terminal()).await;
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    let messages = client.messages(t.conversation_id.unwrap()).await.unwrap().messages;
    let all: Vec<String> = messages.iter().map(|m| format!("{:?} {:?} {}", m.update, m.author, m.text)).collect();
    let found = messages.iter().position(|m| m.update.as_deref() == Some("finding")).unwrap_or_else(|| panic!("{all:#?}"));
    let again = messages.iter().position(|m| m.update.as_deref() == Some("status") && m.text.contains("Your colleagues finished")).unwrap_or_else(|| panic!("{all:#?}"));
    assert!(found < again, "{all:#?}");
    let last = messages.last().unwrap();
    assert!(last.role == MessageRole::Assistant && last.update.is_none(), "the report comes after the result: {all:#?}");
    stop(dir.path(), daemon).await;
}

/// `Holding`, but while `broken` Alex's runs cannot be started (as Claude Code
/// on a command line that is too long).
struct Unstartable {
    inner: Arc<Holding>,
    broken: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl AgentRuntime for Unstartable {
    async fn probe(&self) -> RuntimeCapabilities {
        self.inner.probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        if self.broken.load(std::sync::atomic::Ordering::SeqCst) && request.system.as_deref().is_some_and(|s| s.contains("You are Alex")) {
            return Err(RuntimeError::Unavailable("could not start claude.exe: The filename or extension is too long. (os error 206)".into()));
        }
        self.inner.start(request).await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_whose_run_cannot_start_after_its_colleagues_waits_for_the_user_and_then_finishes() {
    use std::sync::atomic::Ordering;
    let dir = tempfile::tempdir().unwrap();
    let (inner, release) = holding("Alex|Iris", None);
    let runtime = Arc::new(Unstartable { inner, broken: false.into() });
    let daemon = start_with(dir.path(), runtime.clone()).await;
    let client = connect(dir.path()).await;
    let (project, alex, _) = setup(&client).await;
    let parent = create(&client, project, Some(alex), "Dark mode").await;
    let run = running_run(dir.path(), parent).await;
    let (ok, text) = act(dir.path(), run, EmployeeTaskOp::Delegate { employee: "Iris".into(), title: "Dark mode UI".into(), goal: "Thema".into(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(ok, "{text}");
    // Alex finishes his own part first and waits for Iris.
    release.notify_one();
    wait_task(&client, parent, |t| t.status == TaskStatus::Waiting).await;

    // Iris finishes; the run that should bring everything together cannot start.
    runtime.broken.store(true, Ordering::SeqCst);
    release.notify_one();
    let conn = db(dir.path());
    let mut dead = false;
    for _ in 0..1200 {
        dead = conn
            .query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE task_id = ?1 AND status = 'failed')", [parent.to_string()], |r| r.get::<_, bool>(0))
            .unwrap();
        if dead {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(dead, "Alex' last run failed to start");
    // Not "running" forever: it needs the user.
    let t = wait_task(&client, parent, |t| t.status != TaskStatus::Running).await;
    assert_eq!(t.status, TaskStatus::Waiting, "{t:?}");
    let messages = client.messages(t.conversation_id.unwrap()).await.unwrap().messages;
    assert!(messages.iter().any(|m| m.text.contains("could not be started")), "the reason is in the conversation");

    // The user sends again in the task's conversation: Alex answers and the task is done.
    runtime.broken.store(false, Ordering::SeqCst);
    let send = Command::SendEmployeeMessage(aurelius_domain::employee::SendEmployeeMessage { confirm_secret: false, employee_id: alex, text: "Probeer opnieuw".into(), conversation_id: t.conversation_id.unwrap(), images: Vec::new(), files: Vec::new(), interrupt: false });
    client.execute(envelope(send, None)).await.unwrap();
    let t = wait_task(&client, parent, |t| t.status.is_terminal()).await;
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    assert!(t.result.unwrap().starts_with("# Dark mode"));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hand_over_starts_even_when_the_old_run_was_still_going() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, iris) = setup(&client).await;
    let id = create(&client, project, Some(alex), "Overdracht").await;
    let run = running_run(dir.path(), id).await;
    let (ok, text) = act(dir.path(), run, EmployeeTaskOp::HandOver { employee: "Iris".into(), reason: "UI".into() }).await.unwrap();
    assert!(ok, "{text}");
    // The task job runs while Alex' run still goes: it finds that run and does nothing.
    let conn = db(dir.path());
    let mut done = false;
    for _ in 0..400 {
        done = conn.query_row("SELECT state = 'done' FROM jobs WHERE id = ?1", [id.to_string()], |r| r.get::<_, bool>(0)).unwrap_or(false);
        if done {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(done, "the task job ran during Alex' run");
    release.notify_one();
    let t = wait_task(&client, id, |t| t.status.is_terminal()).await;
    assert_eq!((t.status, t.employee_id), (TaskStatus::Succeeded, Some(iris)), "{t:?}");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn while_working_an_employee_asks_in_the_conversation_and_the_task_shows_it() {
    use aurelius_domain::approval::DecideApproval;
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, _) = setup(&client).await;
    let id = create(&client, project, Some(alex), "Bouwen").await;
    let run = running_run(dir.path(), id).await;
    let root = dir.path().to_path_buf();
    let asked = tokio::spawn(async move { ask(&root, run, "call-1", "Bash", serde_json::json!({ "command": "cargo test" })).await });
    // The question belongs to the task's conversation and shows on the task.
    let t = wait_task(&client, id, |t| t.pending_approvals == 1).await;
    let approval = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.run_id == run).expect("pending");
    assert_eq!(approval.conversation_id, t.conversation_id);
    client
        .execute(envelope(Command::DecideApproval(DecideApproval { approval_id: approval.id, action_digest: approval.action_digest.clone(), approve: true, always_allow: false }), None))
        .await
        .unwrap();
    assert!(asked.await.unwrap().unwrap().allow);
    // Decided: the request is no longer among the notifications.
    assert!(client.notifications().await.unwrap().notifications.iter().all(|n| n.title != "Approval needed"));
    release.notify_one();
    let t = wait_task(&client, id, |t| t.status.is_terminal()).await;
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn files_an_employee_names_are_kept_with_its_answer_and_open_in_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex", Some(folder.path().to_path_buf()));
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let CommandResult::ProjectCreated { project_id: project, .. } = client
        .execute(envelope(Command::CreateProject(CreateProject { name: "Shots".into(), description: None, folder: Some(folder.path().display().to_string()) }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    let alex = employee(&client, project, "Alex", "Developer").await;
    let id = create(&client, project, Some(alex), "Screenshots").await;
    let _ = running_run(dir.path(), id).await;
    release.notify_one();
    let t = wait_task(&client, id, |t| t.status.is_terminal()).await;
    let messages = client.messages(t.conversation_id.unwrap()).await.unwrap().messages;
    let answer = messages.iter().rev().find(|m| m.role == MessageRole::Assistant && m.update.is_none()).unwrap();
    assert_eq!(answer.files.len(), 1, "{:?}", answer.files);
    assert_eq!((answer.files[0].name.as_str(), answer.files[0].media_type.as_str()), ("scherm 1.png", "image/png"));
    let (name, media, bytes) = client.file(&answer.files[0].id).await.unwrap();
    assert_eq!((name.as_str(), media.as_str(), bytes.as_slice()), ("scherm 1.png", "image/png", &[0x89, b'P', b'N', b'G', 1, 2, 3][..]));
    assert!(client.file(&TaskId::new().to_string()).await.is_err(), "only files kept with a message");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_employee_keeps_its_own_memory_and_never_overrules_the_user() {
    use aurelius_domain::memory::{MemoryKind, MemoryScope, Remember};
    use aurelius_domain::memory::EmployeeMemoryOp;
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, _) = setup(&client).await;
    let id = create(&client, project, Some(alex), "Vraag").await;
    let run = running_run(dir.path(), id).await;

    let remember = |content: &str| EmployeeMemoryOp::Remember { kind: "decision".into(), scope: "project".into(), content: content.into() };
    let (ok, text) = memory(dir.path(), run, remember("We bewaren alle data lokaal in SQLite")).await.unwrap();
    assert!(ok && text == "Remembered.", "{text}");
    let (ok, text) = memory(dir.path(), run, remember("We bewaren alle data lokaal in SQLite.")).await.unwrap();
    assert!(ok && text == "Already in memory.", "{text}");
    let (ok, text) = memory(dir.path(), run, EmployeeMemoryOp::Recall { query: "sqlite opslag".into() }).await.unwrap();
    assert!(ok && text.contains("SQLite") && text.contains("Decision"), "{text}");
    let (ok, _) = memory(dir.path(), run, EmployeeMemoryOp::Remember { kind: "decision".into(), scope: "project".into(), content: "api key: sk-abcdefghijklmnopqrstuvwxyz123456".into() }).await.unwrap();
    assert!(!ok, "secrets are never stored");
    client
        .execute(envelope(Command::Remember(Remember { scope: MemoryScope::Project(project), kind: MemoryKind::Decision, content: "Releases gaan altijd eerst langs de gebruiker".into(), pinned: false }), None))
        .await
        .unwrap();
    let (ok, text) = memory(dir.path(), run, remember("Releases gaan niet meer langs de gebruiker")).await.unwrap();
    assert!(ok && text.contains("proposal"), "{text}");
    let memory = client.memory(Some(project)).await.unwrap().items;
    let note = memory.iter().find(|m| m.content == "We bewaren alle data lokaal in SQLite").unwrap();
    assert_eq!(note.decided_by.as_deref(), Some("employee"));
    assert_eq!(note.sources[0].label, "Noted by Alex");
    release.notify_one();
    wait_task(&client, id, |t| t.status.is_terminal()).await;
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_growing_memory_is_consolidated_by_itself_and_on_request() {
    use aurelius_domain::memory::{ConsolidateMemory, MemoryKind, MemoryScope, MemoryStatus, PinMemory, Remember};
    use aurelius_domain::memory::EmployeeMemoryOp;
    use aurelius_protocol::MemoryView;
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, _) = setup(&client).await;
    let id = create(&client, project, Some(alex), "Vraag").await;
    let run = running_run(dir.path(), id).await;
    let project_items = |items: Vec<MemoryView>| items.into_iter().filter(|m| m.scope == "project").collect::<Vec<_>>();
    let wait = |done: Box<dyn Fn(&[MemoryView]) -> bool>| {
        let client = &client;
        async move {
            for _ in 0..400 {
                let items = project_items(client.memory(Some(project)).await.unwrap().items);
                if done(&items) {
                    return items;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            panic!("memory did not reach the expected state: {:#?}", client.memory(Some(project)).await.unwrap().items);
        }
    };

    // What the user wrote or pinned is never merged.
    let CommandResult::MemoryStored { memory_id: users } = client
        .execute(envelope(Command::Remember(Remember { scope: MemoryScope::Project(project), kind: MemoryKind::Decision, content: "Releases gaan altijd eerst langs de gebruiker".into(), pinned: false }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    let note = |n: u32| EmployeeMemoryOp::Remember { kind: "decision".into(), scope: "project".into(), content: format!("Note{n} alpha{n} beta{n} gamma{n} delta{n}") };
    memory(dir.path(), run, note(0)).await.unwrap();
    let pinned = project_items(client.memory(Some(project)).await.unwrap().items).into_iter().find(|m| m.content.starts_with("Note0 ")).unwrap().id;
    client.execute(envelope(Command::PinMemory(PinMemory { memory_id: pinned, pinned: true }), None)).await.unwrap();

    // 29 more notes: below the threshold nothing happens; the 30th mergeable one starts it.
    for n in 1..=29 {
        memory(dir.path(), run, note(n)).await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!project_items(client.memory(Some(project)).await.unwrap().items).iter().any(|m| m.content.ends_with("(merged)")), "29 mergeable notes stay as they are");
    memory(dir.path(), run, note(30)).await.unwrap();
    let items = wait(Box::new(|m: &[MemoryView]| m.iter().filter(|x| x.content.ends_with("(merged)")).count() == 15)).await;
    let active = |items: &[MemoryView], id| items.iter().find(|m| m.id == id).unwrap().status == MemoryStatus::Active;
    assert!(active(&items, users) && active(&items, pinned), "the user's and pinned memories stay");
    let notes: Vec<&MemoryView> = items.iter().filter(|m| m.content.starts_with("Note") && !m.content.ends_with("(merged)")).collect();
    assert_eq!(notes.iter().filter(|m| m.status == MemoryStatus::Superseded).count(), 30, "every merged note is superseded, not erased");
    let point = items.iter().find(|m| m.content.ends_with("(merged)")).unwrap();
    assert_eq!((point.status, point.decided_by.as_deref()), (MemoryStatus::Active, Some("policy")));
    assert!(point.sources.iter().any(|s| s.label == "Noted by Alex"), "provenance survives: {:?}", point.sources);

    // On request it runs at once, also within the day.
    let result = client.execute(envelope(Command::ConsolidateMemory(ConsolidateMemory { scope: MemoryScope::Project(project) }), None)).await.unwrap().result;
    assert_eq!(result, CommandResult::MemoryConsolidationQueued { queued: true });
    let items = wait(Box::new(|m: &[MemoryView]| m.iter().filter(|x| x.status == MemoryStatus::Active && x.content.ends_with("(merged) (merged)")).count() == 8)).await;
    assert_eq!(items.iter().filter(|m| m.status == MemoryStatus::Active && m.content.ends_with("(merged)")).count(), 8, "15 points become 8");
    let global = client.execute(envelope(Command::ConsolidateMemory(ConsolidateMemory { scope: MemoryScope::Global }), None)).await;
    assert!(global.is_err() || !matches!(global.unwrap().result, CommandResult::MemoryConsolidationQueued { .. }), "global memory is never consolidated");

    release.notify_one();
    wait_task(&client, id, |t| t.status.is_terminal()).await;
    stop(dir.path(), daemon).await;
}

/// Let an employee work on one task at a time (or not again).
async fn one_at_a_time(client: &Client, employee_id: EmployeeId, name: &str, role: &str, on: bool) {
    use aurelius_domain::employee::UpdateEmployee;
    let update = UpdateEmployee { access_level: None, tool_policy: None, description: None, employee_id, name: name.into(), role: role.into(), title: None, instructions: None, sequential_tasks: Some(on) };
    client.execute(envelope(Command::UpdateEmployee(update), None)).await.unwrap();
}

/// How many of the employee's tasks run now.
fn running_tasks(root: &Path, employee: EmployeeId) -> i64 {
    db(root).query_row("SELECT COUNT(*) FROM tasks WHERE employee_id = ?1 AND status = 'running'", [employee.to_string()], |r| r.get(0)).unwrap()
}

/// The sequence number of the task's latest event of this type.
fn event_seq(root: &Path, task: TaskId, event: &str) -> i64 {
    db(root)
        .query_row("SELECT COALESCE(MAX(seq), 0) FROM events WHERE aggregate_type = 'task' AND aggregate_id = ?1 AND type = ?2", [task.to_string(), event.to_string()], |r| r.get(0))
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sequential_employee_works_on_its_tasks_one_at_a_time_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, iris) = setup(&client).await;
    one_at_a_time(&client, alex, "Alex", "Backend developer", true).await;
    let summaries = client.employees().await.unwrap().employees;
    assert!(summaries.iter().find(|m| m.id == alex).unwrap().sequential_tasks);
    assert!(!summaries.iter().find(|m| m.id == iris).unwrap().sequential_tasks, "off by default");

    let first = create(&client, project, Some(alex), "Eerste").await;
    let second = create(&client, project, Some(alex), "Tweede").await;
    let third = create(&client, project, Some(alex), "Derde").await;
    running_run(dir.path(), first).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let tasks = client.tasks(None).await.unwrap().tasks;
    for id in [second, third] {
        let t = tasks.iter().find(|t| t.id == id).unwrap();
        assert_eq!((t.status, t.conversation_id), (TaskStatus::Queued, None), "{t:?}");
        let reason: String = db(dir.path())
            .query_row(
                "SELECT json_extract(metadata_json, '$.reason') FROM events WHERE aggregate_type = 'task' AND aggregate_id = ?1 AND type = 'task.queued' ORDER BY seq DESC LIMIT 1",
                [id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(reason, "sequential");
    }
    assert_eq!(running_tasks(dir.path(), alex), 1);

    // Each starts when the one before it is done, in the order they were created.
    release.notify_one();
    assert_eq!(wait_task(&client, first, |t| t.status.is_terminal()).await.status, TaskStatus::Succeeded);
    running_run(dir.path(), second).await;
    assert_eq!(running_tasks(dir.path(), alex), 1);
    assert_eq!(client.tasks(None).await.unwrap().tasks.iter().find(|t| t.id == third).unwrap().status, TaskStatus::Queued);
    release.notify_one();
    assert_eq!(wait_task(&client, second, |t| t.status.is_terminal()).await.status, TaskStatus::Succeeded);
    running_run(dir.path(), third).await;
    release.notify_one();
    assert_eq!(wait_task(&client, third, |t| t.status.is_terminal()).await.status, TaskStatus::Succeeded);
    let root = dir.path();
    assert!(event_seq(root, second, "task.started") > event_seq(root, first, "task.succeeded"));
    assert!(event_seq(root, third, "task.started") > event_seq(root, second, "task.succeeded"));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn waiting_for_colleagues_passes_the_turn_on_and_the_task_goes_on_after() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex|Iris", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, _) = setup(&client).await;
    one_at_a_time(&client, alex, "Alex", "Backend developer", true).await;
    let parent = create(&client, project, Some(alex), "Dark mode").await;
    let run = running_run(dir.path(), parent).await;
    let other = create(&client, project, Some(alex), "Logboek").await;
    let (ok, text) = act(dir.path(), run, EmployeeTaskOp::Delegate { employee: "Iris".into(), title: "Dark mode UI".into(), goal: "Thema".into(), criteria: vec![], project: None, ticket: Some("#1".into()) }).await.unwrap();
    assert!(ok, "{text}");
    let child = wait_task(&client, parent, |t| t.children.len() == 1).await.children[0];
    running_run(dir.path(), child).await;

    // Alex finishes his own part and waits for Iris: meanwhile his next task goes.
    release.notify_one();
    wait_task(&client, parent, |t| t.status == TaskStatus::Waiting).await;
    running_run(dir.path(), other).await;
    assert_eq!(running_tasks(dir.path(), alex), 1);

    // Iris is done while the other task runs: Dark mode waits for its turn.
    release.notify_one();
    wait_task(&client, child, |t| t.status.is_terminal()).await;
    let t = wait_task(&client, parent, |t| t.status == TaskStatus::Queued).await;
    assert_eq!(running_tasks(dir.path(), alex), 1, "{t:?}");

    // Then it brings everything together.
    release.notify_one();
    assert_eq!(wait_task(&client, other, |t| t.status.is_terminal()).await.status, TaskStatus::Succeeded);
    let t = wait_task(&client, parent, |t| t.status.is_terminal()).await;
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    let messages = client.messages(t.conversation_id.unwrap()).await.unwrap().messages;
    let all: Vec<String> = messages.iter().map(|m| format!("{:?} {:?} {}", m.update, m.author, m.text)).collect();
    assert!(messages.iter().any(|m| m.update.as_deref() == Some("status") && m.text.contains("Your colleagues finished")), "{all:#?}");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn turning_one_at_a_time_off_starts_the_queued_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let (runtime, release) = holding("Alex", None);
    let daemon = start_with(dir.path(), runtime).await;
    let client = connect(dir.path()).await;
    let (project, alex, _) = setup(&client).await;
    one_at_a_time(&client, alex, "Alex", "Backend developer", true).await;
    let first = create(&client, project, Some(alex), "Eerste").await;
    let second = create(&client, project, Some(alex), "Tweede").await;
    running_run(dir.path(), first).await;

    assert_eq!(client.tasks(None).await.unwrap().tasks.iter().find(|t| t.id == second).unwrap().status, TaskStatus::Queued);

    // Off again: the queued task is free to start, and goes when Alex is free (R2).
    one_at_a_time(&client, alex, "Alex", "Backend developer", false).await;
    assert!(!client.employees().await.unwrap().employees.iter().find(|m| m.id == alex).unwrap().sequential_tasks);
    let queued = |root: &Path| -> i64 { db(root).query_row("SELECT COUNT(*) FROM employee_inbox WHERE task_id = ?1 AND state = 'queued'", [second.to_string()], |r| r.get(0)).unwrap() };
    for _ in 0..400 {
        if queued(dir.path()) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(queued(dir.path()), 1, "in Alex's mailbox");
    assert_eq!(db(dir.path()).query_row("SELECT COUNT(*) FROM runs WHERE task_id = ?1", [second.to_string()], |r| r.get::<_, i64>(0)).unwrap(), 0);
    release.notify_one();
    running_run(dir.path(), second).await;
    release.notify_one();
    for id in [first, second] {
        assert_eq!(wait_task(&client, id, |t| t.status.is_terminal()).await.status, TaskStatus::Succeeded);
    }
    stop(dir.path(), daemon).await;
}

/// Alex delegates "Dark mode UI" to Iris; both of their first runs wait for a release.
async fn delegated_to_iris(root: &Path) -> (Client, tokio::task::JoinHandle<anyhow::Result<()>>, Arc<tokio::sync::Notify>, TaskId, TaskId, RunId, RunId) {
    let (runtime, release) = holding("Alex|Iris", None);
    let daemon = start_with(root, runtime).await;
    let client = connect(root).await;
    free_chart(&client).await;
    let (project, alex, _) = setup(&client).await;
    let parent = create(&client, project, Some(alex), "Dark mode").await;
    let alex_run = running_run(root, parent).await;
    let (ok, text) = act(root, alex_run, EmployeeTaskOp::Delegate { employee: "Iris".into(), title: "Dark mode UI".into(), goal: "Maak het donkere thema".into(), criteria: vec![], project: None, ticket: Some("#1".into()) })
        .await
        .unwrap();
    assert!(ok, "{text}");
    let child = wait_task(&client, parent, |t| !t.children.is_empty()).await.children[0];
    let iris_run = running_run(root, child).await;
    (client, daemon, release, parent, child, alex_run, iris_run)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_employee_interrupts_a_delegated_task_with_a_message() {
    let dir = tempfile::tempdir().unwrap();
    let (client, daemon, release, parent, child, alex_run, iris_run) = delegated_to_iris(dir.path()).await;
    let (ok, text) = act(dir.path(), alex_run, EmployeeTaskOp::MessageTask { task: "Dark mode UI".into(), message: "Gebruik de huidige tokens".into(), interrupt: true }).await.unwrap();
    assert!(ok, "{text}");

    // Iris' running answer is stopped and a new run answers the message.
    let t = wait_task(&client, child, |t| t.status.is_terminal()).await;
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    let messages = client.messages(t.conversation_id.unwrap()).await.unwrap().messages;
    let all: Vec<String> = messages.iter().map(|m| format!("{:?} {:?} {:?} {}", m.role, m.status, m.update, m.text)).collect();
    let said = messages.iter().find(|m| m.update.as_deref() == Some("message")).unwrap_or_else(|| panic!("{all:#?}"));
    assert_eq!((said.author.as_deref(), said.text.as_str()), (Some("Alex"), "Gebruik de huidige tokens"));
    let answers: Vec<_> = messages.iter().filter(|m| m.role == MessageRole::Assistant && m.update.is_none()).collect();
    assert!(answers.iter().any(|m| m.status == aurelius_domain::conversation::MessageStatus::Interrupted), "{all:#?}");
    assert!(answers.last().unwrap().text.contains("[Task message"), "the new answer saw the message: {all:#?}");
    let status: String = db(dir.path()).query_row("SELECT status FROM runs WHERE id = ?1", [iris_run.to_string()], |r| r.get(0)).unwrap();
    assert_eq!(status, "cancelled");

    release.notify_one();
    let t = wait_task(&client, parent, |t| t.status.is_terminal()).await;
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_without_interrupt_waits_for_the_running_answer() {
    let dir = tempfile::tempdir().unwrap();
    let (client, daemon, release, parent, child, alex_run, iris_run) = delegated_to_iris(dir.path()).await;
    let (ok, text) = act(dir.path(), alex_run, EmployeeTaskOp::MessageTask { task: "Dark mode UI".into(), message: "Daarna ook de knoppen".into(), interrupt: false }).await.unwrap();
    assert!(ok, "{text}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let status: String = db(dir.path()).query_row("SELECT status FROM runs WHERE id = ?1", [iris_run.to_string()], |r| r.get(0)).unwrap();
    assert_eq!(status, "running", "not interrupted");
    release.notify_one();
    release.notify_one();
    let t = wait_task(&client, child, |t| t.status.is_terminal()).await;
    let messages = client.messages(t.conversation_id.unwrap()).await.unwrap().messages;
    assert!(messages.iter().any(|m| m.update.as_deref() == Some("message") && m.author.as_deref() == Some("Alex")));
    let answers: Vec<_> = messages.iter().filter(|m| m.role == MessageRole::Assistant && m.update.is_none()).collect();
    assert!(answers.iter().all(|m| m.status == aurelius_domain::conversation::MessageStatus::Complete), "{answers:?}");
    wait_task(&client, parent, |t| t.status.is_terminal()).await;
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_employee_can_only_message_tasks_it_delegated() {
    let dir = tempfile::tempdir().unwrap();
    let (client, daemon, release, parent, _child, alex_run, iris_run) = delegated_to_iris(dir.path()).await;
    let msg = |task: &str| EmployeeTaskOp::MessageTask { task: task.into(), message: "Hoi".into(), interrupt: true };
    // Iris did not delegate Alex' task, and nobody knows "Spook".
    let (ok, text) = act(dir.path(), iris_run, msg(&parent.to_string())).await.unwrap();
    assert!(!ok && text.contains("delegated yourself"), "{text}");
    let (ok, text) = act(dir.path(), alex_run, msg("Spook")).await.unwrap();
    assert!(!ok && text.contains("No task"), "{text}");
    let (ok, text) = act(dir.path(), alex_run, EmployeeTaskOp::MessageTask { task: "Dark mode UI".into(), message: "  ".into(), interrupt: true }).await.unwrap();
    assert!(!ok, "{text}");
    release.notify_one();
    release.notify_one();
    wait_task(&client, parent, |t| t.status.is_terminal()).await;
    // An ended task takes no more messages.
    let (ok, text) = act(dir.path(), alex_run, msg("Dark mode UI")).await.unwrap();
    assert!(!ok && text.contains("already ended"), "{text}");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn work_given_outside_a_ticket_names_one_and_lands_on_it() {
    use aurelius_domain::{conversation::ChatStart, project_conversation::SendProjectMessage};
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 1).await;
    let client = connect(dir.path()).await;
    let (project, _, _) = setup(&client).await;
    let CommandResult::ProjectCreated { project_id: elsewhere, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Elders".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let other = backlog_ticket(&client, elsewhere, "Not here").await;
    let send = SendProjectMessage { confirm_secret: false, project_id: project, text: "Een logo graag".into(), conversation_id: None, images: Vec::new(), files: Vec::new(), interrupt: false, start: ChatStart::default() };
    let CommandResult::ProjectMessageSent { run_id: run, .. } = client.send_project_message(send).await.unwrap().result else { panic!() };
    let give = |ticket: Option<String>| EmployeeTaskOp::Delegate { employee: "Iris".into(), title: "Logo".into(), goal: "Maak het".into(), criteria: vec![], project: None, ticket };

    // Without a ticket, or with another project's: refused and no task.
    let (ok, text) = act(dir.path(), run, give(None)).await.unwrap();
    assert!(!ok && text.contains("Work always belongs to a ticket"), "{text}");
    let (ok, text) = act(dir.path(), run, give(Some(other.to_string()))).await.unwrap();
    assert!(!ok && text.contains("another project"), "{text}");
    assert!(client.tasks(None).await.unwrap().tasks.iter().all(|t| t.title != "Logo"));

    // On the ticket it names: linked to it, without a parent and not the main task.
    let (ok, text) = act(dir.path(), run, give(Some("#1".into()))).await.unwrap();
    assert!(ok, "{text}");
    let task = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.title == "Logo").expect("the task");
    assert_eq!((task.ticket_number, task.parent_task_id, task.project_id), (Some(1), None, project));
    assert!(task.delegated_by.is_some(), "clients tell it from the ticket's main task by this");
    // The ticket's discussion says who gave what, with a link to the task.
    let discussion: String = db(dir.path()).query_row("SELECT conversation_id FROM tickets WHERE number = 1", [], |r| r.get(0)).unwrap();
    let lines = client.messages(aurelius_domain::ConversationId::parse(&discussion).unwrap()).await.unwrap().messages;
    assert!(lines.iter().any(|m| m.system.as_ref().is_some_and(|s| s.task_id == Some(task.id)) && m.text.contains("gave the task “Logo” to Iris")), "{lines:?}");
    stop(dir.path(), daemon).await;
}
