//! Aurelius' own memory and task tools (the approval bridge's) are never
//! asked, in any mode: Claude Code's plan mode still asks for MCP tools,
//! so the bridge allows them at once and nothing lands in the inbox.

mod common;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::{Client, envelope};
use aurelius_domain::{Command, RunId, approval::ApprovalStatus, conversation::SendChatMessage};
use aurelius_protocol::CommandResult;
use common::*;
use std::time::Duration;

/// A chat run that stays alive for a few seconds (slow fake runtime).
async fn live_run(client: &Client, root: &std::path::Path) -> RunId {
    let sent = client
        .execute(envelope(
            Command::SendChatMessage(SendChatMessage { conversation_id: common::conversation(&client).await, text: "houd me wakker".into(), images: Vec::new(), interrupt: false }),
            None,
        ))
        .await
        .unwrap()
        .result;
    let CommandResult::ChatMessageSent { run_id, .. } = sent else { panic!("{sent:?}") };
    for _ in 0..200 {
        let status: String = db(root).query_row("SELECT status FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).unwrap();
        if matches!(status.as_str(), "starting" | "running" | "waiting") {
            return run_id;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the run never started");
}

#[tokio::test(flavor = "multi_thread")]
async fn own_tools_are_allowed_without_asking() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 100).await;
    let client = connect(dir.path()).await;
    let run = live_run(&client, dir.path()).await;
    let before = client.approvals().await.unwrap().approvals.len();

    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    for tool in aurelius_runtime::claude_code::MEMORY_TOOLS {
        let answer = tokio::time::timeout(Duration::from_secs(5), bridge.permission_ask(run, None, tool.to_string(), serde_json::json!({}))).await.unwrap().unwrap();
        assert!(answer.allow, "{tool} allowed without asking");
    }
    assert_eq!(client.approvals().await.unwrap().approvals.len(), before, "nothing was asked");

    // Other tools still ask.
    let other = BridgeClient::connect(dir.path()).await.unwrap();
    let asking = tokio::spawn(async move { other.permission_ask(run, Some("bash-1".into()), "Bash".into(), serde_json::json!({ "command": "dir" })).await });
    let mut asked = false;
    for _ in 0..200 {
        if client.approvals().await.unwrap().approvals.iter().any(|a| a.status == ApprovalStatus::Pending && a.tool == "claude.Bash") {
            asked = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(asked, "Bash is still asked");
    asking.abort();

    stop(dir.path(), daemon).await;
}
