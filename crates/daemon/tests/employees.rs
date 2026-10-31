//! B13 acceptance: projects without employees, free-form company-wide employees,
//! pause/archive. An employee answers in the conversation of its tasks and in its chat.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, ProjectId,
    conversation::{MessageRole, MessageStatus},
    employee::{CreateEmployee, LifecycleStatus, SendEmployeeMessage, SetEmployeeStatus, SetProjectStatus, UpdateEmployee},
    project::CreateProject,
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, ErrorCode};
use common::*;
use std::time::Duration;

async fn project(client: &Client, name: &str) -> ProjectId {
    let env = envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None);
    match client.execute(env).await.unwrap().result {
        CommandResult::ProjectCreated { project_id, .. } => project_id,
        other => panic!("{other:?}"),
    }
}

/// Employees are company-wide; the project only says where the test works.
fn new_employee(_project_id: ProjectId, name: &str, role: &str) -> CreateEmployee {
    CreateEmployee { name: name.into(), role: role.into(), title: None, reports_to: None, instructions: Some("Wees kort.".into()), folder: None }
}

async fn employee(client: &Client, project_id: ProjectId, name: &str, role: &str) -> EmployeeId {
    match client.execute(envelope(Command::CreateEmployee(new_employee(project_id, name, role)), None)).await.unwrap().result {
        CommandResult::EmployeeCreated { employee_id, .. } => employee_id,
        other => panic!("{other:?}"),
    }
}

/// A finished task of the employee: returns the conversation the user can keep talking in.
async fn task_conversation(client: &Client, project_id: ProjectId, employee_id: EmployeeId, title: &str) -> ConversationId {
    let task = CreateTask { project_id, employee_id: Some(employee_id), title: title.into(), goal: "Doe het".into(), acceptance_criteria: vec![], depends_on: vec![], run: Default::default(), files: vec![] };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(task), None)).await.unwrap().result else { panic!() };
    for _ in 0..400 {
        let tasks = client.tasks(Some(project_id)).await.unwrap().tasks;
        if let Some(t) = tasks.iter().find(|t| t.id == task_id && t.status == TaskStatus::Succeeded) {
            return t.conversation_id.expect("a task conversation");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the task did not finish");
}

fn status(employee_id: EmployeeId, status: LifecycleStatus) -> Command {
    Command::SetEmployeeStatus(SetEmployeeStatus { employee_id, status })
}

fn send(employee_id: EmployeeId, conversation_id: ConversationId, text: &str, interrupt: bool) -> aurelius_domain::CommandEnvelope {
    envelope(Command::SendEmployeeMessage(SendEmployeeMessage { employee_id, text: text.into(), conversation_id, images: Vec::new(), files: Vec::new(), interrupt }), None)
}

#[tokio::test(flavor = "multi_thread")]
async fn zero_one_and_many_free_form_employees() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    assert!(client.employees().await.unwrap().employees.is_empty(), "a new project has no employees");

    employee(&client, atlas, "Alex", "Productpartner").await;
    employee(&client, atlas, "Maya", "Iets heel eigens").await;
    let other = project(&client, "Phoenix").await;
    employee(&client, other, "Theo", "Onderzoek").await;

    // One company: everyone, oldest first, whatever project they will work in.
    let employees = client.employees().await.unwrap().employees;
    let roles: Vec<_> = employees.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, ["Productpartner", "Iets heel eigens", "Onderzoek"]);
    assert_eq!(employees[0].instructions.as_deref(), Some("Wees kort."));
    assert!(employees.iter().all(|m| m.projects_active.is_empty()), "nobody works in a project yet");
    let count = client.projects().await.unwrap().projects.iter().find(|p| p.id == atlas).unwrap().employees_active;
    assert_eq!(count, 0);
    // A name is taken once in the company.
    assert!(client.execute(envelope(Command::CreateEmployee(new_employee(other, "alex", "X")), None)).await.is_err());
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn paused_employee_starts_no_run_and_config_applies_to_new_runs() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let alex = employee(&client, atlas, "Alex", "Productpartner").await;
    let conversation = task_conversation(&client, atlas, alex, "Start").await;

    client.execute(envelope(status(alex, LifecycleStatus::Paused), None)).await.unwrap();
    let err = client.execute(send(alex, conversation, "Doe iets", false)).await.unwrap_err();
    assert!(err.to_string().contains("paused"));

    client.execute(envelope(status(alex, LifecycleStatus::Active), None)).await.unwrap();
    let version = client.employees().await.unwrap().employees[0].version;
    let update = Command::UpdateEmployee(UpdateEmployee { employee_id: alex, name: "Alexandra".into(), role: "Strateeg".into(), title: None, instructions: None, sequential_tasks: None });
    client.execute(envelope(update, Some(version))).await.unwrap();

    let CommandResult::EmployeeMessageSent { message_id, .. } = client.execute(send(alex, conversation, "Hallo", false)).await.unwrap().result else { panic!() };
    let mut answer = None;
    for _ in 0..200 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if let Some(a) = messages.iter().find(|m| m.role == MessageRole::Assistant && m.reply_to == Some(message_id) && m.status == MessageStatus::Complete) {
            answer = Some(a.text.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(answer.unwrap().starts_with("[Alexandra, Strateeg]"), "new runs use the new config");
    stop(dir.path(), daemon).await;
}

/// "Send now" to an employee: the earlier answer never completes, the new message is answered.
#[tokio::test(flavor = "multi_thread")]
async fn send_now_to_an_employee_stops_the_earlier_answer() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 20).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let alex = employee(&client, atlas, "Alex", "Productpartner").await;
    let conversation = task_conversation(&client, atlas, alex, "Start").await;
    let CommandResult::EmployeeMessageSent { message_id: first, .. } = client.execute(send(alex, conversation, "Schrijf een lang plan", false)).await.unwrap().result else { panic!() };
    let CommandResult::EmployeeMessageSent { message_id: second, .. } = client.execute(send(alex, conversation, "Toch niet, kort graag", true)).await.unwrap().result else { panic!() };

    let mut settled = Vec::new();
    for _ in 0..400 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if messages.iter().any(|m| m.reply_to == Some(second) && m.status == MessageStatus::Complete) {
            settled = messages;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(!settled.is_empty(), "the new message is answered");
    // Stopped before it started (no answer) or while it streamed (interrupted).
    assert!(settled.iter().filter(|m| m.reply_to == Some(first)).all(|m| m.status == MessageStatus::Interrupted));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn archive_keeps_history_and_blocks_changes() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let alex = employee(&client, atlas, "Alex", "Productpartner").await;
    client.execute(envelope(status(alex, LifecycleStatus::Archived), None)).await.unwrap();
    assert!(client.employees().await.unwrap().employees.is_empty());
    let update = Command::UpdateEmployee(UpdateEmployee { employee_id: alex, name: "X".into(), role: "Y".into(), title: None, instructions: None, sequential_tasks: None });
    let err = client.execute(envelope(update, None)).await.unwrap_err();
    assert!(err.to_string().contains("archived"));

    let archive = Command::SetProjectStatus(SetProjectStatus { project_id: atlas, status: LifecycleStatus::Archived });
    client.execute(envelope(archive, None)).await.unwrap();
    assert!(client.projects().await.unwrap().projects.is_empty());
    // An archived project takes no new work.
    let z = employee(&client, atlas, "Z", "Z").await;
    let task = CreateTask { project_id: atlas, employee_id: Some(z), title: "T".into(), goal: "G".into(), acceptance_criteria: vec![], depends_on: vec![], run: Default::default(), files: vec![] };
    let err = client.execute(envelope(Command::CreateTask(task), None)).await.unwrap_err();
    assert!(matches!(err, aurelius_client::ClientError::Protocol(ref p) if p.code == ErrorCode::Validation));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_conversation_has_its_own_mode_model_and_effort() {
    use aurelius_domain::autonomy::{AgentMode, SetAgentMode, SetConversationRun};
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let atlas = project(&client, "Atlas").await;
    let alex = employee(&client, atlas, "Alex", "Writer").await;
    let task = task_conversation(&client, atlas, alex, "Launch plan").await;
    let main = client.employees().await.unwrap().employees[0].conversation_id;

    // The task's conversation takes its own mode, model and effort; the employee's main one keeps its own.
    client.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id: task, mode: AgentMode::Plan }), None)).await.unwrap();
    let run = Command::SetConversationRun(SetConversationRun { conversation_id: task, model: Some("claude-haiku-4-5".into()), effort: Some("low".into()) });
    client.execute(envelope(run, None)).await.unwrap();
    let m = client.employees().await.unwrap().employees.into_iter().find(|m| m.id == alex).unwrap();
    assert_eq!(m.agent_mode, AgentMode::Ask, "the main conversation keeps its mode");
    let in_task = client.messages(task).await.unwrap();
    assert_eq!(in_task.agent_mode, AgentMode::Plan);
    assert_eq!((in_task.run.model.as_deref(), in_task.run.effort.as_deref()), (Some("claude-haiku-4-5"), Some("low")));
    assert!(client.messages(main).await.unwrap().run.is_empty());
    let bad = Command::SetConversationRun(SetConversationRun { conversation_id: task, model: None, effort: Some("extreme".into()) });
    assert!(client.execute(envelope(bad, None)).await.is_err());

    // Another employee can't write in it.
    let olga = employee(&client, atlas, "Olga", "Research").await;
    assert!(client.execute(send(olga, task, "hi", false)).await.is_err());

    // The main conversation can't be deleted.
    let delete = envelope(Command::DeleteConversation(aurelius_domain::conversation::DeleteConversation { conversation_id: main }), None);
    assert!(client.execute(delete).await.is_err());

    stop(dir.path(), daemon).await;
}
