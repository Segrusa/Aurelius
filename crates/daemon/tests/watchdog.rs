//! The watchdog (AUR-BUG-05): a run that shows no progress at all is stopped with
//! a note in its answer, its conversation goes on with the message that waited,
//! and waiting on the user's approval does not count as hanging (AUR-BUG-05b: open
//! tasks, given from the conversation or waited for, do not switch the watchdog off;
//! a stopped run ends as stopped, an open tool gets 3x).

mod common;
use common::*;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, MessageId, RunId,
    conversation::{MessageRole, MessageStatus, SendChatMessage},
};
use aurelius_protocol::CommandResult;
use aurelius_runtime::{Activity, AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake::FakeRuntime};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;

/// The idle limit in these tests.
const IDLE_MS: u64 = 600;

/// The first run starts and then says nothing, not even when it is stopped (a hung
/// Claude Code process); every later run answers like the fake. With `tool`, it hangs
/// in a tool call it started.
#[derive(Default)]
struct Hangs {
    hung: AtomicBool,
    tool: bool,
}

#[async_trait::async_trait]
impl AgentRuntime for Hangs {
    async fn probe(&self) -> RuntimeCapabilities {
        FakeRuntime::default().probe().await
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        if self.hung.swap(true, Ordering::SeqCst) {
            return FakeRuntime { delay: Duration::from_millis(1), ..Default::default() }.start(request).await;
        }
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let tool = self.tool;
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            let _ = tx.send(RuntimeEvent::TextDelta("Let me look".into())).await;
            if tool {
                let started = Activity::Started { call_id: "c1".into(), tool: "Bash".into(), input: serde_json::json!({ "command": "cargo test" }), parent: None };
                let _ = tx.send(RuntimeEvent::Activity(started)).await;
            }
            // Holds the stream (and ignores the stop) for good.
            let _keep = (tx, cancel_rx);
            std::future::pending::<()>().await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

async fn start(root: &Path, runtime: Arc<dyn AgentRuntime>) -> Daemon {
    // SAFETY (test): all tests in this binary set the same value.
    unsafe { std::env::set_var("AURELIUS_RUN_IDLE_MS", IDLE_MS.to_string()) };
    start_with(root, runtime).await
}

async fn send(client: &Client, conversation_id: ConversationId, text: &str) -> (MessageId, RunId) {
    let send = SendChatMessage { confirm_secret: false, conversation_id, text: text.into(), images: Vec::new(), interrupt: false };
    match client.execute(envelope(Command::SendChatMessage(send), None)).await.unwrap().result {
        CommandResult::ChatMessageSent { message_id, run_id, .. } => (message_id, run_id),
        other => panic!("{other:?}"),
    }
}

fn run_state(root: &Path, run: RunId) -> (String, Option<String>, String) {
    db(root)
        .query_row("SELECT r.status, r.error_code, j.state FROM runs r JOIN jobs j ON j.id = r.id WHERE r.id = ?1", [run.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
}

async fn until_running(root: &Path, run: RunId) {
    for _ in 0..400 {
        if run_state(root, run).0 == "running" {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{run} never ran");
}

/// The answer to `message` once it is no longer partial: (status, text).
async fn settled(client: &Client, conversation: ConversationId, message: MessageId) -> (MessageStatus, String) {
    for _ in 0..800 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if let Some(m) = messages.iter().find(|m| m.role == MessageRole::Assistant && m.reply_to == Some(message) && m.status != MessageStatus::Partial) {
            return (m.status, m.text.clone());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no settled answer to {message}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_without_progress_is_stopped_with_a_note_and_the_waiting_message_goes_next() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path(), Arc::new(Hangs::default())).await;
    let client = connect(dir.path()).await;
    let conversation = common::conversation(&client).await;

    let (first, hung) = send(&client, conversation, "Fix the team chat.").await;
    until_running(dir.path(), hung).await;
    // Sent while it runs: it waits ("sends after the current answer").
    let (waiting, next) = send(&client, conversation, "Hello?").await;
    assert_eq!(run_state(dir.path(), next).0, "created");

    let (status, text) = settled(&client, conversation, first).await;
    assert_eq!(status, MessageStatus::Failed);
    assert!(text.starts_with("Let me look") && text.contains("showed no progress"), "the answer says why: {text}");
    assert_eq!(run_state(dir.path(), hung), ("failed".into(), Some("stalled".into()), "done".into()));

    // The conversation is free: the waiting message is answered.
    let (status, text) = settled(&client, conversation, waiting).await;
    assert_eq!(status, MessageStatus::Complete, "{text}");
    assert!(text.contains("Hello?"), "{text}");
    assert_eq!(run_state(dir.path(), next).0, "succeeded");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn waiting_on_an_approval_is_not_hanging() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path(), Arc::new(Hangs::default())).await;
    let client = connect(dir.path()).await;
    let conversation = common::conversation(&client).await;

    let (first, run) = send(&client, conversation, "Run the tests.").await;
    until_running(dir.path(), run).await;
    // It asks the user (as Claude Code does through the bridge) and waits.
    let rw = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    rw.busy_timeout(Duration::from_secs(5)).unwrap();
    rw.execute(
        "INSERT INTO approvals (id, run_id, call_id, tool, effect, action_blob_id, action_digest, request_event_id, status, expires_at_ms)
         VALUES ('a1', ?1, 'c1', 'Bash', 'execute', (SELECT id FROM blobs LIMIT 1), 'd', (SELECT id FROM events LIMIT 1), 'pending', 9999999999999)",
        [run.to_string()],
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(IDLE_MS * 4)).await;
    assert_eq!(run_state(dir.path(), run).0, "running", "waiting on the user is not hanging");

    // Decided, and still no progress: now it is stopped.
    rw.execute("UPDATE approvals SET status = 'rejected' WHERE id = 'a1'", []).unwrap();
    let (status, text) = settled(&client, conversation, first).await;
    assert_eq!(status, MessageStatus::Failed, "{text}");
    assert_eq!(run_state(dir.path(), run).1.as_deref(), Some("stalled"));
    stop(dir.path(), daemon).await;
}

fn rw(root: &Path) -> rusqlite::Connection {
    let rw = rusqlite::Connection::open(root.join("data/aurelius.sqlite")).unwrap();
    rw.busy_timeout(Duration::from_secs(5)).unwrap();
    rw
}

/// A task in the run's project, given from `origin` (a conversation), with `status`.
fn task(rw: &rusqlite::Connection, run: RunId, id: &str, status: &str, reason: Option<&str>, origin: bool) {
    rw.execute(
        "INSERT INTO tasks(id, project_id, title, brief_blob_id, status, waiting_reason, version, created_at_ms, updated_at_ms, origin_conversation_id)
         SELECT ?2, c.project_id, ?2, (SELECT id FROM blobs LIMIT 1), ?3, ?4, 1, 0, 0, CASE WHEN ?5 THEN c.id END
         FROM runs r JOIN conversations c ON c.id = r.conversation_id WHERE r.id = ?1",
        rusqlite::params![run.to_string(), id, status, reason, origin],
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn open_tasks_given_from_the_conversation_do_not_switch_the_watchdog_off() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path(), Arc::new(Hangs::default())).await;
    let client = connect(dir.path()).await;
    let conversation = common::conversation(&client).await;

    let (first, hung) = send(&client, conversation, "Split the work.").await;
    until_running(dir.path(), hung).await;
    // A manager's conversation: tasks it gave are still open (delegate_task answered at once).
    let rw = rw(dir.path());
    task(&rw, hung, "child-running", "running", None, true);
    task(&rw, hung, "child-queued", "queued", None, true);

    let (status, text) = settled(&client, conversation, first).await;
    assert_eq!(status, MessageStatus::Failed, "{text}");
    assert!(text.contains("showed no progress"), "{text}");
    assert_eq!(run_state(dir.path(), hung).1.as_deref(), Some("stalled"));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_run_of_a_task_waiting_for_its_colleagues_is_watched_and_the_parked_task_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path(), Arc::new(Hangs::default())).await;
    let client = connect(dir.path()).await;
    let conversation = common::conversation(&client).await;

    // A manager's task waits for its colleagues (its own run ended); a message to it
    // starts a run meanwhile, and that run hangs: it is not waiting, it is stopped.
    let (first, run) = send(&client, conversation, "How far are they?").await;
    until_running(dir.path(), run).await;
    let rw = rw(dir.path());
    task(&rw, run, "own", "waiting", Some("dependency"), false);
    rw.execute("UPDATE runs SET task_id = 'own' WHERE id = ?1", [run.to_string()]).unwrap();
    // Another task of the conversation, parked until its colleagues finish: no run to watch.
    task(&rw, run, "parked", "waiting", Some("dependency"), true);

    let (status, text) = settled(&client, conversation, first).await;
    assert_eq!(status, MessageStatus::Failed, "{text}");
    assert_eq!(run_state(dir.path(), run).1.as_deref(), Some("stalled"));
    tokio::time::sleep(Duration::from_millis(IDLE_MS * 2)).await;
    let parked: (String, Option<String>) = rw.query_row("SELECT status, waiting_reason FROM tasks WHERE id = 'parked'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(parked, ("waiting".into(), Some("dependency".into())), "a task parked on its colleagues keeps waiting");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_run_whose_runtime_does_not_end_ends_as_stopped_not_stalled() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path(), Arc::new(Hangs::default())).await;
    let client = connect(dir.path()).await;
    let conversation = common::conversation(&client).await;

    let (first, run) = send(&client, conversation, "Run the tests.").await;
    until_running(dir.path(), run).await;
    // Stop: the hung runtime ignores it.
    let stop_run = Command::StopRun(aurelius_domain::conversation::StopRun { run_id: run });
    client.execute(envelope(stop_run, None)).await.unwrap();
    assert_eq!(run_state(dir.path(), run).0, "cancelling");

    let (status, text) = settled(&client, conversation, first).await;
    assert_eq!(status, MessageStatus::Interrupted, "{text}");
    assert!(!text.contains("showed no progress"), "no stalled note for a stopped run: {text}");
    let (status, error, job) = run_state(dir.path(), run);
    assert_eq!((status.as_str(), job.as_str()), ("cancelled", "done"));
    assert_ne!(error.as_deref(), Some("stalled"));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_open_tool_call_gets_three_times_the_idle_limit() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path(), Arc::new(Hangs { tool: true, ..Default::default() })).await;
    let client = connect(dir.path()).await;
    let conversation = common::conversation(&client).await;

    let (first, run) = send(&client, conversation, "Run the tests.").await;
    until_running(dir.path(), run).await;
    let started = tokio::time::Instant::now();
    // Well past the idle limit, but a tool call is open: it may take longer.
    tokio::time::sleep(Duration::from_millis(IDLE_MS * 2)).await;
    assert_eq!(run_state(dir.path(), run).0, "running", "an open tool call gets more time");

    let (status, text) = settled(&client, conversation, first).await;
    assert_eq!(status, MessageStatus::Failed, "{text}");
    assert_eq!(run_state(dir.path(), run).1.as_deref(), Some("stalled"));
    assert!(started.elapsed() >= Duration::from_millis(IDLE_MS * 3 - 200), "stopped after about 3x the limit: {:?}", started.elapsed());
    stop(dir.path(), daemon).await;
}
