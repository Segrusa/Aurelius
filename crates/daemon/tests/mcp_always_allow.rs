//! 'Always allow' for MCP tools: approving one ask with it approves the other
//! asks for that tool, and later calls pass the bridge without asking, in every
//! run, until the user takes the tool off the list. Plan mode still blocks, and
//! Aurelius' own tools never qualify.

mod common;

use aurelius_client::{Client, bridge::BridgeClient, envelope};
use aurelius_domain::{
    Command, ConversationId, RunId,
    approval::{ApprovalStatus, DecideApproval},
    autonomy::{AgentMode, SetAgentMode},
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
        .execute(envelope(Command::SendChatMessage(SendChatMessage { conversation_id: common::conversation(&client).await, text: "Keep me awake".into(), images: Vec::new(), interrupt: false }), None))
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

fn decide(ask: &ApprovalView, always_allow: bool) -> Command {
    Command::DecideApproval(DecideApproval { approval_id: ask.id, action_digest: ask.action_digest.clone(), approve: true, always_allow })
}

const TOOL: &str = "mcp__github__create_issue";

#[tokio::test(flavor = "multi_thread")]
async fn always_allow_approves_waiting_asks_and_stops_asking_until_removed() {
    let dir = tempfile::tempdir().unwrap();
    let release = Arc::new(Notify::new());
    let daemon = start_with(dir.path(), Arc::new(Hold { release: release.clone() })).await;
    let client = connect(dir.path()).await;
    let root = dir.path().to_path_buf();
    // Two answers at the same time, whatever the machine's load.
    client.set_load_limits(aurelius_protocol::LoadLimitsUpdate { max_runs: Some(8), enabled: Some(false), ..Default::default() }).await.unwrap();
    let (conversation_id, run) = waiting_run(&client, dir.path()).await;
    let (_, other_run) = waiting_run(&client, dir.path()).await;
    assert!(client.mcp_always_allow().await.unwrap().tools.is_empty(), "empty by default");

    // Two asks for the same tool, in two runs, wait in the inbox.
    let ask_in = |run: RunId, call: &str| {
        let (root, call) = (root.clone(), call.to_owned());
        tokio::spawn(async move { BridgeClient::connect(&root).await.unwrap().permission_ask(run, Some(call.clone()), TOOL.into(), serde_json::json!({ "title": call })).await.unwrap() })
    };
    let first = ask_in(run, "a");
    let ask = pending(&client, &format!("claude.{TOOL}")).await;
    assert!(ask.always_allow_eligible, "an MCP tool of the user's own servers");
    let second = ask_in(other_run, "b");
    for _ in 0..200 {
        let waiting = client.approvals().await.unwrap().approvals.into_iter().filter(|a| a.status == ApprovalStatus::Pending).count();
        if waiting == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // Always allow on one: both go ahead and the tool is on the list.
    let decided = client.execute(envelope(decide(&ask, true), None)).await.unwrap().result;
    assert!(matches!(decided, CommandResult::ApprovalDecided { ok: true, status: ApprovalStatus::Consumed, .. }), "{decided:?}");
    assert!(first.await.unwrap().allow);
    assert!(tokio::time::timeout(Duration::from_secs(10), second).await.unwrap().unwrap().allow, "the other waiting ask was approved too");
    assert_eq!(client.mcp_always_allow().await.unwrap().tools, vec![TOOL.to_string()]);
    assert!(client.approvals().await.unwrap().approvals.iter().all(|a| a.status != ApprovalStatus::Pending));

    // Later calls pass at once, in any run, without an inbox entry.
    let before = client.approvals().await.unwrap().approvals.len();
    for r in [run, other_run] {
        let bridge = BridgeClient::connect(&root).await.unwrap();
        let answer = tokio::time::timeout(Duration::from_secs(5), bridge.permission_ask(r, None, TOOL.into(), serde_json::json!({}))).await.unwrap().unwrap();
        assert!(answer.allow, "always allowed");
    }
    assert_eq!(client.approvals().await.unwrap().approvals.len(), before, "nothing was asked");
    // Only that tool: another tool of the same server still asks.
    let other = {
        let root = root.clone();
        tokio::spawn(async move { BridgeClient::connect(&root).await.unwrap().permission_ask(run, Some("d".into()), "mcp__github__delete_repo".into(), serde_json::json!({})).await })
    };
    let asked = pending(&client, "claude.mcp__github__delete_repo").await;
    let reject = DecideApproval { approval_id: asked.id, action_digest: asked.action_digest.clone(), approve: false, always_allow: false };
    client.execute(envelope(Command::DecideApproval(reject), None)).await.unwrap();
    assert!(!other.await.unwrap().unwrap().allow);

    // Plan mode keeps blocking, also for an always-allowed tool.
    client.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id, mode: AgentMode::Plan }), None)).await.unwrap();
    // Refused at once (a switch to Plan) or asked in the inbox (the session already in Plan): never let through.
    let in_plan = ask_in(run, "plan");
    for _ in 0..200 {
        if in_plan.is_finished() {
            break;
        }
        if let Some(a) = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.status == ApprovalStatus::Pending) {
            assert_eq!(a.tool, format!("claude.{TOOL}"));
            let reject = DecideApproval { approval_id: a.id, action_digest: a.action_digest.clone(), approve: false, always_allow: false };
            client.execute(envelope(Command::DecideApproval(reject), None)).await.unwrap();
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(!tokio::time::timeout(Duration::from_secs(10), in_plan).await.unwrap().unwrap().allow, "Plan mode changes and runs nothing");
    client.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id, mode: AgentMode::Ask }), None)).await.unwrap();

    // Removed: the next call asks again.
    assert!(client.mcp_always_allow_remove(TOOL).await.unwrap().tools.is_empty());
    let again = ask_in(run, "e");
    let ask = pending(&client, &format!("claude.{TOOL}")).await;
    client.execute(envelope(decide(&ask, false), None)).await.unwrap();
    assert!(again.await.unwrap().allow);
    assert!(client.mcp_always_allow().await.unwrap().tools.is_empty(), "a plain approval does not add it");

    release.notify_one();
    release.notify_one();
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn aurelius_own_tools_and_work_tools_are_never_always_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let release = Arc::new(Notify::new());
    let daemon = start_with(dir.path(), Arc::new(Hold { release: release.clone() })).await;
    let client = connect(dir.path()).await;
    let root = dir.path().to_path_buf();
    let (_, run) = waiting_run(&client, dir.path()).await;

    for (tool, input) in [("mcp__aurelius__screenshot", serde_json::json!({})), ("Bash", serde_json::json!({ "command": "dir" }))] {
        let bridge = BridgeClient::connect(&root).await.unwrap();
        let waiting = tokio::spawn(async move { bridge.permission_ask(run, None, tool.into(), input).await.unwrap() });
        let ask = pending(&client, &format!("claude.{tool}")).await;
        assert!(!ask.always_allow_eligible, "{tool}");
        let refused = client.execute(envelope(decide(&ask, true), None)).await;
        assert!(refused.is_err(), "{tool}: {refused:?}");
        // Still waiting; a plain approval works.
        client.execute(envelope(decide(&ask, false), None)).await.unwrap();
        assert!(waiting.await.unwrap().allow);
    }
    assert!(client.mcp_always_allow().await.unwrap().tools.is_empty());

    release.notify_one();
    stop(dir.path(), daemon).await;
}
