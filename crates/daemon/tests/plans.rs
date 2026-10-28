//! A plan Claude Code hands in (ExitPlanMode) is decided in the inbox: approve
//! (the work continues in auto or accept-edits mode), ask for changes (feedback required),
//! or reject. Goes through the real bridge path: bridge endpoint → daemon → inbox → decision.

mod common;
use common::*;

use aurelius_client::{Client, bridge::BridgeClient, bridge::PermissionAnswer, envelope};
use aurelius_domain::{
    ApprovalId, Command, ConversationId, RunId,
    approval::{ApprovalStatus, DecidePlan, PlanDecision},
    autonomy::{AgentMode, SetAgentMode, SetBrowserAllowed},
    conversation::SendChatMessage,
};
use aurelius_protocol::{ApprovalView, CommandResult};
use std::path::Path;
use std::time::Duration;

/// A chat run that stays alive (the fake streams slowly; `start_with_delay`).
async fn live_run(root: &Path, client: &Client) -> (ConversationId, RunId) {
    let sent = client
        .execute(envelope(Command::SendChatMessage(SendChatMessage { conversation_id: common::conversation(&client).await, text: "houd me wakker".into(), images: Vec::new(), interrupt: false }), None))
        .await
        .unwrap()
        .result;
    let CommandResult::ChatMessageSent { conversation_id, run_id, .. } = sent else { panic!("{sent:?}") };
    // Plans can only be handed in to a run that is alive.
    for _ in 0..200 {
        let status: String = db(root).query_row("SELECT status FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).unwrap();
        if status == "running" {
            return (conversation_id, run_id);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the run never started");
}

async fn pending_plan(client: &Client) -> ApprovalView {
    for _ in 0..200 {
        if let Some(a) = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.status == ApprovalStatus::Pending && a.tool == "claude.ExitPlanMode") {
            return a;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the plan never reached the inbox");
}

/// Claude hands in a plan; the handle gives what Claude gets back once it is decided.
async fn hand_in(root: &Path, run: RunId, call: &str) -> tokio::task::JoinHandle<PermissionAnswer> {
    let bridge = BridgeClient::connect(root).await.unwrap();
    let call = call.to_owned();
    tokio::spawn(async move {
        let input = serde_json::json!({ "plan": "# Plan\n\n1. Create hello.txt", "planFilePath": "C:/plans/p.md" });
        bridge.permission_ask(run, Some(call), "ExitPlanMode".into(), input).await.unwrap()
    })
}

async fn decide(client: &Client, approval_id: ApprovalId, decision: PlanDecision, feedback: Option<&str>) -> Result<aurelius_domain::CommandReceipt, aurelius_client::ClientError> {
    client.execute(envelope(Command::DecidePlan(DecidePlan { approval_id, decision, feedback: feedback.map(str::to_owned) }), None)).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plan_is_approved_revised_or_rejected_in_the_inbox() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 400).await;
    let client = connect(dir.path()).await;
    let (_, run) = live_run(dir.path(), &client).await;

    // Changes: the plan shows up with its text; feedback is required; Claude gets the words back.
    let waiting = hand_in(dir.path(), run, "plan-1").await;
    let plan = pending_plan(&client).await;
    assert_eq!(plan.arguments["plan"], "# Plan\n\n1. Create hello.txt");
    assert!(decide(&client, plan.id, PlanDecision::Revise, None).await.is_err(), "changes need feedback");
    decide(&client, plan.id, PlanDecision::Revise, Some("Also add a README")).await.unwrap();
    let answer = waiting.await.unwrap();
    assert!(!answer.allow && answer.updated_permissions.is_none());
    let message = answer.message.unwrap();
    assert!(message.contains("Also add a README") && message.contains("ExitPlanMode"), "{message}");
    // Decided already (on this or another device): refused.
    assert!(decide(&client, plan.id, PlanDecision::Approve, None).await.is_err());

    // Approve: the plan goes ahead in Auto.
    let waiting = hand_in(dir.path(), run, "plan-2").await;
    let plan = pending_plan(&client).await;
    decide(&client, plan.id, PlanDecision::Approve, None).await.unwrap();
    let answer = waiting.await.unwrap();
    assert!(answer.allow && answer.message.is_none());
    assert_eq!(answer.updated_input.unwrap()["plan"], "# Plan\n\n1. Create hello.txt");
    // Aurelius' Auto: the session leaves plan mode without allow rules (they would outlive a switch back); the bridge allows the work.
    assert_eq!(answer.updated_permissions.unwrap(), serde_json::json!([{ "type": "setMode", "mode": "default", "destination": "session" }]));

    // Approve with edits: the plan goes ahead, edits go through and the rest is asked.
    let waiting = hand_in(dir.path(), run, "plan-e").await;
    let plan = pending_plan(&client).await;
    decide(&client, plan.id, PlanDecision::ApproveEdits, None).await.unwrap();
    let answer = waiting.await.unwrap();
    assert!(answer.allow && answer.message.is_none());
    assert_eq!(answer.updated_input.unwrap()["plan"], "# Plan\n\n1. Create hello.txt");
    assert_eq!(answer.updated_permissions.unwrap(), serde_json::json!([{ "type": "setMode", "mode": "acceptEdits", "destination": "session" }]));

    // Reject: Claude is told to stop, with the reason.
    let waiting = hand_in(dir.path(), run, "plan-3").await;
    let plan = pending_plan(&client).await;
    decide(&client, plan.id, PlanDecision::Reject, Some("Wrong direction")).await.unwrap();
    let answer = waiting.await.unwrap();
    assert!(!answer.allow && answer.message.unwrap().contains("Wrong direction"));

    stop(dir.path(), daemon).await;
}

/// Auto is the user's current mode, checked on every call: switching back to
/// Ask in the middle of an answer makes the very next command ask again.
#[tokio::test(flavor = "multi_thread")]
async fn a_switch_back_from_auto_holds_mid_answer() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 400).await;
    let client = connect(dir.path()).await;
    let (conversation_id, run) = live_run(dir.path(), &client).await;
    let waiting = hand_in(dir.path(), run, "plan-a").await;
    let plan = pending_plan(&client).await;
    decide(&client, plan.id, PlanDecision::Approve, None).await.unwrap();
    assert!(waiting.await.unwrap().allow);

    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let bash = || serde_json::json!({ "command": "echo hi" });
    let answer = bridge.permission_ask(run, Some("b1".into()), "Bash".into(), bash()).await.unwrap();
    assert!(answer.allow, "Auto: the command goes ahead");

    client.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id, mode: AgentMode::Ask }), None)).await.unwrap();
    let asked = tokio::spawn(async move { bridge.permission_ask(run, Some("b2".into()), "Bash".into(), bash()).await.unwrap() });
    let mut in_inbox = false;
    for _ in 0..200 {
        if client.approvals().await.unwrap().approvals.iter().any(|a| a.status == ApprovalStatus::Pending && a.tool.ends_with("Bash")) {
            in_inbox = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(in_inbox && !asked.is_finished(), "back in Ask: the next command waits for the user");
    asked.abort();

    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approved_plan_never_waves_the_browser_through() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 400).await;
    let client = connect(dir.path()).await;
    let (conversation_id, run) = live_run(dir.path(), &client).await;
    client.execute(envelope(Command::SetBrowserAllowed(SetBrowserAllowed { conversation_id, allowed: true }), None)).await.unwrap();

    let waiting = hand_in(dir.path(), run, "plan-b").await;
    let plan = pending_plan(&client).await;
    decide(&client, plan.id, PlanDecision::Approve, None).await.unwrap();
    let answer = waiting.await.unwrap();
    assert!(answer.allow);
    // Not Claude Code's auto mode (it could skip asking for the browser), and no allow rules either.
    assert_eq!(answer.updated_permissions.unwrap(), serde_json::json!([{ "type": "setMode", "mode": "default", "destination": "session" }]));
    // The browser the user switched on goes through in Auto without a question.
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let browsing = tokio::spawn(async move { bridge.permission_ask(run, Some("nav".into()), "mcp__claude-in-chrome__navigate".into(), serde_json::json!({ "url": "https://example.com" })).await.unwrap() });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(browsing.is_finished(), "the browser is not asked in Auto");
    browsing.abort();

    // The plan stays readable in the chat, with the decision under it.
    let messages = client.messages(conversation_id).await.unwrap().messages;
    let shown = messages.iter().find(|m| m.update.as_deref() == Some("plan")).expect("the plan is in the conversation");
    assert_eq!((shown.text.as_str(), shown.approval_id), ("# Plan\n\n1. Create hello.txt", Some(plan.id)));
    assert!(messages.iter().any(|m| m.update.as_deref() == Some("status") && m.text == "Approved the plan."));
    // The answer goes on below the plan and the decision.
    let last = messages.last().unwrap();
    assert_eq!((last.run_id, last.role), (Some(run), aurelius_domain::conversation::MessageRole::Assistant), "{messages:#?}");

    stop(dir.path(), daemon).await;
}
