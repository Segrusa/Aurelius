//! Retry with backoff, separate wait states, permanent
//! failure, and recovery of work claimed by a daemon that died.

mod common;
use common::*;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId,
    conversation::{MessageRole, MessageStatus, SendChatMessage},
};
use aurelius_protocol::CommandResult;
use aurelius_runtime::{AgentRuntime, Capability, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, fake::FakeRuntime};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// Fails the first `failures` starts with `error`, then behaves like the fake.
struct Scripted {
    failures: u32,
    error: RuntimeError,
    starts: AtomicU32,
    hang: bool,
}

#[async_trait::async_trait]
impl AgentRuntime for Scripted {
    async fn probe(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            runtime_kind: "scripted".into(),
            version: "test".into(),
            streaming: Capability::Supported,
            cancel: Capability::Supported,
            resume: Capability::Unsupported,
            tools: Capability::Unsupported,
            native_sandbox: Capability::Unsupported,
        }
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        if self.hang {
            std::future::pending::<()>().await;
        }
        let n = self.starts.fetch_add(1, Ordering::SeqCst);
        if n < self.failures {
            return Err(self.error.clone());
        }
        FakeRuntime { delay: Duration::from_millis(2), ..Default::default() }.start(request).await
    }
}

fn scripted(failures: u32, error: RuntimeError) -> Arc<Scripted> {
    Arc::new(Scripted { failures, error, starts: AtomicU32::new(0), hang: false })
}

async fn start_scripted(root: &Path, runtime: Arc<dyn AgentRuntime>) -> Daemon {
    // SAFETY (test): all tests in this binary set the same value.
    unsafe { std::env::set_var("AURELIUS_BACKOFF_BASE_MS", "20") };
    start_with(root, runtime).await
}

async fn chat(client: &Client, text: &str) -> ConversationId {
    let env = envelope(Command::SendChatMessage(SendChatMessage { conversation_id: common::conversation(&client).await, text: text.into(), images: Vec::new(), interrupt: false }), None);
    match client.execute(env).await.unwrap().result {
        CommandResult::ChatMessageSent { conversation_id, .. } => conversation_id,
        other => panic!("{other:?}"),
    }
}

async fn settled_answer(client: &Client, id: ConversationId) -> Option<MessageStatus> {
    for _ in 0..200 {
        let messages = client.messages(id).await.unwrap().messages;
        if let Some(a) = messages.iter().find(|m| m.role == MessageRole::Assistant && m.status != MessageStatus::Partial) {
            return Some(a.status);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    None
}

fn run_state(root: &Path) -> (String, Option<String>, String) {
    db(root)
        .query_row(
            "SELECT r.status, r.error_code, j.state FROM runs r JOIN jobs j ON j.id = r.id ORDER BY r.rowid DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn transient_failures_are_retried_with_backoff() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = scripted(2, RuntimeError::Unavailable("network".into()));
    let daemon = start_scripted(dir.path(), runtime.clone()).await;
    let client = connect(dir.path()).await;
    let id = chat(&client, "probeer opnieuw").await;
    assert_eq!(settled_answer(&client, id).await, Some(MessageStatus::Complete));
    assert_eq!(runtime.starts.load(Ordering::SeqCst), 3, "two failures, then success");
    let retries: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM events WHERE type = 'run.waiting'", [], |r| r.get(0)).unwrap();
    assert_eq!(retries, 2);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_failure_parks_without_polling() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = scripted(u32::MAX, RuntimeError::Auth("login expired".into()));
    let daemon = start_scripted(dir.path(), runtime.clone()).await;
    let client = connect(dir.path()).await;
    chat(&client, "hallo").await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    let (status, error, job) = run_state(dir.path());
    assert_eq!((status.as_str(), error.as_deref(), job.as_str()), ("waiting", Some("auth"), "ready"));
    assert_eq!(runtime.starts.load(Ordering::SeqCst), 1, "parked: no retry loop against a dead login");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn permanent_failure_ends_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = scripted(u32::MAX, RuntimeError::Permanent("unsupported_model".into()));
    let daemon = start_scripted(dir.path(), runtime.clone()).await;
    let client = connect(dir.path()).await;
    chat(&client, "hallo").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (status, error, job) = run_state(dir.path());
    assert_eq!((status.as_str(), error.as_deref(), job.as_str()), ("failed", Some("unsupported_model"), "dead"));
    assert_eq!(runtime.starts.load(Ordering::SeqCst), 1);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_never_starts_says_why_in_the_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let error = "could not start claude.exe: The filename or extension is too long. (os error 206)";
    let runtime = scripted(u32::MAX, RuntimeError::Unavailable(error.into()));
    let daemon = start_scripted(dir.path(), runtime.clone()).await;
    let client = connect(dir.path()).await;
    let id = chat(&client, "hallo").await;
    // Every retry waits a little longer: allow for all of them.
    let mut answer = None;
    for _ in 0..1200 {
        let messages = client.messages(id).await.unwrap().messages;
        answer = messages.into_iter().find(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Failed);
        if answer.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let answer = answer.expect("the run fails after its retries, with an answer that says why");
    assert_eq!(run_state(dir.path()).0, "failed");
    assert!(answer.text.contains("could not be started") && answer.text.contains("os error 206"), "{}", answer.text);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_cannot_be_planned_fails_and_frees_the_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = scripted(0, RuntimeError::Unavailable(String::new()));
    let daemon = start_scripted(dir.path(), runtime.clone()).await;
    let client = connect(dir.path()).await;
    // An unreadable config snapshot makes planning fail on every attempt.
    let rw = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    rw.busy_timeout(Duration::from_secs(5)).unwrap();
    rw.execute_batch("CREATE TRIGGER break_plan AFTER INSERT ON runs BEGIN UPDATE runs SET resolved_config_json = '[1]' WHERE id = NEW.id; END;").unwrap();
    let id = chat(&client, "kapot").await;
    // Every retry waits a little longer: allow for all of them, also under load.
    for _ in 0..1200 {
        if run_state(dir.path()).2 != "leased" && run_state(dir.path()).2 != "ready" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let (status, error, job) = run_state(dir.path());
    assert_eq!((status.as_str(), error.as_deref(), job.as_str()), ("failed", Some("retries_exhausted:plan_failed"), "dead"));
    assert_eq!(runtime.starts.load(Ordering::SeqCst), 0);

    // The next message in the same conversation is no longer blocked.
    rw.execute_batch("DROP TRIGGER break_plan;").unwrap();
    let env = envelope(Command::SendChatMessage(SendChatMessage { conversation_id: id, text: "nu wel".into(), images: Vec::new(), interrupt: false }), None);
    client.execute(env).await.unwrap();
    let mut done = false;
    for _ in 0..200 {
        let messages = client.messages(id).await.unwrap().messages;
        if messages.iter().filter(|m| m.role == MessageRole::Assistant).any(|m| m.status == MessageStatus::Complete) {
            done = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(done, "second message answered");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn work_claimed_by_a_dead_daemon_runs_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    // The first daemon claims the job and then hangs inside the runtime start.
    let hanging = Arc::new(Scripted { failures: 0, error: RuntimeError::Unavailable(String::new()), starts: AtomicU32::new(0), hang: true });
    let daemon = start_scripted(dir.path(), hanging).await;
    let id = {
        let client = connect(dir.path()).await;
        let id = chat(&client, "na herstart").await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        let (_, _, job) = run_state(dir.path());
        assert_eq!(job, "leased");
        id
    };
    stop(dir.path(), daemon).await;

    // A new daemon reconciles: the run never started, so it is safe to run it again.
    let daemon = start_scripted(dir.path(), scripted(0, RuntimeError::Unavailable(String::new()))).await;
    let client = connect(dir.path()).await;
    assert_eq!(settled_answer(&client, id).await, Some(MessageStatus::Complete));
    stop(dir.path(), daemon).await;
}
