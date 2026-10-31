//! B23 fault injection with the real service process: a hard kill in the
//! middle of an answer (A15), and a storage write failure.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId,
    conversation::{MessageRole, MessageStatus, SendChatMessage},
};
use aurelius_protocol::{CommandResult, MessageView};
use std::path::Path;
use std::process::{Child, Command as Process};
use std::time::Duration;
use tokio::sync::mpsc;

fn spawn(root: &Path, delay_ms: u32) -> Child {
    Process::new(env!("CARGO_BIN_EXE_aurelius-daemon"))
        .env("AURELIUS_DATA_DIR", root)
        // Same environment as this test process, so both use the same service endpoint.
        .env("AURELIUS_ENV", aurelius_host::Environment::current().as_str())
        .env("AURELIUS_FAKE_DELAY_MS", delay_ms.to_string())
        .env("AURELIUS_NO_TRAY", "1")
        .env("AURELIUS_NO_AUTO_BACKUP", "1")
        .spawn()
        .expect("start the service")
}

async fn connect(root: &Path) -> Client {
    for _ in 0..200 {
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        if let Ok(client) = Client::connect(&aurelius_host::endpoint(root), "test", tx).await {
            common::sign_in(root, &client, common::BOARD_MEMBER).await;
            return client;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the service did not start");
}

async fn send(client: &Client, text: &str) -> ConversationId {
    send_in(client, common::conversation(client).await, text).await
}

async fn send_in(client: &Client, conversation_id: ConversationId, text: &str) -> ConversationId {
    match client.execute(envelope(Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id, text: text.into(), images: Vec::new(), interrupt: false }), None)).await.unwrap().result {
        CommandResult::ChatMessageSent { conversation_id, .. } => conversation_id,
        other => panic!("{other:?}"),
    }
}

fn assistant(messages: &[MessageView]) -> Vec<&MessageView> {
    messages.iter().filter(|m| m.role == MessageRole::Assistant).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn hard_kill_mid_answer_is_reconciled_not_rerun() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = spawn(dir.path(), 120);
    let client = connect(dir.path()).await;
    let conversation = send(&client, "Tell me a long story").await;

    // Wait until the answer is streaming, then kill the process outright.
    let mut streaming = false;
    for _ in 0..200 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if assistant(&messages).iter().any(|m| m.status == MessageStatus::Partial && !m.text.is_empty()) {
            streaming = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(streaming, "the answer started streaming");
    service.kill().unwrap();
    service.wait().unwrap();
    drop(client);

    // After a restart the interrupted answer is settled, not continued or repeated.
    let mut service = spawn(dir.path(), 1);
    let client = connect(dir.path()).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    let messages = client.messages(conversation).await.unwrap().messages;
    let answers = assistant(&messages);
    assert_eq!(answers.len(), 1, "no second answer was started blindly");
    assert!(matches!(answers[0].status, MessageStatus::Failed | MessageStatus::Interrupted), "{:?}", answers[0].status);
    let jobs = client.jobs().await.unwrap().jobs;
    assert!(
        jobs.iter().all(|j| !matches!(j.state, aurelius_domain::job::JobState::Ready | aurelius_domain::job::JobState::Leased)),
        "nothing re-queued: {jobs:?}"
    );

    // The service is healthy: a new question gets a complete answer.
    let next = send(&client, "Are you there?").await;
    let mut done = false;
    for _ in 0..400 {
        let messages = client.messages(next).await.unwrap().messages;
        if assistant(&messages).iter().any(|m| m.status == MessageStatus::Complete) {
            done = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(done);
    aurelius_client::bridge::shutdown_daemon(dir.path()).await.unwrap();
    assert!(service.wait().unwrap().success());
}

#[tokio::test(flavor = "multi_thread")]
async fn storage_write_failure_fails_the_command_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = spawn(dir.path(), 1);
    let client = connect(dir.path()).await;
    let conversation = common::conversation(&client).await;
    let asked = |messages: Vec<MessageView>| messages.iter().filter(|m| m.role == MessageRole::User).count();

    // Inject a write failure: the blob staging folder becomes a file.
    let staging = dir.path().join("blobs").join("tmp");
    std::fs::remove_dir_all(&staging).unwrap();
    std::fs::write(&staging, b"not a folder").unwrap();
    let failed = client.execute(envelope(Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: conversation, text: "lost?".into(), images: Vec::new(), interrupt: false }), None)).await;
    assert!(failed.is_err(), "the command reports the failure");
    assert_eq!(asked(client.messages(conversation).await.unwrap().messages), 0, "nothing half-written");

    // Space back: everything works again, without a restart.
    std::fs::remove_file(&staging).unwrap();
    std::fs::create_dir_all(&staging).unwrap();
    send_in(&client, conversation, "and now?").await;
    assert_eq!(asked(client.messages(conversation).await.unwrap().messages), 1);
    aurelius_client::bridge::shutdown_daemon(dir.path()).await.unwrap();
    assert!(service.wait().unwrap().success());
}
