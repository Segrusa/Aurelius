//! A running answer asks the user before a tool (through the bridge endpoint),
//! waits in the inbox, and continues with exactly the user's decision.

mod common;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, RunId,
    approval::{ApprovalStatus, DecideApproval},
    conversation::SendChatMessage,
};
use aurelius_protocol::CommandResult;
use common::*;
use std::time::Duration;

/// A chat answer that stays alive for a few seconds (slow fake runtime).
async fn running_chat(client: &Client, root: &std::path::Path, text: &str) -> (ConversationId, RunId) {
    let env = envelope(
        Command::SendChatMessage(SendChatMessage { conversation_id: common::conversation(&client).await, text: text.into(), images: Vec::new(), interrupt: false }),
        None,
    );
    let CommandResult::ChatMessageSent { conversation_id, run_id, .. } = client.execute(env).await.unwrap().result else { panic!("not a chat message") };
    for _ in 0..200 {
        let status: String = db(root).query_row("SELECT status FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).unwrap();
        if matches!(status.as_str(), "starting" | "running" | "waiting") {
            return (conversation_id, run_id);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the run never started");
}

async fn pending(client: &Client) -> aurelius_protocol::ApprovalView {
    for _ in 0..200 {
        if let Some(a) = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.status == ApprovalStatus::Pending) {
            return a;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no pending approval");
}

fn decide(approval: &aurelius_protocol::ApprovalView, approve: bool) -> aurelius_domain::CommandEnvelope {
    envelope(Command::DecideApproval(DecideApproval { approval_id: approval.id, action_digest: approval.action_digest.clone(), approve, always_allow: false }), None)
}

#[tokio::test(flavor = "multi_thread")]
async fn ask_waits_for_the_user_then_continues() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 100).await;
    let client = connect(dir.path()).await;
    let (_, run) = running_chat(&client, dir.path(), "hallo").await;

    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let asked = tokio::spawn(async move { bridge.permission_ask(run, Some("bash-1".into()), "Bash".into(), serde_json::json!({ "command": "dir" })).await.unwrap() });

    let approval = pending(&client).await;
    assert_eq!(approval.tool, "claude.Bash");
    assert_eq!(approval.effect, "execute");
    assert_eq!(approval.arguments["command"], "dir");
    assert_eq!(approval.requester, "Chat");
    assert_eq!(approval.run_id, run);
    // Nobody decided: the bridge still waits.
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(!asked.is_finished());

    client.execute(decide(&approval, true)).await.unwrap();
    let answer = tokio::time::timeout(Duration::from_secs(5), asked).await.unwrap().unwrap();
    assert!(answer.allow, "{answer:?}");

    // A second response is refused: the approval is used.
    assert!(client.execute(decide(&approval, true)).await.is_err());
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn rejection_denies_and_a_changed_action_is_not_approved() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 100).await;
    let client = connect(dir.path()).await;
    let (_, run) = running_chat(&client, dir.path(), "hallo").await;

    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let asked = tokio::spawn(async move { bridge.permission_ask(run, Some("bash-1".into()), "Bash".into(), serde_json::json!({ "command": "del *" })).await.unwrap() });
    let approval = pending(&client).await;
    client.execute(decide(&approval, false)).await.unwrap();
    let answer = tokio::time::timeout(Duration::from_secs(5), asked).await.unwrap().unwrap();
    assert!(!answer.allow);
    assert_eq!(answer.message.as_deref(), Some("The user rejected this action."));

    // A decision on another digest than the one shown never approves.
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let asked = tokio::spawn(async move { bridge.permission_ask(run, Some("bash-2".into()), "Bash".into(), serde_json::json!({ "command": "dir" })).await.unwrap() });
    let approval = pending(&client).await;
    let wrong = envelope(Command::DecideApproval(DecideApproval { approval_id: approval.id, action_digest: "not-the-digest".into(), approve: true, always_allow: false }), None);
    client.execute(wrong).await.unwrap();
    let answer = tokio::time::timeout(Duration::from_secs(5), asked).await.unwrap().unwrap();
    assert!(!answer.allow);
    let status = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.id == approval.id).map(|a| a.status);
    assert!(matches!(status, None | Some(ApprovalStatus::Invalidated)), "{status:?}");
    stop(dir.path(), daemon).await;
}
