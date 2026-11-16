//! Aurelius' own memory and task tools (the approval bridge's) are never
//! asked, in any mode: Claude Code's plan mode still asks for MCP tools,
//! so the bridge allows them at once and nothing lands in the inbox.

mod common;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::{Client, envelope};
use aurelius_domain::approval::DecideApproval;
use aurelius_domain::autonomy::{AgentMode, SetAgentMode};
use aurelius_domain::{Command, ConversationId, RunId, approval::ApprovalStatus, conversation::SendChatMessage};
use aurelius_protocol::CommandResult;
use common::*;
use std::time::Duration;

/// A chat run in `conversation_id` that stays alive for a few seconds (slow fake runtime).
async fn live_run(client: &Client, root: &std::path::Path, conversation_id: ConversationId) -> RunId {
    let sent = client
        .execute(envelope(
            Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id, text: "houd me wakker".into(), images: Vec::new(), files: vec![], interrupt: false }),
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
    let run = live_run(&client, dir.path(), common::conversation(&client).await).await;
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

/// Plan mode refuses files, commands and external actions, but an employee's own
/// memory (remember, recall) keeps working: the plan-mode guidance says
/// so, and the bridge lets them through without asking.
#[tokio::test(flavor = "multi_thread")]
async fn own_tools_work_in_plan_mode_and_work_tools_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 100).await;
    let client = connect(dir.path()).await;
    let conversation_id = common::conversation(&client).await;
    client.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id, mode: AgentMode::Plan }), None)).await.unwrap();
    assert_eq!(client.messages(conversation_id).await.unwrap().agent_mode, AgentMode::Plan);
    let run = live_run(&client, dir.path(), conversation_id).await;
    let before = client.approvals().await.unwrap().approvals.len();

    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    for tool in ["mcp__aurelius__remember", "mcp__aurelius__recall"] {
        assert!(aurelius_runtime::claude_code::MEMORY_TOOLS.contains(&tool), "{tool} is one of the own tools");
        let answer = tokio::time::timeout(Duration::from_secs(5), bridge.permission_ask(run, None, tool.to_string(), serde_json::json!({}))).await.unwrap().unwrap();
        assert!(answer.allow, "{tool} allowed in plan mode without asking");
    }
    assert_eq!(client.approvals().await.unwrap().approvals.len(), before, "nothing was asked");

    // Commands and edits are never let through on their own: refused at once, or
    // asked in the inbox (refused here), never allowed.
    for (tool, input) in [("Bash", serde_json::json!({ "command": "rm -rf build" })), ("Edit", serde_json::json!({ "file_path": "src/main.rs", "old_string": "a", "new_string": "b" }))] {
        let root = dir.path().to_path_buf();
        let asking = tokio::spawn(async move { BridgeClient::connect(&root).await.unwrap().permission_ask(run, Some(format!("{tool}-1")), tool.into(), input).await });
        for _ in 0..200 {
            if asking.is_finished() {
                break;
            }
            if let Some(a) = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.status == ApprovalStatus::Pending && a.tool == format!("claude.{tool}")) {
                let reject = DecideApproval { approval_id: a.id, action_digest: a.action_digest.clone(), approve: false, always_allow: false };
                client.execute(envelope(Command::DecideApproval(reject), None)).await.unwrap();
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let answer = tokio::time::timeout(Duration::from_secs(10), asking).await.unwrap().unwrap().unwrap();
        assert!(!answer.allow, "{tool} is not allowed in plan mode");
    }

    stop(dir.path(), daemon).await;
}

/// C2: what an employee has off, or wants asked first, comes before every mode.
#[tokio::test(flavor = "multi_thread")]
async fn off_is_denied_and_asks_first_goes_to_the_inbox_even_in_auto() {
    use aurelius_domain::employee::{CreateEmployee, SendEmployeeMessage};
    use aurelius_domain::employee_tools::ToolPolicy;
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 100).await;
    let client = connect(dir.path()).await;
    let policy = ToolPolicy { tools: Some(vec!["Read".into(), "mcp__aurelius__create_ticket".into()]), approval_tools: vec!["Read".into(), "mcp__aurelius__create_ticket".into()], mcp_servers: None };
    let hire = CreateEmployee { access_level: None, tool_policy: Some(policy), start_profile: None, description: None, name: "Ann".into(), role: "Dev".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(hire), None)).await.unwrap().result else { panic!("not hired") };
    client.execute(envelope(Command::SetAgentMode(SetAgentMode { conversation_id, mode: AgentMode::Auto }), None)).await.unwrap();
    let tell = SendEmployeeMessage { confirm_secret: false, employee_id, text: "houd me wakker".into(), conversation_id, images: Vec::new(), files: Vec::new(), interrupt: false };
    let CommandResult::EmployeeMessageSent { run_id: run, .. } = client.execute(envelope(Command::SendEmployeeMessage(tell), None)).await.unwrap().result else { panic!("not sent") };
    for _ in 0..200 {
        let status: Vec<String> = { let conn = db(dir.path()); let mut q = conn.prepare("SELECT status FROM runs WHERE id = ?1").unwrap(); q.query_map([run.to_string()], |r| r.get(0)).unwrap().map(Result::unwrap).collect() };
        if status.first().is_some_and(|s| matches!(s.as_str(), "starting" | "running" | "waiting")) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let ask = |tool: &str| {
        let (root, tool) = (dir.path().to_path_buf(), tool.to_owned());
        tokio::spawn(async move { BridgeClient::connect(&root).await.unwrap().permission_ask(run, Some(format!("{tool}-1")), tool, serde_json::json!({})).await })
    };
    for tool in ["Bash", "mcp__aurelius__move_ticket"] {
        let answer = tokio::time::timeout(Duration::from_secs(5), ask(tool)).await.unwrap().unwrap().unwrap();
        assert!(!answer.allow, "{tool} is off");
    }
    let answer = tokio::time::timeout(Duration::from_secs(5), ask("mcp__aurelius__remember")).await.unwrap().unwrap().unwrap();
    assert!(answer.allow, "what cannot be switched is not asked");
    assert!(client.approvals().await.unwrap().approvals.iter().all(|a| a.status != ApprovalStatus::Pending), "nothing asked so far");

    // Auto would let Read through, and own tools never ask: these do.
    for (tool, shown) in [("Read", "claude.Read"), ("mcp__aurelius__create_ticket", "claude.mcp__aurelius__create_ticket")] {
        let asking = ask(tool);
        let mut approval = None;
        for _ in 0..200 {
            approval = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.status == ApprovalStatus::Pending && a.tool == shown);
            if approval.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let approval = approval.unwrap_or_else(|| panic!("{tool} was not asked"));
        let approve = DecideApproval { approval_id: approval.id, action_digest: approval.action_digest.clone(), approve: true, always_allow: false };
        client.execute(envelope(Command::DecideApproval(approve), None)).await.unwrap();
        let answer = tokio::time::timeout(Duration::from_secs(10), asking).await.unwrap().unwrap().unwrap();
        assert!(answer.allow, "{tool} goes ahead once approved");
    }
    stop(dir.path(), daemon).await;
}
