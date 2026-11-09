//! The employee's mailbox end to end (R1-R3): one run at a time per employee,
//! merging per thread, parking on a question and going on when it is answered.
//! `tests/queued.rs` covers Queued, Send now and delete; recovery is in
//! `aurelius_application::inbox_tests`.

mod common;
use common::*;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, MessageId, ProjectId, RunId, TaskId, TicketId,
    conversation::{MessageRole, MessageStatus},
    employee::{CreateEmployee, SendEmployeeMessage},
    project::CreateProject,
    task::{CreateTask, TaskStatus},
    ticket::{CreateTicket, TicketPriority},
};
use aurelius_protocol::{CommandResult, MessageView};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// The fake, but a turn ending with `[hold]` waits for the gate (or a stop); a turn
/// with `[ask]` ends on a question. Each request's new turns are noted for the test.
struct Gate {
    open: tokio::sync::watch::Receiver<bool>,
    last_prompts: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl AgentRuntime for Gate {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let prompt = request.turns.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join("\n");
        self.last_prompts.lock().unwrap().push(prompt.clone());
        let hold = request.turns.last().is_some_and(|t| t.text.trim_end().ends_with("[hold]"));
        let text = if prompt.contains("[ask]") && !prompt.contains("the label") { "Which one do you mean: the host or the label?".to_string() } else { fake::answer_for(&request) };
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

struct Setup {
    daemon: Daemon,
    release: tokio::sync::watch::Sender<bool>,
    prompts: Arc<Mutex<Vec<String>>>,
}

async fn start_gate(root: &Path) -> Setup {
    let (release, open) = tokio::sync::watch::channel(false);
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let daemon = start_with(root, Arc::new(Gate { open, last_prompts: prompts.clone() })).await;
    Setup { daemon, release, prompts }
}

async fn project_with_alex(client: &Client) -> (ProjectId, EmployeeId, ConversationId) {
    let CommandResult::ProjectCreated { project_id, .. } = client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result else { panic!() };
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Alex".into(), role: "Backend developer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    (project_id, employee_id, conversation_id)
}

async fn say(client: &Client, employee_id: EmployeeId, conversation_id: ConversationId, text: &str) -> (MessageId, RunId) {
    let send = SendEmployeeMessage { confirm_secret: false, employee_id, text: text.into(), conversation_id, images: Vec::new(), files: Vec::new(), interrupt: false };
    match client.execute(envelope(Command::SendEmployeeMessage(send), None)).await.unwrap().result {
        CommandResult::EmployeeMessageSent { message_id, run_id, .. } => (message_id, run_id),
        other => panic!("{other:?}"),
    }
}

async fn status_of(root: &Path, run: RunId) -> Option<String> {
    db(root).query_row("SELECT status FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).ok()
}

async fn wait_status(root: &Path, run: RunId, want: &str) {
    for _ in 0..600 {
        if status_of(root, run).await.as_deref() == Some(want) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("run {run} never got status {want}: {:?}", status_of(root, run).await);
}

async fn messages(client: &Client, id: ConversationId) -> Vec<MessageView> {
    client.messages(id).await.unwrap().messages
}

async fn answer(client: &Client, id: ConversationId, message: MessageId) -> MessageView {
    for _ in 0..600 {
        if let Some(a) = messages(client, id).await.into_iter().find(|m| m.reply_to == Some(message) && m.status != MessageStatus::Partial) {
            return a;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("message {message} got no answer");
}

fn scalar(root: &Path, sql: &str) -> i64 {
    db(root).query_row(sql, [], |r| r.get(0)).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_employee_does_one_run_at_a_time_across_its_conversations() {
    let dir = tempfile::tempdir().unwrap();
    let s = start_gate(dir.path()).await;
    let client = connect(dir.path()).await;
    let (project, alex, chat) = project_with_alex(&client).await;

    let (_, first) = say(&client, alex, chat, "eerste [hold]").await;
    wait_status(dir.path(), first, "running").await;

    // A task for the same employee waits in its mailbox while the run goes.
    let cmd = CreateTask { confirm_secret: false, files: vec![], run: Default::default(), project_id: project, employee_id: Some(alex), title: "Logo".into(), goal: "Maak een logo".into(), acceptance_criteria: vec![] };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    let conn = db(dir.path());
    for _ in 0..600 {
        if conn.query_row("SELECT COUNT(*) FROM employee_inbox WHERE task_id = ?1 AND state = 'queued'", [task_id.to_string()], |r| r.get::<_, i64>(0)).unwrap() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM runs WHERE task_id = ?1", [task_id.to_string()], |r| r.get::<_, i64>(0)).unwrap(), 0, "no run of the task yet");
    assert_eq!(scalar(dir.path(), "SELECT COUNT(*) FROM jobs WHERE kind = 'run.chat' AND state = 'leased'"), 1);

    s.release.send(true).unwrap();
    wait_task(&client, task_id, |status| status.is_terminal()).await;
    stop(dir.path(), s.daemon).await;
}

async fn wait_task(client: &Client, id: TaskId, done: impl Fn(TaskStatus) -> bool) -> TaskStatus {
    for _ in 0..800 {
        let status = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == id).unwrap().status;
        if done(status) {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("task never reached the expected status");
}

#[tokio::test(flavor = "multi_thread")]
async fn what_waits_in_one_thread_becomes_one_run_that_reads_it_all() {
    let dir = tempfile::tempdir().unwrap();
    let s = start_gate(dir.path()).await;
    let client = connect(dir.path()).await;
    let (_, alex, chat) = project_with_alex(&client).await;

    let (_, first) = say(&client, alex, chat, "eerste [hold]").await;
    wait_status(dir.path(), first, "running").await;
    let (second, _) = say(&client, alex, chat, "tweede").await;
    let (third, third_run) = say(&client, alex, chat, "derde").await;
    assert!(messages(&client, chat).await.iter().any(|m| m.id == second && m.queued), "it shows as waiting");

    s.release.send(true).unwrap();
    assert_eq!(answer(&client, chat, third).await.status, MessageStatus::Complete);
    wait_status(dir.path(), third_run, "succeeded").await;
    let all = messages(&client, chat).await;
    assert_eq!(all.iter().filter(|m| m.role == MessageRole::Assistant && m.update.is_none()).count(), 2, "two runs, not three");
    assert!(!all.iter().any(|m| m.queued));
    let prompt = s.prompts.lock().unwrap().last().cloned().unwrap();
    assert!(prompt.contains("tweede") && prompt.contains("derde"), "the run reads them both: {prompt}");
    assert_eq!(db(dir.path()).query_row("SELECT COUNT(*) FROM run_triggers WHERE run_id = ?1", [third_run.to_string()], |r| r.get::<_, i64>(0)).unwrap(), 2);
    stop(dir.path(), s.daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_parked_on_a_question_keeps_new_items_until_the_answer_comes() {
    let dir = tempfile::tempdir().unwrap();
    let s = start_gate(dir.path()).await;
    let client = connect_as(dir.path(), "grace").await;
    let (project, alex, _) = project_with_alex(&client).await;

    // The owner's run ends on a question: the ticket stays in Doing, its thread is parked.
    let made = client
        .create_ticket(CreateTicket {
            responsible_user_id: None,
            confirm_secret: false,
            project_id: project,
            title: "Name".into(),
            description: Some("Rename it [ask]".into()),
            priority: TicketPriority::default(),
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: Some(alex),
            run: aurelius_domain::task::RunSettings { mode: Some(aurelius_domain::autonomy::AgentMode::Auto), ..Default::default() },
            files: vec![],
            start: true,
            from_message: None,
            from_idea: None,
        })
        .await
        .unwrap();
    let CommandResult::TicketCreated { ticket_id, .. } = made.result else { panic!("{:?}", made.result) };
    let root = until_waiting(&client, ticket_id).await;
    assert_eq!(scalar(dir.path(), "SELECT COUNT(*) FROM employee_threads WHERE state = 'parked'"), 1);

    // A result arrives that is nobody's answer: it queues behind the question.
    let conversation = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == root).unwrap().conversation_id.unwrap();
    let thread: String = db(dir.path()).query_row("SELECT thread_key FROM employee_threads WHERE state = 'parked'", [], |r| r.get(0)).unwrap();
    assert_eq!(thread, format!("ticket:{ticket_id}"));
    let writer = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    writer.busy_timeout(Duration::from_secs(5)).unwrap();
    let waiting = writer.execute(
        "INSERT INTO employee_inbox(id, employee_id, thread_key, conversation_id, kind, source, payload_json, source_event_id, not_before_ms, run_id, created_at_ms)
         SELECT 'colleague-note', employee_id, thread_key, conversation_id, 'message', 'employee', payload_json, source_event_id, 0, ?1, 1 FROM employee_inbox LIMIT 1",
        [RunId::new().to_string()],
    );
    assert_eq!(waiting.unwrap(), 1);
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(scalar(dir.path(), "SELECT COUNT(*) FROM employee_inbox WHERE id = 'colleague-note' AND state = 'queued'"), 1, "it waits");

    // The board member's answer is a resume: the run goes on and the note comes along.
    let answer_text = "the label";
    say(&client, alex, conversation, answer_text).await;
    wait_task(&client, root, |status| status != TaskStatus::Waiting).await;
    for _ in 0..600 {
        if scalar(dir.path(), "SELECT COUNT(*) FROM employee_inbox WHERE id = 'colleague-note' AND state <> 'queued'") == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(scalar(dir.path(), "SELECT COUNT(*) FROM employee_inbox WHERE id = 'colleague-note' AND state <> 'queued'"), 1, "the note was taken with or after the resume");
    assert_eq!(scalar(dir.path(), "SELECT COUNT(*) FROM employee_inbox WHERE kind = 'resume'"), 1);
    assert_eq!(scalar(dir.path(), "SELECT COUNT(*) FROM employee_threads WHERE state = 'parked'"), 0);
    let _ = s.release;
    stop(dir.path(), s.daemon).await;
}

async fn until_waiting(client: &Client, id: TicketId) -> TaskId {
    for _ in 0..800 {
        let detail = client.ticket(id).await.unwrap();
        if let Some(t) = detail.root_task.filter(|t| t.status == TaskStatus::Waiting) {
            return t.id;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the ticket never waited: {:?}", client.ticket(id).await.unwrap().root_task.map(|t| t.status));
}
