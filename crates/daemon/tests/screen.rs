//! Screenshots (the agent bridge's screen tools). Off by default: each one is
//! asked for in the inbox and the agent waits for the decision. With the
//! conversation's Screen on, the bridge gets an allow at once and nothing is asked.

mod common;

use aurelius_client::{Client, bridge::BridgeClient, envelope};
use aurelius_domain::{
    Command, ConversationId, RunId,
    approval::{ApprovalStatus, DecideApproval},
    autonomy::SetScreenAllowed,
    conversation::SendChatMessage,
};
use aurelius_protocol::{ApprovalView, CommandResult};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use common::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Notify, mpsc};

/// A runtime whose answer stays alive until released, so the run can ask permissions.
struct Hold {
    release: Arc<Notify>,
}

#[async_trait::async_trait]
impl AgentRuntime for Hold {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let release = self.release.clone();
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            release.notified().await;
            let _ = tx.send(RuntimeEvent::TextDelta("done".into())).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

/// A chat run that stays alive until `release`.
async fn waiting_run(client: &Client, root: &std::path::Path) -> (ConversationId, RunId) {
    let sent = client
        .execute(envelope(Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "Keep me awake".into(), images: Vec::new(), files: vec![], interrupt: false }), None))
        .await
        .unwrap()
        .result;
    let CommandResult::ChatMessageSent { conversation_id, run_id, .. } = sent else { panic!("{sent:?}") };
    for _ in 0..200 {
        let status: Option<String> = db(root).query_row("SELECT status FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).ok();
        if matches!(status.as_deref(), Some("starting" | "running")) {
            return (conversation_id, run_id);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the run never started");
}

async fn pending(client: &Client, tool: &str) -> ApprovalView {
    for _ in 0..200 {
        if let Some(a) = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.status == ApprovalStatus::Pending && a.tool == tool) {
            return a;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{tool} never reached the inbox");
}

#[tokio::test(flavor = "multi_thread")]
async fn screenshots_are_asked_for_until_the_user_allows_the_screen() {
    let dir = tempfile::tempdir().unwrap();
    let release = Arc::new(Notify::new());
    let daemon = start_with(dir.path(), Arc::new(Hold { release: release.clone() })).await;
    let client = connect(dir.path()).await;
    let (conversation_id, run) = waiting_run(&client, dir.path()).await;
    assert!(!client.messages(conversation_id).await.unwrap().screen_allowed, "off by default");
    let root = dir.path().to_path_buf();

    // Off: the agent asks; the user approves once in the inbox.
    let bridge = BridgeClient::connect(&root).await.unwrap();
    let waiting = tokio::spawn(async move { bridge.permission_ask(run, None, "mcp__aurelius__screenshot".into(), serde_json::json!({ "window": "Excel" })).await.unwrap() });
    let ask = pending(&client, "claude.mcp__aurelius__screenshot").await;
    assert_eq!(ask.arguments["window"], "Excel");
    assert_eq!(ask.effect, "read");
    client
        .execute(envelope(Command::DecideApproval(DecideApproval { approval_id: ask.id, action_digest: ask.action_digest.clone(), approve: true, always_allow: false }), None))
        .await
        .unwrap();
    let answer = waiting.await.unwrap();
    assert!(answer.allow, "approved in the inbox");

    // Rejected: the agent is told so.
    let bridge = BridgeClient::connect(&root).await.unwrap();
    let waiting = tokio::spawn(async move { bridge.permission_ask(run, None, "mcp__aurelius__screenshot".into(), serde_json::json!({})).await.unwrap() });
    let ask = pending(&client, "claude.mcp__aurelius__screenshot").await;
    client
        .execute(envelope(Command::DecideApproval(DecideApproval { approval_id: ask.id, action_digest: ask.action_digest.clone(), approve: false, always_allow: false }), None))
        .await
        .unwrap();
    let answer = waiting.await.unwrap();
    assert!(!answer.allow && answer.message.is_some());

    // On: allowed at once, nothing lands in the inbox.
    let set = |allowed| client.execute(envelope(Command::SetScreenAllowed(SetScreenAllowed { conversation_id, allowed }), None));
    assert!(matches!(set(true).await.unwrap().result, CommandResult::ScreenAllowedSet { allowed: true, .. }));
    assert!(client.messages(conversation_id).await.unwrap().screen_allowed);
    let before = client.approvals().await.unwrap().approvals.len();
    let bridge = BridgeClient::connect(&root).await.unwrap();
    let answer = tokio::time::timeout(Duration::from_secs(5), bridge.permission_ask(run, None, "mcp__aurelius__list_windows".into(), serde_json::json!({}))).await.unwrap().unwrap();
    assert!(answer.allow, "allowed without asking");
    assert_eq!(client.approvals().await.unwrap().approvals.len(), before, "nothing was asked");

    // Only the screen tools skip asking: other tools still ask.
    let bridge = BridgeClient::connect(&root).await.unwrap();
    let other = tokio::spawn(async move { bridge.permission_ask(run, Some("bash-1".into()), "Bash".into(), serde_json::json!({ "command": "dir" })).await });
    pending(&client, "claude.Bash").await;
    other.abort();

    assert!(matches!(set(false).await.unwrap().result, CommandResult::ScreenAllowedSet { allowed: false, .. }));
    assert!(!client.messages(conversation_id).await.unwrap().screen_allowed);
    let missing = client.execute(envelope(Command::SetScreenAllowed(SetScreenAllowed { conversation_id: ConversationId::new(), allowed: true }), None)).await;
    assert!(missing.is_err(), "unknown conversation");

    release.notify_one();
    stop(dir.path(), daemon).await;
}
