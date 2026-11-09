//! The board tools of employees through the real bridge channel (AUR-BOARD-TOOL-001): the daemon
//! maps `list_tickets`, `read_ticket`, `create_ticket`, `update_ticket`, `move_ticket` and
//! `post_to_ticket` to the board, checks who may use them, redacts vault values in what a run
//! writes and leaves nothing behind when it refuses.

mod common;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::{Client, envelope};
use aurelius_domain::employee::{CreateEmployee, SendEmployeeMessage};
use aurelius_domain::project::CreateProject;
use aurelius_domain::{Command, CommandResult, ConversationId, EmployeeId, RunId};
use aurelius_protocol::EmployeeTaskOp;
use common::*;
use std::path::Path;
use std::time::Duration;

async fn hire(client: &Client, name: &str, boss: Option<EmployeeId>) -> (EmployeeId, ConversationId) {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Dev".into(), title: None, reports_to: boss, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    (employee_id, conversation_id)
}

/// A run in the employee's chat on the signed-in board member's login.
async fn run_in(client: &Client, root: &Path, employee_id: EmployeeId, conversation_id: ConversationId) -> RunId {
    let message = SendEmployeeMessage { employee_id, text: "hello".into(), conversation_id, images: Vec::new(), files: Vec::new(), interrupt: false, confirm_secret: false };
    let sent = client.execute(envelope(Command::SendEmployeeMessage(message), None)).await.unwrap().result;
    let CommandResult::EmployeeMessageSent { run_id, .. } = sent else { panic!("{sent:?}") };
    for _ in 0..200 {
        let seen: i64 = db(root).query_row("SELECT COUNT(*) FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).unwrap();
        if seen == 1 {
            return run_id;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no run");
}

fn count(root: &Path, sql: &str) -> i64 {
    db(root).query_row(sql, [], |r| r.get(0)).unwrap()
}

fn ticket_row(root: &Path, title_like: &str) -> Option<(String, String)> {
    db(root)
        .query_row("SELECT k.title, c.kind FROM tickets k JOIN board_columns c ON c.id = k.column_id WHERE k.title LIKE ?1", [format!("%{title_like}%")], |r| Ok((r.get(0)?, r.get(1)?)))
        .ok()
}

#[tokio::test(flavor = "multi_thread")]
async fn management_makes_and_starts_a_ticket_and_a_plain_agent_cannot() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with_delay(dir.path(), 100).await;
    let client = connect_as(dir.path(), "grace").await;
    client.execute(envelope(Command::CreateProject(CreateProject { name: "Orbit".into(), description: None, folder: None }), None)).await.unwrap();
    let (atlas, atlas_chat) = hire(&client, "Atlas", None).await;
    let (ann, ann_chat) = hire(&client, "Ann", Some(atlas)).await;
    let atlas_run = run_in(&client, dir.path(), atlas, atlas_chat).await;
    let ann_run = run_in(&client, dir.path(), ann, ann_chat).await;
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let create = |title: &str| EmployeeTaskOp::CreateTicket { project: Some("Orbit".into()), title: title.into(), description: Some("Why.".into()), priority: Some("high".into()), labels: vec!["api".into()] };

    // A plain agent is refused, and nothing is written.
    // Only the ticket's own events count: the two runs of this test go on writing theirs.
    let of_tickets = "SELECT COUNT(*) FROM events WHERE aggregate_type = 'ticket'";
    let (events, tickets) = (count(dir.path(), of_tickets), count(dir.path(), "SELECT COUNT(*) FROM tickets"));
    let (ok, text) = bridge.employee_act(ann_run, create("By Ann")).await.unwrap();
    assert!(!ok && text.contains("Only management creates tickets"), "{text}");
    assert_eq!((count(dir.path(), of_tickets), count(dir.path(), "SELECT COUNT(*) FROM tickets")), (events, tickets));

    // The head of the chart makes one: Backlog, no task, no run, no job.
    let (runs, tasks, jobs) = (count(dir.path(), "SELECT COUNT(*) FROM runs"), count(dir.path(), "SELECT COUNT(*) FROM tasks"), count(dir.path(), "SELECT COUNT(*) FROM jobs WHERE kind LIKE 'ticket.%'"));
    let (ok, text) = bridge.employee_act(atlas_run, create("Export to CSV")).await.unwrap();
    assert!(ok, "{text}");
    assert_eq!(ticket_row(dir.path(), "Export to CSV"), Some(("Export to CSV".into(), "backlog".into())));
    assert_eq!((count(dir.path(), "SELECT COUNT(*) FROM runs"), count(dir.path(), "SELECT COUNT(*) FROM tasks"), count(dir.path(), "SELECT COUNT(*) FROM jobs WHERE kind LIKE 'ticket.%'")), (runs, tasks, jobs), "no run, task or job");
    let (kind, actor): (String, String) = db(dir.path())
        .query_row("SELECT actor_kind, actor_id FROM events WHERE type = 'ticket.created' ORDER BY seq DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!((kind.as_str(), actor), ("employee", atlas.to_string()));

    // Everyone reads it; a plain agent cannot change or start it, the head of the chart starts it.
    let (ok, list) = bridge.employee_act(ann_run, EmployeeTaskOp::ListTickets { project: None, column: None, query: Some("csv".into()), limit: None }).await.unwrap();
    assert!(ok && list.contains("Export to CSV") && list.contains("[backlog]"), "{list}");
    let (ok, read) = bridge.employee_act(ann_run, EmployeeTaskOp::ReadTicket { ticket: "#1".into(), last: Some(5) }).await.unwrap();
    assert!(ok && read.contains("Why."), "{read}");
    let (ok, text) = bridge.employee_act(ann_run, EmployeeTaskOp::MoveTicket { ticket: "#1".into(), column: "todo".into() }).await.unwrap();
    assert!(!ok && text.contains("Only management moves"), "{text}");
    let (ok, text) = bridge.employee_act(ann_run, EmployeeTaskOp::UpdateTicket { design_url: None, ticket: "#1".into(), title: Some("Ann's".into()), description: None, priority: None, labels: None }).await.unwrap();
    assert!(!ok && text.contains("you work on"), "{text}");
    for column in ["done", "doing", "review", "testing", "plan_review"] {
        let (ok, _) = bridge.employee_act(atlas_run, EmployeeTaskOp::MoveTicket { ticket: "#1".into(), column: column.into() }).await.unwrap();
        assert!(!ok, "{column}");
    }
    assert_eq!(ticket_row(dir.path(), "Export to CSV").unwrap().1, "backlog");
    let (ok, text) = bridge.employee_act(atlas_run, EmployeeTaskOp::UpdateTicket { design_url: None, ticket: "#1".into(), title: Some("Export to CSV and JSON".into()), description: None, priority: None, labels: None }).await.unwrap();
    assert!(ok, "{text}");
    let (ok, text) = bridge.employee_act(atlas_run, EmployeeTaskOp::MoveTicket { ticket: "#1".into(), column: "todo".into() }).await.unwrap();
    assert!(ok, "{text}");
    assert_eq!(ticket_row(dir.path(), "CSV and JSON").unwrap().1, "todo");
    assert_eq!(count(dir.path(), "SELECT COUNT(*) FROM tasks WHERE ticket_id IS NOT NULL"), 1, "the start made the main task");
    let (ok, text) = bridge.employee_act(ann_run, EmployeeTaskOp::PostToTicket { ticket: "#1".into(), message: "Looks good.".into() }).await.unwrap();
    assert!(ok, "{text}");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_vault_value_in_a_ticket_title_or_message_never_reaches_the_board() {
    let dir = tempfile::tempdir().unwrap();
    let owner = dir.path().display().to_string();
    let daemon = start_with_delay(dir.path(), 100).await;
    let client = connect_as(dir.path(), "grace").await;
    client.execute(envelope(Command::CreateProject(CreateProject { name: "Orbit".into(), description: None, folder: None }), None)).await.unwrap();
    let (atlas, atlas_chat) = hire(&client, "Atlas", None).await;
    hire(&client, "Ann", Some(atlas)).await;
    let run = run_in(&client, dir.path(), atlas, atlas_chat).await;
    let secret = "sk-test-FAKE-board-tool-canary-7731";
    aurelius_runtime::redact::add_to_set(&owner, aurelius_runtime::redact::Secret { name: "BOARD_TOOL_TOKEN".into(), label: String::new(), value: zeroize::Zeroizing::new(secret.into()) });
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let made = bridge.employee_act(run, EmployeeTaskOp::CreateTicket { project: Some("Orbit".into()), title: format!("Rotate {secret}"), description: Some(format!("token {secret}")), priority: None, labels: vec![] }).await.unwrap();
    let posted = bridge.employee_act(run, EmployeeTaskOp::PostToTicket { ticket: "#1".into(), message: format!("the key is {secret}") }).await.unwrap();
    aurelius_runtime::redact::install_set(&owner, Vec::new());
    assert!(made.0 && posted.0, "{made:?} {posted:?}");
    let title: String = db(dir.path()).query_row("SELECT title FROM tickets WHERE number = 1", [], |r| r.get(0)).unwrap();
    assert!(title.starts_with("Rotate ") && !title.contains(secret), "{title}");
    // No database row, blob or event holds the value.
    let hits: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM events WHERE metadata_json LIKE ?1", [format!("%{secret}%")], |r| r.get(0)).unwrap();
    assert_eq!(hits, 0);
    for entry in walk(&dir.path().join("data")) {
        if entry.extension().is_some_and(|e| e == "sqlite" || e == "wal") {
            continue;
        }
        let bytes = std::fs::read(&entry).unwrap_or_default();
        assert!(!bytes.windows(secret.len()).any(|w| w == secret.as_bytes()), "{} holds the value", entry.display());
    }
    stop(dir.path(), daemon).await;
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() { out.extend(walk(&p)) } else { out.push(p) }
        }
    }
    out
}
