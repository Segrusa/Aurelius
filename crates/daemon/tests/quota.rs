//! A run that reaches the Claude usage limit is not failed: it waits for the reset
//! (parked, no attempt used), its task waits with it, and it goes again by itself,
//! also after a daemon restart.

mod common;
use common::*;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ProjectId, EmployeeId, TaskId,
    conversation::{MessageStatus, SendChatMessage},
    employee::CreateEmployee,
    project::CreateProject,
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, TaskView};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, SessionUse, fake};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// Every run hits the usage limit, which resets at `reset_at_ms` (none = unknown).
struct Limited {
    reset_at_ms: Option<i64>,
    /// The session each run asked for, with the text of its last turn.
    seen: Arc<Mutex<Vec<(SessionUse, String)>>>,
}

impl Limited {
    fn new(reset_at_ms: Option<i64>) -> Self {
        Self { reset_at_ms, seen: Default::default() }
    }
}

/// The fake runtime, keeping the session each run asked for.
struct Recording {
    seen: Arc<Mutex<Vec<(SessionUse, String)>>>,
}

#[async_trait::async_trait]
impl AgentRuntime for Recording {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        self.seen.lock().unwrap().push((request.session, request.turns.last().map(|t| t.text.clone()).unwrap_or_default()));
        fake::FakeRuntime::default().start(request).await
    }
}

#[async_trait::async_trait]
impl AgentRuntime for Limited {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        self.seen.lock().unwrap().push((request.session, request.turns.last().map(|t| t.text.clone()).unwrap_or_default()));
        let reset_at_ms = self.reset_at_ms;
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            let _ = tx.send(RuntimeEvent::TextDelta("Half an answ".into())).await;
            let _ = tx.send(RuntimeEvent::Quota { reset_at_ms }).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

/// (run status, error code, job state, job not-before, job attempts, last run.waiting metadata) of the only run.
fn parked(root: &Path) -> Option<(String, Option<String>, String, i64, i64, String)> {
    db(root)
        .query_row(
            "SELECT r.status, r.error_code, j.state, j.not_before_ms, j.attempts,
                    COALESCE((SELECT metadata_json FROM events WHERE aggregate_id = r.id AND type = 'run.waiting' ORDER BY seq DESC LIMIT 1), '')
             FROM runs r JOIN jobs j ON j.id = r.id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .ok()
}

async fn parked_run(root: &Path) -> (String, Option<String>, String, i64, i64, String) {
    for _ in 0..400 {
        if let Some(p) = parked(root).filter(|p| p.0 == "waiting") {
            return p;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the run did not wait: {:?}", parked(root));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_at_the_usage_limit_waits_for_the_reset_instead_of_failing() {
    let dir = tempfile::tempdir().unwrap();
    let reset = now_ms() + 3_600_000;
    let daemon = start_with(dir.path(), Arc::new(Limited::new(Some(reset)))).await;
    let client = connect(dir.path()).await;
    let send = SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "hello".into(), images: Vec::new(), interrupt: false };
    let CommandResult::ChatMessageSent { conversation_id, message_id, .. } = client.execute(envelope(Command::SendChatMessage(send), None)).await.unwrap().result else { panic!() };

    let (status, error, job, not_before, attempts, waiting) = parked_run(dir.path()).await;
    assert_eq!((status.as_str(), error.as_deref(), job.as_str()), ("waiting", Some("quota"), "ready"));
    assert_eq!(not_before, reset + 30_000, "it goes again just after the reset");
    assert_eq!(attempts, 0, "waiting for the limit never uses up an attempt");
    assert!(waiting.contains("\"quota\"") && waiting.contains(&format!("{}", reset + 30_000)), "{waiting}");
    let answer = client.messages(conversation_id).await.unwrap().messages.into_iter().find(|m| m.reply_to == Some(message_id));
    assert!(answer.is_none_or(|m| m.status != MessageStatus::Failed), "the answer is not failed");
    // Clients see until when, also for a plain chat (no task).
    let job = client.jobs().await.unwrap().jobs.into_iter().find(|j| j.conversation_id == Some(conversation_id)).unwrap();
    assert_eq!((job.not_before_ms, job.reason.as_deref()), (Some(reset + 30_000), Some("quota")));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_reset_time_it_tries_again_after_the_default_delay() {
    let dir = tempfile::tempdir().unwrap();
    let before = now_ms();
    let daemon = start_with(dir.path(), Arc::new(Limited::new(None))).await;
    let client = connect(dir.path()).await;
    let send = SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "hello".into(), images: Vec::new(), interrupt: false };
    client.execute(envelope(Command::SendChatMessage(send), None)).await.unwrap();
    let (_, error, job, not_before, attempts, _) = parked_run(dir.path()).await;
    assert_eq!((error.as_deref(), job.as_str(), attempts), (Some("quota"), "ready", 0));
    assert!(not_before >= before + aurelius_domain::scheduler::QUOTA_RETRY_MS, "{not_before}");
    stop(dir.path(), daemon).await;
}

async fn task(client: &Client, id: TaskId) -> TaskView {
    client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == id).unwrap()
}

async fn task_with(client: &Client) -> TaskId {
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let _: ProjectId = project_id;
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Alex".into(), role: "Backend developer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    let _: EmployeeId = employee_id;
    let cmd = CreateTask { confirm_secret: false, files: vec![], run: Default::default(), project_id, employee_id: Some(employee_id), title: "Do it".into(), goal: "Do the thing".into(), acceptance_criteria: vec![] };
    match client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result {
        CommandResult::TaskCreated { task_id } => task_id,
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_waits_for_the_limit_and_continues_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let reset = now_ms() + 3_600_000;
    let limited = Limited::new(Some(reset));
    let first = limited.seen.clone();
    let daemon = start_with(dir.path(), Arc::new(limited)).await;
    let client = connect(dir.path()).await;
    let id = task_with(&client).await;
    parked_run(dir.path()).await;
    let mut t = task(&client, id).await;
    for _ in 0..200 {
        if t.status == TaskStatus::Waiting {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        t = task(&client, id).await;
    }
    assert_eq!((t.status, t.waiting_reason.as_deref()), (TaskStatus::Waiting, Some("quota")), "not failed, not escalated");
    assert_eq!(t.waiting_until_ms, Some(reset + 30_000), "clients can show until when it waits");
    stop(dir.path(), daemon).await;

    // The limit has reset by the time the daemon is back.
    rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap().execute("UPDATE jobs SET not_before_ms = 0 WHERE state = 'ready'", []).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let daemon = start_with(dir.path(), Arc::new(Recording { seen: seen.clone() })).await;
    let client = connect(dir.path()).await;
    let mut t = task(&client, id).await;
    for _ in 0..400 {
        if matches!(t.status, TaskStatus::Review | TaskStatus::Succeeded) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        t = task(&client, id).await;
    }
    assert!(matches!(t.status, TaskStatus::Review | TaskStatus::Succeeded), "the task went on by itself: {:?}", t.status);
    stop(dir.path(), daemon).await;

    // The first attempt kept its session; the next carries on in it, with the
    // note in its turns for when the session is gone.
    let first = first.lock().unwrap().clone();
    assert_eq!(first.first().map(|f| f.0), Some(SessionUse::Keep), "{first:?}");
    let seen = seen.lock().unwrap().clone();
    assert!(seen.iter().any(|(session, last)| *session == SessionUse::Resume && last.contains("usage limit")), "{seen:?}");
}

/// (run status, job state) of the only run.
fn run_and_job(root: &Path) -> (String, String) {
    db(root).query_row("SELECT r.status, j.state FROM runs r JOIN jobs j ON j.id = r.id", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_parked_run_ends_it_at_once_and_its_chat_can_be_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with(dir.path(), Arc::new(Limited::new(Some(now_ms() + 3_600_000)))).await;
    let client = connect(dir.path()).await;
    let send = SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "hello".into(), images: Vec::new(), interrupt: false };
    let CommandResult::ChatMessageSent { conversation_id, message_id, run_id, .. } = client.execute(envelope(Command::SendChatMessage(send), None)).await.unwrap().result else { panic!() };
    parked_run(dir.path()).await;

    let stop_run = aurelius_domain::conversation::StopRun { run_id };
    let result = client.execute(envelope(Command::StopRun(stop_run), None)).await.unwrap().result;
    assert!(matches!(result, CommandResult::StopRequested { already_finished: false, .. }), "{result:?}");
    // No 'cancelling' until the reset: the run and its job end with the stop.
    assert_eq!(run_and_job(dir.path()), ("cancelled".to_string(), "cancelled".to_string()));
    let answer = client.messages(conversation_id).await.unwrap().messages.into_iter().find(|m| m.reply_to == Some(message_id)).expect("the half answer");
    assert_eq!(answer.status, MessageStatus::Interrupted);

    let delete = aurelius_domain::conversation::DeleteConversation { conversation_id };
    let result = client.execute(envelope(Command::DeleteConversation(delete), None)).await.unwrap().result;
    assert!(matches!(result, CommandResult::ConversationDeleted { .. }), "{result:?}");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_task_ends_its_parked_run_so_it_never_starts_at_the_reset() {
    let dir = tempfile::tempdir().unwrap();
    let limited = Limited::new(Some(now_ms() + 3_600_000));
    let seen = limited.seen.clone();
    let daemon = start_with(dir.path(), Arc::new(limited)).await;
    let client = connect(dir.path()).await;
    let id = task_with(&client).await;
    parked_run(dir.path()).await;

    let result = client.execute(envelope(Command::CancelTask(aurelius_domain::task::CancelTask { task_id: id }), None)).await.unwrap().result;
    assert!(matches!(result, CommandResult::TaskCancelled { .. }), "{result:?}");
    assert_eq!(run_and_job(dir.path()), ("cancelled".to_string(), "cancelled".to_string()));
    assert_eq!(task(&client, id).await.status, TaskStatus::Cancelled);

    // The reset comes: nothing of the task starts again.
    rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap().execute("UPDATE jobs SET not_before_ms = 0", []).unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(seen.lock().unwrap().len(), 1, "only the first attempt ran");
    assert_eq!(run_and_job(dir.path()), ("cancelled".to_string(), "cancelled".to_string()));
    stop(dir.path(), daemon).await;
}
