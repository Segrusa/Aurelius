//! The watchdog (AUR-BUG-05): a run that shows no progress at all is stopped with
//! a note in its answer, its conversation goes on with the message that waited,
//! and waiting on the user's approval does not count as hanging.

mod common;
use common::*;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, MessageId, RunId,
    conversation::{MessageRole, MessageStatus, SendChatMessage},
};
use aurelius_protocol::CommandResult;
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake::FakeRuntime};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;

/// The idle limit in these tests.
const IDLE_MS: u64 = 600;

/// The first run starts and then says nothing, not even when it is stopped (a hung
/// Claude Code process); every later run answers like the fake.
#[derive(Default)]
struct Hangs {
    hung: AtomicBool,
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
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            let _ = tx.send(RuntimeEvent::TextDelta("Let me look".into())).await;
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
    let send = SendChatMessage { conversation_id, text: text.into(), images: Vec::new(), interrupt: false };
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
