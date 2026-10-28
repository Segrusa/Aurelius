//! Waiting messages: a user message sent while an answer runs
//! waits behind it; it can be deleted or sent now, in chats and task
//! conversations, until its own answer starts.

mod common;
use common::*;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, MessageId, ProjectId, RunId, TaskId,
    conversation::{MessageRole, MessageStatus, SendChatMessage},
    employee::{CreateEmployee, SendEmployeeMessage, UpdateEmployee},
    project::CreateProject,
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, MessageView, TaskView};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// The fake, but a run whose last turn ends with `[hold]` waits until the gate
/// opens (or it is stopped), so a test can queue messages behind it.
struct Gate {
    open: tokio::sync::watch::Receiver<bool>,
}

#[async_trait::async_trait]
impl AgentRuntime for Gate {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        // Ends with: a stopped answer is left out of the context, so the user's
        // turns around it run together and an earlier `[hold]` may come first.
        let hold = request.turns.last().is_some_and(|t| t.text.trim_end().ends_with("[hold]"));
        let text = fake::answer_for(&request);
        let mut open = self.open.clone();
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            if hold {
                tokio::select! {
                    _ = async { open.wait_for(|o| *o).await.map(|_| ()) } => {}
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

async fn start_gate(root: &Path) -> (Daemon, tokio::sync::watch::Sender<bool>) {
    let (release, open) = tokio::sync::watch::channel(false);
    (start_with(root, Arc::new(Gate { open })).await, release)
}

/// Wait until `run` is answering.
async fn running(root: &Path, run: RunId) {
    let conn = db(root);
    for _ in 0..400 {
        let status: Option<String> = conn.query_row("SELECT status FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).ok();
        if status.as_deref() == Some("running") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("run {run} did not start");
}

async fn chat(client: &Client, conversation_id: Option<ConversationId>, text: &str) -> (ConversationId, MessageId, RunId) {
    let send = SendChatMessage { conversation_id: match conversation_id { Some(id) => id, None => common::conversation(client).await }, text: text.into(), images: Vec::new(), interrupt: false };
    match client.execute(envelope(Command::SendChatMessage(send), None)).await.unwrap().result {
        CommandResult::ChatMessageSent { conversation_id, message_id, run_id } => (conversation_id, message_id, run_id),
        other => panic!("{other:?}"),
    }
}

async fn to_employee(client: &Client, employee_id: EmployeeId, conversation_id: ConversationId, text: &str) -> (ConversationId, MessageId, RunId) {
    let send = SendEmployeeMessage { employee_id, text: text.into(), conversation_id, images: Vec::new(), files: Vec::new(), interrupt: false };
    match client.execute(envelope(Command::SendEmployeeMessage(send), None)).await.unwrap().result {
        CommandResult::EmployeeMessageSent { conversation_id, message_id, run_id } => (conversation_id, message_id, run_id),
        other => panic!("{other:?}"),
    }
}

async fn messages(client: &Client, id: ConversationId) -> Vec<MessageView> {
    client.messages(id).await.unwrap().messages
}

fn queued(all: &[MessageView], id: MessageId) -> bool {
    all.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("message {id} is gone")).queued
}

/// The answer to `message` once it ended.
async fn answer(client: &Client, id: ConversationId, message: MessageId) -> MessageView {
    for _ in 0..400 {
        if let Some(a) = messages(client, id).await.into_iter().find(|m| m.reply_to == Some(message) && m.status != MessageStatus::Partial) {
            return a;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let all: Vec<String> = messages(client, id).await.iter().map(|m| format!("{} {:?} {:?} reply_to={:?} queued={} {}", m.ordinal, m.role, m.status, m.reply_to, m.queued, m.text.chars().take(40).collect::<String>())).collect();
    panic!("message {message} got no answer: {all:#?}");
}

async fn project_with_employee(client: &Client) -> (ProjectId, EmployeeId) {
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let create = CreateEmployee { name: "Alex".into(), role: "Backend developer".into(), title: None, reports_to: None, instructions: None, folder: None, department_id: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    (project_id, employee_id)
}

async fn create_task(client: &Client, project_id: ProjectId, employee_id: EmployeeId, title: &str, goal: &str) -> TaskId {
    let cmd = CreateTask { files: vec![], run: Default::default(), project_id, employee_id: Some(employee_id), title: title.into(), goal: goal.into(), acceptance_criteria: vec![], depends_on: vec![] };
    match client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result {
        CommandResult::TaskCreated { task_id } => task_id,
        other => panic!("{other:?}"),
    }
}

async fn task(client: &Client, id: TaskId) -> TaskView {
    client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == id).unwrap()
}

async fn wait_task(client: &Client, id: TaskId, done: impl Fn(&TaskView) -> bool) -> TaskView {
    for _ in 0..600 {
        let t = task(client, id).await;
        if done(&t) {
            return t;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("task did not reach the expected state: {:?}", task(client, id).await);
}

/// The run of a task's conversation that answers now.
async fn task_run(root: &Path, task: TaskId) -> RunId {
    let conn = db(root);
    for _ in 0..400 {
        if let Ok(run) = conn.query_row("SELECT id FROM runs WHERE task_id = ?1 AND conversation_id IS NOT NULL AND status = 'running'", [task.to_string()], |r| r.get::<_, String>(0)) {
            return RunId::parse(&run).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no running run for the task");
}

fn refused(err: aurelius_client::ClientError, why: &str) {
    assert!(err.to_string().contains(why), "expected “{why}”, got {err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_chat_message_can_be_deleted_until_its_answer_starts() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, release) = start_gate(dir.path()).await;
    let client = connect(dir.path()).await;
    let (id, first, run) = chat(&client, None, "eerste [hold]").await;
    running(dir.path(), run).await;
    let (_, second, second_run) = chat(&client, Some(id), "tweede").await;
    let (_, third, _) = chat(&client, Some(id), "derde").await;

    let all = messages(&client, id).await;
    assert!(!queued(&all, first), "its answer runs");
    assert!(queued(&all, second) && queued(&all, third));

    let result = client.delete_queued_message(id, second).await.unwrap().result;
    assert_eq!(result, CommandResult::QueuedMessageDeleted { conversation_id: id, message_id: second, run_id: second_run });
    let all = messages(&client, id).await;
    assert!(all.iter().all(|m| m.id != second), "the message is gone");
    assert!(queued(&all, third), "the next one still waits");
    let status: String = db(dir.path()).query_row("SELECT status FROM runs WHERE id = ?1", [second_run.to_string()], |r| r.get(0)).unwrap();
    assert_eq!(status, "cancelled");

    refused(client.delete_queued_message(id, second).await.unwrap_err(), "does not exist");
    refused(client.delete_queued_message(id, first).await.unwrap_err(), "already started");
    refused(client.send_queued_now(id, first).await.unwrap_err(), "already started");

    release.send(true).unwrap();
    assert_eq!(answer(&client, id, first).await.status, MessageStatus::Complete);
    let last = answer(&client, id, third).await;
    assert_eq!(last.status, MessageStatus::Complete);
    let all = messages(&client, id).await;
    assert_eq!(all.iter().filter(|m| m.role == MessageRole::Assistant).count(), 2, "the deleted message got no answer");
    assert!(!queued(&all, third));
    refused(client.delete_queued_message(id, third).await.unwrap_err(), "already started");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn send_now_stops_what_runs_ahead_and_keeps_the_rest_in_line() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, release) = start_gate(dir.path()).await;
    let client = connect(dir.path()).await;
    let (id, first, run) = chat(&client, None, "eerste [hold]").await;
    running(dir.path(), run).await;
    let (_, second, second_run) = chat(&client, Some(id), "tweede [hold]").await;
    let (_, third, _) = chat(&client, Some(id), "derde").await;

    let result = client.send_queued_now(id, second).await.unwrap().result;
    assert_eq!(result, CommandResult::QueuedMessageSentNow { conversation_id: id, message_id: second, run_id: second_run });
    assert_eq!(answer(&client, id, first).await.status, MessageStatus::Interrupted, "the running answer stopped");
    running(dir.path(), second_run).await;
    let all = messages(&client, id).await;
    assert!(!queued(&all, second), "its answer runs now");
    assert!(queued(&all, third), "the message after it keeps waiting");
    refused(client.send_queued_now(id, second).await.unwrap_err(), "already started");

    release.send(true).unwrap();
    let answered = answer(&client, id, second).await;
    let last = answer(&client, id, third).await;
    assert_eq!((answered.status, last.status), (MessageStatus::Complete, MessageStatus::Complete));
    assert!(last.ordinal > answered.ordinal, "answered in order");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_skipped_by_send_now_no_longer_waits() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, _release) = start_gate(dir.path()).await;
    let client = connect(dir.path()).await;
    let (id, _, run) = chat(&client, None, "eerste [hold]").await;
    running(dir.path(), run).await;
    let (_, second, _) = chat(&client, Some(id), "tweede").await;
    let (_, third, _) = chat(&client, Some(id), "derde").await;

    client.send_queued_now(id, third).await.unwrap();
    assert_eq!(answer(&client, id, third).await.status, MessageStatus::Complete);
    let all = messages(&client, id).await;
    assert!(!queued(&all, second) && all.iter().all(|m| m.reply_to != Some(second)), "its queued answer was stopped");
    refused(client.send_queued_now(id, second).await.unwrap_err(), "not waiting");
    refused(client.delete_queued_message(id, second).await.unwrap_err(), "not waiting");
    refused(client.delete_queued_message(ConversationId::new(), second).await.unwrap_err(), "does not exist");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn in_a_task_conversation_send_now_answers_and_the_task_finishes_while_the_next_task_waits_its_turn() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, _release) = start_gate(dir.path()).await;
    let client = connect(dir.path()).await;
    let (project, alex) = project_with_employee(&client).await;
    let one_at_a_time = UpdateEmployee { employee_id: alex, name: "Alex".into(), role: "Backend developer".into(), title: None, instructions: None, sequential_tasks: Some(true) };
    client.execute(envelope(Command::UpdateEmployee(one_at_a_time), None)).await.unwrap();
    let id = create_task(&client, project, alex, "Dark mode", "Werk aan dark mode [hold]").await;
    let run = task_run(dir.path(), id).await;
    let next = create_task(&client, project, alex, "Logo", "Maak een logo").await;
    let conversation = task(&client, id).await.conversation_id.unwrap();

    let (_, second, _) = to_employee(&client, alex, conversation, "Ook de instellingen").await;
    let (_, third, _) = to_employee(&client, alex, conversation, "Kort graag").await;
    assert!(queued(&messages(&client, conversation).await, second));
    client.delete_queued_message(conversation, second).await.unwrap();
    assert_eq!(task(&client, id).await.status, TaskStatus::Running, "its answer still runs");
    let waiting = task(&client, next).await;
    assert_eq!(waiting.status, TaskStatus::Queued, "the next task waits its turn");
    assert_eq!(waiting.waiting_reason.as_deref(), Some("in_line"), "the server says it is not at work");
    assert_eq!(task(&client, id).await.waiting_reason, None);

    client.send_queued_now(conversation, third).await.unwrap();
    let status: String = {
        let conn = db(dir.path());
        let mut status = String::new();
        for _ in 0..400 {
            status = conn.query_row("SELECT status FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).unwrap();
            if status == "cancelled" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        status
    };
    assert_eq!(status, "cancelled", "the task's running answer stopped");
    let t = wait_task(&client, id, |t| t.status.is_terminal()).await;
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    assert_eq!(answer(&client, conversation, third).await.status, MessageStatus::Complete);
    // Only now the next task gets its turn.
    wait_task(&client, next, |t| t.status != TaskStatus::Queued).await;
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_waiting_message_of_a_task_leaves_its_running_answer_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, release) = start_gate(dir.path()).await;
    let client = connect(dir.path()).await;
    let (project, alex) = project_with_employee(&client).await;
    let id = create_task(&client, project, alex, "Dark mode", "Werk aan dark mode [hold]").await;
    task_run(dir.path(), id).await;
    let conversation = task(&client, id).await.conversation_id.unwrap();
    let (_, second, _) = to_employee(&client, alex, conversation, "Ook de instellingen").await;
    client.delete_queued_message(conversation, second).await.unwrap();

    // The answer ends after the delete: the task finishes as usual.
    release.send(true).unwrap();
    let t = wait_task(&client, id, |t| t.status.is_terminal()).await;
    assert_eq!(t.status, TaskStatus::Succeeded, "{t:?}");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finished_task_shows_a_live_run_while_a_follow_up_answers_and_keeps_its_status() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, release) = start_gate(dir.path()).await;
    let client = connect(dir.path()).await;
    let (project, alex) = project_with_employee(&client).await;
    let id = create_task(&client, project, alex, "Dark mode", "Werk aan dark mode").await;
    let done = wait_task(&client, id, |t| t.status.is_terminal()).await;
    assert_eq!((done.status, done.live_run), (TaskStatus::Succeeded, None));
    let conversation = done.conversation_id.unwrap();
    let events = |kind: &str| -> i64 { db(dir.path()).query_row("SELECT COUNT(*) FROM events WHERE aggregate_id = ?1 AND type = ?2", [id.to_string(), kind.to_string()], |r| r.get(0)).unwrap() };
    assert_eq!(events("task.run_changed"), 0);

    // The user writes in the finished task: its answer runs, the status stays.
    let (_, first, run) = to_employee(&client, alex, conversation, "Nog een aanpassing [hold]").await;
    running(dir.path(), run).await;
    let live = wait_task(&client, id, |t| t.live_run.is_some()).await;
    assert_eq!((live.status, live.live_run), (TaskStatus::Succeeded, Some(aurelius_protocol::LiveRun::Running)));
    assert!(events("task.run_changed") >= 1, "clients are told the live run changed");
    // A message behind it: still live.
    let (_, _, _) = to_employee(&client, alex, conversation, "En nog iets").await;
    assert_eq!(task(&client, id).await.live_run, Some(aurelius_protocol::LiveRun::Running));

    // Both answers end: the live run is gone and the task is as finished as before.
    let before = events("task.run_changed");
    release.send(true).unwrap();
    answer(&client, conversation, first).await;
    let after = wait_task(&client, id, |t| t.live_run.is_none()).await;
    assert_eq!(after.status, TaskStatus::Succeeded, "{after:?}");
    assert!(events("task.run_changed") > before, "its end is announced too");
    stop(dir.path(), daemon).await;
}
