//! Tasks: worked on in a conversation with their employee, ending with a report.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, EventKind, EmployeeId, ProjectId, TaskId,
    employee::CreateEmployee,
    project::CreateProject,
    task::{CancelTask, CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, Event, TaskView};
use common::*;
use std::time::Duration;
use tokio::sync::mpsc;

async fn setup(client: &Client) -> (ProjectId, EmployeeId) {
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(
            Command::CreateEmployee(CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Alex".into(), role: "Productpartner".into(), title: None, reports_to: None, instructions: None, folder: None }),
            None,
        ))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    (project_id, employee_id)
}

async fn create(client: &Client, project_id: ProjectId, employee_id: Option<EmployeeId>, title: &str, deps: Vec<TaskId>) -> Result<TaskId, aurelius_client::ClientError> {
    let cmd = CreateTask {
        confirm_secret: false,
        files: vec![],
        run: Default::default(),
        project_id,
        employee_id,
        title: title.into(),
        goal: format!("Vergelijk ideeën voor {title}"),
        acceptance_criteria: vec!["Elk resultaat noemt een bron".into(), "Er is een aanbeveling".into()],
        depends_on: deps,
    };
    match client.execute(envelope(Command::CreateTask(cmd), None)).await?.result {
        CommandResult::TaskCreated { task_id } => Ok(task_id),
        other => panic!("{other:?}"),
    }
}

async fn wait_task(client: &Client, id: TaskId, done: impl Fn(&TaskView) -> bool) -> TaskView {
    for _ in 0..400 {
        if let Some(t) = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == id) {
            if done(&t) {
                return t;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("task did not reach the expected state: {:?}", client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == id));
}

/// The kinds of the events about `task`, in order, up to and including the first `until`.
async fn task_events(events: &mut mpsc::UnboundedReceiver<Event>, task: TaskId, until: EventKind) -> Vec<EventKind> {
    let id = task.to_string();
    let mut kinds = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(20), events.recv()).await.expect("an event in time").expect("the event stream is open");
        if event.aggregate_id == id {
            kinds.push(event.kind);
            if event.kind == until {
                return kinds;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_is_worked_on_in_a_conversation_and_ends_with_a_report() {
    use aurelius_domain::conversation::MessageRole;
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (project, employee) = setup(&client).await;
    let id = create(&client, project, Some(employee), "het nieuwe product", vec![]).await.unwrap();

    let task = wait_task(&client, id, |t| t.status.is_terminal()).await;
    assert_eq!(task.status, TaskStatus::Succeeded, "{task:?}");
    // The employee took it on in a conversation of its own, named after the task.
    let conversation = task.conversation_id.expect("a conversation");
    // The task conversation is the employee's, in the task's project; its chats list stays its own chat.
    let alex = client.employees().await.unwrap().employees.into_iter().find(|m| m.id == employee).unwrap();
    assert!(!alex.conversations.iter().any(|c| c.id == conversation));
    let snapshot = client.messages(conversation).await.unwrap();
    assert_eq!((snapshot.employee_id, snapshot.project_id, snapshot.read_only), (Some(employee), Some(project), false));
    let messages = snapshot.messages;
    assert_eq!(messages[0].update.as_deref(), Some("brief"));
    assert!(messages[0].text.contains("Vergelijk ideeën voor het nieuwe product"));
    let answer = messages.iter().rev().find(|m| m.role == MessageRole::Assistant && m.update.is_none()).expect("the employee's answer");
    // Its last answer is the task's report, under the task's title.
    let report = task.result.expect("a report");
    assert!(report.starts_with("# het nieuwe product"), "{report}");
    assert!(report.contains(answer.text.trim()), "{report}");

    let conn = db(dir.path());
    let runs: i64 = conn.query_row("SELECT COUNT(*) FROM runs WHERE task_id = ?1 AND conversation_id = ?2", [id.to_string(), conversation.to_string()], |r| r.get(0)).unwrap();
    assert_eq!(runs, 1, "the conversation's run counts for the task");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn drafts_dependencies_cycles_and_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (project, employee) = setup(&client).await;

    // Without an employee a task is a draft and nothing runs.
    let draft = create(&client, project, None, "concept", vec![]).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let t = client.tasks(Some(project)).await.unwrap().tasks.into_iter().find(|t| t.id == draft).unwrap();
    assert_eq!(t.status, TaskStatus::Draft);
    assert!(t.conversation_id.is_none(), "nothing worked on it");

    // B depends on A: B finishes after A.
    let a = create(&client, project, Some(employee), "A", vec![]).await.unwrap();
    let b = create(&client, project, Some(employee), "B", vec![a]).await.unwrap();
    wait_task(&client, b, |t| t.status == TaskStatus::Succeeded).await;
    let a_view = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == a).unwrap();
    assert_eq!(a_view.status, TaskStatus::Succeeded);

    // Unknown dependency is refused.
    assert!(create(&client, project, Some(employee), "C", vec![TaskId::new()]).await.is_err());

    // Cancel a draft.
    client.execute(envelope(Command::CancelTask(CancelTask { task_id: draft }), None)).await.unwrap();
    let t = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == draft).unwrap();
    assert_eq!(t.status, TaskStatus::Cancelled);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn creating_a_task_for_an_employee_delivers_created_then_queued() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (client, mut events) = connect_events(dir.path()).await;
    let (project, employee) = setup(&client).await;
    client.subscribe(0).await.unwrap();

    let id = create(&client, project, Some(employee), "typed", vec![]).await.unwrap();
    let kinds = task_events(&mut events, id, EventKind::TaskQueued).await;
    assert_eq!(kinds, [EventKind::TaskCreated, EventKind::TaskQueued]);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finished_task_delivers_task_succeeded() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (client, mut events) = connect_events(dir.path()).await;
    let (project, employee) = setup(&client).await;
    client.subscribe(0).await.unwrap();

    let id = create(&client, project, Some(employee), "typed", vec![]).await.unwrap();
    let kinds = task_events(&mut events, id, EventKind::TaskSucceeded).await;
    assert_eq!(kinds.first(), Some(&EventKind::TaskCreated));
    assert_eq!(kinds.last(), Some(&EventKind::TaskSucceeded));
    assert_eq!(wait_task(&client, id, |t| t.status.is_terminal()).await.status, TaskStatus::Succeeded);
    stop(dir.path(), daemon).await;
}
